use parking_lot::Mutex;
use crate::services::conpty::ConPty;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use uuid::Uuid;

use crate::base64_encode;

/// How many ANSI-stripped scrollback lines a session keeps for the `muster
/// capture` CLI (and other backend consumers). Kept small: the full terminal
/// buffer lives in xterm.js on the frontend.
const MAX_SCROLLBACK_LINES: usize = 400;

/// One login shell owned by a terminal pane. The PTY is spawned via
/// `spawn()`, after which the master is held for `resize`, the writer for
/// `send_text`, and the reader drives an output pump thread.
pub struct TerminalSession {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: Mutex<String>,
    pub working_directory: Mutex<Option<String>>,
    pub shell_name: String,
    pub launch_directory: String,
    pub has_exited: Mutex<bool>,
    /// Whether `attach_read_loop` has taken the reader for this session
    /// (set to `true` inside `attach_read_loop`, used by `start_read_loops`
    /// to skip sessions that already have a running read pump).
    pub read_loop_started: Mutex<bool>,

    /// Last time a bell notification was sent for this session, for rate
    /// limiting (BEL bursts are common, e.g. `cat` of a binary file).
    last_bell_notify: Mutex<Option<Instant>>,
    /// Last OSC 9;4 progress value seen, so we only emit on change.
    progress: Mutex<Option<(u8, u8)>>,
    /// When the last chunk of PTY output was read. Drives agent idle
    /// detection ("the coding agent is waiting for input").
    last_output_at: Mutex<Instant>,
    /// Last time an agent "waiting for input" notification was sent, for
    /// rate limiting.
    last_agent_notify: Mutex<Option<Instant>>,
    /// Ring buffer of ANSI-stripped output lines (for `muster capture`).
    scrollback: Mutex<VecDeque<String>>,
    /// Partial line not yet terminated by `\n`, accumulated across chunks.
    scrollback_pending: Mutex<String>,
    /// Escape-sequence state carried across chunk boundaries, so a sequence
    /// split by a read is still stripped.
    ansi: Mutex<AnsiStripper>,

    conpty: Mutex<Option<ConPty>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
}

impl TerminalSession {
    pub fn new(project_id: Uuid, directory: String) -> Self {
        let shell = crate::services::shell::detect_default_shell();
        Self {
            id: Uuid::new_v4(),
            project_id,
            title: Mutex::new(shell.name.clone()),
            working_directory: Mutex::new(Some(directory.clone())),
            shell_name: shell.name,
            launch_directory: directory,
            has_exited: Mutex::new(false),
            read_loop_started: Mutex::new(false),
            last_bell_notify: Mutex::new(None),
            progress: Mutex::new(None),
            last_output_at: Mutex::new(Instant::now()),
            last_agent_notify: Mutex::new(None),
            scrollback: Mutex::new(VecDeque::new()),
            scrollback_pending: Mutex::new(String::new()),
            ansi: Mutex::new(AnsiStripper::default()),
            conpty: Mutex::new(None),
            writer: Mutex::new(None),
        }
    }

    pub fn spawn(self: &Arc<Self>, cols: u16, rows: u16) -> std::io::Result<()> {
        let mut conpty = ConPty::create(cols, rows)?;

        let shell = crate::services::shell::detect_default_shell();
        let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
        env.insert("TERM".into(), "alacritty".into());
        env.insert("COLORTERM".into(), "truecolor".into());
        env.insert("TERM_PROGRAM".into(), "muster".into());
        env.insert("TERM_PROGRAM_VERSION".into(), env!("CARGO_PKG_VERSION").into());
        #[cfg(windows)]
        if let Some(path) = crate::services::shell::fresh_path_from_registry() {
            env.insert("PATH".into(), path);
        }
        let env_vec: Vec<(String, String)> = env.into_iter().collect();

        let pid = conpty.spawn_shell(&shell.path, &shell.args, &self.launch_directory, &env_vec)?;

        crate::services::procs::track_session(self.id, pid);

        let writer = conpty.take_writer()?;
        *self.writer.lock() = Some(writer);
        *self.conpty.lock() = Some(conpty);

        Ok(())
    }

