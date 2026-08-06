//! Session / terminal commands, plus process & port queries for the Info
//! panel.

use tauri::{Manager, State, Window};
use uuid::Uuid;

use super::{emit_state, unknown_window, SharedState};
use crate::models::session::SessionInfo;
use crate::services::i18n::translate;

#[tauri::command]
pub async fn spawn_session(
    window: Window,
    state: State<'_, SharedState>,
    directory: Option<String>,
) -> Result<SessionInfo, String> {
    let Some(s) = state.get_label(window.label()) else { return Err(unknown_window(window.label())); };
    let lang = state.settings.lock().language.clone();
    // ConPty creation + CreateProcessW + shell detection (PATH scan,
    // registry read, integration-script write) can take 100ms–1s, so the
    // whole spawn runs on the blocking pool instead of the UI thread.
    let app = window.app_handle().clone();
    let label = window.label().to_string();
    let s2 = s.clone();
    let info = tokio::task::spawn_blocking(move || {
        let session_id = s2.lock().spawn_session_in_selected(directory);
        let session_id = session_id.ok_or_else(|| translate("no-selected-project", &lang).to_string())?;
        let session_arc = s2.lock().sessions.get(&session_id).cloned();
        let Some(session) = session_arc else { return Err(translate("session-not-registered", &lang).to_string()); };
        session.spawn(80, 24).map_err(|e| e.to_string())?;
        session.attach_read_loop(app, label);
        Ok(SessionInfo::from(session.as_ref()))
    })
    .await
    .map_err(|e| e.to_string())??;
    emit_state(&window, &s.lock());
    Ok(info)
}

#[tauri::command]
pub fn send_text(window: Window, state: State<SharedState>, id: Uuid, text: String) {
    let Some(s) = state.get_label(window.label()) else { return };
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) { session.send_text(&text); }
}

#[tauri::command]
pub async fn resize_terminal(window: Window, state: State<'_, SharedState>, id: Uuid, cols: u16, rows: u16) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(()) };
    // `resize` writes the ConPTY signal pipe with a blocking WriteFile —
    // keep it off the main thread (only the lock-free Arc clone happens
    // under the AppState lock).
    let session = {
        let g = s.lock();
        g.sessions.get(&id).cloned()
    };
    if let Some(session) = session {
        let _ = tokio::task::spawn_blocking(move || session.resize(cols, rows)).await;
    }
    Ok(())
}

/// Clear the focused terminal: ask the shell to clear its screen (`clear` /
/// `cls`). The frontend additionally wipes the xterm scrollback locally.
#[tauri::command]
pub fn clear_terminal(window: Window, state: State<SharedState>, id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) {
        let cmd = if session.shell_name == "cmd" { "cls\r" } else { "clear\r" };
        session.send_text(cmd);
    }
}

#[tauri::command]
pub async fn terminate_session(window: Window, state: State<'_, SharedState>, id: Uuid) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(()) };
    let session = {
        let g = s.lock();
        g.sessions.get(&id).cloned()
    };
    if let Some(session) = session {
        let _ = tokio::task::spawn_blocking(move || session.terminate()).await;
    }
    Ok(())
}

// --- Processes / ports (Info panel) ------------------------------------------

/// PROCESSES section: every process of the session (Job Object members, or
/// the shell's ppid descendants when untracked), shell row first. Runs on
/// the blocking pool: the full process enumeration is too heavy for the
/// async runtime.
#[tauri::command]
pub async fn session_processes(session_id: Uuid, shell_pid: u32) -> Vec<crate::services::procs::ProcessInfo> {
    tokio::task::spawn_blocking(move || {
        let pids = crate::services::procs::session_pids(session_id, shell_pid);
        crate::services::procs::process_infos(&pids)
    })
    .await
    .unwrap_or_default()
}

/// Force-kill a process (Windows has only TerminateProcess semantics, hence a
/// single kill command - see services::procs::kill). The PID is validated
/// against the session's tracked process set to prevent killing arbitrary
/// system processes. Runs on the blocking pool: the validation does a full
/// process enumeration.
#[tauri::command]
pub async fn kill_process(session_id: Uuid, shell_pid: u32, pid: u32) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let valid_pids = crate::services::procs::session_pids(session_id, shell_pid);
        if !valid_pids.contains(&pid) {
            return Err("PID does not belong to this session".to_string());
        }
        crate::services::procs::kill(pid)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// PORTS section: listening TCP ports owned by the given pids (the session's
/// processes). When `project_root` is Some (the "project ports" setting),
/// listeners from any process working in that directory are included too, so
/// dev servers started outside this session still show. The directory
/// matching is skipped for broad roots (home dir, drive root). Runs on the
/// blocking pool: port lookup shells out to netstat.
#[tauri::command]
pub async fn session_ports(
    pids: Vec<u32>,
    project_root: Option<String>,
) -> Vec<crate::services::procs::ListenPort> {
    tokio::task::spawn_blocking(move || crate::services::procs::listen_ports(&pids, project_root.as_deref()))
        .await
        .unwrap_or_default()
}

/// Called by the frontend after its pty:data / pty:exit listeners are
/// registered, so that restored sessions' read pumps start after the
/// listeners are in place (closes the race between PTY output and listener
/// setup during application startup).
#[tauri::command]
pub fn init_read_loops(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    crate::bootstrap::start_read_loops(window.app_handle(), window.label(), &s);
}
