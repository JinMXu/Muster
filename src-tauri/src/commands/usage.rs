//! Usage tracking commands.

use tauri::State;

use super::SharedState;
use crate::services::usage;

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
