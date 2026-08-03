//! Local IPC bridge so external CLIs — and AI coding agents — can drive
//! Muster from the shell. The GUI app runs a small JSON-over-TCP server on
//! `127.0.0.1:<ephemeral port>`; the connection details (port + a random
//! per-launch token) are written to `<app data>/ipc.json`. Re-invoking the
//! `muster` binary with a verb (e.g. `muster split`, `muster send <id> ...`)
//! acts as a one-shot client: it reads `ipc.json`, talks to the server,
//! prints the answer and exits. If Muster isn't running, the client starts
//! it and waits for the server to come up.
//!
//! Wire format (line-delimited JSON, one request per connection):
//!   request:  `{"id":1,"token":"...","verb":"split","args":{...}}`
//!   response: `{"id":1,"ok":true,"data":{...}}` or `{"ok":false,"error":"..."}`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use uuid::Uuid;

use crate::commands::SharedState;
use crate::models::pane::PaneContent;
use crate::models::session::TerminalSession;

/// Max accepted request size (a `send` of a huge script is the realistic
/// ceiling; anything beyond is a misbehaving client).
const MAX_REQUEST_BYTES: usize = 256 * 1024;
/// Max response size we buffer on the client side.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// How long the CLI waits for a freshly spawned Muster to bring up its
/// server before giving up.
const BOOT_WAIT: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------
// Server (GUI side)
// ---------------------------------------------------------------------------

/// Start the IPC server. Called once from bootstrap setup; runs for the
/// app's lifetime. Writes `<app data>/ipc.json` with the port + token.
pub fn start(app: AppHandle) {
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("ipc: failed to bind: {e}");
            return;
        }
    };
    let port = match listener.local_addr() {
        Ok(a) => a.port(),
        Err(e) => {
            log::warn!("ipc: failed to read local addr: {e}");
            return;
        }
    };
    let token = Uuid::new_v4().simple().to_string();
    if let Err(e) = write_info(port, &token) {
        log::warn!("ipc: failed to write ipc.json: {e}");
        return;
    }
    log::info!("ipc: listening on 127.0.0.1:{port}");

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let app = app.clone();
            let token = token.clone();
            std::thread::spawn(move || handle_connection(stream, &app, &token));
        }
    });
}

fn ipc_info_path() -> PathBuf {
    crate::services::persist::app_data_dir().join("ipc.json")
}

fn write_info(port: u16, token: &str) -> std::io::Result<()> {
    let dir = ipc_info_path();
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dir, json!({ "port": port, "token": token }).to_string())
}

#[derive(Deserialize)]
struct Request {
    id: u64,
    token: String,
    verb: String,
    #[serde(default)]
    args: Value,
}

#[derive(Deserialize, Default)]
struct Args {
    directory: Option<String>,
    id: Option<Uuid>,
    text: Option<String>,
    enter: Option<bool>,
    lines: Option<usize>,
    command: Option<String>,
    timeout_secs: Option<u64>,
    vertical: Option<bool>,
}

fn handle_connection(mut stream: TcpStream, app: &AppHandle, token: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    // Read until newline, capping the total size.
    loop {
        match stream.read(&mut byte) {
            Ok(0) => {
                write_response(&stream, 0, Err("empty request".into()));
                return;
            }
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                buf.push(byte[0]);
                if buf.len() > MAX_REQUEST_BYTES {
                    write_response(&stream, 0, Err("request too large".into()));
                    return;
                }
            }
            Err(_) => return,
        }
    }

    let req: Request = match serde_json::from_slice(&buf) {
        Ok(r) => r,
        Err(e) => {
            write_response(&stream, 0, Err(format!("bad request: {e}")));
            return;
        }
    };
    if req.token != token {
        write_response(&stream, req.id, Err("unauthorized".into()));
        return;
    }
    let result = dispatch(app, &req.verb, req.args);
    write_response(&stream, req.id, result);
}

