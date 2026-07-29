use std::sync::Arc;
use parking_lot::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use uuid::Uuid;

use crate::commands::SharedState;
use crate::models::app::AppState;
use crate::models::session::TerminalSession;
use crate::services::config::Settings;

pub fn run() {
    let shared = SharedState::new(Settings::load());

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Open any directories passed in argv as new projects in the
            // running instance's main window (focus follows the OS launch,
            // and the main window is the only one guaranteed to exist).
            let state = app.state::<SharedState>();
            let main = state.for_label("main");
            for arg in argv.iter().skip(1) {
                if std::path::Path::new(arg).is_dir() {
                    main.lock().new_project(Some(arg.to_string()));
                    // Bring up the PTY for the session the project just created.
                    spawn_pending(app, "main", &main);
                    if let Some(window) = app.get_webview_window("main") {
                        let view = main.lock().view();
                        let _ = window.emit("state-changed", view);
                    }
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                // The main window may be hidden (tray keep-alive); bring it back.
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(shared)
        .setup(|app| {
            let _ = env_logger::try_init();

            // Register the bundled fonts (JetBrains Mono + Nerd Font symbols)
            // as process-private fonts so the terminal/editor resolve them
            // even when they aren't installed system-wide.
            crate::theme::fonts::register_fonts();

            // Drop orphaned secondary-window snapshots left by earlier runs
            // (random labels make them unrestorable; none can be live yet).
            crate::services::persist::prune_secondary_snapshots();

            // System tray with keep-alive: closing the last window only hides
            // it; the process lives on here until "Quit".
            let new_window_item = MenuItem::with_id(app, "tray-new-window", "New Window", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&new_window_item, &quit_item])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Muster")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "tray-new-window" => {
                        let _ = spawn_window(app);
                    }
                    "tray-quit" => {
                        save_all_snapshots(app);
                        terminate_all_sessions(app);
                        app.exit(0);
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let _tray = tray.build(app)?;

            // Restore the main window's previous session (projects/tabs/
            // splits and the sidebar layout) from its saved snapshot.
            // Secondary windows restore when reopened via the new_window
            // command.
            let state = app.state::<SharedState>();
            let main = state.for_label("main");
            if let Some(snapshot) = crate::services::persist::load_snapshot_for("main") {
                main.lock().restore(&snapshot);
            }

            // If nothing was restored, open a starter project + terminal.
            if main.lock().projects.is_empty() {
                main.lock().new_project(None);
            }

            spawn_pending(app.handle(), "main", &main);

            if let Some(window) = app.get_webview_window("main") {
                let view = main.lock().view();
                let _ = window.emit("state-changed", view);
            }

            // Periodic autosave of every window's layout, so a crash doesn't
            // lose it (the close handler also saves, but only on a clean
            // exit).
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let state = handle.state::<SharedState>();
                for (label, s) in state.all() {
                    let snapshot = {
                        let g = s.lock();
                        build_snapshot(&g)
                    };
                    let _ = crate::services::persist::save_snapshot_for(&label, &snapshot);
                }
            });

            // Usage tracking: background scan loop.
            {
                let handle = app.handle().clone();
                let usage_cache = handle.state::<crate::commands::SharedState>().usage.clone();
                crate::services::usage::spawn_scan_loop(handle, usage_cache);
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Save and tear down only THIS window's state; other windows
                // keep running.
                let label = window.label().to_string();
                let state = window.state::<SharedState>();
                save_window_snapshot(&state, &label);
                // Keep-alive: closing the LAST window only hides it — the
                // process stays in the tray and its sessions keep running.
                // "New Window" from the tray reopens; "Quit" exits properly.
                if window.app_handle().webview_windows().len() <= 1 {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                terminate_window_sessions(&state, &label);
            }
            // Belt-and-braces cleanup: a destroyed window must not leak its
            // sessions or registry entry, even if CloseRequested never fired.
            WindowEvent::Destroyed => {
                let label = window.label().to_string();
                let state = window.state::<SharedState>();
                if let Some(s) = state.get_label(&label) {
                    for session in s.lock().sessions.values() {
                        session.terminate();
                    }
                }
                state.remove_label(&label);
                // Drop this window's file-tree watcher (its debounce thread
                // exits once the channel disconnects).
                crate::services::watch::remove_label(&label);
                // Secondary windows have random, per-launch labels, so their
                // snapshot can never be restored — delete it rather than let
                // it accumulate in the app data dir. The main window's
                // snapshot must always survive.
                if label != "main" {
                    let _ = crate::services::persist::delete_snapshot_for(&label);
                }
            }
            _ => {}
        });

    let builder = crate::commands::register_all(builder);

    builder.run(tauri::generate_context!()).expect("error while running muster");
}

