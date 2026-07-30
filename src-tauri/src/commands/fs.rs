//! Filesystem commands. Business logic (listing, validation, rename path
//! remapping) lives in `services::explorer`; handlers only resolve inputs,
//! call the service, and convert errors.

use tauri::{Manager, State, Window};

use super::{emit_state, unknown_window, SharedState};
use crate::services::explorer;

#[tauri::command]
pub fn list_directory(path: String) -> Vec<explorer::DirEntry> {
    explorer::list_directory(&path)
}

#[tauri::command]
pub fn trash_file(path: String) -> Result<(), String> {
    explorer::trash(&path)
}

#[tauri::command]
pub fn create_file(state: State<SharedState>, parent_dir: String, name: String, is_directory: bool) -> Result<String, String> {
    let lang = &state.settings.lock().language;
    explorer::create_entry(&parent_dir, &name, is_directory, lang)
}

#[tauri::command]
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
    emit_state(&window, &s.lock());
    Ok(new_path)
}

/// Replace the set of watched directories for the invoking window (file tree
/// auto-refresh). The frontend sends every directory it currently displays;
/// watch sets are per window label so windows don't clobber each other.
#[tauri::command]
pub fn watch_directories(window: Window, paths: Vec<String>) {
    crate::services::watch::set_watched_directories(window.app_handle(), window.label(), paths);
}

#[tauri::command]
pub fn install_explorer_context_menu() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    explorer::install_context_menu(&exe.to_string_lossy())
}

#[tauri::command]
pub fn add_to_path() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::services::cli::add_to_path(&exe.to_string_lossy())
}

#[tauri::command]
pub fn remove_from_path() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::services::cli::remove_from_path(&exe.to_string_lossy())
}

#[tauri::command]
pub fn is_on_path() -> Result<bool, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::services::cli::is_on_path(&exe.to_string_lossy())
}