fn write_response(stream: &TcpStream, id: u64, result: Result<Value, String>) {
    let payload = match result {
        Ok(data) => json!({ "id": id, "ok": true, "data": data }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    };
    let mut s = payload.to_string();
    s.push('\n');
    if let Ok(mut clone) = stream.try_clone() {
        let _ = clone.write_all(s.as_bytes());
    }
}

/// The window CLI commands operate on by default: "main" when it exists
/// (it always does in practice), otherwise any live window.
fn target_label(shared: &SharedState) -> Option<String> {
    if shared.get_label("main").is_some() {
        return Some("main".into());
    }
    shared.all().first().map(|(label, _)| label.clone())
}

/// Find a session by id in any window, returning its owning window label.
fn find_session(shared: &SharedState, id: Uuid) -> Option<(String, Arc<TerminalSession>)> {
    for (label, s) in shared.all() {
        if let Some(session) = s.lock().sessions.get(&id).cloned() {
            return Some((label, session));
        }
    }
    None
}

fn emit_state_changed(app: &AppHandle, label: &str, state: &Arc<Mutex<crate::models::app::AppState>>) {
    let view = state.lock().view();
    let _ = app.emit_to(label, "state-changed", view);
}

fn dispatch(app: &AppHandle, verb: &str, args_value: Value) -> Result<Value, String> {
    let shared = app.state::<SharedState>().inner();
    let args: Args = serde_json::from_value(args_value).unwrap_or_default();
    match verb {
        "doctor" => verb_doctor(shared),
        "ls" => verb_ls(shared),
        "agents" => verb_agents(shared),
        "new" => verb_new(app, shared, &args),
        "split" => verb_split(app, shared, &args),
        "send" => verb_send(shared, &args),
        "capture" => verb_capture(shared, &args),
        "procs" => verb_procs(shared, &args),
        "run" => verb_run(app, shared, &args),
        other => Err(format!("unknown verb '{other}'")),
    }
}

fn verb_doctor(shared: &SharedState) -> Result<Value, String> {
    let windows = shared.all();
    let sessions: usize = windows
        .iter()
        .map(|(_, s)| s.lock().sessions.len())
        .sum();
    let agents = shared.agents.lock().statuses.len();
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "windows": windows.len(),
        "sessions": sessions,
        "agents": agents,
    }))
}

fn verb_ls(shared: &SharedState) -> Result<Value, String> {
    let mut windows = Vec::new();
    for (label, s) in shared.all() {
        let g = s.lock();
        let projects: Vec<Value> = g
            .projects
            .iter()
            .map(|p| {
                let tabs: Vec<Value> = p
                    .tabs
                    .iter()
                    .map(|t| {
                        let panes: Vec<Value> = t
                            .columns
                            .iter()
                            .flat_map(|c| c.panes.iter())
                            .map(|pane| {
                                let (kind, detail) = match &pane.content {
                                    PaneContent::Session(id) => (
                                        "session",
                                        g.sessions
                                            .get(id)
                                            .map(|s| s.current_directory())
                                            .unwrap_or_default(),
                                    ),
                                    PaneContent::File(id) => (
                                        "file",
                                        g.files
                                            .get(id)
                                            .map(|f| f.path.lock().clone())
                                            .unwrap_or_default(),
                                    ),
                                    PaneContent::Diff(id) => (
                                        "diff",
                                        g.diffs.get(id).map(|d| d.title()).unwrap_or_default(),
                                    ),
                                };
                                json!({ "id": pane.id, "kind": kind, "detail": detail })
                            })
                            .collect();
                        json!({ "id": t.id, "focused_pane_id": t.focused_pane_id, "panes": panes })
                    })
                    .collect();
                json!({
                    "id": p.id,
                    "name": p.name(&g),
                    "directory": p.custom_directory,
                    "tabs": tabs,
                })
            })
            .collect();
        windows.push(json!({ "label": label, "projects": projects }));
    }
    Ok(json!({ "windows": windows }))
}

fn verb_agents(shared: &SharedState) -> Result<Value, String> {
    let mut rows: Vec<Value> = shared
        .agents
        .lock()
        .statuses
        .iter()
        .map(|(id, s)| json!({ "id": id, "agent": s.agent, "state": s.state }))
        .collect();
    rows.sort_by_key(|r| r["id"].as_str().unwrap_or("").to_string());
    Ok(json!({ "sessions": rows }))
}

