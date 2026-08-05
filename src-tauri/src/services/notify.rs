//! System toast notifications with click-to-jump support.
//!
//! tauri-plugin-notification (2.x) shows toasts via notify-rust but drops
//! the activation handle, so it offers no click callback on Windows. We use
//! tauri-winrt-notification (already in the dependency tree via notify-rust)
//! directly and register an in-process `on_activated` handler: clicking the
//! toast shows + focuses the owning window and emits `notification-activate`
//! with the session id; the frontend then jumps to that session.

use tauri::{AppHandle, Emitter, Manager};
use tauri_winrt_notification::{Sound, Toast};
use uuid::Uuid;

/// Show a system notification for `session_id` in window `label`. Clicking
/// it focuses that window and tells its frontend to jump to the session.
pub fn send(app: &AppHandle, label: &str, session_id: Uuid, body: String) {
    let app_id = app_id(app);
    let app2 = app.clone();
    let label2 = label.to_string();
    let toast = Toast::new(&app_id)
        .title("Muster")
        .text2(&body)
        // Attach the default notification sound explicitly: without an
        // <audio> node Windows falls back to it anyway, but making it
        // explicit keeps the sound tied to the toast regardless of defaults.
        .sound(Some(Sound::Default))
        .on_activated(move |_| {
            if let Some(window) = app2.get_webview_window(&label2) {
                // The window may be hidden (tray keep-alive) or minimized.
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
                let _ = window.emit(
                    "notification-activate",
                    serde_json::json!({ "id": session_id }),
                );
            }
            Ok(())
        });
    if let Err(e) = toast.show() {
        log::warn!("notification failed: {e}");
    }
}

/// The toast's AppUserModelID, mirroring tauri-plugin-notification's rule:
/// the installed app uses its bundle identifier (registered by the
/// installer); a dev run from `target/{debug,release}` has no registered
/// AUMID, so it borrows PowerShell's.
fn app_id(app: &AppHandle) -> String {
    let dev = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.display().to_string()))
        .map(|dir| {
            let sep = std::path::MAIN_SEPARATOR;
            dir.ends_with(&format!("{sep}target{sep}debug"))
                || dir.ends_with(&format!("{sep}target{sep}release"))
        })
        .unwrap_or(false);
    if dev {
        Toast::POWERSHELL_APP_ID.to_string()
    } else {
        app.config().identifier.clone()
    }
}
