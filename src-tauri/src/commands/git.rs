//! Git commands - thin pass-throughs to `services::git`, wrapped in
//! `spawn_blocking` so repo I/O doesn't starve Tauri's async runtime.

#[tauri::command]
pub async fn git_status(repo_root: String) -> crate::services::git::GitStatusInfo {
    tokio::task::spawn_blocking(move || crate::services::git::status(&repo_root))
        .await
        .unwrap_or_else(|_| crate::services::git::GitStatusInfo::default())
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
pub async fn git_stage(repo_root: String, path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::stage(&repo_root, &path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stage_all(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::stage_all(&repo_root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_unstage(repo_root: String, path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::unstage(&repo_root, &path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_unstage_all(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::unstage_all(&repo_root))
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
    repo_root: String,
    path: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::services::git::discard_guarded(&repo_root, &path, &guard))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_discard_all_guarded(
    repo_root: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::services::git::discard_all_guarded(&repo_root, &guard))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_commit(repo_root: String, message: String, include_all: bool, amend: bool) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::services::git::commit(&repo_root, &message, include_all, amend).map(|oid| oid.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_switch_branch(repo_root: String, name: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::switch_branch(&repo_root, &name))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_create_branch(repo_root: String, name: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::create_branch(&repo_root, &name))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_fetch(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::fetch(&repo_root))
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
pub async fn git_push(repo_root: String, remote: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::push(&repo_root, &remote))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stash_all(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::stash_all(&repo_root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stash_pop(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::stash_pop(&repo_root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_init(repo_root: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::services::git::init(&repo_root))
        .await
        .map_err(|e| e.to_string())?
}
