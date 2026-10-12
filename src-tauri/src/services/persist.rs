use std::path::PathBuf;

/// Snapshot store locations under the user's per-app data directory.
pub fn app_data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| dirs::config_dir().unwrap_or_default());
    if cfg!(debug_assertions) {
        base.join("muster-dev")
    } else {
        base.join("muster")
    }
}

/// Each window persists its own layout. "main" keeps the original
/// `sessions.json` filename for back-compat with existing installs; every
/// other window gets `sessions-<label>.json`.
pub fn session_snapshot_path_for(label: &str) -> PathBuf {
    if label == "main" {
        app_data_dir().join("sessions.json")
    } else {
        app_data_dir().join(format!("sessions-{label}.json"))
    }
}

use crate::models::project::SessionSnapshot;

/// How many previous versions of each snapshot to keep. Every save shifts the
/// old files down one slot (`.1` is the immediately-previous layout), so a
/// bad or corrupted write never destroys the only copy of the user's layout.
const SNAPSHOT_BACKUPS: usize = 10;

fn snapshot_backup_path(path: &std::path::Path, n: usize) -> PathBuf {
    path.with_extension(format!("json.{n}"))
}

/// Shift the backup chain down and copy the current snapshot into `.1`.
/// Best-effort: a failed shift leaves a gap the next rotation heals; a failed
/// copy just means this generation has no backup.
fn rotate_snapshot_backups(path: &std::path::Path) {
    for n in (1..SNAPSHOT_BACKUPS).rev() {
        let from = snapshot_backup_path(path, n);
        let to = snapshot_backup_path(path, n + 1);
        let _ = std::fs::rename(&from, &to);
    }
    let _ = std::fs::copy(path, snapshot_backup_path(path, 1));
}

pub fn save_snapshot_for(label: &str, snapshot: &SessionSnapshot) -> std::io::Result<()> {
    let path = session_snapshot_path_for(label);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Propagate serialization failures: writing an empty string would
    // clobber the existing snapshot with an unusable file.
    // Compact (non-pretty) serialization: the file is only read back by the
    // app, and the autosave loop writes it every few seconds.
    let json = serde_json::to_string(snapshot).map_err(std::io::Error::other)?;
    // Rotate before the rename: `.1` ends up holding the content being
    // replaced, and a crash anywhere below still leaves the live file intact.
    if path.exists() {
        rotate_snapshot_backups(&path);
    }
    // Write to a temp file and rename into place so a crash mid-write can't
    // leave a truncated snapshot behind (the backup fallback in `load` is the
    // next line of defense). `std::fs::rename` replaces the destination on
    // Windows.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn load_snapshot_for(label: &str) -> Option<SessionSnapshot> {
    let path = session_snapshot_path_for(label);
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<SessionSnapshot>(&text) {
        Ok(s) => Some(s),
        Err(e) => {
            // Corrupt snapshot: back it up (like config.rs does) so the user
            // can recover manually, then fall back to the newest rotation
            // backup that still parses, instead of silently losing the
            // layout.
            log::warn!("corrupt snapshot {}: {e}", path.display());
            let bak = path.with_extension("json.bak");
            let _ = std::fs::rename(&path, &bak);
            for n in 1..=SNAPSHOT_BACKUPS {
                let backup = snapshot_backup_path(&path, n);
                let Ok(text) = std::fs::read_to_string(&backup) else { continue };
                match serde_json::from_str::<SessionSnapshot>(&text) {
                    Ok(s) => {
                        log::info!("recovered snapshot {} from backup {}", path.display(), backup.display());
                        return Some(s);
                    }
                    Err(e) => log::warn!("backup {} also unusable: {e}", backup.display()),
                }
            }
            None
        }
    }
}

/// Delete a window's snapshot file (and its rotation backups). A missing
/// file is not an error (the window may never have been autosaved).
pub fn delete_snapshot_for(label: &str) -> std::io::Result<()> {
    let path = session_snapshot_path_for(label);
    for n in 1..=SNAPSHOT_BACKUPS {
        let _ = std::fs::remove_file(snapshot_backup_path(&path, n));
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Remove leftover random-label secondary-window snapshots from older
/// versions that used `win-<uuid8>` labels. Deterministic `win-N` labels
/// (introduced for cross-relaunch restore) are preserved so they can be
/// restored at startup. Best-effort: failures are logged, never propagated.
/// The main window's `sessions.json` is never touched.
pub fn prune_secondary_snapshots() {
    let Ok(entries) = std::fs::read_dir(app_data_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(rest) = name.strip_prefix("sessions-win-") {
            if let Some(stem) = rest.strip_suffix(".json") {
                // Keep deterministic numeric labels (win-1, win-2, …) — they
                // are restorable. Only prune non-numeric (old random) labels.
                if stem.parse::<usize>().is_ok() {
                    continue;
                }
                if let Err(e) = std::fs::remove_file(entry.path()) {
                    log::warn!("failed to remove orphan snapshot {}: {e}", entry.path().display());
                }
            }
        }
    }
}

/// Enumerate every deterministic secondary-window snapshot label found on
/// disk, sorted by numeric suffix. Used at startup to restore windows in
/// creation order.
pub fn list_secondary_labels() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(app_data_dir()) else { return Vec::new() };
    let mut nums: Vec<usize> = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if let Some(rest) = name.strip_prefix("sessions-win-") {
                if let Some(stem) = rest.strip_suffix(".json") {
                    if let Ok(n) = stem.parse::<usize>() {
                        nums.push(n);
                    }
                }
            }
        }
    }
    nums.sort_unstable();
    nums.into_iter().map(|n| format!("win-{n}")).collect()
}

