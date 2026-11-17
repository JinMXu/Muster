#![cfg(windows)]

//! Dropped-file paths from Explorer, via WebView2's supported web-message
//! bridge.
//!
//! `dragDropEnabled` stays off in tauri.conf.json — that is required for
//! in-page HTML5 drag & drop on Windows, and it also leaves WebView2's own
//! external-drop handling enabled, so OS file drops already arrive in the
//! page as ordinary HTML5 `drop` events carrying `dataTransfer.files`. What
//! the page cannot see is the *disk path* of those File objects. WebView2
//! exposes exactly that: the page posts the FileList back with
//! `chrome.webview.postMessageWithAdditionalObjects(msg, files)` and the
//! host reads `ICoreWebView2File::Path` for each (stable since WebView2
//! 1.0.1774.30). This module owns that host side and re-emits the paths as
//! the `native-drop-paths` event, keyed by the page's correlation token.
//!
//! (Intercepting OLE drop targets natively was probed and rejected: the
//! Chromium drop window lives in the WebView2 host process, and the
//! `OleDropTargetInterface` property is not a raw IDropTarget pointer. See
//! docs/superpowers/specs/2026-11-17-clipboard-file-paste-design.md.)

use tauri::{Emitter, Manager};

use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2WebMessageReceivedEventArgs as Args;

use crate::events::NativeDropPaths;

/// Prefix the page puts on bridge messages: `muster:files:<token>`.
const MSG_PREFIX: &str = "muster:files:";

/// Install the web-message bridge on `label`'s webview. Idempotent per app
/// run in practice: each call adds one handler that only reacts to our
/// marker, so a duplicate install is harmless.
pub fn attach(app: tauri::AppHandle, label: String) {
    let Some(window) = app.get_webview_window(&label) else { return };
    let log_label = label.clone();
    let _ = window.with_webview(move |platform| {
        use webview2_com::WebMessageReceivedEventHandler;

        let Ok(core) = (unsafe { platform.controller().CoreWebView2() }) else {
            log::warn!("drop-paths [{log_label}]: CoreWebView2 unavailable");
            return;
        };
        let app = app.clone();
        let label = log_label.clone();
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_sender, args| {
            // Every web message lands here (Tauri's own IPC included); only
            // ours carries the marker, everything else returns immediately.
            let Some(args) = args else { return Ok(()) };
            let Some(token) = token_of(&args) else { return Ok(()) };
            let paths = file_paths(&args);
            log::info!(
                "drop-paths [{label}]: bridge got {} path(s) for token {token}",
                paths.len()
            );
            // Emit even when empty: the frontend clears its pending token and
            // an unresolvable drop (rare, e.g. non-file additional objects)
            // stays a no-op instead of a silent leak.
            let _ = app.emit_to(&label, "native-drop-paths", NativeDropPaths { token, paths });
            Ok(())
        }));
        let mut token = 0i64;
        match unsafe { core.add_WebMessageReceived(&handler, &mut token) } {
            Ok(()) => log::info!("drop-paths [{log_label}]: bridge installed"),
            Err(e) => log::warn!("drop-paths [{log_label}]: install failed: {e}"),
        }
    });
}

/// Our correlation token, if this message is one of ours.
fn token_of(args: &Args) -> Option<String> {
    let text = message_text(args)?;
    let rest = &text[text.find(MSG_PREFIX)? + MSG_PREFIX.len()..];
    let token: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    (!token.is_empty()).then_some(token)
}

/// Message body as a string. `postMessageWithAdditionalObjects` may deliver
/// it either as a plain string or as JSON (a quoted string); accept both.
fn message_text(args: &Args) -> Option<String> {
    let mut raw: *mut u16 = std::ptr::null_mut();
    if unsafe { args.TryGetWebMessageAsString(std::ptr::addr_of_mut!(raw) as *mut _) }.is_ok() {
        let s = take_com_string(raw);
        if !s.is_empty() {
            return Some(s);
        }
    }
    let mut raw: *mut u16 = std::ptr::null_mut();
    if unsafe { args.WebMessageAsJson(std::ptr::addr_of_mut!(raw) as *mut _) }.is_ok() {
        return Some(take_com_string(raw));
    }
    None
}

/// Absolute paths of the DOM File objects attached to the message.
fn file_paths(args: &Args) -> Vec<String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs2,
    };
    use windows_core::Interface;

    let Ok(args2) = args.cast::<ICoreWebView2WebMessageReceivedEventArgs2>() else {
        return Vec::new();
    };
    let Ok(collection) = (unsafe { args2.AdditionalObjects() }) else {
        return Vec::new();
    };
    let mut count = 0u32;
    if unsafe { collection.Count(&mut count) }.is_err() {
        return Vec::new();
    }
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        let Ok(object) = (unsafe { collection.GetValueAtIndex(i) }) else { continue };
        let Ok(file) = object.cast::<ICoreWebView2File>() else { continue };
        let mut raw: *mut u16 = std::ptr::null_mut();
        if unsafe { file.Path(std::ptr::addr_of_mut!(raw) as *mut _) }.is_ok() {
            let path = take_com_string(raw);
            if !path.is_empty() {
                paths.push(path);
            }
        }
    }
    paths
}

/// Consume a COM-allocated (CoTaskMem) wide string.
fn take_com_string(ptr: *mut u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
    }
    let s = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)) };
    unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(ptr as *const std::ffi::c_void)) };
    s
}

/// DragQueryFileW pair: count query, then per-file wide-string reads.
/// Shared with `read_clipboard_files` (same HDROP shape, different source).
pub fn hdrop_to_strings(hdrop: windows::Win32::UI::Shell::HDROP) -> Vec<String> {
    use windows::Win32::UI::Shell::DragQueryFileW;

    // ifile = 0xFFFFFFFF is the documented "how many files" query.
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        // NULL-buffer query returns the length WITHOUT the null; the copy
        // needs room for it, hence len + 1.
        let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
        let mut buf = vec![0u16; len + 1];
        let written = unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) } as usize;
        paths.push(String::from_utf16_lossy(&buf[..written]));
    }
    paths
}