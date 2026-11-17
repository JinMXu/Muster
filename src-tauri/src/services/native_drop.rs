//! HDROP path extraction shared by the clipboard command and the (currently
//! dormant) native drag-drop experiment. See
//! docs/superpowers/specs/2026-11-17-clipboard-file-paste-design.md for the
//! full story of why OS file drags are not wired up.
//!
//! Chaining an IDropTarget onto the WebView2 child windows was probed and
//! rejected on real Windows (2026-11-17):
//! - The Chromium drop-target windows (`Chrome_WidgetWin_1`) belong to the
//!   WebView2 host process, not to Muster — Register/RevokeDragDrop cannot
//!   operate on another process's windows, and forwarding DragOver would be
//!   cross-process COM proxying.
//! - The `OleDropTargetInterface` window property stores an ole32-internal
//!   wrapper pointer, NOT the raw IDropTarget pointer (self-registration
//!   proved prop != passed pointer). QI-ing it is an access violation.
//! - Enabling Tauri's `dragDropEnabled` instead would make wry revoke and
//!   replace Chromium's targets — killing the in-page HTML5 drag & drop the
//!   FileTree and pane-rearrange UI depend on.
//!
//! The frontend side (useNativeFileDrop + the nativeFileDrop event) stays in
//! place but dormant; if WebView2 ever ships a supported interception API,
//! re-enable by emitting the event from wherever that API lands.

use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

/// DragQueryFileW pair: count query, then per-file wide-string reads.
/// Shared with `read_clipboard_files` (same HDROP shape from a different
/// clipboard source).
pub fn hdrop_to_strings(hdrop: HDROP) -> Vec<String> {
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