#[cfg(test)]
mod tests {
    // The tests below all operate on the real app data directory and prune /
    // write / delete overlapping snapshot filenames. Serialize them so they
    // can't race each other (previously win-3 flaked: one test deleted the
    // other's file mid-run).
    static DIR_LOCK: once_cell::sync::Lazy<parking_lot::Mutex<()>> =
        once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(()));

    #[test]
    fn delete_snapshot_removes_file_and_is_idempotent() {
        let _guard = DIR_LOCK.lock();
        // No "win-" prefix: the prune test runs in parallel and would
        // delete a matching file out from under this test.
        let label = "testdelete";
        let path = super::session_snapshot_path_for(label);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{}").unwrap();
        assert!(path.exists());
        super::delete_snapshot_for(label).unwrap();
        assert!(!path.exists());
        // Deleting a missing snapshot is not an error.
        super::delete_snapshot_for(label).unwrap();
    }

    #[test]
    fn prune_removes_secondary_but_keeps_main() {
        let _guard = DIR_LOCK.lock();
        let dir = super::app_data_dir();
        std::fs::create_dir_all(&dir).unwrap();

        let orphan = dir.join("sessions-win-testprune.json");
        std::fs::write(&orphan, b"{}").unwrap();

        // Preserve a real main snapshot if one exists; only stand one up
        // temporarily when it doesn't.
        let main = dir.join("sessions.json");
        let main_existed = main.exists();
        if !main_existed {
            std::fs::write(&main, b"{}").unwrap();
        }

        super::prune_secondary_snapshots();

        assert!(!orphan.exists(), "orphan win-* snapshot should be pruned");
        assert!(main.exists(), "main snapshot must survive pruning");
        if !main_existed {
            std::fs::remove_file(&main).unwrap();
        }
    }

    #[test]
    fn prune_keeps_numeric_secondary_snapshots() {
        let _guard = DIR_LOCK.lock();
        let dir = super::app_data_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // Numeric-label snapshots (win-1, win-2, …) are restorable and must
        // survive pruning.
        let numeric = dir.join("sessions-win-3.json");
        std::fs::write(&numeric, b"{}").unwrap();
        // Non-numeric (old random UUID) snapshots must be pruned.
        let random = dir.join("sessions-win-abcd1234.json");
        std::fs::write(&random, b"{}").unwrap();

        super::prune_secondary_snapshots();

        assert!(numeric.exists(), "numeric-label snapshot should survive pruning");
        assert!(!random.exists(), "random-label snapshot should be pruned");

        let _ = std::fs::remove_file(&numeric);
    }

    #[test]
    fn list_secondary_labels_sorted() {
        let _guard = DIR_LOCK.lock();
        let dir = super::app_data_dir();
        std::fs::create_dir_all(&dir).unwrap();
        for n in [3, 1, 12] {
            std::fs::write(dir.join(format!("sessions-win-{n}.json")), b"{}").unwrap();
        }
        let labels = super::list_secondary_labels();
        assert_eq!(labels, vec!["win-1".to_string(), "win-3".to_string(), "win-12".to_string()]);
        for n in [1, 3, 12] {
            let _ = std::fs::remove_file(dir.join(format!("sessions-win-{n}.json")));
        }
    }

    fn empty_snapshot_json() -> String {
        serde_json::to_string(&super::SessionSnapshot {
            projects: Vec::new(),
            selected_project_index: None,
            is_left_sidebar_visible: None,
            is_right_panel_visible: None,
            right_panel_tab: None,
        })
        .unwrap()
    }

    #[test]
    fn save_rotates_previous_snapshot_into_backups() {
        let _guard = DIR_LOCK.lock();
        let label = "testrotate";
        let path = super::session_snapshot_path_for(label);
        let _ = std::fs::remove_file(&path);
        let snapshot: super::SessionSnapshot = serde_json::from_str(&empty_snapshot_json()).unwrap();

        // First save: no previous file, so no backup should appear.
        super::save_snapshot_for(label, &snapshot).unwrap();
        assert!(!super::snapshot_backup_path(&path, 1).exists());

        // Second save: the first save's content moves into `.1`.
        super::save_snapshot_for(label, &snapshot).unwrap();
        assert!(super::snapshot_backup_path(&path, 1).exists());

        super::delete_snapshot_for(label).unwrap();
        assert!(!super::snapshot_backup_path(&path, 1).exists(), "delete must remove backups");
    }

    #[test]
    fn load_falls_back_to_newest_parsable_backup() {
        let _guard = DIR_LOCK.lock();
        let label = "testrecover";
        let path = super::session_snapshot_path_for(label);
        let _ = std::fs::remove_file(&path);
        let good = empty_snapshot_json();

        // Seed `.1` with a valid snapshot, then corrupt the live file.
        std::fs::write(super::snapshot_backup_path(&path, 1), &good).unwrap();
        std::fs::write(&path, b"{not json").unwrap();

        let restored = super::load_snapshot_for(label).expect("backup should be recovered");
        assert!(restored.projects.is_empty());

        // `load` renamed the corrupt file to .bak before recovering; clean up.
        let _ = std::fs::remove_file(path.with_extension("json.bak"));
        super::delete_snapshot_for(label).unwrap();
    }
}
