//! Project management commands.

use tauri::{Manager, State, Window};
use uuid::Uuid;

use super::{emit_state, SharedState};

/// Keeps `for_label` (get-or-create): the command returns a bare `Uuid` with
/// no error channel, and it is only ever invoked by a live window — which
/// the Destroyed cleanup path will own.
#[tauri::command]
pub fn new_project(window: Window, state: State<SharedState>, directory: Option<String>) -> Uuid {
    let s = state.for_label(window.label());
    let id = s.lock().new_project(directory);
    // new_project spawns a session model without a PTY — bring it up now,
    // same as bootstrap does for restored/starter sessions.
    crate::bootstrap::spawn_pending(window.app_handle(), window.label(), &s);
    emit_state(&window, &s.lock());
    id
}

#[tauri::command]
pub fn close_project(window: Window, state: State<SharedState>, id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().close_project(id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_project(window: Window, state: State<SharedState>, id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().select_project(id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_project_by_index(window: Window, state: State<SharedState>, idx: usize) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().select_project_by_index(idx);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_next_project(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().select_next_project();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_previous_project(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().select_previous_project();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn move_project(window: Window, state: State<SharedState>, from: Uuid, to: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().move_project(from, to);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn rename_project(window: Window, state: State<SharedState>, id: Uuid, name: Option<String>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().rename_project(id, name);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn set_project_directory(window: Window, state: State<SharedState>, id: Uuid, directory: Option<String>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().set_project_directory(id, directory);
    emit_state(&window, &s.lock());
}
