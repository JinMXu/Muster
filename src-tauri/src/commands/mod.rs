//! All Tauri commands bridging the React frontend to the Rust backend.
//!
//! Every window owns an independent `AppState` (multi-window support):
//! commands declare a `window: Window` parameter, which Tauri injects as the
//! invoking window, and resolve their state via
//! `state.for_label(window.label())`. Change events are emitted back to that
//! window only, so the frontend needs no window-awareness of its own.
//!
//! Handlers are split by domain into submodules; this file holds the shared
//! per-window state registry, the state-change emit helper, and the single
//! `register_all` entry point.

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use tauri::{Emitter, Window};

use crate::models::app::AppState;
use crate::services::agents::AgentCache;
use crate::services::config::Settings;

mod editor;
mod fs;
mod git;
mod panes;
mod project;
mod state;
mod tabs;
mod terminal;
mod usage;
mod window;

/// Per-window state registry: each window label owns an independent
/// `AppState` (projects/tabs/sessions/files/diffs) so windows restore and
/// persist separately. Settings stay shared across every window.
pub struct SharedState {
    states: Mutex<HashMap<String, Arc<Mutex<AppState>>>>,
    settings: Arc<Mutex<Settings>>,
    pub agents: Arc<Mutex<AgentCache>>,
    /// Token usage ledger + run tracker. Shared across windows (usage is
    /// machine-wide, not per-window).
    pub usage: Arc<crate::services::usage::UsageStore>,
    /// One-shot UI action queued by a tray menu item while the main window
    /// is being (re)created. The frontend consumes it on mount via
    /// `take_pending_action`, so a tray click can't be lost to the race
    /// between the window build and the webview's event listeners.
    pub pending_action: Arc<Mutex<Option<String>>>,
}

