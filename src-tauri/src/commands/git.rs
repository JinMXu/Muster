//! Git commands — thin pass-throughs to `services::git`.

#[tauri::command]
pub fn git_status(repo_root: String) -> crate::services::git::GitStatusInfo {
    crate::services::git::status(&repo_root)
}

/// Anchor for the right panels: the toplevel of the repo containing `cwd`,
/// or `cwd` unchanged when no repository is found upwards.
#[tauri::command]
pub fn resolve_project_root(cwd: String) -> String {
    crate::services::git::project_root(&cwd)
}

#[tauri::command]
pub fn git_stage(repo_root: String, path: String) -> Result<(), String> {
    crate::services::git::stage(&repo_root, &path)
}

#[tauri::command]
pub fn git_stage_all(repo_root: String) -> Result<(), String> {
    crate::services::git::stage_all(&repo_root)
}

#[tauri::command]
pub fn git_unstage(repo_root: String, path: String) -> Result<(), String> {
    crate::services::git::unstage(&repo_root, &path)
}

#[tauri::command]
pub fn git_unstage_all(repo_root: String) -> Result<(), String> {
    crate::services::git::unstage_all(&repo_root)
}

#[tauri::command]
pub fn git_guard(repo_root: String, paths: Vec<String>) -> crate::services::git::GitGuard {
    crate::services::git::guard(&repo_root, &paths)
}

#[tauri::command]
pub fn git_discard_guarded(
    repo_root: String,
    path: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    crate::services::git::discard_guarded(&repo_root, &path, &guard)
}

#[tauri::command]
pub fn git_discard_all_guarded(
    repo_root: String,
    guard: crate::services::git::GitGuard,
) -> Result<String, String> {
    crate::services::git::discard_all_guarded(&repo_root, &guard)
}

#[tauri::command]
pub fn git_commit(repo_root: String, message: String, include_all: bool, amend: bool) -> Result<String, String> {
    crate::services::git::commit(&repo_root, &message, include_all, amend).map(|oid| oid.to_string())
}

#[tauri::command]
pub fn git_switch_branch(repo_root: String, name: String) -> Result<(), String> {
    crate::services::git::switch_branch(&repo_root, &name)
}

#[tauri::command]
pub fn git_create_branch(repo_root: String, name: String) -> Result<(), String> {
    crate::services::git::create_branch(&repo_root, &name)
}

#[tauri::command]
pub fn git_fetch(repo_root: String) -> Result<(), String> {
    crate::services::git::fetch(&repo_root)
}

#[tauri::command]
pub fn git_pull(repo_root: String) -> Result<(), String> {
    crate::services::git::pull(&repo_root)
}

#[tauri::command]
pub fn git_push(repo_root: String, remote: String) -> Result<(), String> {
    crate::services::git::push(&repo_root, &remote)
}

#[tauri::command]
pub fn git_stash_all(repo_root: String) -> Result<(), String> {
    crate::services::git::stash_all(&repo_root)
}

#[tauri::command]
pub fn git_stash_pop(repo_root: String) -> Result<(), String> {
    crate::services::git::stash_pop(&repo_root)
}

#[tauri::command]
pub fn git_init(repo_root: String) -> Result<(), String> {
    crate::services::git::init(&repo_root)
}
