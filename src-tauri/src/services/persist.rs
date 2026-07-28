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

pub fn history_dir() -> PathBuf { app_data_dir().join("history") }

use crate::models::project::SessionSnapshot;

pub fn save_snapshot_for(label: &str, snapshot: &SessionSnapshot) -> std::io::Result<()> {
    let path = session_snapshot_path_for(label);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(snapshot).unwrap_or_default();
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_snapshot_for(label: &str) -> Option<SessionSnapshot> {
    let path = session_snapshot_path_for(label);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}
