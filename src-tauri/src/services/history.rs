use std::path::PathBuf;

/// Stores captured VT scrollback so a relaunch can replay it above the shell.
/// Each file is keyed by its session pane id (UUID). Opt-in via settings.
pub fn store_path(session_id: uuid::Uuid) -> PathBuf {
    super::persist::history_dir().join(format!("{session_id}.vt"))
}

pub fn save_history(session_id: uuid::Uuid, content: &str) -> std::io::Result<()> {
    if content.is_empty() {
        let _ = std::fs::remove_file(store_path(session_id));
        return Ok(());
    }
    let path = store_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

pub fn load_history(session_id: uuid::Uuid) -> Option<String> {
    std::fs::read_to_string(store_path(session_id)).ok()
}

pub fn clear_history(session_id: uuid::Uuid) {
    let _ = std::fs::remove_file(store_path(session_id));
}