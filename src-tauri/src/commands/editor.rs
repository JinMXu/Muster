//! Editor file and diff tab commands.

use tauri::{Emitter, State, Window};
use uuid::Uuid;

use super::{emit_state, unknown_window, SharedState};

#[tauri::command]
#[specta::specta]
pub async fn open_file(window: Window, state: State<'_, SharedState>, path: String, to_side: bool) -> Result<Option<Uuid>, String> {
    // A late invoke from an already-destroyed window resolves to null, as
    // before (async commands borrowing `State` must return a Result).
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    // Construct the tab on the blocking pool FIRST (its disk read is up to
    // 5 MiB), then take the AppState lock only to insert it — the lock held
    // across the read stalled every sync command of the window (`send_text`
    // per keystroke included) for the read's duration.
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || {
        let file = std::sync::Arc::new(crate::models::file::FileTab::open(&path));
        s2.lock().open_file(file, to_side)
    })
    .await
    .ok()
    .flatten();
    emit_state(&window, &s);
    Ok(id)
}

/// Open a file at a specific line (terminal error-path click, quick-open,
/// search results). Refuses to open a path that doesn't exist on disk, and
/// emits `file-reveal` so the hosting editor scrolls to the line. Returns the
/// file tab id (or `None` when the path is missing / no project selected).
#[tauri::command]
#[specta::specta]
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
    // Construct on the pool, insert under a brief lock (see `open_file`).
    let id = tokio::task::spawn_blocking(move || {
        let file = std::sync::Arc::new(crate::models::file::FileTab::open(&path));
        s2.lock().open_file(file, false)
    })
    .await
    .ok()
    .flatten();
    emit_state(&window, &s);
    if let Some(id) = id {
        let _ = window.emit("file-reveal", serde_json::json!({ "id": id, "line": line }));
    }
    Ok(id)
}

#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
pub fn tab_dirty_files(window: Window, state: State<SharedState>, tab_id: Uuid) -> Vec<crate::models::app::DirtyFileInfo> {
    state
        .get_label(window.label())
        .map(|s| s.lock().tab_dirty_files(tab_id))
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn project_dirty_files(window: Window, state: State<SharedState>, project_id: Uuid) -> Vec<crate::models::app::DirtyFileInfo> {
    state
        .get_label(window.label())
        .map(|s| s.lock().project_dirty_files(project_id))
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub async fn open_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String, staged: bool) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    // The git2 reload runs in DiffTab::new on the blocking pool; the AppState
    // lock is taken only for the insert (see `open_file`).
    let lang = crate::services::i18n::effective(&state.settings.lock().language);
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || {
        let diff = std::sync::Arc::new(crate::models::diff::DiffTab::new(repo_root, path, staged, &lang));
        s2.lock().open_diff(diff)
    })
    .await
    .ok()
    .flatten();
    emit_state(&window, &s);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn open_commit_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String, old_rev: String, new_rev: String) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    let lang = crate::services::i18n::effective(&state.settings.lock().language);
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || {
        let diff = std::sync::Arc::new(crate::models::diff::DiffTab::with_revs(repo_root, path, old_rev, new_rev, &lang));
        s2.lock().open_commit_diff(diff)
    })
    .await
    .ok()
    .flatten();
    emit_state(&window, &s);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn reload_diff(window: Window, state: State<'_, SharedState>, id: Uuid) -> Result<(), String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(()) };
    // Only hold the lock to clone the Arc; the git2 reload (blocking repo
    // I/O) runs on the blocking pool without the AppState lock held.
    let diff = {
        let g = s.lock();
        g.diffs.get(&id).cloned()
    };
    if let Some(d) = diff {
        let lang = crate::services::i18n::effective(&state.settings.lock().language);
        d.set_lang(&lang);
        let _ = tokio::task::spawn_blocking(move || d.reload()).await;
    }
    Ok(())
}

/// Diff of `path` against its HEAD version (new side = the live worktree).
#[tauri::command]
#[specta::specta]
pub async fn open_workdir_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    let lang = crate::services::i18n::effective(&state.settings.lock().language);
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || {
        let diff = std::sync::Arc::new(crate::models::diff::DiffTab::new_workdir(repo_root, path, &lang));
        s2.lock().open_workdir_diff(diff)
    })
    .await
    .ok()
    .flatten();
    emit_state(&window, &s);
    Ok(id)
}

/// Diff of `path` between `old_rev` (a checkpoint oid) and the live worktree.
#[tauri::command]
#[specta::specta]
pub async fn open_checkpoint_diff(window: Window, state: State<'_, SharedState>, repo_root: String, path: String, old_rev: String) -> Result<Option<Uuid>, String> {
    let Some(s) = state.get_label(window.label()) else { return Ok(None) };
    let lang = crate::services::i18n::effective(&state.settings.lock().language);
    let s2 = s.clone();
    let id = tokio::task::spawn_blocking(move || {
        let diff = std::sync::Arc::new(crate::models::diff::DiffTab::new_checkpoint(repo_root, path, old_rev, &lang));
        s2.lock().open_checkpoint_diff(diff)
    })
    .await
    .ok()
    .flatten();
    emit_state(&window, &s);
    Ok(id)
}
