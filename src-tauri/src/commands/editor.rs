//! Editor file and diff tab commands.

use tauri::{Emitter, State, Window};
use uuid::Uuid;

use super::{emit_state, unknown_window, SharedState};

#[tauri::command]
pub fn open_file(window: Window, state: State<SharedState>, path: String, to_side: bool) -> Option<Uuid> {
    let s = state.get_label(window.label())?;
    let id = s.lock().open_file(&path, to_side);
    emit_state(&window, &s.lock());
    id
}

/// Open a file at a specific line (terminal error-path click, quick-open,
/// search results). Refuses to open a path that doesn't exist on disk, and
/// emits `file-reveal` so the hosting editor scrolls to the line. Returns the
/// file tab id (or `None` when the path is missing / no project selected).
#[tauri::command]
pub fn open_file_at(window: Window, state: State<SharedState>, path: String, line: u32) -> Option<Uuid> {
    if !std::path::Path::new(&path).is_file() {
        return None;
    }
    let s = state.get_label(window.label())?;
    let id = s.lock().open_file(&path, false);
    emit_state(&window, &s.lock());
    if let Some(id) = id {
        let _ = window.emit("file-reveal", serde_json::json!({ "id": id, "line": line }));
    }
    id
}

#[tauri::command]
pub fn file_text_changed(window: Window, state: State<SharedState>, id: Uuid, text: String) {
    let Some(s) = state.get_label(window.label()) else { return };
    // Only hold the lock to clone the Arc; release it before set_text,
    // which clones & compares strings that can be megabytes large.
    let file = {
        let g = s.lock();
        g.files.get(&id).cloned()
    };
    if let Some(f) = file {
        f.set_text(text);
    }
}

#[tauri::command]
pub fn save_selected_file(window: Window, state: State<SharedState>) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Err(unknown_window(window.label())); };
    let (file, file_id) = {
        let g = s.lock();
        let Some(tab) = g.selected_tab() else { return Ok(()) };
        let Some(pane) = tab.focused_pane() else { return Ok(()) };
        use crate::models::pane::PaneContent;
        if let PaneContent::File(id) = &pane.content {
            if let Some(f) = g.files.get(id) {
                (Some(f.clone()), *id)
            } else { (None, Uuid::default()) }
        } else { (None, Uuid::default()) }
    };
    if let Some(f) = file {
        f.save()?;
        let _ = window.emit("file-saved", serde_json::json!({ "id": file_id }));
    }
    Ok(())
}

#[tauri::command]
pub fn save_file(window: Window, state: State<SharedState>, id: Uuid, text: Option<String>) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Err(unknown_window(window.label())); };
    let file = {
        let g = s.lock();
        g.files.get(&id).cloned()
    };
    if let Some(f) = file {
        if let Some(text) = text {
            f.set_text(text);
        }
        f.save()?;
        let _ = window.emit("file-saved", serde_json::json!({ "id": id }));
    }
    Ok(())
}

#[tauri::command]
pub fn tab_dirty_files(window: Window, state: State<SharedState>, tab_id: Uuid) -> Vec<crate::models::app::DirtyFileInfo> {
    state
        .get_label(window.label())
        .map(|s| s.lock().tab_dirty_files(tab_id))
        .unwrap_or_default()
}

#[tauri::command]
pub fn project_dirty_files(window: Window, state: State<SharedState>, project_id: Uuid) -> Vec<crate::models::app::DirtyFileInfo> {
    state
        .get_label(window.label())
        .map(|s| s.lock().project_dirty_files(project_id))
        .unwrap_or_default()
}

#[tauri::command]
pub fn open_diff(window: Window, state: State<SharedState>, repo_root: String, path: String, staged: bool) -> Option<Uuid> {
    let s = state.get_label(window.label())?;
    let id = s.lock().open_diff(&repo_root, &path, staged);
    emit_state(&window, &s.lock());
    id
}

#[tauri::command]
pub fn open_commit_diff(window: Window, state: State<SharedState>, repo_root: String, path: String, old_rev: String, new_rev: String) -> Option<Uuid> {
    let s = state.get_label(window.label())?;
    let id = s.lock().open_commit_diff(&repo_root, &path, &old_rev, &new_rev);
    emit_state(&window, &s.lock());
    id
}

#[tauri::command]
pub fn reload_diff(window: Window, state: State<SharedState>, id: Uuid) {
    let Some(s) = state.get_label(window.label()) else { return };
    let g = s.lock();
    if let Some(d) = g.diffs.get(&id) { d.reload(); }
}

/// Diff of `path` against its HEAD version (new side = the live worktree).
#[tauri::command]
pub fn open_workdir_diff(window: Window, state: State<SharedState>, repo_root: String, path: String) -> Option<Uuid> {
    let s = state.get_label(window.label())?;
    let id = s.lock().open_workdir_diff(&repo_root, &path);
    emit_state(&window, &s.lock());
    id
}

/// Diff of `path` between `old_rev` (a checkpoint oid) and the live worktree.
#[tauri::command]
pub fn open_checkpoint_diff(window: Window, state: State<SharedState>, repo_root: String, path: String, old_rev: String) -> Option<Uuid> {
    let s = state.get_label(window.label())?;
    let id = s.lock().open_checkpoint_diff(&repo_root, &path, &old_rev);
    emit_state(&window, &s.lock());
    id
}
