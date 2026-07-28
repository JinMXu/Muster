//! Filesystem watching for the file tree: the frontend sends the directories
//! it currently displays (root + expanded/loaded dirs) and we emit
//! `fs-changed` when one of them changes on disk, so the tree refreshes
//! without polling.

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
    /// Lazily created on the first `watch_directories` call.
    watcher: Option<RecommendedWatcher>,
    /// Directories currently watched, exactly as the frontend sent them.
    dirs: HashSet<PathBuf>,
}

static WATCH: Lazy<Mutex<WatchSet>> = Lazy::new(|| {
    Mutex::new(WatchSet {
        watcher: None,
        dirs: HashSet::new(),
    })
});

/// Replace the watched set with `paths`. Watching is strictly non-recursive:
/// recursively watching a whole project (node_modules!) is far too
/// expensive, and the tree only ever displays loaded directories anyway.
pub fn set_watched_directories(app: &AppHandle, paths: Vec<String>) {
    let mut wanted: HashSet<PathBuf> = paths
        .into_iter()
        .map(PathBuf::from)
        // Skip dirs that no longer exist — nothing to watch there.
        .filter(|p| p.is_dir())
        .collect();

    let mut watch = WATCH.lock();
    if watch.watcher.is_none() {
        // First call: create the watcher plus the debounce thread that turns
        // raw notify events into throttled `fs-changed` emits.
        let (tx, rx) = channel::<notify::Result<Event>>();
        match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(watcher) => {
                watch.watcher = Some(watcher);
                let app = app.clone();
                std::thread::spawn(move || debounce_loop(app, rx));
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

/// Drain raw notify events, coalesce per directory, and emit `fs-changed`.
fn debounce_loop(app: AppHandle, rx: Receiver<notify::Result<Event>>) {
    // Dirs with unseen events -> last event time.
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                for dir in affected_dirs(&event) {
                    pending.insert(dir, Instant::now());
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
                let mut watch = WATCH.lock();
                watch.dirs.remove(&dir);
                if let Some(w) = watch.watcher.as_mut() {
                    let _ = w.unwatch(&dir);
                }
            }
            let _ = app.emit("fs-changed", serde_json::json!({ "dir": dir.to_string_lossy() }));
        }
    }
}

/// Map an event to the watched directories it touches, skipping dotfile
/// noise (.git churn, editor swap files, ...).
fn affected_dirs(event: &Event) -> Vec<PathBuf> {
    let watch = WATCH.lock();
    let mut out: Vec<PathBuf> = Vec::new();
    for path in &event.paths {
        let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if let Some(parent) = path.parent() {
            let parent = parent.to_path_buf();
            if watch.dirs.contains(&parent) && !out.contains(&parent) {
                out.push(parent);
            }
        }
    }
    out
}
