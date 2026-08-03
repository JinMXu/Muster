//! Tab management commands.

use tauri::{Emitter, State, Window};
use uuid::Uuid;

use super::{emit_state, unknown_window, SharedState};

#[tauri::command]
pub fn close_selected_tab(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().close_selected_tab();
    emit_state(&window, &s.lock());
}

/// Close one tab by id in a single atomic call. Closing must not go through
/// `select_tab` first: a selection change between select and close would
/// close the wrong tab. The current selection is only reassigned when the
/// closed tab was the selected one (see AppState::remove_tab).
#[tauri::command]
pub fn close_tab(window: Window, state: State<SharedState>, tab_id: Uuid) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Err(unknown_window(window.label())); };
    s.lock().close_tab(tab_id);
    emit_state(&window, &s.lock());
    Ok(())
}

/// Switch active tab. Emits a narrow `tab-focused` event (just the tab UUID)
/// instead of a full `state-changed` broadcast - tab switching is the most
/// frequent state mutation and the frontend already has all tab data in its
/// local state; it only needs to know which tab is now selected.
#[tauri::command]
pub fn select_tab(window: Window, state: State<SharedState>, id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    let mut g = s.lock();
    g.select_tab(id);
    // Only emit when the tab was actually selected (select_tab checks
    // existence inside the selected project).
    let selected = g.selected_project().and_then(|p| p.selected_tab_id);
    drop(g);
    if selected == Some(id) {
        let _ = window.emit("tab-focused", id);
    }
}

#[tauri::command]
pub fn select_next_tab(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    let mut g = s.lock();
    g.select_next_tab();
    let selected = g.selected_project().and_then(|p| p.selected_tab_id);
    drop(g);
    if let Some(id) = selected {
        let _ = window.emit("tab-focused", id);
    }
}

#[tauri::command]
pub fn select_previous_tab(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    let mut g = s.lock();
    g.select_previous_tab();
    let selected = g.selected_project().and_then(|p| p.selected_tab_id);
    drop(g);
    if let Some(id) = selected {
        let _ = window.emit("tab-focused", id);
    }
}

#[tauri::command]
pub fn move_tab(window: Window, state: State<SharedState>, from: Uuid, to: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().move_tab(from, to);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn rename_tab(window: Window, state: State<SharedState>, id: Uuid, name: Option<String>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().rename_tab(id, name);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn close_other_tabs(window: Window, state: State<SharedState>, tab_id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().close_other_tabs(tab_id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn close_tabs_to_right(window: Window, state: State<SharedState>, tab_id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().close_tabs_to_right(tab_id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn close_all_tabs(window: Window, state: State<SharedState>) {
    let Some(s) = state.get_label(window.label()) else { return };
    s.lock().close_all_tabs();
    emit_state(&window, &s.lock());
}

/// Path behind a pane for the tab context menu's Reveal/Copy Path items.
#[tauri::command]
pub fn pane_context_path(window: Window, state: State<SharedState>, tab_id: Uuid, pane_id: Uuid) -> Option<String> {
    state
        .get_label(window.label())
        .and_then(|s| s.lock().pane_context_path(tab_id, pane_id))
}
