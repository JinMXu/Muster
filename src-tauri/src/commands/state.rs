//! State queries, settings, and theme commands.

use tauri::{AppHandle, Emitter, Manager, State, Window};
use uuid::Uuid;

use super::SharedState;
use crate::models::app::AppStateView;
use crate::models::session::SessionInfo;
use crate::services::config::Settings;

/// The window's initial state hydrate. Keeps `for_label` (get-or-create): a
/// window asking for its state is alive by definition, and `AppStateView`
/// has no error channel for the unknown-label case.
#[tauri::command]
pub fn get_state(window: Window, state: State<SharedState>) -> AppStateView {
    state.for_label(window.label()).lock().view()
}

#[tauri::command]
pub fn get_settings(state: State<SharedState>) -> Settings {
    state.settings.lock().clone()
}

/// Consume a one-shot UI action queued by the tray (e.g. "open-settings"
/// while the main window was being recreated). Called once by a window's
/// frontend right after mount; returns `None` when nothing is pending.
#[tauri::command]
pub fn take_pending_action(state: State<SharedState>) -> Option<String> {
    state.pending_action.lock().take()
}

/// Factory defaults for the Settings modal's "Reset" button — returned
/// without touching the persisted config (the modal's Save does that).
#[tauri::command]
pub fn default_settings() -> Settings {
    Settings::default()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<SharedState>, settings: Settings) -> Result<(), String> {
    let mut guard = state.settings.lock();
    *guard = settings;
    guard.save().map_err(|e| e.to_string())?;

    // Keep the tray menu in sync with the new language without a restart:
    // the tray exposes no menu read accessor, so rebuild it in place.
    let lang = crate::services::i18n::effective(&guard.language);
    if let Some(tray) = app.tray_by_id("main-tray") {
        use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
        if let (Ok(show_main), Ok(open_folder), Ok(open_settings), Ok(quit), Ok(separator)) = (
            MenuItem::with_id(&app, "tray-show-main", crate::services::i18n::translate("tray-show-main", &lang), true, None::<&str>),
            MenuItem::with_id(&app, "tray-open-folder", crate::services::i18n::translate("tray-open-folder", &lang), true, None::<&str>),
            MenuItem::with_id(&app, "tray-open-settings", crate::services::i18n::translate("tray-open-settings", &lang), true, None::<&str>),
            MenuItem::with_id(&app, "tray-quit", crate::services::i18n::translate("tray-quit", &lang), true, None::<&str>),
            PredefinedMenuItem::separator(&app),
        ) {
            if let Ok(menu) = Menu::with_items(&app, &[&show_main, &open_folder, &open_settings, &separator, &quit]) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    }
    // Retitle already-open diff tabs in the new language.
    for (_, s) in state.all() {
        let g = s.lock();
        for d in g.diffs.values() {
            d.set_lang(&lang);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn available_themes() -> Vec<String> { crate::theme::catalog::available() }

#[tauri::command]
pub fn available_themes_with_info() -> Vec<crate::theme::ThemeInfo> {
    crate::theme::catalog::available_with_info()
}

#[tauri::command]
pub fn theme_colors(name: String, dark: bool) -> crate::theme::ThemeColors {
    crate::theme::ThemeColors::resolve(&name, dark)
}

#[tauri::command]
pub fn session_info(window: Window, state: State<SharedState>, id: Uuid) -> Option<SessionInfo> {
    state.get_label(window.label()).and_then(|s| s.lock().session_info(id))
}

#[tauri::command]
pub fn list_all_sessions(window: Window, state: State<SharedState>) -> Vec<SessionInfo> {
    state
        .get_label(window.label())
        .map(|s| s.lock().list_all_sessions())
        .unwrap_or_default()
}

#[tauri::command]
pub fn file_info(window: Window, state: State<SharedState>, id: Uuid) -> Option<crate::models::file::FileTabInfo> {
    state.get_label(window.label()).and_then(|s| s.lock().file_info(id))
}

#[tauri::command]
pub fn diff_info(window: Window, state: State<SharedState>, id: Uuid) -> Option<crate::models::diff::DiffTabInfo> {
    state.get_label(window.label()).and_then(|s| s.lock().diff_info(id))
}

/// Jump the user to the pane hosting `session_id`, even when that pane lives
/// in another window. Selects its project + tab, focuses its pane, emits
/// `state-changed` to the owning window, and brings that window to the
/// foreground. Returns true when the session was found in any window; false
/// when it no longer exists anywhere (the caller drops the row).
#[tauri::command]
pub fn focus_agent_session(app: AppHandle, state: State<SharedState>, session_id: Uuid) -> bool {
    for (label, s) in state.all() {
        let mut g = s.lock();
        if g.focus_session(session_id) {
            let view = g.view();
            drop(g);
            let _ = app.emit_to(&label, "state-changed", view);
            if let Some(w) = app.get_webview_window(&label) {
                let _ = w.set_focus();
            }
            return true;
        }
    }
    false
}
