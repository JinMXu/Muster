//! Token usage views: overview (mini-bar), agent×model comparison, run
//! history, coverage and a manual refresh.
//!
//! The queries attribute every retained event against every run, which is
//! real CPU once months of ledger accumulate — so they run on the blocking
//! pool, never the main thread (Tauri sync commands execute there).

use tauri::State;
use uuid::Uuid;

use super::SharedState;
use crate::services::usage::{AgentCoverage, UsageCompare, UsageOverview, UsageRunView};

#[tauri::command]
#[specta::specta]
pub async fn usage_overview(
    state: State<'_, SharedState>,
    since_ms: i64,
) -> Result<UsageOverview, String> {
    let usage = state.usage.clone();
    tokio::task::spawn_blocking(move || usage.overview(since_ms))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn usage_compare(
    state: State<'_, SharedState>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
    project_id: Option<Uuid>,
) -> Result<UsageCompare, String> {
    let usage = state.usage.clone();
    tokio::task::spawn_blocking(move || usage.compare(since_ms, until_ms, project_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn usage_runs(
    state: State<'_, SharedState>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
    project_id: Option<Uuid>,
    limit: Option<usize>,
) -> Result<Vec<UsageRunView>, String> {
    let usage = state.usage.clone();
    tokio::task::spawn_blocking(move || {
        usage.runs(since_ms, until_ms, project_id, limit.unwrap_or(200))
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn usage_coverage(state: State<'_, SharedState>) -> Result<Vec<AgentCoverage>, String> {
    let usage = state.usage.clone();
    tokio::task::spawn_blocking(move || usage.coverage())
        .await
        .map_err(|e| e.to_string())
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
