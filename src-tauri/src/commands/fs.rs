//! Filesystem commands. Business logic (listing, validation, rename path
//! remapping) lives in `services::explorer`; handlers only resolve inputs,
//! call the service, and convert errors.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tauri::{Manager, State, Window};

use super::{emit_state, unknown_window, SharedState};
use crate::services::explorer;

/// Latest search job per window. Starting a new search flips the previous
/// job's flag so a stale query scan stops instead of running to completion
/// on the blocking pool.
static SEARCH_JOBS: Lazy<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn begin_search(label: &str) -> Arc<AtomicBool> {
    let mut jobs = SEARCH_JOBS.lock();
    if let Some(previous) = jobs.get(label) {
        previous.store(true, Ordering::Relaxed);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    jobs.insert(label.to_string(), cancel.clone());
    cancel
}

/// Runs on the blocking pool like `search_files`: a huge or slow (e.g.
/// network) directory listing must not stall Tauri's main thread, where
/// non-async commands execute.
#[tauri::command]
#[specta::specta]
pub async fn list_directory(path: String) -> Vec<explorer::DirEntry> {
    tokio::task::spawn_blocking(move || explorer::list_directory(&path))
        .await
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn trash_file(state: State<SharedState>, path: String) -> Result<(), String> {
    let lang = state.settings.lock().language.clone();
    explorer::trash(&path, &lang)
}

#[tauri::command]
#[specta::specta]
pub fn create_file(state: State<SharedState>, parent_dir: String, name: String, is_directory: bool) -> Result<String, String> {
    let lang = &state.settings.lock().language;
    explorer::create_entry(&parent_dir, &name, is_directory, lang)
}

#[tauri::command]
#[specta::specta]
pub fn rename_path(window: Window, state: State<SharedState>, from: String, to: String) -> Result<String, String> {
    let lang = state.settings.lock().language.clone();
    let new_path = explorer::rename(&from, &to, &lang)?;
    // Open editor tabs follow the rename: a tab on the renamed path, or on
    // anything under a renamed directory, is updated to the new path.
    let Some(s) = state.get_label(window.label()) else { return Err(unknown_window(window.label())); };
    {
        let g = s.lock();
        for f in g.files.values() {
            if let Some(p) = explorer::remap_renamed_path(&f.path(), &from, &new_path) {
                f.update_path(&p);
            }
        }
    }
    emit_state(&window, &s);
    Ok(new_path)
}

/// Replace the set of watched directories for the invoking window (file tree
/// auto-refresh). The frontend sends every directory it currently displays;
/// watch sets are per window label so windows don't clobber each other.
#[tauri::command]
#[specta::specta]
pub fn watch_directories(window: Window, paths: Vec<String>) {
    crate::services::watch::set_watched_directories(window.app_handle(), window.label(), paths);
}

/// Full-text search under `root` for the Search panel. Runs on the blocking
/// pool so a big repository can't starve Tauri's async runtime. A newer
/// search from the same window cancels the previous one.
#[tauri::command]
#[specta::specta]
pub async fn search_files(
    window: Window,
    root: String,
    query: String,
    case_sensitive: bool,
) -> Result<Vec<crate::services::search::SearchMatch>, String> {
    let cancel = begin_search(window.label());
    tokio::task::spawn_blocking(move || {
        crate::services::search::search_in_project_cancellable(&root, &query, case_sensitive, &cancel)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List every file under `root` (relative paths, gitignore-aware) for the
/// Ctrl+P quick-open list. Runs on the blocking pool like `search_files`.
#[tauri::command]
#[specta::specta]
pub async fn list_project_files(root: String) -> Vec<String> {
    tokio::task::spawn_blocking(move || crate::services::search::list_project_files(&root))
        .await
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn install_explorer_context_menu(state: State<SharedState>) -> Result<(), String> {
    let lang = state.settings.lock().language.clone();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    explorer::install_context_menu(&exe.to_string_lossy(), &lang)
}

#[tauri::command]
#[specta::specta]
pub fn add_to_path(state: State<SharedState>) -> Result<(), String> {
    let lang = state.settings.lock().language.clone();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::services::cli::add_to_path(&exe.to_string_lossy(), &lang)
}

#[tauri::command]
#[specta::specta]
pub fn remove_from_path(state: State<SharedState>) -> Result<(), String> {
    let lang = state.settings.lock().language.clone();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::services::cli::remove_from_path(&exe.to_string_lossy(), &lang)
}

#[tauri::command]
#[specta::specta]
pub fn is_on_path(state: State<SharedState>) -> Result<bool, String> {
    let lang = state.settings.lock().language.clone();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::services::cli::is_on_path(&exe.to_string_lossy(), &lang)
}
