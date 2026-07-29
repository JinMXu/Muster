//! All Tauri commands bridging the React frontend to the Rust backend.
//!
//! Every window owns an independent `AppState` (multi-window support):
//! commands declare a `window: Window` parameter, which Tauri injects as the
//! invoking window, and resolve their state via
//! `state.for_label(window.label())`. Change events are emitted back to that
//! window only, so the frontend needs no window-awareness of its own.

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use uuid::Uuid;

use crate::models::app::{AppState, AppStateView};
use crate::models::pane::{FocusDirection, PaneContent, PaneDropEdge, ResizeDirection};
use crate::models::project::RightPanel;
use crate::models::session::SessionInfo;
use crate::services::config::Settings;
use crate::services::i18n::translate;
use crate::services::usage::{self, UsageCache};

/// Per-window state registry: each window label owns an independent
/// `AppState` (projects/tabs/sessions/files/diffs) so windows restore and
/// persist separately. Settings stay shared across every window.
pub struct SharedState {
    states: Mutex<HashMap<String, Arc<Mutex<AppState>>>>,
    settings: Arc<Mutex<Settings>>,
    pub usage: Arc<Mutex<UsageCache>>,
}

impl SharedState {
    pub fn new(settings: Settings) -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            settings: Arc::new(Mutex::new(settings)),
            usage: Arc::new(Mutex::new(UsageCache::default())),
        }
    }

    /// State for one window, creating an empty one (with shared settings) on
    /// first use. Snapshot restore happens in the explicit restore paths
    /// (bootstrap for "main", `new_window` for the rest), not here.
    pub fn for_label(&self, label: &str) -> Arc<Mutex<AppState>> {
        self.states
            .lock()
            .entry(label.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(AppState::new(self.settings.clone()))))
            .clone()
    }

    /// Drop a closed window's state. Its sessions are terminated beforehand
    /// by the close/destroy handlers in bootstrap.
    pub fn remove_label(&self, label: &str) {
        self.states.lock().remove(label);
    }

    /// Snapshot of every live (label, state) pair, for the autosave thread.
    pub fn all(&self) -> Vec<(String, Arc<Mutex<AppState>>)> {
        self.states.lock().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

/// Helper: emit a fresh `state-changed` event after a mutation, to the
/// invoking window only — other windows keep their own independent state.
fn emit_state(window: &Window, state: &AppState) {
    let _ = window.emit("state-changed", state.view());
}

// --- State / queries -------------------------------------------------------

#[tauri::command]
pub fn get_state(window: Window, state: State<SharedState>) -> AppStateView {
    state.for_label(window.label()).lock().view()
}

#[tauri::command]
pub fn get_settings(state: State<SharedState>) -> Settings {
    state.settings.lock().clone()
}

/// Factory defaults for the Settings modal's "Reset" button — returned
/// without touching the persisted config (the modal's Save does that).
#[tauri::command]
pub fn default_settings() -> Settings {
    Settings::default()
}

#[tauri::command]
pub fn save_settings(state: State<SharedState>, settings: Settings) -> Result<(), String> {
    *state.settings.lock() = settings;
    state.settings.lock().save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn available_themes() -> Vec<String> { crate::theme::catalog::available() }

#[tauri::command]
pub fn theme_colors(name: String, dark: bool) -> crate::theme::ThemeColors {
    crate::theme::ThemeColors::resolve(&name, dark)
}

#[tauri::command]
pub fn session_info(window: Window, state: State<SharedState>, id: Uuid) -> Option<SessionInfo> {
    state.for_label(window.label()).lock().session_info(id)
}

#[tauri::command]
pub fn list_all_sessions(window: Window, state: State<SharedState>) -> Vec<SessionInfo> {
    state.for_label(window.label()).lock().list_all_sessions()
}

#[tauri::command]
pub fn file_info(window: Window, state: State<SharedState>, id: Uuid) -> Option<crate::models::file::FileTabInfo> {
    state.for_label(window.label()).lock().file_info(id)
}

#[tauri::command]
pub fn diff_info(window: Window, state: State<SharedState>, id: Uuid) -> Option<crate::models::diff::DiffTabInfo> {
    state.for_label(window.label()).lock().diff_info(id)
}

// --- Windows ---------------------------------------------------------------

/// Open a new window with its own independent projects/tabs/sessions. The
/// state is registered (and restored from the window's own snapshot, if any)
/// BEFORE the window is built, so the webview's first `get_state` invoke
/// already sees the restored layout. The actual work lives in
/// `bootstrap::spawn_window`, which the tray menu's "New Window" also calls.
#[tauri::command]
pub fn new_window(app: AppHandle) -> Result<(), String> {
    crate::bootstrap::spawn_window(&app)
}

// --- Projects --------------------------------------------------------------

#[tauri::command]
pub fn new_project(window: Window, state: State<SharedState>, directory: Option<String>) -> Uuid {
    let s = state.for_label(window.label());
    let id = s.lock().new_project(directory);
    // new_project spawns a session model without a PTY — bring it up now,
    // same as bootstrap does for restored/starter sessions.
    crate::bootstrap::spawn_pending(window.app_handle(), window.label(), &s);
    emit_state(&window, &s.lock());
    id
}

#[tauri::command]
pub fn close_project(window: Window, state: State<SharedState>, id: Uuid) {
    let s = state.for_label(window.label());
    s.lock().close_project(id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_project(window: Window, state: State<SharedState>, id: Uuid) {
    let s = state.for_label(window.label());
    s.lock().select_project(id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_project_by_index(window: Window, state: State<SharedState>, idx: usize) {
    let s = state.for_label(window.label());
    s.lock().select_project_by_index(idx);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_next_project(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().select_next_project();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_previous_project(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().select_previous_project();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn move_project(window: Window, state: State<SharedState>, from: Uuid, to: Uuid) {
    let s = state.for_label(window.label());
    s.lock().move_project(from, to);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn rename_project(window: Window, state: State<SharedState>, id: Uuid, name: Option<String>) {
    let s = state.for_label(window.label());
    s.lock().rename_project(id, name);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn set_project_directory(window: Window, state: State<SharedState>, id: Uuid, directory: Option<String>) {
    let s = state.for_label(window.label());
    s.lock().set_project_directory(id, directory);
    emit_state(&window, &s.lock());
}

// --- Sessions / terminal ---------------------------------------------------

#[tauri::command]
pub fn spawn_session(
    window: Window,
    state: State<SharedState>,
    directory: Option<String>,
) -> Result<SessionInfo, String> {
    let s = state.for_label(window.label());
    let lang = &state.settings.lock().language;
    let session_id = s.lock().spawn_session_in_selected(directory);
    let session_id = session_id.ok_or_else(|| translate("no-selected-project", lang).to_string())?;
    let session_arc = s.lock().sessions.get(&session_id).cloned();
    let Some(session) = session_arc else { return Err(translate("session-not-registered", lang).to_string()); };
    session.spawn(80, 24).map_err(|e| e.to_string())?;
    session.attach_read_loop(window.app_handle().clone(), window.label().to_string());
    let info = SessionInfo::from(session.as_ref());
    emit_state(&window, &s.lock());
    Ok(info)
}

#[tauri::command]
pub fn send_text(window: Window, state: State<SharedState>, id: Uuid, text: String) {
    let s = state.for_label(window.label());
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) { session.send_text(&text); }
}

#[tauri::command]
pub fn resize_terminal(window: Window, state: State<SharedState>, id: Uuid, cols: u16, rows: u16) {
    let s = state.for_label(window.label());
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) { session.resize(cols, rows); }
}

/// Clear the focused terminal: ask the shell to clear its screen (`clear` /
/// `cls`). The frontend additionally wipes the xterm scrollback locally.
#[tauri::command]
pub fn clear_terminal(window: Window, state: State<SharedState>, id: Uuid) {
    let s = state.for_label(window.label());
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) {
        let cmd = if session.shell_name == "cmd" { "cls\r" } else { "clear\r" };
        session.send_text(cmd);
    }
}

#[tauri::command]
pub fn terminate_session(window: Window, state: State<SharedState>, id: Uuid) {
    let s = state.for_label(window.label());
    let g = s.lock();
    if let Some(session) = g.sessions.get(&id) { session.terminate(); }
}

// --- Processes / ports (Info panel) ------------------------------------------

/// PROCESSES section: every process of the session (Job Object members, or
/// the shell's ppid descendants when untracked), shell row first.
#[tauri::command]
pub fn session_processes(session_id: Uuid, shell_pid: u32) -> Vec<crate::services::procs::ProcessInfo> {
    let pids = crate::services::procs::session_pids(session_id, shell_pid);
    crate::services::procs::process_infos(&pids)
}

/// Force-kill a process (Windows has only TerminateProcess semantics, hence a
/// single kill command — see services::procs::kill).
#[tauri::command]
pub fn kill_process(pid: u32) -> Result<(), String> {
    crate::services::procs::kill(pid)
}

/// PORTS section: listening TCP ports owned by the given pids, plus any
/// process belonging to the project directory (working directory or command
/// line under it), so dev servers started outside this session still show.
#[tauri::command]
pub fn session_ports(
    pids: Vec<u32>,
    project_root: Option<String>,
) -> Vec<crate::services::procs::ListenPort> {
    crate::services::procs::listen_ports(&pids, project_root.as_deref())
}

// --- Tabs ------------------------------------------------------------------

#[tauri::command]
pub fn close_selected_tab(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().close_selected_tab();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_tab(window: Window, state: State<SharedState>, id: Uuid) {
    let s = state.for_label(window.label());
    s.lock().select_tab(id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_next_tab(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().select_next_tab();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn select_previous_tab(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().select_previous_tab();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn move_tab(window: Window, state: State<SharedState>, from: Uuid, to: Uuid) {
    let s = state.for_label(window.label());
    s.lock().move_tab(from, to);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn rename_tab(window: Window, state: State<SharedState>, id: Uuid, name: Option<String>) {
    let s = state.for_label(window.label());
    s.lock().rename_tab(id, name);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn close_other_tabs(window: Window, state: State<SharedState>, tab_id: Uuid) {
    let s = state.for_label(window.label());
    s.lock().close_other_tabs(tab_id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn close_tabs_to_right(window: Window, state: State<SharedState>, tab_id: Uuid) {
    let s = state.for_label(window.label());
    s.lock().close_tabs_to_right(tab_id);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn close_all_tabs(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().close_all_tabs();
    emit_state(&window, &s.lock());
}

/// Path behind a pane for the tab context menu's Reveal/Copy Path items.
#[tauri::command]
pub fn pane_context_path(window: Window, state: State<SharedState>, tab_id: Uuid, pane_id: Uuid) -> Option<String> {
    state.for_label(window.label()).lock().pane_context_path(tab_id, pane_id)
}

// --- Splits / panes --------------------------------------------------------

#[tauri::command]
pub fn split(window: Window, state: State<SharedState>, edge: PaneDropEdge) -> Result<(), String> {
    let s = state.for_label(window.label());
    {
        let mut g = s.lock();
        g.split(edge);
    }
    // Spawn the PTY for the new session created by `split`. The session id
    // is the focused pane's session (the one we just inserted).
    let new_session_opt = {
        let g = s.lock();
        g.selected_tab()
            .and_then(|t| t.focused_pane())
            .and_then(|p| match &p.content {
                PaneContent::Session(id) => g.sessions.get(id).cloned(),
                _ => None,
            })
    };
    if let Some(session) = new_session_opt {
        let _ = session.spawn(80, 24);
        session.attach_read_loop(window.app_handle().clone(), window.label().to_string());
    }
    emit_state(&window, &s.lock());
    Ok(())
}

#[tauri::command]
pub fn focus_pane(window: Window, state: State<SharedState>, direction: FocusDirection) {
    let s = state.for_label(window.label());
    s.lock().focus_pane(direction);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn resize_pane(window: Window, state: State<SharedState>, direction: ResizeDirection) {
    let s = state.for_label(window.label());
    s.lock().resize_pane(direction);
    emit_state(&window, &s.lock());
}

/// Drag-resize one divider: `vertical` = between columns `index`/`index + 1`,
/// otherwise between panes `index`/`index + 1` of `columns[column_index]`.
#[tauri::command]
pub fn resize_pane_divider(
    window: Window,
    state: State<SharedState>,
    tab_id: Uuid,
    vertical: bool,
    column_index: usize,
    index: usize,
    delta: f32,
) {
    let s = state.for_label(window.label());
    s.lock().resize_pane_divider(tab_id, vertical, column_index, index, delta);
    emit_state(&window, &s.lock());
}

/// Drag & drop rearrange: move `pane_id` to `edge` of `target_pane_id`
/// within tab `tab_id` (intra-tab only; see PaneTab::move_pane).
#[tauri::command]
pub fn move_pane(
    window: Window,
    state: State<SharedState>,
    tab_id: Uuid,
    pane_id: Uuid,
    target_pane_id: Uuid,
    edge: PaneDropEdge,
) {
    let s = state.for_label(window.label());
    s.lock().move_pane(tab_id, pane_id, target_pane_id, edge);
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn toggle_pane_zoom(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().toggle_pane_zoom();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn equalize_panes(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().equalize_panes();
    emit_state(&window, &s.lock());
}

// --- Sidebar / panel -------------------------------------------------------

#[tauri::command]
pub fn toggle_left_sidebar(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().toggle_left_sidebar();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn toggle_right_panel(window: Window, state: State<SharedState>) {
    let s = state.for_label(window.label());
    s.lock().toggle_right_panel();
    emit_state(&window, &s.lock());
}

#[tauri::command]
pub fn toggle_panel(window: Window, state: State<SharedState>, panel: RightPanel) {
    let s = state.for_label(window.label());
    s.lock().toggle_panel(panel);
    emit_state(&window, &s.lock());
}

// --- Files / diffs ---------------------------------------------------------

#[tauri::command]
pub fn open_file(window: Window, state: State<SharedState>, path: String, to_side: bool) -> Option<Uuid> {
    let s = state.for_label(window.label());
    let id = s.lock().open_file(&path, to_side);
    emit_state(&window, &s.lock());
    id
}

#[tauri::command]
pub fn file_text_changed(window: Window, state: State<SharedState>, id: Uuid, text: String) {
    let s = state.for_label(window.label());
    let g = s.lock();
    if let Some(f) = g.files.get(&id) { f.set_text(text); }
}

#[tauri::command]
pub fn save_selected_file(window: Window, state: State<SharedState>) -> Result<(), String> {
    let s = state.for_label(window.label());
    let saved = s.lock().save_selected_file()?;
    if let Some(id) = saved {
        let _ = window.emit("file-saved", serde_json::json!({ "id": id }));
    }
    Ok(())
}

#[tauri::command]
pub fn save_file(window: Window, state: State<SharedState>, id: Uuid) -> Result<(), String> {
    let s = state.for_label(window.label());
    s.lock().save_file(id)?;
    let _ = window.emit("file-saved", serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command]
pub fn tab_dirty_files(window: Window, state: State<SharedState>, tab_id: Uuid) -> Vec<crate::models::app::DirtyFileInfo> {
    state.for_label(window.label()).lock().tab_dirty_files(tab_id)
}

#[tauri::command]
pub fn project_dirty_files(window: Window, state: State<SharedState>, project_id: Uuid) -> Vec<crate::models::app::DirtyFileInfo> {
    state.for_label(window.label()).lock().project_dirty_files(project_id)
}

#[tauri::command]
pub fn open_diff(window: Window, state: State<SharedState>, repo_root: String, path: String, staged: bool) -> Option<Uuid> {
    let s = state.for_label(window.label());
    let id = s.lock().open_diff(&repo_root, &path, staged);
    emit_state(&window, &s.lock());
    id
}

#[tauri::command]
pub fn reload_diff(window: Window, state: State<SharedState>, id: Uuid) {
    let s = state.for_label(window.label());
    let g = s.lock();
    if let Some(d) = g.diffs.get(&id) { d.reload(); }
}

// --- Filesystem -----------------------------------------------------------

#[derive(Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}

#[tauri::command]
pub fn list_directory(path: String) -> Vec<DirEntry> {
    let Ok(entries) = std::fs::read_dir(&path) else { return Vec::new() };
    let mut items: Vec<DirEntry> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name == ".git" || name == "." || name == ".." {
                return None;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some(DirEntry { name: name.clone(), path: e.path().to_string_lossy().to_string(), is_directory: is_dir })
        })
        .collect();
    items.sort_by(|a, b| {
        if a.is_directory != b.is_directory {
            b.is_directory.cmp(&a.is_directory)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });
    items
}

#[tauri::command]
pub fn trash_file(path: String) -> Result<(), String> {
    crate::services::explorer::trash(&path)
}

#[tauri::command]
pub fn create_file(state: State<SharedState>, parent_dir: String, name: String, is_directory: bool) -> Result<String, String> {
    let lang = &state.settings.lock().language;
    let path = std::path::Path::new(&parent_dir).join(&name);
    if path.exists() { return Err(translate("name-already-exists", lang).replace("{name}", &name)); }
    if is_directory {
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    } else {
        std::fs::write(&path, "").map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().to_string())
}

/// Reject names that can't be renamed to: empty, "." / "..", or containing
/// a path separator. The frontend mirrors these rules before invoking.
fn validate_rename_name(name: &str, lang: &str) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(translate("invalid-name", lang).replace("{name}", name));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(translate("name-no-slash", lang).to_string());
    }
    Ok(())
}

/// True when both paths resolve to the same on-disk entry. Case-only
/// renames hit this on Windows: the target "exists" because the filesystem
/// is case-insensitive, and `fs::rename` handles the rename fine.
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Rewrite `path` after `from` was renamed to `to`: an exact match, or a
/// path inside the renamed directory (separator-boundary prefix match).
fn remap_renamed_path(path: &str, from: &str, to: &str) -> Option<String> {
    if path == from {
        return Some(to.to_string());
    }
    let rest = path.strip_prefix(from)?;
    if rest.starts_with('\\') || rest.starts_with('/') {
        return Some(format!("{to}{rest}"));
    }
    None
}

#[tauri::command]
pub fn rename_path(window: Window, state: State<SharedState>, from: String, to: String) -> Result<String, String> {
    let from_path = std::path::Path::new(&from);
    let to_path = from_path.parent().unwrap_or(std::path::Path::new("")).join(&to);
    let name = to_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lang = &state.settings.lock().language;
    validate_rename_name(name, lang)?;
    if to_path.exists() && !same_file(from_path, &to_path) {
        return Err(translate("name-already-exists", lang).replace("{name}", name));
    }
    std::fs::rename(from_path, &to_path).map_err(|e| e.to_string())?;
    let new_path = to_path.to_string_lossy().to_string();
    // Open editor tabs follow the rename: a tab on the renamed path, or on
    // anything under a renamed directory, is updated to the new path.
    let s = state.for_label(window.label());
    {
        let g = s.lock();
        for f in g.files.values() {
            if let Some(p) = remap_renamed_path(&f.path(), &from, &new_path) {
                f.update_path(&p);
            }
        }
    }
    emit_state(&window, &s.lock());
    Ok(new_path)
}

/// Replace the set of watched directories (file tree auto-refresh). The
/// frontend sends every directory it currently displays.
#[tauri::command]
pub fn watch_directories(window: Window, paths: Vec<String>) {
    crate::services::watch::set_watched_directories(window.app_handle(), paths);
}

// --- Git -------------------------------------------------------------------

#[tauri::command]
pub fn git_status(repo_root: String) -> crate::services::git::GitStatusInfo {
    crate::services::git::status(&repo_root)
}

/// Anchor for the right panels: the toplevel of the repo containing `cwd`,
/// or `cwd` unchanged when no repository is found upwards.
#[tauri::command]
pub fn resolve_project_root(cwd: String) -> String {
    crate::services::git::project_root(&cwd)
}

#[tauri::command]
pub fn git_stage(repo_root: String, path: String) -> Result<(), String> {
    crate::services::git::stage(&repo_root, &path)
}

#[tauri::command]
pub fn git_stage_all(repo_root: String) -> Result<(), String> {
    crate::services::git::stage_all(&repo_root)
}

#[tauri::command]
pub fn git_unstage(repo_root: String, path: String) -> Result<(), String> {
    crate::services::git::unstage(&repo_root, &path)
}

#[tauri::command]
pub fn git_unstage_all(repo_root: String) -> Result<(), String> {
    crate::services::git::unstage_all(&repo_root)
}

#[tauri::command]
pub fn git_guard(repo_root: String, paths: Vec<String>) -> crate::services::git::GitGuard {
    crate::services::git::guard(&repo_root, &paths)
}

#[tauri::command]
pub fn git_discard_guarded(
    repo_root: String,
    path: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    crate::services::git::discard_guarded(&repo_root, &path, &guard)
}

#[tauri::command]
pub fn git_discard_all_guarded(
    repo_root: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    crate::services::git::discard_all_guarded(&repo_root, &guard)
}

#[tauri::command]
pub fn git_commit(repo_root: String, message: String, include_all: bool, amend: bool) -> Result<String, String> {
    crate::services::git::commit(&repo_root, &message, include_all, amend).map(|oid| oid.to_string())
}

#[tauri::command]
pub fn git_switch_branch(repo_root: String, name: String) -> Result<(), String> {
    crate::services::git::switch_branch(&repo_root, &name)
}

#[tauri::command]
pub fn git_create_branch(repo_root: String, name: String) -> Result<(), String> {
    crate::services::git::create_branch(&repo_root, &name)
}

#[tauri::command]
pub fn git_fetch(repo_root: String) -> Result<(), String> {
    crate::services::git::fetch(&repo_root)
}

#[tauri::command]
pub fn git_pull(repo_root: String) -> Result<(), String> {
    crate::services::git::pull(&repo_root)
}

#[tauri::command]
pub fn git_push(repo_root: String, remote: String) -> Result<(), String> {
    crate::services::git::push(&repo_root, &remote)
}

#[tauri::command]
pub fn git_stash_all(repo_root: String) -> Result<(), String> {
    crate::services::git::stash_all(&repo_root)
}

#[tauri::command]
pub fn git_stash_pop(repo_root: String) -> Result<(), String> {
    crate::services::git::stash_pop(&repo_root)
}

#[tauri::command]
pub fn git_init(repo_root: String) -> Result<(), String> {
    crate::services::git::init(&repo_root)
}

// --- Misc ------------------------------------------------------------------

#[tauri::command]
pub fn install_explorer_context_menu() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::services::explorer::install_context_menu(&exe.to_string_lossy())
}

// --- Usage tracking --------------------------------------------------------

#[tauri::command]
pub fn usage_summary(state: State<SharedState>) -> usage::UsageSummary {
    state.usage.lock().summary()
}

#[tauri::command]
pub fn usage_sessions(
    state: State<SharedState>,
    tool: Option<usage::ToolKind>,
    since: Option<i64>,
    limit: Option<usize>,
) -> Vec<usage::UsageSession> {
    state.usage.lock().sessions_filtered(tool, since, limit)
}

#[tauri::command]
pub async fn usage_refresh(state: State<'_, SharedState>) -> Result<(), String> {
    let cache = state.usage.clone();
    tokio::task::spawn_blocking(move || {
        usage::scan_once(&cache);
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Register every command (called from the Tauri Builder).
pub fn register_all(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_settings,
            default_settings,
            save_settings,
            available_themes,
            theme_colors,
            session_info,
            list_all_sessions,
            file_info,
            diff_info,
            new_window,
            new_project,
            close_project,
            select_project,
            select_project_by_index,
            select_next_project,
            select_previous_project,
            move_project,
            rename_project,
            set_project_directory,
            spawn_session,
            send_text,
            resize_terminal,
            clear_terminal,
            terminate_session,
            session_processes,
            kill_process,
            session_ports,
            close_selected_tab,
            select_tab,
            select_next_tab,
            select_previous_tab,
            move_tab,
            rename_tab,
            close_other_tabs,
            close_tabs_to_right,
            close_all_tabs,
            pane_context_path,
            split,
            focus_pane,
            resize_pane,
            resize_pane_divider,
            move_pane,
            toggle_pane_zoom,
            equalize_panes,
            toggle_left_sidebar,
            toggle_right_panel,
            toggle_panel,
            open_file,
            file_text_changed,
            save_selected_file,
            save_file,
            tab_dirty_files,
            project_dirty_files,
            open_diff,
            reload_diff,
            list_directory,
            trash_file,
            create_file,
            rename_path,
            watch_directories,
            git_status,
            resolve_project_root,
            git_stage,
            git_stage_all,
            git_unstage,
            git_unstage_all,
            git_guard,
            git_discard_guarded,
            git_discard_all_guarded,
            git_commit,
            git_switch_branch,
            git_create_branch,
            git_fetch,
            git_pull,
            git_push,
            git_stash_all,
            git_stash_pop,
            git_init,
            install_explorer_context_menu,
            usage_summary,
            usage_sessions,
            usage_refresh,
        ])
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rename_name_accepts_plain_names() {
        assert!(validate_rename_name("foo.txt", "en").is_ok());
        assert!(validate_rename_name("a b.rs", "en").is_ok());
        assert!(validate_rename_name(".gitignore", "en").is_ok());
    }

    #[test]
    fn validate_rename_name_rejects_bad_names() {
        assert!(validate_rename_name("", "en").is_err());
        assert!(validate_rename_name(".", "en").is_err());
        assert!(validate_rename_name("..", "en").is_err());
        assert!(validate_rename_name("a/b", "en").is_err());
        assert!(validate_rename_name("a\\b", "en").is_err());
    }

    #[test]
    fn remap_renamed_path_exact_match() {
        assert_eq!(
            remap_renamed_path("C:\\proj\\a.txt", "C:\\proj\\a.txt", "C:\\proj\\b.txt"),
            Some("C:\\proj\\b.txt".to_string())
        );
    }

    #[test]
    fn remap_renamed_path_inside_renamed_dir() {
        assert_eq!(
            remap_renamed_path("C:\\proj\\dir\\sub\\f.txt", "C:\\proj\\dir", "C:\\proj\\renamed"),
            Some("C:\\proj\\renamed\\sub\\f.txt".to_string())
        );
    }

    #[test]
    fn remap_renamed_path_rejects_sibling_prefix() {
        // "dir2" starts with "dir" but not on a separator boundary.
        assert_eq!(remap_renamed_path("C:\\proj\\dir2\\f.txt", "C:\\proj\\dir", "C:\\proj\\x"), None);
        assert_eq!(remap_renamed_path("C:\\other\\f.txt", "C:\\proj\\dir", "C:\\proj\\x"), None);
    }

    #[test]
    fn remap_renamed_path_accepts_forward_slash_boundary() {
        assert_eq!(
            remap_renamed_path("C:/proj/dir/f.txt", "C:/proj/dir", "C:/proj/x"),
            Some("C:/proj/x/f.txt".to_string())
        );
    }
}
