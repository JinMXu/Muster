//! Token usage views: overview (mini-bar), agent×model comparison, run
//! history, coverage and a manual refresh.

use tauri::State;
use uuid::Uuid;

use super::SharedState;
use crate::services::usage::{AgentCoverage, UsageCompare, UsageOverview, UsageRunView};

#[tauri::command]
#[specta::specta]
pub fn usage_overview(state: State<SharedState>, since_ms: i64) -> UsageOverview {
    state.usage.overview(since_ms)
}

#[tauri::command]
#[specta::specta]
pub fn usage_compare(
    state: State<SharedState>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
    project_id: Option<Uuid>,
) -> UsageCompare {
    state.usage.compare(since_ms, until_ms, project_id)
}

#[tauri::command]
#[specta::specta]
pub fn usage_runs(
    state: State<SharedState>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
    project_id: Option<Uuid>,
    limit: Option<usize>,
) -> Vec<UsageRunView> {
    state
        .usage
        .runs(since_ms, until_ms, project_id, limit.unwrap_or(200))
}

#[tauri::command]
#[specta::specta]
pub fn usage_coverage(state: State<SharedState>) -> Vec<AgentCoverage> {
    state.usage.coverage()
}

/// Force a scan. The normal scanner runs every 10s; this backs the manual
/// refresh button.
#[tauri::command]
#[specta::specta]
pub async fn usage_refresh(state: State<'_, SharedState>) -> Result<u64, String> {
    let usage = state.usage.clone();
    tokio::task::spawn_blocking(move || usage.scan().0)
        .await
        .map_err(|e| e.to_string())
}