/// Open a new window with its own independent projects/tabs/sessions. The
/// state is registered (and restored from the window's own snapshot, if any)
/// BEFORE the window is built, so the webview's first `get_state` invoke
/// already sees the restored layout. Shared by the `new_window` command and
/// the tray menu's "New Window".
pub fn spawn_window(app: &AppHandle) -> Result<(), String> {
    let label = format!("win-{}", &Uuid::new_v4().simple().to_string()[..8]);

    let state = app.state::<SharedState>();
    let s = state.for_label(&label);
    {
        let mut g = s.lock();
        if g.projects.is_empty() {
            if let Some(snapshot) = crate::services::persist::load_snapshot_for(&label) {
                g.restore(&snapshot);
            }
        }
        // A fresh window gets the same starter project + terminal as first launch.
        if g.projects.is_empty() {
            g.new_project(None);
        }
    }
    spawn_pending(app, &label, &s);

    // Mirror the chrome settings of the "main" window in tauri.conf.json.
    WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Muster")
        .inner_size(1000.0, 680.0)
        .min_inner_size(720.0, 480.0)
        .decorations(false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Save this window's layout snapshot (used by the close handler and by the
/// tray's Quit action). No-op if the window's state is already gone.
pub fn save_window_snapshot(state: &SharedState, label: &str) {
    let Some(s) = state.get_label(label) else { return };
    let snapshot = {
        let g = s.lock();
        build_snapshot(&g)
    };
    let _ = crate::services::persist::save_snapshot_for(label, &snapshot);
}

/// Terminate any surviving sessions of `label`'s window so the parent
/// process can exit cleanly.
pub fn terminate_window_sessions(state: &SharedState, label: &str) {
    let Some(s) = state.get_label(label) else { return };
    let g = s.lock();
    for project in &g.projects {
        for sid in project.session_ids() {
            if let Some(session) = g.sessions.get(&sid) {
                session.terminate();
            }
        }
    }
}

/// Save every live window's snapshot (tray Quit, where the per-window close
/// handler never runs).
fn save_all_snapshots(app: &AppHandle) {
    let state = app.state::<SharedState>();
    for (label, s) in state.all() {
        let snapshot = {
            let g = s.lock();
            build_snapshot(&g)
        };
        let _ = crate::services::persist::save_snapshot_for(&label, &snapshot);
    }
}

/// Terminate every session of every window (tray Quit).
fn terminate_all_sessions(app: &AppHandle) {
    let state = app.state::<SharedState>();
    for (_, s) in state.all() {
        for session in s.lock().sessions.values() {
            session.terminate();
        }
    }
}

/// Spawn PTYs for every session of `label`'s window that doesn't have one
/// yet. Restored sessions (and the starter session) are created without a
/// PTY; commands (spawn_session / split) spawn their own. The read loops
/// are scoped to `label` so output lands in the owning window only.
pub fn spawn_pending(app: &AppHandle, label: &str, state: &Arc<Mutex<AppState>>) {
    let pending: Vec<Arc<TerminalSession>> = state
        .lock()
        .sessions
        .values()
        .filter(|s| !s.is_spawned())
        .cloned()
        .collect();
    for session in pending {
        let _ = session.spawn(80, 24);
        session.attach_read_loop(app.clone(), label.to_string());
    }
}

fn build_snapshot(state: &AppState) -> crate::models::project::SessionSnapshot {
    use crate::models::pane::PaneContent;
    use crate::models::project::PaneContentSnapshot;
    use crate::models::project::{ColumnSnapshot, PaneSnapshot, ProjectSnapshot, SessionSnapshot, TabSnapshot};

    let projects = state
        .projects
        .iter()
        .map(|p| {
            ProjectSnapshot {
                custom_name: p.custom_name.clone(),
                custom_directory: p.custom_directory.clone(),
                tabs: p
                    .tabs
                    .iter()
                    .map(|t| {
                        let (fc, fr) = t.focused_location().unwrap_or((0, 0));
                        TabSnapshot {
                            columns: t
                                .columns
                                .iter()
                                .map(|col| ColumnSnapshot {
                                    panes: col
                                        .panes
                                        .iter()
                                        .map(|pane| PaneSnapshot {
                                            content: match &pane.content {
                                                PaneContent::Session(id) => PaneContentSnapshot::Session {
                                                    working_directory: state
                                                        .sessions
                                                        .get(id)
                                                        .map(|s| s.current_directory())
                                                        .unwrap_or_default(),
                                                },
                                                PaneContent::File(id) => PaneContentSnapshot::File {
                                                    path: state.files.get(id).map(|f| f.path()).unwrap_or_default(),
                                                },
                                                PaneContent::Diff(id) => {
                                                    let d = state.diffs.get(id);
                                                    PaneContentSnapshot::Diff {
                                                        repo_root: d.map(|x| x.repo_root.clone()).unwrap_or_default(),
                                                        path: d.map(|x| x.path.clone()).unwrap_or_default(),
                                                        staged: d.map(|x| x.staged).unwrap_or(false),
                                                    }
                                                }
                                            },
                                            weight: pane.weight,
                                        })
                                        .collect(),
                                    weight: col.weight,
                                })
                                .collect(),
                            focused_column: fc,
                            focused_row: fr,
                            custom_name: t.custom_name.clone(),
                        }
                    })
                    .collect(),
                selected_tab_index: p.selected_tab_id.and_then(|id| p.tabs.iter().position(|t| t.id == id)),
            }
        })
        .collect();

    SessionSnapshot {
        projects,
        selected_project_index: state
            .selected_project_id
            .and_then(|id| state.projects.iter().position(|p| p.id == id)),
        is_left_sidebar_visible: Some(state.is_left_sidebar_visible),
        is_right_panel_visible: Some(state.is_panel_visible),
        right_panel_tab: Some(state.panel_tab.clone()),
    }
}