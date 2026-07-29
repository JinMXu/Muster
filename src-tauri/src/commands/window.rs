//! Window management commands.

use tauri::AppHandle;

/// Open a new window with its own independent projects/tabs/sessions. The
/// state is registered (and restored from the window's own snapshot, if any)
/// BEFORE the window is built, so the webview's first `get_state` invoke
/// already sees the restored layout. The actual work lives in
/// `bootstrap::spawn_window`, which the tray menu's "New Window" also calls.
#[tauri::command]
pub fn new_window(app: AppHandle) -> Result<(), String> {
    crate::bootstrap::spawn_window(&app)
}
