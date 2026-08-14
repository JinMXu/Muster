//! Git commands - thin pass-throughs to `services::git`, wrapped in
//! `spawn_blocking` so repo I/O doesn't starve Tauri's async runtime.

use std::collections::HashMap;
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tauri::State;

use super::SharedState;
use crate::services::i18n::effective;

/// Per-root serialization for `git_status`. A cold-cache scan of a big repo
/// can take minutes — without this gate, concurrent polls of the same repo
/// pile up on the blocking pool and crowd out everything else. Later callers
/// queue behind the in-flight one and then re-check the TTL cache (which the
/// first caller just refreshed), so a queued call returns the fresh cached
/// result instead of running a second full scan back to back.
static STATUS_LOCKS: Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
pub async fn git_status(state: State<'_, SharedState>, repo_root: String) -> Result<crate::services::git::GitStatusInfo, String> {
    let lang = effective(&state.settings.lock().language);
    let lock = {
        STATUS_LOCKS
            .lock()
            .entry(repo_root.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    tokio::task::spawn_blocking(move || {
        let _guard = lock.lock();
        // Double-check after acquiring the lock: while this call was queued,
        // the holder may have just recomputed, so its fresh cache entry
        // serves this call too.
        if let Some(info) = crate::services::git::cached_status(&repo_root) {
            return info;
        }
        crate::services::git::status(&repo_root, &lang)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Anchor for the right panels: the toplevel of the repo containing `cwd`,
/// or `cwd` unchanged when no repository is found upwards.
#[tauri::command]
pub async fn resolve_project_root(cwd: String) -> String {
    let cwd_fallback = cwd.clone();
    tokio::task::spawn_blocking(move || crate::services::git::project_root(&cwd))
        .await
        .unwrap_or(cwd_fallback)
}

#[tauri::command]
pub async fn git_stage(state: State<'_, SharedState>, repo_root: String, path: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::stage(&repo_root, &path, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stage_all(state: State<'_, SharedState>, repo_root: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::stage_all(&repo_root, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_unstage(state: State<'_, SharedState>, repo_root: String, path: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::unstage(&repo_root, &path, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_unstage_all(state: State<'_, SharedState>, repo_root: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::unstage_all(&repo_root, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_guard(repo_root: String, paths: Vec<String>) -> crate::services::git::GitGuard {
    tokio::task::spawn_blocking(move || crate::services::git::guard(&repo_root, &paths))
        .await
        .unwrap_or_else(|_| crate::services::git::GitGuard::default())
}

#[tauri::command]
pub async fn git_discard_guarded(
    state: State<'_, SharedState>,
    repo_root: String,
    path: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::discard_guarded(&repo_root, &path, &guard, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_discard_all_guarded(
    state: State<'_, SharedState>,
    repo_root: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::discard_all_guarded(&repo_root, &guard, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_commit(state: State<'_, SharedState>, repo_root: String, message: String, include_all: bool, amend: bool) -> Result<String, String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::commit(&repo_root, &message, include_all, amend, &lang).map(|oid| oid.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_switch_branch(state: State<'_, SharedState>, repo_root: String, name: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::switch_branch(&repo_root, &name, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_create_branch(state: State<'_, SharedState>, repo_root: String, name: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::create_branch(&repo_root, &name, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_fetch(state: State<'_, SharedState>, repo_root: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::fetch(&repo_root, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_pull(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::pull(&repo_root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_push(state: State<'_, SharedState>, repo_root: String, remote: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::push(&repo_root, &remote, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stash_all(state: State<'_, SharedState>, repo_root: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::stash_all(&repo_root, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stash_pop(state: State<'_, SharedState>, repo_root: String) -> Result<(), String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::stash_pop(&repo_root, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_init(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::init(&repo_root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_file_history(state: State<'_, SharedState>, repo_root: String, path: String) -> Result<Vec<crate::services::git::FileCommit>, String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::file_history(&repo_root, &path, &lang))
        .await
        .map_err(|e| e.to_string())
}

/// HEAD content of one file, or `None` when it isn't tracked at HEAD — used
/// by the editor's inline diff gutter (old side vs the live buffer).
#[tauri::command]
pub async fn git_head_content(state: State<'_, SharedState>, repo_root: String, path: String) -> Result<Option<String>, String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::file_at_head(&repo_root, &path, &lang))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_blame(
    state: State<'_, SharedState>,
    repo_root: String,
    path: String,
) -> Result<Vec<crate::services::git::BlameLine>, String> {
    let lang = effective(&state.settings.lock().language);
    tokio::task::spawn_blocking(move || crate::services::git::blame(&repo_root, &path, &lang))
        .await
        .map_err(|e| e.to_string())?
}

/// Current HEAD oid — the anchor for a "checkpoint".
#[tauri::command]
pub async fn git_head_oid(repo_root: String) -> Option<String> {
    tokio::task::spawn_blocking(move || crate::services::git::head_oid(&repo_root))
        .await
        .ok()
        .flatten()
}

/// Repo-relative paths changed since `checkpoint` (worktree + index + commits
/// made after it).
#[tauri::command]
pub async fn git_checkpoint_changes(repo_root: String, checkpoint: String) -> Vec<String> {
    tokio::task::spawn_blocking(move || crate::services::git::checkpoint_changed_files(&repo_root, &checkpoint))
        .await
        .unwrap_or_default()
}