fn verb_new(app: &AppHandle, shared: &SharedState, args: &Args) -> Result<Value, String> {
    let label = target_label(shared).ok_or("no window is open")?;
    let state = shared.get_label(&label).ok_or("window state gone")?;
    let project_id = { state.lock().new_project(args.directory.clone()) };
    crate::bootstrap::spawn_pending(app, &label, &state);
    crate::bootstrap::start_read_loops(app, &label, &state);
    emit_state_changed(app, &label, &state);
    let session_id = {
        let g = state.lock();
        g.projects
            .iter()
            .rev()
            .find(|p| p.id == project_id)
            .and_then(|p| p.session_ids().into_iter().next())
    };
    let session_id = session_id.ok_or("no session was created")?;
    Ok(json!({ "session_id": session_id, "project_id": project_id }))
}

fn verb_split(app: &AppHandle, shared: &SharedState, args: &Args) -> Result<Value, String> {
    let label = target_label(shared).ok_or("no window is open")?;
    let state = shared.get_label(&label).ok_or("window state gone")?;
    let session_id = {
        let mut g = state.lock();
        if g.selected_project().is_none() {
            g.new_project(args.directory.clone());
        }
        let edge = if args.vertical.unwrap_or(false) {
            crate::models::pane::PaneDropEdge::Bottom
        } else {
            crate::models::pane::PaneDropEdge::Right
        };
        g.split_with_dir(edge, args.directory.clone())
            .ok_or("no tab to split")?
    };
    let session = state
        .lock()
        .sessions
        .get(&session_id)
        .cloned()
        .ok_or("session vanished")?;
    session.spawn(120, 30).map_err(|e| e.to_string())?;
    session.attach_read_loop(app.clone(), label.clone());
    emit_state_changed(app, &label, &state);
    Ok(json!({ "session_id": session_id }))
}

fn verb_send(shared: &SharedState, args: &Args) -> Result<Value, String> {
    let id = args.id.ok_or("send requires 'id'")?;
    let text = args.text.clone().unwrap_or_default();
    let (_label, session) = find_session(shared, id).ok_or("unknown session")?;
    let text = if args.enter.unwrap_or(false) {
        format!("{text}\r")
    } else {
        text
    };
    session.send_text(&text);
    Ok(json!({ "sent": true }))
}

fn verb_capture(shared: &SharedState, args: &Args) -> Result<Value, String> {
    let id = args.id.ok_or("capture requires 'id'")?;
    let max = args.lines.unwrap_or(200).clamp(1, 2000);
    let (_label, session) = find_session(shared, id).ok_or("unknown session")?;
    let text = session.scrollback_lines(max).concat();
    Ok(json!({ "text": text }))
}

fn verb_procs(shared: &SharedState, args: &Args) -> Result<Value, String> {
    let id = args.id.ok_or("procs requires 'id'")?;
    let (_label, session) = find_session(shared, id).ok_or("unknown session")?;
    let shell_pid = session.shell_pid().unwrap_or(0);
    let pids = crate::services::procs::session_pids(id, shell_pid);
    let procs: Vec<Value> = crate::services::procs::process_infos(&pids)
        .into_iter()
        .map(|p| {
            json!({
                "pid": p.pid,
                "name": p.name,
                "cpu": p.cpu,
                "mem_bytes": p.mem_bytes,
                "exe": p.exe,
            })
        })
        .collect();
    let ports: Vec<Value> = crate::services::procs::listen_ports(&pids, None)
        .into_iter()
        .map(|p| json!({ "port": p.port, "pid": p.pid, "process_name": p.process_name }))
        .collect();
    Ok(json!({ "shell_pid": shell_pid, "procs": procs, "ports": ports }))
}

