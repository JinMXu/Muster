//! Filesystem watching for the file tree: the frontend sends the directories
//! it currently displays (root + expanded/loaded dirs) and we emit
//! `fs-changed` when one of them changes on disk, so the tree refreshes
//! without polling.
//!
//! Watch sets are per window (like `SharedState`): each window label owns an
//! independent watcher, and `fs-changed` is emitted to that window only, so
//! two windows never overwrite each other's directories or receive events
//! for a tree they don't display.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

/// Editors often write several events per save; coalesce them into one
/// `fs-changed` per directory within this window.
const DEBOUNCE: Duration = Duration::from_millis(300);

struct WatchSet {
    /// Lazily created on the first `watch_directories` call for the label.
    watcher: Option<RecommendedWatcher>,
    /// Directories currently watched, exactly as the frontend sent them.
    dirs: HashSet<PathBuf>,
}

/// Per-window watch sets, keyed by window label.
static WATCH: Lazy<Mutex<HashMap<String, WatchSet>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Replace the watched set for window `label` with `paths`. Watching is
/// strictly non-recursive: recursively watching a whole project
/// (node_modules!) is far too expensive, and the tree only ever displays
/// loaded directories anyway.
pub fn set_watched_directories(app: &AppHandle, label: &str, paths: Vec<String>) {
    let mut wanted: HashSet<PathBuf> = paths
        .into_iter()
        .map(PathBuf::from)
        // Skip dirs that no longer exist — nothing to watch there.
        .filter(|p| p.is_dir())
        .collect();

    let mut all = WATCH.lock();
    let watch = all.entry(label.to_string()).or_insert_with(|| WatchSet {
        watcher: None,
        dirs: HashSet::new(),
    });
    if watch.watcher.is_none() {
        // First call for this window: create the watcher plus the debounce
        // thread that turns raw notify events into throttled `fs-changed`
        // emits scoped to this window's label.
        let (tx, rx) = channel::<notify::Result<Event>>();
        match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(watcher) => {
                watch.watcher = Some(watcher);
                let app = app.clone();
                let label = label.to_string();
                std::thread::spawn(move || debounce_loop(app, label, rx));
            }
            Err(e) => {
                log::error!("failed to create fs watcher: {e}");
                return;
            }
        }
    }
    // Take the current set out of the guard so the watcher borrow below
    // doesn't conflict with reads of `dirs`.
    let current = std::mem::take(&mut watch.dirs);
    let Some(watcher) = watch.watcher.as_mut() else {
        watch.dirs = current;
        return;
    };

    for dir in current.difference(&wanted) {
        let _ = watcher.unwatch(dir);
    }
    let mut failed = Vec::new();
    for dir in wanted.difference(&current) {
        if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            log::warn!("cannot watch {}: {e}", dir.display());
            failed.push(dir.clone());
        }
    }
    for dir in failed {
        wanted.remove(&dir);
    }
    watch.dirs = wanted;
}

/// Drop a closed window's watch set (called from the window-destroyed
/// cleanup path in bootstrap). Dropping the watcher disconnects the debounce
/// thread's channel, so the thread exits on its own.
pub fn remove_label(label: &str) {
    WATCH.lock().remove(label);
}

/// Drain raw notify events for one window, coalesce per directory, and emit
/// `fs-changed` to that window only.
fn debounce_loop(app: AppHandle, label: String, rx: Receiver<notify::Result<Event>>) {
    // Dirs with unseen events -> last event time.
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                let all = WATCH.lock();
                if let Some(watch) = all.get(&label) {
                    for dir in affected_dirs(&watch.dirs, &event) {
                        pending.insert(dir, Instant::now());
                    }
                }
            }
            Ok(Err(e)) => log::warn!("fs watch error: {e}"),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        // Emit dirs that have been quiet for the debounce window.
        let now = Instant::now();
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, t)| now.duration_since(**t) >= DEBOUNCE)
            .map(|(d, _)| d.clone())
            .collect();
        for dir in ready {
            pending.remove(&dir);
            if !dir.is_dir() {
                // The watched dir vanished (deleted/renamed): drop it so we
                // stop watching — and stop emitting for — a dead path.
                let mut all = WATCH.lock();
                if let Some(watch) = all.get_mut(&label) {
                    watch.dirs.remove(&dir);
                    if let Some(w) = watch.watcher.as_mut() {
                        let _ = w.unwatch(&dir);
                    }
                }
            }
            let _ = app.emit_to(&label, "fs-changed", serde_json::json!({ "dir": dir.to_string_lossy() }));
        }
    }
}

/// Map an event to the watched directories it touches, skipping dotfile
/// noise (.git churn, editor swap files, ...).
fn affected_dirs(watched: &HashSet<PathBuf>, event: &Event) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in &event.paths {
        let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if let Some(parent) = path.parent() {
            let parent = parent.to_path_buf();
            if watched.contains(&parent) && !out.contains(&parent) {
                out.push(parent);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{EventKind, ModifyKind};

    fn modify_event(paths: &[&str]) -> Event {
        let mut e = Event::new(EventKind::Modify(ModifyKind::Any));
        for p in paths {
            e = e.add_path(PathBuf::from(p));
        }
        e
    }

    #[test]
    fn affected_dirs_matches_parent_and_skips_dotfiles() {
        let watched: HashSet<PathBuf> = [PathBuf::from("project/src"), PathBuf::from("project")]
            .into_iter()
            .collect();

        let e = modify_event(&["project/src/main.rs", "project/src/.main.rs.swp", ".gitignore"]);
        let dirs = affected_dirs(&watched, &e);

        // The swap file is dotfile noise; `.gitignore`'s parent is not watched.
        assert_eq!(dirs, vec![PathBuf::from("project/src")]);

        // Multiple files in the same dir coalesce to one entry.
        let e = modify_event(&["project/a.txt", "project/b.txt"]);
        assert_eq!(affected_dirs(&watched, &e), vec![PathBuf::from("project")]);

        // Nothing watched -> nothing reported.
        let e = modify_event(&["elsewhere/x.txt"]);
        assert!(affected_dirs(&watched, &e).is_empty());
    }

    #[test]
    fn watch_sets_are_per_label_and_remove_label_is_scoped() {
        let a = format!("test-a-{}", uuid::Uuid::new_v4());
        let b = format!("test-b-{}", uuid::Uuid::new_v4());

        {
            let mut all = WATCH.lock();
            all.insert(a.clone(), WatchSet { watcher: None, dirs: HashSet::from([PathBuf::from("/a")]) });
            all.insert(b.clone(), WatchSet { watcher: None, dirs: HashSet::from([PathBuf::from("/b")]) });
        }

        remove_label(&a);

        {
            let all = WATCH.lock();
            assert!(!all.contains_key(&a), "label a is gone");
            assert!(all.contains_key(&b), "label b is untouched");
        }

        // Removing an unknown label is a no-op.
        remove_label("never-registered");
        remove_label(&b);
    }
}
