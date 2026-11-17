#![cfg(windows)]

//! Chained OLE drop target for OS file drags (Explorer).
//!
//! `dragDropEnabled` stays off in tauri.conf.json on purpose: enabling it
//! makes wry `RevokeDragDrop` every WebView2 child window and replace
//! Chromium's drop targets, which kills the in-page HTML5 drag & drop the
//! FileTree and the pane-rearrange UI depend on. With the flag off, OS file
//! drops fall through to Chromium and Muster never sees them — so this
//! module registers its own IDropTarget on every WebView2 child window that
//! already has one, chaining to the original: drags carrying CF_HDROP
//! (Explorer files) become `native-file-drop` events, anything else is
//! forwarded to Chromium's target untouched, preserving HTML5 DnD exactly.
//!
//! Each install owns its own STA thread. `RegisterDragDrop` binds the
//! target to the calling thread's apartment, and OLE dispatches the
//! DragEnter/Over/Leave/Drop callbacks through that apartment's message
//! pump — which this thread runs for the lifetime of the process.
//!
//! The Chromium target is found via the OLE window property
//! `OleDropTargetInterface` (`RegisterDragDrop` stores it there). That
//! property is an ole32 implementation detail; if it ever disappears this
//! module degrades to no file-drop events, never to a broken webview.
//! Object lifetime needs no bookkeeping: `RegisterDragDrop` takes its own
//! reference, and `RevokeDragDrop` (or window destruction) releases it.

use std::cell::Cell;

use windows::core::{implement, w};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, POINTL};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL,
};
use windows::Win32::System::Ole::{
    CF_HDROP, IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium,
    DROPEFFECT, DROPEFFECT_COPY,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumChildWindows, GetMessageW, GetPropW, MSG,
};

/// Install the chained drop target for `label`'s window. Fire-and-forget:
/// the wait for WebView2's child windows happens on the install thread, so
/// callers don't block or race the async webview creation.
pub fn install(app: tauri::AppHandle, label: String) {
    let _ = std::thread::Builder::new()
        .name("native-drop".into())
        .spawn(move || run(app, label));
}

fn run(app: tauri::AppHandle, label: String) {
    use tauri::Manager;

    unsafe {
        // STA: RegisterDragDrop refuses other apartment types.
        if OleInitialize(None).is_err() {
            return;
        }
        let Some(window) = app.get_webview_window(&label) else { return };
        // tauri hands out HWND from its own (newer) windows crate; convert
        // the raw pointer into this crate's HWND (isize-backed).
        let Ok(top) = window.hwnd() else { return };
        let top = HWND(top.0 as _);
        let Some(children) = wait_for_chromium_targets(top) else { return };
        for hwnd in children {
            wrap(hwnd, app.clone(), label.clone());
        }
        // The message pump that keeps the apartment alive — OLE delivers the
        // drop callbacks as messages to this thread; without this loop they
        // queue forever. GetMessageW returns 0 (WM_QUIT) / -1 (error) to end.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }
    }
}

/// Poll for WebView2 child windows that carry a registered IDropTarget.
/// The webview's HWNDs appear some time after the window build; ~15s of
/// retries covers slow first paints, and the thread parks in GetMessageW
/// afterwards either way.
fn wait_for_chromium_targets(top: HWND) -> Option<Vec<HWND>> {
    use std::time::Duration;

    for _ in 0..60 {
        let found = chromium_targets(top);
        if !found.is_empty() {
            return Some(found);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    None
}

/// Every child of `top` that currently has a registered drop target.
fn chromium_targets(top: HWND) -> Vec<HWND> {
    let mut found: Vec<HWND> = Vec::new();
    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<HWND>);
        if has_registered_target(hwnd) {
            out.push(hwnd);
        }
        true.into() // keep enumerating
    }
    unsafe {
        let _ = EnumChildWindows(top, Some(callback), LPARAM(&mut found as *mut Vec<HWND> as _));
    }
    found
}

fn has_registered_target(hwnd: HWND) -> bool {
    unsafe { !GetPropW(hwnd, w!("OleDropTargetInterface")).0.is_null() }
}