/// Run a command in a fresh terminal tab and wait for it to finish, then
/// return the captured output (plus the exit code when the shell echoed it).
fn verb_run(app: &AppHandle, shared: &SharedState, args: &Args) -> Result<Value, String> {
    let command = args.command.clone().unwrap_or_default();
    if command.is_empty() {
        return Err("run requires a non-empty 'command'".into());
    }
    let timeout = args.timeout_secs.unwrap_or(600).clamp(1, 3600);
    let label = target_label(shared).ok_or("no window is open")?;
    let state = shared.get_label(&label).ok_or("window state gone")?;
    let session_id = {
        let mut g = state.lock();
        if g.selected_project().is_none() {
            g.new_project(args.directory.clone());
        }
        g.spawn_session_in_selected(args.directory.clone())
            .ok_or("no project")?
    };
    // Spawn PTYs + read pumps for the new session AND any other pending
    // session (e.g. the starter terminal of a freshly opened project), so
    // nothing is left as a dead tab.
    crate::bootstrap::spawn_pending(app, &label, &state);
    crate::bootstrap::start_read_loops(app, &label, &state);
    let session = state
        .lock()
        .sessions
        .get(&session_id)
        .cloned()
        .ok_or("session vanished")?;
    session.resize(120, 30);

    // Completion is signalled by a marker line the shell prints AFTER the
    // command finishes, not by the process tree — a cmdlet-only command
    // (echo, Copy-Item, ...) never spawns a child process, so "only the
    // shell is left" is true from the very first poll.
    //
    // The marker is a SECOND line sent separately, so each shell expands its
    // exit-status variable when the line executes (after the foreground
    // command finished), not when the line was buffered. `$LASTEXITCODE` is
    // stale for pure-cmdlet commands — documented limitation.
    let probe = match session.shell_name.to_lowercase().as_str() {
        "pwsh" | "powershell" => "echo \"__MUSTER_RC=$LASTEXITCODE\"",
        "cmd" => "echo __MUSTER_RC=%errorlevel%",
        _ => "echo __MUSTER_RC=$?",
    };
    session.send_text(&format!("{command}\r"));
    session.send_text(&format!("{probe}\r"));
    emit_state_changed(app, &label, &state);

    // Wait for the marker line to appear in the capture ring (the shell
    // runs the probe once the foreground command has finished).
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let mut timed_out = false;
    let mut exit_code = None;
    loop {
        if session.is_exited() {
            break;
        }
        let lines = session.scrollback_lines(400);
        if let Some(code) = find_rc_marker(&lines) {
            exit_code = Some(code);
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    // Let the tail (the next prompt) land before the final capture.
    std::thread::sleep(Duration::from_millis(300));

    let lines = session.scrollback_lines(600);
    // Only clean the probe out of the output when the probe actually ran;
    // on a timeout a stray user line that looks like the marker must stay.
    let (output, _) = if exit_code.is_some() {
        extract_rc(&lines.concat())
    } else {
        (lines.concat(), None)
    };
    Ok(json!({
        "session_id": session_id,
        "output": output,
        "exit_code": exit_code,
        "timed_out": timed_out,
    }))
}

/// Find the most recent `__MUSTER_RC=<number>` OUTPUT line in the ring.
/// Only full lines that START with the marker count — the echoed input line
/// (`> echo "__MUSTER_RC=..."`) starts with the command text instead, so
/// typing the probe doesn't trip the detection.
fn find_rc_marker(lines: &[String]) -> Option<i64> {
    for line in lines.iter().rev() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("__MUSTER_RC=") {
            if let Ok(code) = rest.trim().parse::<i64>() {
                return Some(code);
            }
        }
    }
    None
}

/// Pull the probe region out of `run`'s captured output: the shell's echo
/// of the probe command, the `__MUSTER_RC=<n>` marker, and the prompt that
/// follows. Returns the command's output plus its exit code.
///
/// The cut point is the LAST line before the marker that contains the probe
/// command text — PSReadLine-style prompts interleave the input echo with
/// history overlays, so `contains` is deliberately loose. Any line that
/// starts with `__MUSTER_RC=<number>` is treated as the marker (deliberately
/// colliding with that sentinel in command output is unsupported).
fn extract_rc(output: &str) -> (String, Option<i64>) {
    let lines: Vec<&str> = output.split('\n').collect();
    let marker_pos = lines.iter().rposition(|l| {
        l.trim()
            .strip_prefix("__MUSTER_RC=")
            .and_then(|r| r.trim().parse::<i64>().ok())
            .is_some()
    });
    let Some(mp) = marker_pos else {
        return (output.to_string(), None);
    };
    let rc = lines[mp]
        .trim()
        .strip_prefix("__MUSTER_RC=")
        .and_then(|r| r.trim().parse::<i64>().ok());
    // Cut at the probe's own input echo, which sits just before the marker
    // (with prompt/history chrome possibly sharing its line).
    let probe_echo = (0..mp).rev().find(|&i| {
        let l = lines[i].to_lowercase();
        l.contains("echo \"__muster_rc") || l.contains("echo __muster_rc")
    });
    let cut = probe_echo.unwrap_or(mp);
    let mut out = lines[..cut].join("\n");
    if output.ends_with('\n') {
        out.push('\n');
    }
    (out, rc)
}

// ---------------------------------------------------------------------------
// Client (CLI side)
// ---------------------------------------------------------------------------

pub mod client {
    use super::*;

    /// The verbs this CLI understands. Anything else falls through to the
    /// normal GUI launch (e.g. `muster .` opens a project).
    const VERBS: &[&str] = &["doctor", "ls", "agents", "new", "split", "send", "capture", "procs", "run"];

    /// Entry point from `main`: if argv looks like a CLI invocation, talk to
    /// the running app and exit; otherwise return None so the GUI boots.
    pub fn dispatch(argv: &[String]) -> Option<i32> {
        // Emit UTF-8 on Windows consoles (the default ANSI codepage would
        // transcode non-ASCII output to mojibake on a human-facing console;
        // agents piping our stdout get bytes either way).
        set_console_utf8();
        if argv.len() < 2 || !VERBS.contains(&argv[1].as_str()) {
            return None;
        }
        match run(&argv[1..]) {
            Ok(code) => Some(code),
            Err(e) => {
                eprintln!("muster: {e}");
                Some(1)
            }
        }
    }

    #[cfg(windows)]
    fn set_console_utf8() {
        use windows::Win32::System::Console::SetConsoleOutputCP;
        unsafe {
            // 65001 = CP_UTF8.
            let _ = SetConsoleOutputCP(65001);
        }
    }

    #[cfg(not(windows))]
    fn set_console_utf8() {}

    pub(crate) struct Parsed {
        pub(crate) json: bool,
        pub(crate) dir: Option<String>,
        pub(crate) enter: bool,
        pub(crate) lines: Option<usize>,
        pub(crate) timeout: Option<u64>,
        pub(crate) vertical: bool,
        pub(crate) positionals: Vec<String>,
    }

    /// Parse everything after the verb. `--` ends flag parsing so command
    /// text containing flag-shaped tokens works (`muster run -- npm --version`).
    pub(crate) fn parse_args(args: &[String]) -> Result<Parsed, String> {
        let mut out = Parsed {
            json: false,
            dir: None,
            enter: false,
            lines: None,
            timeout: None,
            vertical: false,
            positionals: Vec::new(),
        };
        let mut i = 0;
        let mut positional_only = false;
        while i < args.len() {
            let a = args[i].as_str();
            if positional_only {
                out.positionals.push(args[i].clone());
                i += 1;
                continue;
            }
            match a {
                "--" => positional_only = true,
                "--json" => out.json = true,
                "--enter" => out.enter = true,
                "--v" => out.vertical = true,
                "--h" => out.vertical = false,
                "--dir" | "-d" => {
                    i += 1;
                    out.dir = args.get(i).cloned();
                }
                "--lines" => {
                    i += 1;
                    out.lines = Some(args
                        .get(i)
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or("--lines needs a number")?);
                }
                "--timeout" => {
                    i += 1;
                    out.timeout = Some(args
                        .get(i)
                        .and_then(|s| s.parse::<u64>().ok())
                        .ok_or("--timeout needs a number")?);
                }
                other if other.starts_with("--") => {
                    return Err(format!("unknown flag '{other}' (use '--' before command text)"));
                }
                _ => out.positionals.push(args[i].clone()),
            }
            i += 1;
        }
        Ok(out)
    }

    fn run(argv: &[String]) -> Result<i32, String> {
        let verb = argv[0].as_str();
        let parsed = parse_args(&argv[1..])?;
        let info = ensure_info()?;

        let mut args = json!({});
        match verb {
            "new" => {
                let dir = parsed.dir.or_else(|| parsed.positionals.first().cloned());
                args = json!({ "directory": dir });
            }
            "split" => {
                args = json!({ "directory": parsed.dir, "vertical": parsed.vertical });
            }
            "send" => {
                let id = parsed
                    .positionals
                    .first()
                    .ok_or("send needs a session id")?;
                let text = parsed.positionals[1..].join(" ");
                args = json!({ "id": id, "text": text, "enter": parsed.enter });
            }
            "capture" | "procs" => {
                let id = parsed
                    .positionals
                    .first()
                    .ok_or_else(|| format!("{verb} needs a session id"))?;
                args = json!({ "id": id, "lines": parsed.lines });
            }
            "run" => {
                let command = parsed.positionals.join(" ");
                args = json!({
                    "command": command,
                    "directory": parsed.dir,
                    "timeout_secs": parsed.timeout,
                });
            }
            _ => {}
        }

        let data = request(&info, verb, args)?;
        let code = print_result(verb, &data, parsed.json);
        Ok(code)
    }

    fn ensure_info() -> Result<IpcInfo, String> {
        if let Some(info) = read_info() {
            if try_connect(&info).is_ok() {
                return Ok(info);
            }
        }
        // Muster isn't running (or is an old build without the bridge).
        // Start it and wait for the server to come up.
        let exe = std::env::current_exe().map_err(|e| format!("cannot locate muster binary: {e}"))?;
        log::debug!("ipc client: starting {exe:?}");
        if let Err(e) = std::process::Command::new(&exe).spawn() {
            return Err(format!("cannot start Muster: {e}"));
        }
        let deadline = Instant::now() + BOOT_WAIT;
        loop {
            if let Some(info) = read_info() {
                if try_connect(&info).is_ok() {
                    return Ok(info);
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        Err(
            "Muster is not running and could not be reached after starting it. \
             If Muster was already running, restart it to enable the command-line bridge."
                .into(),
        )
    }

    struct IpcInfo {
        port: u16,
        token: String,
    }

    fn read_info() -> Option<IpcInfo> {
        let text = std::fs::read_to_string(ipc_info_path()).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        Some(IpcInfo {
            port: v["port"].as_u64()? as u16,
            token: v["token"].as_str()?.to_string(),
        })
    }

    fn try_connect(info: &IpcInfo) -> Result<(), String> {
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", info.port)
            .parse::<std::net::SocketAddr>()
            .map_err(|e| e.to_string())?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500))
            .map_err(|e| e.to_string())?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let req = json!({ "id": 0, "token": info.token, "verb": "doctor", "args": {} });
        let mut s = req.to_string();
        s.push('\n');
        stream.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
        let mut line = String::new();
        let n = BufReader::new(&stream)
            .take(MAX_RESPONSE_BYTES as u64)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("empty response".into());
        }
        let resp: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if resp["ok"] == true {
            Ok(())
        } else {
            Err(resp["error"].as_str().unwrap_or("server error").to_string())
        }
    }

    fn request(info: &IpcInfo, verb: &str, args: Value) -> Result<Value, String> {
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", info.port)
            .parse::<std::net::SocketAddr>()
            .map_err(|e| e.to_string())?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
            .map_err(|e| format!("cannot reach Muster: {e}"))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
        let req = json!({ "id": 1, "token": info.token, "verb": verb, "args": args });
        let mut s = req.to_string();
        s.push('\n');
        stream.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
        let mut line = String::new();
        let n = BufReader::new(&stream)
            .take(MAX_RESPONSE_BYTES as u64)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("empty response".into());
        }
        let resp: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if resp["ok"] == true {
            Ok(resp["data"].clone())
        } else {
            Err(resp["error"].as_str().unwrap_or("server error").to_string())
        }
    }

    /// Prints the response; returns the process exit code to use (0 on
    /// success, the command's own exit code for plain `run` output).
    fn print_result(verb: &str, data: &Value, json: bool) -> i32 {
        if json {
            if let Ok(s) = serde_json::to_string_pretty(data) {
                println!("{s}");
            }
            return 0;
        }
        match verb {
            "new" | "split" => {
                if let Some(id) = data["session_id"].as_str() {
                    println!("{id}");
                }
                0
            }
            "run" => {
                // session id first (so the caller can follow up), then the
                // captured output; the exit code is our own exit status.
                if let Some(id) = data["session_id"].as_str() {
                    println!("{id}");
                }
                print!("{}", data["output"].as_str().unwrap_or(""));
                if data["timed_out"] == true {
                    eprintln!("muster: run timed out (partial output above)");
                }
                data["exit_code"].as_i64().map(|c| (c & 0xff) as i32).unwrap_or(0)
            }
            "send" => {
                println!("ok");
                0
            }
            "capture" => {
                print!("{}", data["text"].as_str().unwrap_or(""));
                0
            }
            "procs" => {
                if let Some(shell) = data["shell_pid"].as_u64() {
                    println!("shell pid: {shell}");
                }
                for p in data["procs"].as_array().unwrap_or(&vec![]) {
                    println!(
                        "{:>7}  {:<24} {:>6.1}%  {:>9}",
                        p["pid"].as_u64().unwrap_or(0),
                        p["name"].as_str().unwrap_or(""),
                        p["cpu"].as_f64().unwrap_or(0.0),
                        p["mem_bytes"].as_u64().unwrap_or(0)
                    );
                }
                for p in data["ports"].as_array().unwrap_or(&vec![]) {
                    println!(
                        "LISTEN {:>5}  pid {:>7}  {}",
                        p["port"].as_u64().unwrap_or(0),
                        p["pid"].as_u64().unwrap_or(0),
                        p["process_name"].as_str().unwrap_or("")
                    );
                }
                0
            }
            "ls" => {
                for w in data["windows"].as_array().unwrap_or(&vec![]) {
                    println!("window {}", w["label"].as_str().unwrap_or(""));
                    for p in w["projects"].as_array().unwrap_or(&vec![]) {
                        println!(
                            "  project {}  ({})",
                            p["name"].as_str().unwrap_or(""),
                            p["directory"].as_str().unwrap_or("")
                        );
                        for t in p["tabs"].as_array().unwrap_or(&vec![]) {
                            for pane in t["panes"].as_array().unwrap_or(&vec![]) {
                                println!(
                                    "    pane {} {}  {}",
                                    pane["id"].as_str().unwrap_or(""),
                                    pane["kind"].as_str().unwrap_or(""),
                                    pane["detail"].as_str().unwrap_or("")
                                );
                            }
                        }
                    }
                }
                0
            }
            "agents" => {
                for s in data["sessions"].as_array().unwrap_or(&vec![]) {
                    println!(
                        "{}  {:<12} {}",
                        s["id"].as_str().unwrap_or(""),
                        s["agent"].as_str().unwrap_or(""),
                        s["state"].as_str().unwrap_or("")
                    );
                }
                0
            }
            "doctor" => {
                println!("version: {}", data["version"].as_str().unwrap_or(""));
                println!("windows: {}", data["windows"].as_u64().unwrap_or(0));
                println!("sessions: {}", data["sessions"].as_u64().unwrap_or(0));
                println!("agents: {}", data["agents"].as_u64().unwrap_or(0));
                0
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_rc_parses_and_removes_marker() {
        let (out, rc) = extract_rc("hello\nworld\n__MUSTER_RC=0\n");
        assert_eq!(out, "hello\nworld\n");
        assert_eq!(rc, Some(0));
        let (out, rc) = extract_rc("fail\n__MUSTER_RC=1\n");
        assert_eq!(out, "fail\n");
        assert_eq!(rc, Some(1));
        // No marker: output untouched, exit code unknown.
        let (out, rc) = extract_rc("just text");
        assert_eq!(out, "just text");
        assert_eq!(rc, None);
    }

    #[test]
    fn extract_rc_only_touches_trailing_marker() {
        // A user's own line that happens to look like the marker mid-output
        // must survive.
        let (out, rc) = extract_rc("__MUSTER_RC=42\n__MUSTER_RC=7\n");
        assert_eq!(out, "__MUSTER_RC=42\n");
        assert_eq!(rc, Some(7));
    }

    #[test]
    fn extract_rc_drops_probe_echo_and_following_prompt() {
        // The marker may be followed by the shell's next prompt; everything
        // from the marker onward is Muster's own output.
        let (out, rc) = extract_rc("hello\n__MUSTER_RC=3\n❯ kero-windows\n");
        assert_eq!(out, "hello\n");
        assert_eq!(rc, Some(3));
        // The probe command's echo is dropped too.
        let (out, rc) = extract_rc("hello\n> echo \"__MUSTER_RC=$LASTEXITCODE\"\n__MUSTER_RC=3\n");
        assert_eq!(out, "hello\n");
        assert_eq!(rc, Some(3));
        let (out, rc) = extract_rc("hello\n> echo __MUSTER_RC=%errorlevel%\n__MUSTER_RC=4\n");
        assert_eq!(out, "hello\n");
        assert_eq!(rc, Some(4));
        // PSReadLine-style prompts share the input echo's line with history
        // chrome; `contains`-based cutting still finds it.
        let (out, rc) =
            extract_rc("hello\n[History]echo \"__MUSTER_RC=$LASTEXITCODE\"\n__MUSTER_RC=0\n");
        assert_eq!(out, "hello\n");
        assert_eq!(rc, Some(0));
        // A marker with no probe echo still counts as the sentinel.
        let (out, rc) = extract_rc("output\n__MUSTER_RC=99\n");
        assert_eq!(out, "output\n");
        assert_eq!(rc, Some(99));
    }

    #[test]
    fn find_rc_marker_ignores_input_echo_and_finds_output() {
        // The typed probe line starts with the command text, not the marker.
        let lines = vec![
            "> echo \"__MUSTER_RC=$LASTEXITCODE\"\n".to_string(),
            "__MUSTER_RC=0\n".to_string(),
        ];
        assert_eq!(find_rc_marker(&lines), Some(0));

        // The most recent marker wins; non-numeric content is skipped.
        let lines = vec![
            "__MUSTER_RC=nope\n".to_string(),
            "__MUSTER_RC=7\n".to_string(),
        ];
        assert_eq!(find_rc_marker(&lines), Some(7));

        assert_eq!(find_rc_marker(&["just text\n".to_string()]), None);
        assert_eq!(find_rc_marker(&[]), None);
    }
}

#[cfg(test)]
mod client_tests {
    use super::client::parse_args;

    #[test]
    fn parse_args_flags_positionals_and_separator() {
        let p = parse_args(&["--json".into(), "--dir".into(), "C:\\x".into(), "--enter".into(), "abc".into()])
            .unwrap();
        assert!(p.json && p.enter);
        assert_eq!(p.dir.as_deref(), Some("C:\\x"));
        assert_eq!(p.positionals, vec!["abc"]);

        // `--` ends flag parsing so command text keeps its own flags.
        let p = parse_args(&["--".into(), "npm".into(), "--version".into()]).unwrap();
        assert_eq!(p.positionals, vec!["npm", "--version"]);

        // Unknown flags are an error (they usually mean a missing `--`).
        assert!(parse_args(&["--bogus".into()]).is_err());
        assert!(parse_args(&["--lines".into()]).is_err());
        assert!(parse_args(&["--lines".into(), "x".into()]).is_err());
    }
}
