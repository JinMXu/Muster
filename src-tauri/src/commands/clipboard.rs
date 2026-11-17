//! Clipboard reads the WebView cannot make. The Web Clipboard API
//! (`navigator.clipboard`) is text-only as far as Muster is concerned:
//! files copied in Explorer land on the clipboard as CF_HDROP — a handle
//! list with no `text/plain` counterpart — so "copy a file, paste into the
//! terminal" reads as empty from JS and needs a native read instead. See
//! docs/superpowers/specs/2026-11-17-clipboard-file-paste-design.md.

/// Files currently on the clipboard as CF_HDROP (Explorer "copy"). The
/// frontend converts them to quoted shell paths and feeds the PTY.
///
/// Never errors: anything unavailable — no files on the clipboard, the
/// clipboard held open by another process, a non-Windows build — degrades
/// to an empty list and the paste flow falls through as if nothing were
/// copied. Runs on the IPC thread like `send_text`; the retry budget below
/// (≤10ms) is the same order as the AppState lock it already waits on.
#[cfg(windows)]
#[tauri::command]
#[specta::specta]
pub fn read_clipboard_files() -> Vec<String> {
    clipboard_files_win()
}

#[cfg(not(windows))]
#[tauri::command]
#[specta::specta]
pub fn read_clipboard_files() -> Vec<String> {
    Vec::new()
}

#[cfg(windows)]
fn clipboard_files_win() -> Vec<String> {
    use std::time::Duration;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::UI::Shell::HDROP;

    // CF_HDROP = 15, kept raw to avoid pulling in the Win32_System_Ole
    // feature just for the typed constant.
    const CF_HDROP: u32 = 15;

    if unsafe { IsClipboardFormatAvailable(CF_HDROP) }.is_err() {
        return Vec::new();
    }
    // OpenClipboard fails while another process holds the clipboard open
    // (Explorer keeps it briefly after every copy); retry a little before
    // reporting "no files".
    let mut opened = false;
    for _ in 0..5 {
        if unsafe { OpenClipboard(HWND::default()) }.is_ok() {
            opened = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    if !opened {
        return Vec::new();
    }
    struct CloseOnDrop;
    impl Drop for CloseOnDrop {
        fn drop(&mut self) {
            unsafe { let _ = CloseClipboard(); }
        }
    }
    let _close = CloseOnDrop;

    let Ok(handle) = (unsafe { GetClipboardData(CF_HDROP) }) else {
        return Vec::new();
    };
    let hdrop = HDROP(handle.0 as *mut _);
    crate::services::native_drop::hdrop_to_strings(hdrop)
}