/// Swap in the chaining target on one child window, keeping the original.
fn wrap(hwnd: HWND, app: tauri::AppHandle, label: String) {
    use windows::core::{Interface, IUnknown};

    unsafe {
        let prop = GetPropW(hwnd, w!("OleDropTargetInterface"));
        if prop.0.is_null() {
            return;
        }
        // The property holds a raw IDropTarget pointer (no ref held for us);
        // rebuild the wrapper and QI to get a properly referenced clone. The
        // transient wrapper's Release only gives back a registration ref that
        // was never ours — Chromium's own reference stays.
        let unknown: IUnknown = std::mem::transmute_copy(&prop.0);
        let Ok(original) = unknown.cast::<IDropTarget>() else {
            return;
        };
        let target: IDropTarget = ChainedDropTarget {
            hwnd,
            original,
            file_drag: Cell::new(false),
            app,
            label,
        }
        .into();
        let _ = RegisterDragDrop(hwnd, &target);
    }
}

/// Extract CF_HDROP file paths from a drag data object, or None when the
/// drag carries no files (then the original target must handle it). The
/// storage medium is released here — after path extraction — per OLE
/// ownership rules.
fn hdrop_from_data(data: Option<&IDataObject>) -> Option<Vec<String>> {
    let data = data?;
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let mut medium: STGMEDIUM = unsafe { data.GetData(&format) }.ok()?;
    let paths = hdrop_to_strings(HDROP(unsafe { medium.u.hGlobal.0 }));
    unsafe { ReleaseStgMedium(&mut medium) };
    Some(paths)
}

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

fn emit(app: &tauri::AppHandle, label: &str, kind: &str, paths: Vec<String>, point: POINT) {
    use tauri::Emitter;

    let payload = crate::events::NativeFileDrop {
        kind: kind.to_string(),
        paths,
        x: point.x,
        y: point.y,
    };
    let _ = app.emit_to(label, "native-file-drop", payload);
}

/// Convert screen coordinates to the wrapped window's physical client
/// coordinates — the space the frontend expects (it divides by
/// devicePixelRatio for CSS pixels).
fn to_client(hwnd: HWND, pt: &POINTL) -> POINT {
    let mut client = POINT { x: pt.x, y: pt.y };
    unsafe { let _ = ScreenToClient(hwnd, &mut client); }
    client
}

#[implement(IDropTarget)]
struct ChainedDropTarget {
    /// The WebView2 child window this target sits on; ScreenToClient basis
    /// for the coordinates emitted below.
    hwnd: HWND,
    /// Chromium's target, preserved for everything that isn't a file drag.
    original: IDropTarget,
    /// Whether the active drag carries files — OLE shows us the data object
    /// only in DragEnter/Drop, so DragOver/Leave forward on this flag.
    file_drag: Cell<bool>,
    app: tauri::AppHandle,
    label: String,
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for ChainedDropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Option<&IDataObject>,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let client = to_client(self.hwnd, pt);
        match hdrop_from_data(pdataobj) {
            Some(paths) => {
                self.file_drag.set(true);
                emit(&self.app, &self.label, "enter", paths, client);
                unsafe { *pdweffect = DROPEFFECT_COPY };
            }
            None => {
                self.file_drag.set(false);
                unsafe { self.original.DragEnter(pdataobj, grfkeystate, *pt, pdweffect)? }
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        if self.file_drag.get() {
            let client = to_client(self.hwnd, pt);
            emit(&self.app, &self.label, "over", Vec::new(), client);
            unsafe { *pdweffect = DROPEFFECT_COPY };
        } else {
            unsafe { self.original.DragOver(grfkeystate, *pt, pdweffect)? };
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        if self.file_drag.get() {
            self.file_drag.set(false);
            emit(&self.app, &self.label, "leave", Vec::new(), POINT { x: 0, y: 0 });
        } else {
            unsafe { self.original.DragLeave()? };
        }
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Option<&IDataObject>,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let client = to_client(self.hwnd, pt);
        match hdrop_from_data(pdataobj) {
            Some(paths) => {
                self.file_drag.set(false);
                emit(&self.app, &self.label, "drop", paths, client);
                unsafe { *pdweffect = DROPEFFECT_COPY };
            }
            None => unsafe {
                self.original.Drop(pdataobj, grfkeystate, *pt, pdweffect)?
            },
        }
        Ok(())
    }
}
