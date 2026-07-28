use parking_lot::Mutex;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

use crate::base64_encode;

/// Convert any error's `to_string()` into a generic `io::Error`. portable-pty
/// returns `anyhow::Error` rather than `io::Error`, so we wrap for our
/// `io::Result` return type.
fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

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

    /// Last time a bell notification was sent for this session, for rate
    /// limiting (BEL bursts are common, e.g. `cat` of a binary file).
    last_bell_notify: Mutex<Option<Instant>>,
    /// Last OSC 9;4 progress value seen, so we only emit on change.
    progress: Mutex<Option<(u8, u8)>>,

    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
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
            last_bell_notify: Mutex::new(None),
            progress: Mutex::new(None),
            master: Mutex::new(None),
            writer: Mutex::new(None),
            child: Mutex::new(None),
        }
    }

    pub fn spawn(self: &Arc<Self>, cols: u16, rows: u16) -> std::io::Result<()> {
        use portable_pty::native_pty_system;
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(io_err)?;

        let shell = crate::services::shell::detect_default_shell();
        let mut cmd = CommandBuilder::new(&shell.path);
        cmd.args(&shell.args);
        cmd.cwd(&self.launch_directory);
        cmd.env("TERM", "alacritty");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "muster");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));

        let child = pair.slave.spawn_command(cmd).map_err(io_err)?;
        // The slave end must be dropped on the parent side so that EOF flows
        // correctly when the child exits; we simply let it go out of scope.
        drop(pair.slave);

        // Track the shell in a kill-on-close Job Object so the Info panel can
        // find the whole process tree even when MSYS2 re-parenting breaks the
        // ppid chain. Best-effort: on failure the ppid-BFS fallback applies.
        if let Some(pid) = child.process_id() {
            crate::services::procs::track_session(self.id, pid);
        }

        let writer = pair.master.take_writer().map_err(io_err)?;

        *self.writer.lock() = Some(writer);
        *self.child.lock() = Some(child);
        *self.master.lock() = Some(pair.master);

        Ok(())
    }

    /// Start the output pump thread. All events (`pty:data`, `pty:exit`,
    /// `pty:progress`) are emitted to the window `label` that owns this
    /// session only — with multiple windows each must see just its own
    /// terminals.
    pub fn attach_read_loop(self: &Arc<Self>, app: AppHandle, label: String) {
        let guard = self.master.lock();
        let Some(master) = guard.as_ref() else { return };
        let reader = match master.try_clone_reader() {
            Ok(r) => r,
            Err(_) => return,
        };
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
                        let payload = serde_json::json!({
                            "id": session_id,
                            "data": base64_encode(&buf[..n]),
                        });
                        let _ = app.emit_to(&label, "pty:data", payload);
                        let scan = scanner.feed(&buf[..n]);
                        if let Some(cwd) = scan.cwd {
                            let mut wd = inner.working_directory.lock();
                            if wd.as_deref() != Some(cwd.as_str()) {
                                *wd = Some(cwd);
                            }
                        }
                        if let Some((state, progress)) = scan.progress {
                            update_progress(&app, &label, &inner, state, progress);
                        }
                        if scan.bells > 0 {
                            notify_bell(&app, &label, &inner);
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = app.emit_to(&label, "pty:exit", serde_json::json!({ "id": session_id }));
            *inner.has_exited.lock() = true;
        });
    }

    pub fn title(&self) -> String { self.title.lock().clone() }
    pub fn set_title(&self, t: String) { *self.title.lock() = t; }
    pub fn current_directory(&self) -> String {
        self.working_directory.lock().clone().unwrap_or_else(|| self.launch_directory.clone())
    }

    pub fn send_text(&self, text: &str) {
        if let Some(w) = self.writer.lock().as_mut() {
            let _ = w.write_all(text.as_bytes());
            let _ = w.flush();
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        if let Some(master) = self.master.lock().as_ref() {
            let _ = master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        }
    }

    pub fn terminate(&self) {
        if let Some(child) = self.child.lock().as_mut() {
            let _ = child.kill();
        }
        // Closing the job also kills anything still in it (dev servers etc.).
        crate::services::procs::untrack_session(self.id);
        *self.has_exited.lock() = true;
    }

    pub fn is_exited(&self) -> bool { *self.has_exited.lock() }

    /// Whether the PTY child process has been spawned for this session.
    /// Sessions created by snapshot restore start unspawned; bootstrap spawns
    /// them once the app handle is ready.
    pub fn is_spawned(&self) -> bool { self.master.lock().is_some() }
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
        let pid = s.child.lock().as_ref().and_then(|c| c.process_id());
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
    /// Last byte was ESC — the next byte decides whether an OSC starts (`]`),
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

/// Send a system notification for a terminal bell, but only when the
/// session's own window isn't focused (the user isn't looking) and at most
/// once every 2 seconds per session. Notifications themselves stay global.
fn notify_bell(app: &AppHandle, label: &str, session: &TerminalSession) {
    let focused = app
        .get_webview_window(label)
        .map(|w| w.is_focused().unwrap_or(true))
        .unwrap_or(true);
    if focused {
        return;
    }
    {
        let mut last = session.last_bell_notify.lock();
        let now = Instant::now();
        if last.is_some_and(|t| now.duration_since(t) < Duration::from_secs(2)) {
            return;
        }
        *last = Some(now);
    }
    let _ = app
        .notification()
        .builder()
        .title("Muster")
        .body(format!("{} — Bell", session.title()))
        .show();
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
}
