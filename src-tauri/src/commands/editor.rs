//! Editor file and diff tab commands.

use tauri::{Emitter, State, Window};
use uuid::Uuid;

use super::{emit_state, unknown_window, SharedState};

#[tauri::command]
pub async fn open_file(window: Window, state: State<'_, SharedState>, path: String, to_side: bool) -> Result<Option<Uuid>, String> {
    // A late invoke from an already-destroyed window resolves to null, as
    // before (async commands borrowing `State` must return a Result).
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    // The disk read (up to 5 MiB) happens inside `open_file`; run it on the
    // blocking pool so it doesn't stall the async runtime / main thread.
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || s2.lock().open_file(&path, to_side))
        .await
        .ok()
        .flatten();
    emit_state(&window, &s.lock());
    Ok(id)
}

/// Open a file at a specific line (terminal error-path click, quick-open,
/// search results). Refuses to open a path that doesn't exist on disk, and
/// emits `file-reveal` so the hosting editor scrolls to the line. Returns the
/// file tab id (or `None` when the path is missing / no project selected).
#[tauri::command]
pub async fn open_file_at(window: Window, state: State<'_, SharedState>, path: String, line: u32) -> Result<Option<Uuid>, String> {
    // The existence stat and the disk read both run on the blocking pool.
    let stat_path = path.clone();
    let is_file = tokio::task::spawn_blocking(move || std::path::Path::new(&stat_path).is_file())
        .await
        .unwrap_or(false);
    if !is_file {
        return Ok(None);
    }
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || s2.lock().open_file(&path, false))
        .await
        .ok()
        .flatten();
    emit_state(&window, &s.lock());
    if let Some(id) = id {
        let _ = window.emit("file-reveal", serde_json::json!({ "id": id, "line": line }));
    }
    Ok(id)
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
pub async fn open_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String, staged: bool) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    // `open_diff` loads both sides via git2 synchronously; run it on the
    // blocking pool so repo I/O doesn't stall the async runtime.
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || s2.lock().open_diff(&repo_root, &path, staged))
        .await
        .ok()
        .flatten();
    emit_state(&window, &s.lock());
    Ok(id)
}

#[tauri::command]
pub async fn open_commit_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String, old_rev: String, new_rev: String) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    // git2 diff loading happens inside `open_commit_diff`; blocking pool, as
    // with `open_diff`.
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || s2.lock().open_commit_diff(&repo_root, &path, &old_rev, &new_rev))
        .await
        .ok()
        .flatten();
    emit_state(&window, &s.lock());
    Ok(id)
}

#[tauri::command]
pub async fn reload_diff(window: Window, state: State<'_, SharedState>, id: Uuid) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(()) };
    // Only hold the lock to clone the Arc; the git2 reload (blocking repo
    // I/O) runs on the blocking pool without the AppState lock held.
    let diff = {
        let g = s.lock();
        g.diffs.get(&id).cloned()
    };
    if let Some(d) = diff {
        let _ = tokio::task::spawn_blocking(move || d.reload()).await;
    }
    Ok(())
}

/// Diff of `path` against its HEAD version (new side = the live worktree).
#[tauri::command]
pub async fn open_workdir_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || s2.lock().open_workdir_diff(&repo_root, &path))
        .await
        .ok()
        .flatten();
    emit_state(&window, &s.lock());
    Ok(id)
}

/// Diff of `path` between `old_rev` (a checkpoint oid) and the live worktree.
#[tauri::command]
pub async fn open_checkpoint_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String, old_rev: String) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || s2.lock().open_checkpoint_diff(&repo_root, &path, &old_rev))
        .await
        .ok()
        .flatten();
    emit_state(&window, &s.lock());
    Ok(id)
}