impl SharedState {
    pub fn new(settings: Settings) -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            settings: Arc::new(Mutex::new(settings)),
            agents: Arc::new(Mutex::new(AgentCache::default())),
            usage: Arc::new(crate::services::usage::UsageStore::new()),
            pending_action: Arc::new(Mutex::new(None)),
        }
    }

    /// State for one window, creating an empty one (with shared settings) on
    /// first use. Snapshot restore happens in the explicit restore paths
    /// (bootstrap for "main", `new_window` for the rest), not here. Only the
    /// bootstrap/entry paths should use this; command handlers that merely
    /// query or mutate an existing window's state use `get_label` instead.
    pub fn for_label(&self, label: &str) -> Arc<Mutex<AppState>> {
        self.states
            .lock()
            .entry(label.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(AppState::new(self.settings.clone()))))
            .clone()
    }

    /// State for one window if it is registered. Read-only lookup: unlike
    /// `for_label`, a late invoke from an already-destroyed window gets
    /// `None` instead of resurrecting an empty state nobody will clean up.
    pub fn get_label(&self, label: &str) -> Option<Arc<Mutex<AppState>>> {
        self.states.lock().get(label).cloned()
    }

    /// Shared settings handle (e.g. bootstrap reads the language for the
    /// tray menu before any window state exists).
    pub fn settings(&self) -> Arc<Mutex<Settings>> {
        self.settings.clone()
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
///
/// The view is built under the state lock, but the JSON serialization and
/// IPC enqueue happen after it is released: holding the per-window mutex
/// through `window.emit` would block unrelated commands (PTY metadata,
/// session lookups, ...) for the duration of the serialize.
fn emit_state(window: &Window, state: &Arc<Mutex<AppState>>) {
    let view = state.lock().view();
    let _ = window.emit("state-changed", view);
}

/// Error for fallible commands whose window state no longer exists (a late
/// invoke from an already-destroyed window).
fn unknown_window(label: &str) -> String {
    format!("no state for window '{label}' (already closed)")
}

/// Single source of truth for command registration and TypeScript bindings.
///
/// The runtime uses `invoke_handler()`; the bindings export test uses the same
/// builder with a mock runtime, so a command can't be exposed without being
/// registered (or vice versa).
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    use tauri_specta::{collect_commands, collect_events, Builder};

    Builder::<tauri::Wry>::new()
        // Preserve the pre-migration behavior: a failed command rejects the
        // invoke promise instead of returning a result union.
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // DTO 64-bit values are counts/sizes/epoch-ms that serde already
        // serializes as JSON numbers; BigInt would break existing consumers.
        .dangerously_cast_bigints_to_number()
        // Unified serde mode: one type per DTO instead of separate
        // _Serialize/_Deserialize variants (the event payloads are simple
        // enough that phase differences only add noise).
        .disable_serde_phases()
        .commands(collect_commands![
            state::get_state,
            state::get_settings,
            state::config_path,
            state::take_pending_action,
            state::default_settings,
            state::save_settings,
            state::available_themes,
            state::available_themes_with_info,
            state::theme_colors,
            state::session_info,
            state::list_all_sessions,
            state::agent_status_snapshot,
            state::file_info,
            state::diff_info,
            state::focus_agent_session,
            usage::usage_overview,
            usage::usage_compare,
            usage::usage_runs,
            usage::usage_coverage,
            usage::usage_refresh,
            window::new_window,
            project::new_project,
            project::close_project,
            project::select_project,
            project::select_project_by_index,
            project::select_next_project,
            project::select_previous_project,
            project::move_project,
            project::rename_project,
            project::set_project_directory,
            terminal::list_shells,
            terminal::spawn_session,
            terminal::send_text,
            terminal::terminal_ack,
            terminal::resize_terminal,
            terminal::clear_terminal,
            terminal::restart_session_agent,
            terminal::terminate_session,
            terminal::session_processes,
            terminal::kill_process,
            terminal::session_ports,
            terminal::init_read_loops,
            tabs::close_selected_tab,
            tabs::close_tab,
            tabs::select_tab,
            tabs::select_next_tab,
            tabs::select_previous_tab,
            tabs::move_tab,
            tabs::rename_tab,
            tabs::close_other_tabs,
            tabs::close_tabs_to_right,
            tabs::close_all_tabs,
            tabs::pane_context_path,
            panes::split,
            panes::focus_pane,
            panes::resize_pane,
            panes::resize_pane_divider,
            panes::move_pane,
            panes::move_pane_cross_tab,
            panes::close_pane,
            panes::toggle_pane_zoom,
            panes::equalize_panes,
            panes::toggle_left_sidebar,
            panes::toggle_right_panel,
            panes::toggle_panel,
            editor::open_file,
            editor::open_file_at,
            editor::file_text_changed,
            editor::save_selected_file,
            editor::save_file,
            editor::tab_dirty_files,
            editor::project_dirty_files,
            editor::open_diff,
            editor::open_commit_diff,
            editor::open_workdir_diff,
            editor::open_checkpoint_diff,
            editor::reload_diff,
            fs::list_directory,
            fs::trash_file,
            fs::create_file,
            fs::rename_path,
            fs::watch_directories,
            fs::search_files,
            fs::list_project_files,
            git::git_status,
            git::git_status_revision,
            git::resolve_project_root,
            git::git_stage,
            git::git_stage_all,
            git::git_unstage,
            git::git_unstage_all,
            git::git_guard,
            git::git_discard_guarded,
            git::git_discard_all_guarded,
            git::git_commit,
            git::git_switch_branch,
            git::git_create_branch,
            git::git_fetch,
            git::git_pull,
            git::git_push,
            git::git_stash_all,
            git::git_stash_pop,
            git::git_init,
            git::git_file_history,
            git::git_head_content,
            git::git_blame,
            git::git_head_oid,
            git::git_checkpoint_changes,
            fs::install_explorer_context_menu,
            fs::add_to_path,
            fs::remove_from_path,
            fs::is_on_path,
        ])
        .events(collect_events![
            crate::events::StateChanged,
            crate::events::TabFocused,
            crate::events::FsChanged,
            crate::events::FileSaved,
            crate::events::FileReveal,
            crate::events::PtyData,
            crate::events::PtyExit,
            crate::events::PtyProgress,
            crate::events::SessionCwdChanged,
            crate::events::AgentStatusChanged,
            crate::events::AllAgentStatus,
            crate::events::NotificationActivate,
            crate::events::OpenSettings,
            crate::events::SettingsChanged,
            crate::events::UsageUpdated,
        ])
}

/// Register every command (called from the Tauri Builder).
pub fn register_all(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(specta_builder().invoke_handler())
}