    /// Start the output pump thread. All events (`pty:data`, `pty:exit`,
    /// `pty:progress`) are emitted to the window `label` that owns this
    /// session only 閳?with multiple windows each must see just its own
    /// terminals.
    pub fn attach_read_loop(self: &Arc<Self>, app: AppHandle, label: String) {
        let mut guard = self.conpty.lock();
        let Some(conpty) = guard.as_mut() else {
            log::warn!("attach_read_loop: session {} has no conpty (not spawned?)", self.id);
            return;
        };
        let reader = match conpty.take_reader() {
            Ok(r) => r,
            Err(_) => return, // reader already taken (e.g. StrictMode double-mount)
        };
        log::info!("attach_read_loop: session {} starting read pump", self.id);
        *self.read_loop_started.lock() = true;
        let session_id = self.id;
        let inner = self.clone();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            // Incremental escape-sequence scanner: sequences may be split
            // across read chunk boundaries, so partial OSC bodies are kept
            // inside the scanner between chunks.
            let mut scanner = OscScanner::new();
            loop {
                if inner.is_exited() { break }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        inner.record_output(&buf[..n]);
                        let payload = serde_json::json!({
                            "id": session_id,
                            "data": base64_encode(&buf[..n]),
                        });
                        let _ = app.emit_to(&label, "pty:data", payload);
                        let scan = scanner.feed(&buf[..n]);
                        if let Some(cwd) = scan.cwd {
                            let mut wd = inner.working_directory.lock();
                            if wd.as_deref() != Some(cwd.as_str()) {
                                *wd = Some(cwd.to_string());
                                // Notify the frontend immediately so panels can
                                // re-root (e.g. `cd` into a git worktree) without
                                // waiting for the 2s poll.
                                let _ = app.emit_to(
                                    &label,
                                    "session-cwd-changed",
                                    serde_json::json!({
                                        "id": session_id,
                                        "cwd": cwd,
                                    }),
                                );
                            }
                        }
                        if let Some((state, progress)) = scan.progress {
                            update_progress(&app, &label, &inner, state, progress);
                        }
                        if scan.bells > 0 {
                            notify_bell(&app, &label, &inner);
                        }
                    }
                    Err(e) => { log::debug!("pty read ended (session {}): {e}", session_id); break; },
                }
            }
            // The shell exited on its own (EOF / read error), e.g. the user
            // typed `exit`: `terminate()` is never called on this path, so
            // release the Job Object here too. Idempotent 閳?a later
            // `terminate()` untracks a second time harmlessly. Closing the
            // job also reaps any children the exited shell left behind
            // (KILL_ON_JOB_CLOSE), which is the intended cleanup.
            crate::services::procs::untrack_session(session_id);
            let _ = app.emit_to(&label, "pty:exit", serde_json::json!({ "id": session_id }));
            *inner.has_exited.lock() = true;
        });
    }

    pub fn title(&self) -> String { self.title.lock().clone() }
    pub fn set_title(&self, t: String) { *self.title.lock() = t; }
    pub fn current_directory(&self) -> String {
        self.working_directory.lock().clone().unwrap_or_else(|| self.launch_directory.clone())
    }

    /// The PTY child's pid (the login shell), if the session is spawned.
    pub fn shell_pid(&self) -> Option<u32> {
        self.conpty.lock().as_ref().and_then(|c| c.process_id())
    }

    /// How long since the last chunk of PTY output was read. Used by the
    /// agent poller to decide whether a coding agent is working or has
    /// stopped to wait for input.
    pub fn idle_for(&self) -> Duration {
        self.last_output_at.lock().elapsed()
    }

    /// Feed one chunk of raw PTY output into the capture ring buffer:
    /// updates `last_output_at` and appends ANSI-stripped lines. Called by
    /// the read pump on every chunk.
    pub fn record_output(&self, bytes: &[u8]) {
        *self.last_output_at.lock() = Instant::now();
        let mut stripped = String::new();
        self.ansi.lock().feed(bytes, &mut stripped);
        if stripped.is_empty() {
            return;
        }
        let mut pending = self.scrollback_pending.lock();
        pending.push_str(&stripped);
        while let Some(pos) = pending.find('\n') {
            let line: String = pending.drain(..=pos).collect();
            let mut sb = self.scrollback.lock();
            // Line-overwrite dedup: PSReadLine-style UIs redraw the current
            // line with `\r` while the user types, and progress bars redraw
            // themselves. Each redraw arrives as a full line that extends
            // the previous one — replace instead of stacking, so the ring
            // keeps the final rendering rather than every keystroke frame.
            if let Some(prev) = sb.back_mut() {
                let prev_t = prev.trim_end_matches('\n');
                let line_t = line.trim_end_matches('\n');
                if line_t.starts_with(prev_t) && line_t.len() > prev_t.len() {
                    *prev = line;
                    continue;
                }
            }
            sb.push_back(line);
            while sb.len() > MAX_SCROLLBACK_LINES {
                sb.pop_front();
            }
        }
    }

    /// The last `max` scrollback lines (with newlines), plus any partial
    /// unterminated tail. For `muster capture`. Lock order matches
    /// `record_output` (pending, then ring) to avoid deadlock.
    pub fn scrollback_lines(&self, max: usize) -> Vec<String> {
        let tail = self.scrollback_pending.lock().clone();
        let mut lines: Vec<String> = self.scrollback.lock().iter().cloned().collect();
        if !tail.is_empty() {
            lines.push(tail);
        }
        let skip = lines.len().saturating_sub(max);
        lines.split_off(skip)
    }

    /// Whether a "waiting for input" notification was sent within the last
    /// `cooldown`; if not, records the send and returns true (caller should
    /// notify).
    pub fn try_mark_agent_notify(&self, cooldown: Duration) -> bool {
        let mut last = self.last_agent_notify.lock();
        let now = Instant::now();
        if last.is_some_and(|t| now.duration_since(t) < cooldown) {
            return false;
        }
        *last = Some(now);
        true
    }

    pub fn send_text(&self, text: &str) {
        if let Some(w) = self.writer.lock().as_mut() {
            if let Err(e) = w.write_all(text.as_bytes()) {
                log::warn!("pty write failed (session {}): {e}", self.id);
                return;
            }
            if let Err(e) = w.flush() {
                log::warn!("pty flush failed (session {}): {e}", self.id);
            }
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        if let Some(conpty) = self.conpty.lock().as_ref() {
            if let Err(e) = conpty.resize(cols, rows) {
                log::warn!("pty resize failed (session {}): {e}", self.id);
            }
        }
    }

    pub fn terminate(&self) {
        if let Some(conpty) = self.conpty.lock().as_mut() {
            conpty.kill_child();
        }
        crate::services::procs::untrack_session(self.id);
        *self.has_exited.lock() = true;
    }

    pub fn is_exited(&self) -> bool { *self.has_exited.lock() }

    /// Whether the PTY child process has been spawned for this session.
    /// Sessions created by snapshot restore start unspawned; bootstrap spawns
    /// them once the app handle is ready.
    pub fn is_spawned(&self) -> bool { self.conpty.lock().is_some() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub working_directory: String,
    pub shell_name: String,
    pub has_exited: bool,
    pub pid: Option<u32>,
}

impl From<&TerminalSession> for SessionInfo {
    fn from(s: &TerminalSession) -> Self {
        let pid = s.conpty.lock().as_ref().and_then(|c| c.process_id());
        Self {
            id: s.id,
            project_id: s.project_id,
            title: s.title(),
            working_directory: s.current_directory(),
            shell_name: s.shell_name.clone(),
            has_exited: s.is_exited(),
            pid,
        }
    }
}

/// Result of feeding one output chunk through the OSC scanner.
#[derive(Default)]
struct ScanOut {
    cwd: Option<String>,
    /// OSC 9;4 (ConEmu progress) as (state, progress 0-100).
    progress: Option<(u8, u8)>,
    /// BEL bytes seen outside of OSC sequences (real bells, not terminators).
    bells: usize,
}

/// Incremental scanner for the escape sequences we care about in PTY output:
/// OSC 7 / 9;9 cwd reports, OSC 9;4 progress, and bare BEL bytes. Fed
/// chunk-by-chunk from the read loop; sequences split across read boundaries
/// are reassembled from the buffered partial body.
struct OscScanner {
    /// OSC body bytes collected so far (after `ESC ]`); None when not inside
    /// a sequence.
    body: Option<Vec<u8>>,
    /// Last byte was ESC 閳?the next byte decides whether an OSC starts (`]`),
    /// a sequence ends (`\`, i.e. ST), or the escape was something else.
    esc: bool,
}

impl OscScanner {
    fn new() -> Self {
        Self { body: None, esc: false }
    }

    fn feed(&mut self, bytes: &[u8]) -> ScanOut {
        let mut out = ScanOut::default();
        for &b in bytes {
            if self.esc {
                self.esc = false;
                match b {
                    // New OSC begins (aborts any in-progress one).
                    b']' => self.body = Some(Vec::new()),
                    // ST terminates the sequence.
                    b'\\' => {
                        if let Some(body) = self.body.take() {
                            handle_sequence(&body, &mut out);
                        }
                    }
                    // ESC aborted any in-progress sequence; this byte is
                    // ordinary data (and may itself start a new escape).
                    0x1b => {
                        self.body = None;
                        self.esc = true;
                    }
                    0x07 => {
                        self.body = None;
                        out.bells += 1;
                    }
                    _ => self.body = None,
                }
                continue;
            }
            match b {
                0x1b => self.esc = true,
                // BEL either terminates an OSC sequence or is a real bell.
                0x07 => {
                    if let Some(body) = self.body.take() {
                        handle_sequence(&body, &mut out);
                    } else {
                        out.bells += 1;
                    }
                }
                _ => {
                    // Cap the buffered body so a runaway sequence can't grow
                    // it without bound.
                    if let Some(body) = self.body.as_mut() {
                        if body.len() < 16384 {
                            body.push(b);
                        }
                    }
                }
            }
        }
        out
    }
}

/// Interpret one complete OSC sequence body (the bytes between `ESC ]` and
/// the BEL/ST terminator), recording the latest cwd / progress seen.
fn handle_sequence(body: &[u8], out: &mut ScanOut) {
    let body = String::from_utf8_lossy(body);
    if let Some(rest) = body.strip_prefix("7;") {
        if let Some(path) = parse_osc7_path(rest) {
            out.cwd = Some(path);
        }
    } else if let Some(rest) = body.strip_prefix("9;9;") {
        out.cwd = Some(rest.trim_matches('"').to_string());
    } else if let Some(rest) = body.strip_prefix("9;4;") {
        // ConEmu progress: `9;4;<state>;<progress>` with state 0=remove,
        // 1=normal, 2=error, 3=indeterminate, 4=warning.
        let mut parts = rest.split(';');
        if let (Some(state), Some(progress)) = (parts.next(), parts.next()) {
            if let (Ok(state), Ok(progress)) = (state.parse::<u8>(), progress.parse::<u8>()) {
                out.progress = Some((state, progress.min(100)));
            }
        }
    }
}

/// Emit `pty:progress` (only when the value changes) and mirror it onto the
/// Windows taskbar button of the session's own window. Aggregation rule for
/// multiple sessions reporting progress at once: the most recently updated
/// session wins.
fn update_progress(app: &AppHandle, label: &str, session: &TerminalSession, state: u8, progress: u8) {
    let mut last = session.progress.lock();
    if *last == Some((state, progress)) {
        return;
    }
    *last = Some((state, progress));
    drop(last);
    let _ = app.emit_to(
        label,
        "pty:progress",
        serde_json::json!({ "id": session.id, "state": state, "progress": progress }),
    );
    if let Some(window) = app.get_webview_window(label) {
        // Map the OSC 9;4 state onto a taskbar status; 4 (warning) has no
        // direct equivalent, so it shows as Paused (yellow).
        let status = match state {
            0 => ProgressBarStatus::None,
            1 => ProgressBarStatus::Normal,
            2 => ProgressBarStatus::Error,
            3 => ProgressBarStatus::Indeterminate,
            _ => ProgressBarStatus::Paused,
        };
        let _ = window.set_progress_bar(ProgressBarState {
            status: Some(status),
            progress: Some(progress as u64),
        });
    }
}

/// React to a terminal bell: show a system notification when the session's
/// own window isn't focused (the user isn't looking), otherwise play the
/// system sound directly since the toast is suppressed. Either way, at most
/// once every 2 seconds per session. Notifications themselves stay global.
fn notify_bell(app: &AppHandle, label: &str, session: &TerminalSession) {
    {
        let mut last = session.last_bell_notify.lock();
        let now = Instant::now();
        if last.is_some_and(|t| now.duration_since(t) < Duration::from_secs(2)) {
            return;
        }
        *last = Some(now);
    }
    let focused = app
        .get_webview_window(label)
        .map(|w| w.is_focused().unwrap_or(true))
        .unwrap_or(true);
    if focused {
        // Plays asynchronously, so the PTY reader thread isn't blocked.
        unsafe {
            let _ = windows::Win32::System::Diagnostics::Debug::MessageBeep(
                windows::Win32::UI::WindowsAndMessaging::MB_ICONASTERISK,
            );
        }
        return;
    }
    crate::services::notify::send(app, label, session.id, format!("{} \u{2014} Bell", session.title()));
}

/// Extract the path component of an OSC 7 `file://host/path` URI.
fn parse_osc7_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let slash = rest.find('/')?;
    Some(percent_decode(&rest[slash..]))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Strip ANSI/VT escape sequences from PTY output, approximating what a
/// human would read: CSI (`ESC [ ... letter`), OSC (`ESC ] ... BEL/ST`) and
/// other single-byte escapes are removed. `\r` becomes a line break
/// (progress bars rewrite the line with `\r`), `\n` and `\t` survive, all
/// other C0 controls are dropped. The stripper is stateful across chunks so
/// a sequence split by a read boundary is still stripped (the previous
/// stateless version leaked the tail half as literal text).
#[derive(Default)]
struct AnsiStripper {
    state: StripState,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum StripState {
    #[default]
    Normal,
    /// Last byte was ESC.
    AfterEsc,
    /// Inside `ESC [ ... final byte`.
    InCsi,
    /// Inside `ESC ] ... terminator`.
    InOsc,
    /// Inside an OSC and the last byte was ESC (waiting for `\` = ST).
    InOscEsc,
}

impl AnsiStripper {
    fn feed(&mut self, bytes: &[u8], out: &mut String) {
        for i in 0..bytes.len() {
            let b = bytes[i];
            match self.state {
                StripState::Normal => match b {
                    0x1b => self.state = StripState::AfterEsc,
                    0x0a | 0x0d => out.push('\n'),
                    0x09 => out.push('\t'),
                    b if b < 0x20 => {}
                    _ => push_utf8(bytes, i, out),
                },
                StripState::AfterEsc => match b {
                    b'[' => self.state = StripState::InCsi,
                    b']' => self.state = StripState::InOsc,
                    // ESC ESC: still escaped (next byte decides).
                    0x1b => {}
                    _ => self.state = StripState::Normal,
                },
                StripState::InCsi => match b {
                    0x1b => self.state = StripState::AfterEsc, // abort CSI
                    0x40..=0x7e => self.state = StripState::Normal, // final byte
                    _ => {}
                },
                StripState::InOsc => match b {
                    0x07 => self.state = StripState::Normal, // BEL terminates
                    0x1b => self.state = StripState::InOscEsc,
                    _ => {}
                },
                StripState::InOscEsc => {
                    if b == b'\\' {
                        self.state = StripState::Normal; // ST terminates
                    } else {
                        self.state = StripState::InOsc;
                    }
                }
            }
        }
    }
}

/// Push one UTF-8 character (which starts at `bytes[i]`) into `out`, when
/// the full sequence is present in this chunk; incomplete tails are dropped.
fn push_utf8(bytes: &[u8], i: usize, out: &mut String) {
    let rest = &bytes[i..];
    let width = if rest[0] < 0x80 {
        1
    } else if rest[0] >> 5 == 0b110 {
        2
    } else if rest[0] >> 4 == 0b1110 {
        3
    } else if rest[0] >> 3 == 0b11110 {
        4
    } else {
        1
    };
    if width > rest.len() {
        return; // truncated multibyte tail at the chunk boundary
    }
    if let Ok(s) = std::str::from_utf8(&rest[..width]) {
        out.push_str(s);
    }
}

/// One-shot strip of a complete buffer (stateless convenience wrapper, used
/// by tests; the read loop uses the stateful `AnsiStripper`).
#[cfg(test)]
fn strip_ansi(bytes: &[u8]) -> String {
    let mut s = AnsiStripper::default();
    let mut out = String::new();
    s.feed(bytes, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bell_terminator_is_not_counted_as_bell() {
        let mut s = OscScanner::new();
        // OSC 7 terminated by BEL, then a real bell.
        let out = s.feed(b"\x1b]7;file://host/C:/work\x07\x07");
        assert_eq!(out.cwd.as_deref(), Some("/C:/work"));
        assert_eq!(out.bells, 1);
    }

    #[test]
    fn progress_st_terminated() {
        let mut s = OscScanner::new();
        let out = s.feed(b"\x1b]9;4;1;50\x1b\\");
        assert_eq!(out.progress, Some((1, 50)));
        assert_eq!(out.bells, 0);
    }

    #[test]
    fn progress_clamps_and_ignores_garbage() {
        let mut s = OscScanner::new();
        assert_eq!(s.feed(b"\x1b]9;4;2;140\x07").progress, Some((2, 100)));
        assert_eq!(s.feed(b"\x1b]9;4;nope\x07").progress, None);
        assert_eq!(s.feed(b"\x1b]9;4;\x07").progress, None);
    }

    #[test]
    fn sequence_split_across_chunks() {
        let mut s = OscScanner::new();
        assert_eq!(s.feed(b"\x1b]9;4;").progress, None);
        let out = s.feed(b"3;0\x07");
        assert_eq!(out.progress, Some((3, 0)));
        // BEL acting as the terminator must not count as a bell.
        assert_eq!(out.bells, 0);
    }

    #[test]
    fn esc_inside_body_aborts_sequence() {
        let mut s = OscScanner::new();
        // ESC that isn't followed by `\` aborts the OSC; the bell after it
        // is a real one.
        let out = s.feed(b"\x1b]9;4;1;50\x1bX\x07");
        assert_eq!(out.progress, None);
        assert_eq!(out.bells, 1);
    }

    #[test]
    fn plain_text_and_bells() {
        let mut s = OscScanner::new();
        let out = s.feed(b"hello\x07world\x07");
        assert_eq!(out.bells, 2);
        assert_eq!(out.cwd, None);
        assert_eq!(out.progress, None);
    }

    #[test]
    fn strip_ansi_removes_csi_osc_and_controls() {
        // CSI color codes, OSC title, BEL, \r\n -> \n, tab preserved.
        let input = b"\x1b[31mred\x1b[0m\x1b]0;title\x07\n\rline2\t\x07\x08";
        assert_eq!(strip_ansi(input), "red\n\nline2\t");
    }

    #[test]
    fn strip_ansi_cr_becomes_newline() {
        // A progress bar that rewrites the line with \r: the final state is
        // the last line.
        assert_eq!(strip_ansi(b"10%\r20%\r100%"), "10%\n20%\n100%");
    }

    #[test]
    fn strip_ansi_handles_utf8_and_split_escapes() {
        // CJK text passes through untouched.
        assert_eq!(strip_ansi("你好世界".as_bytes()), "你好世界");
        // A multibyte char split across the chunk boundary is dropped (the
        // terminal display is unaffected; only the capture ring loses it).
        let cjk = "你".as_bytes();
        assert_eq!(cjk.len(), 3);
        assert_eq!(strip_ansi(&cjk[..2]), "");
        assert_eq!(strip_ansi(&cjk[2..]), "");
        assert_eq!(strip_ansi(cjk), "你");
    }

    #[test]
    fn strip_ansi_state_carries_across_chunks() {
        // ESC at the end of one chunk, CSI in the next: both halves are
        // stripped (the stateful stripper, unlike a per-chunk one-shot,
        // doesn't leak the literal "[31m").
        let mut s = AnsiStripper::default();
        let mut out = String::new();
        s.feed(b"\x1b", &mut out);
        s.feed(b"[31mred\x1b[0m", &mut out);
        assert_eq!(out, "red");

        // OSC split across three chunks, terminated by BEL.
        let mut s = AnsiStripper::default();
        let mut out = String::new();
        s.feed(b"\x1b]0;", &mut out);
        s.feed(b"my title", &mut out);
        s.feed(b"\x07ok", &mut out);
        assert_eq!(out, "ok");

        // OSC terminated by ST (ESC \) split across chunks.
        let mut s = AnsiStripper::default();
        let mut out = String::new();
        s.feed(b"\x1b]9;4;1;50", &mut out);
        s.feed(b"\x1b\\", &mut out);
        assert_eq!(out, "");
    }

    #[test]
    fn scrollback_ring_keeps_last_lines_and_pending_tail() {
        let s = TerminalSession::new(uuid::Uuid::new_v4(), "C:\\work".into());
        for _ in 0..10 {
            s.record_output(b"line\n");
        }
        let lines = s.scrollback_lines(5);
        assert_eq!(lines, vec!["line\n"; 5]);
        // Unterminated tail is included as the final line.
        s.record_output(b"tail-");
        let lines = s.scrollback_lines(100);
        assert_eq!(lines.last().map(String::as_str), Some("tail-"));
        // ...and completes when the next chunk arrives.
        s.record_output(b"done\n");
        let lines = s.scrollback_lines(100);
        assert_eq!(lines.last().map(String::as_str), Some("tail-done\n"));
    }

    #[test]
    fn scrollback_dedups_line_redraws() {
        let s = TerminalSession::new(uuid::Uuid::new_v4(), "C:\\work".into());
        // Per-keystroke line redraws (`\r`-rewritten): each extends the last.
        for i in 0.."echo HI".len() {
            let prefix: String = "echo HI".chars().take(i + 1).collect();
            s.record_output(format!("{prefix}\r").as_bytes());
        }
        // Only the final rendering survives.
        assert_eq!(s.scrollback_lines(100), vec!["echo HI\n"]);

        // A line that is NOT an extension (e.g. a new prompt) still stacks.
        s.record_output(b"\n");
        assert_eq!(s.scrollback_lines(100), vec!["echo HI\n", "\n"]);
    }

    #[test]
    fn record_output_updates_last_output_at() {
        let s = TerminalSession::new(uuid::Uuid::new_v4(), "C:\\work".into());
        s.record_output(b"x");
        // The elapsed time since the write is near zero.
        assert!(s.idle_for() < Duration::from_secs(1));
    }
}
