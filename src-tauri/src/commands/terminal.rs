//! Session / terminal commands, plus process & port queries for the Info
//! panel.

use tauri::{Manager, State, Window};
use uuid::Uuid;

use super::{emit_state, unknown_window, SharedState};
use crate::models::session::SessionInfo;
use crate::services::i18n::translate;

#[tauri::command]
pub fn spawn_session(
    window: Window,
    state: State<SharedState>,
    directory: Option<String>,
) -> Result<SessionInfo, String> {
    let Some(s) = state.get_label(window.label()) else { return Err(unknown_window(window.label())); };
    let lang = &state.settings.lock().language;
    let session_id = s.lock().spawn_session_in_selected(directory);
    let session_id = session_id.ok_or_else(|| translate("no-selected-project", lang).to_string())?;
    let session_arc = s.lock().sessions.get(&session_id).cloned();
    let Some(session) = session_arc else { return Err(translate("session-not-registered", lang).to_string()); };
    session.spawn(80, 24).map_err(|e| e.to_string())?;
    session.attach_read_loop(window.app_handle().clone(), window.label().to_string());
    let info = SessionInfo::from(session.as_ref());
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
pub fn resize_terminal(window: Window, state: State<SharedState>, id: Uuid, cols: u16, rows: u16) {
    let Some(s) = state.get_label(window.label()) else { return };
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) { session.resize(cols, rows); }
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
pub fn terminate_session(window: Window, state: State<SharedState>, id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) { session.terminate(); }
}

// --- Processes / ports (Info panel) ------------------------------------------

/// PROCESSES section: every process of the session (Job Object members, or
/// the shell's ppid descendants when untracked), shell row first.
#[tauri::command]
pub fn session_processes(session_id: Uuid, shell_pid: u32) -> Vec<crate::services::procs::ProcessInfo> {
    let pids = crate::services::procs::session_pids(session_id, shell_pid);
    crate::services::procs::process_infos(&pids)
}

/// Force-kill a process (Windows has only TerminateProcess semantics, hence a
/// single kill command — see services::procs::kill).
#[tauri::command]
pub fn kill_process(pid: u32) -> Result<(), String> {
    crate::services::procs::kill(pid)
}

/// PORTS section: listening TCP ports owned by the given pids, plus any
/// process belonging to the project directory (working directory or command
/// line under it), so dev servers started outside this session still show.
/// The directory matching is skipped for broad roots (home dir, drive root).
#[tauri::command]
pub fn session_ports(
    pids: Vec<u32>,
    project_root: Option<String>,
) -> Vec<crate::services::procs::ListenPort> {
    crate::services::procs::listen_ports(&pids, project_root.as_deref())
}
