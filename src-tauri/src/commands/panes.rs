//! Split / pane layout commands, plus sidebar and panel toggles.

use tauri::{Manager, State, Window};
use uuid::Uuid;

use super::{emit_state, unknown_window, SharedState};
use crate::models::pane::{FocusDirection, PaneContent, PaneDropEdge, ResizeDirection};
use crate::models::project::RightPanel;

#[tauri::command]
pub fn split(window: Window, state: State<SharedState>, edge: PaneDropEdge) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Err(unknown_window(window.label())); };
    {
        let mut g = s.lock();
        g.split(edge);
    }
    // Spawn the PTY for the new session created by `split`. The session id
    // is the focused pane's session (the one we just inserted).
    let new_session_opt = {
        let g = s.lock();
        g.selected_tab()
            .and_then(|t| t.focused_pane())
            .and_then(|p| match &p.content {
                PaneContent::Session(id) => g.sessions.get(id).cloned(),
                _ => None,
            })
    };
    if let Some(session) = new_session_opt {
        session.spawn(80, 24).map_err(|e| e.to_string())?;
        session.attach_read_loop(window.app_handle().clone(), window.label().to_string());
    }
    emit_state(&window, &s.lock());
    Ok(())
}

#[tauri::command]
pub fn focus_pane(window: Window, state: State<SharedState>, direction: FocusDirection) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().focus_pane(direction);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn resize_pane(window: Window, state: State<SharedState>, direction: ResizeDirection) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().resize_pane(direction);
    emit_state(&window, &s.lock());
}

/// Drag-resize one divider: `vertical` = between columns `index`/`index + 1`,
/// otherwise between panes `index`/`index + 1` of `columns[column_index]`.
/// Skips `emit_state` to avoid full-tree serialization on every mousemove;
/// the frontend updates pane weights locally during drag.
#[tauri::command]
pub fn resize_pane_divider(
    window: Window,
    state: State<SharedState>,
    tab_id: Uuid,
    vertical: bool,
    column_index: usize,
    index: usize,
    delta: f32,
) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().resize_pane_divider(tab_id, vertical, column_index, index, delta);
}

/// Drag & drop rearrange: move `pane_id` to `edge` of `target_pane_id`
/// within tab `tab_id` (intra-tab only; see PaneTab::move_pane).
#[tauri::command]
pub fn move_pane(
    window: Window,
    state: State<SharedState>,
    tab_id: Uuid,
    pane_id: Uuid,
    target_pane_id: Uuid,
    edge: PaneDropEdge,
) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().move_pane(tab_id, pane_id, target_pane_id, edge);
    emit_state(&window, &s.lock());
}

/// Drag & drop a pane onto another tab (dropped on the tab's strip header):
/// detach `pane_id` from `source_tab_id` and add it as a new columnise in
/// `target_tab_id`. Refused when the source tab would be emptied or the
/// pane is the only pane in its tab.
#[tauri::command]
pub fn move_pane_cross_tab(
    window: Window,
    state: State<SharedState>,
    source_tab_id: Uuid,
    pane_id: Uuid,
    target_tab_id: Uuid,
) -> bool {
    let Some(s) = state.get_label(window.label()) else { return false };
    let moved = s.lock().move_pane_cross_tab(source_tab_id, pane_id, target_tab_id);
    if moved {
        emit_state(&window, &s.lock());
    }
    moved
}

#[tauri::command]
pub fn toggle_pane_zoom(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().toggle_pane_zoom();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn equalize_panes(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().equalize_panes();
    emit_state(&window, &s.lock());
}

// --- Sidebar / panel -------------------------------------------------------

#[tauri::command]
pub fn toggle_left_sidebar(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().toggle_left_sidebar();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn toggle_right_panel(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().toggle_right_panel();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn toggle_panel(window: Window, state: State<SharedState>, panel: RightPanel) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().toggle_panel(panel);
    emit_state(&window, &s.lock());
}
