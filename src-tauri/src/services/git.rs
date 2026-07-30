use git2::{BranchType, Oid, Repository, Status, StatusOptions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusEntry {
    pub path: String,
    pub staged: char,
    pub unstaged: char,
    pub is_untracked: bool,
    pub is_conflict: bool,
    pub orig_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentCommit {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub relative_date: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitStatusInfo {
    pub is_repo: bool,
    pub repo_root: String,
    pub root_path: String,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub has_upstream: bool,
    pub merge_entries: Vec<GitStatusEntry>,
    pub staged_entries: Vec<GitStatusEntry>,
    pub changed_entries: Vec<GitStatusEntry>,
    pub branches: Vec<String>,
    pub remotes: Vec<String>,
    pub recent_commits: Vec<RecentCommit>,
    pub stash_count: usize,
    pub error: Option<String>,
    /// True when this repository is a linked worktree (not the main checkout).
    pub is_worktree: bool,
}

impl GitStatusInfo {
    pub fn empty(root_path: String) -> Self {
        Self { root_path, ..Default::default() }
    }
}

/// Resolve the repository rooted at or containing `dir`.
fn repo_at(dir: &str) -> Result<Repository, String> {
    Repository::discover(dir).map_err(|e| e.message().to_string())
}

/// Anchored project root for a directory: the toplevel of the git repository
/// containing `cwd`, or `cwd` itself unchanged when it isn't inside any
/// repository (bare repos without a workdir also fall back to `cwd`).
pub fn project_root(cwd: &str) -> String {
    match repo_at(cwd) {
        Ok(repo) => repo
            .workdir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| cwd.to_string()),
        Err(_) => cwd.to_string(),
    }
}

pub fn status(root: &str) -> GitStatusInfo {
    let repo = match repo_at(root) {
        Ok(r) => r,
        Err(_) => return GitStatusInfo::empty(root.to_string()),
    };
    let repo_root = repo.workdir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    // A linked worktree has `is_worktree() == true`; the main checkout has
    // `false`. Used by the frontend to show a "worktree" indicator in the
    // Info panel so users know the panels re-rooted into a linked worktree.
    let is_worktree = repo.is_worktree();
    let mut info = GitStatusInfo {
        is_repo: true,
        repo_root: repo_root.clone(),
        root_path: root.to_string(),
        is_worktree,
        ..Default::default()
    };

    if let Ok(head) = repo.head() {
        if head.is_branch() {
            info.branch = head.shorthand().map(str::to_owned);
        } else {
            info.branch = Some("detached HEAD".into());
        }
    }
    if let Some(branch_name) = &info.branch {
        if let Ok(branch) = repo.find_branch(branch_name, BranchType::Local) {
            if let Ok(upstream) = branch.upstream() {
                info.upstream = upstream.name().ok().and_then(|n| n.map(str::to_owned));
                info.has_upstream = info.upstream.is_some();
                let local_ref = branch.get();
                let upstream_ref = upstream.get();
                if let (Some(local_oid), Some(up_oid)) =
                    (local_ref.target(), upstream_ref.target())
                {
                    let (ahead, behind) = repo
                        .graph_ahead_behind(local_oid, up_oid)
                        .unwrap_or((0, 0));
                    info.ahead = ahead;
                    info.behind = behind;
                }
            }
        }
    }

    // Old paths for staged renames: StatusEntry doesn't carry them, so run a
    // HEAD→index diff with similarity detection to collect old → new pairs.
    let mut renamed: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Ok(head) = repo.head() {
        if let Ok(tree) = head.peel_to_tree() {
            if let Ok(mut diff) = repo.diff_tree_to_index(Some(&tree), None, None) {
                let mut find_opts = git2::DiffFindOptions::new();
                find_opts.renames(true);
                if diff.find_similar(Some(&mut find_opts)).is_ok() {
                    for delta in diff.deltas() {
                        if delta.status() == git2::Delta::Renamed {
                            if let (Some(old), Some(new)) =
                                (delta.old_file().path(), delta.new_file().path())
                            {
                                renamed.insert(
                                    new.to_string_lossy().to_string(),
                                    old.to_string_lossy().to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    let mut opts = StatusOptions::new();
    opts.include_untracked(true).renames_head_to_index(true).renames_index_to_workdir(true);
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let staged = status_char_index(entry.status());
            let unstaged = status_char_workdir(entry.status());
            let is_conflict = entry.status().is_conflicted();
            let is_untracked = entry.status().contains(Status::WT_NEW);
            let orig_path = if staged == 'R' { renamed.get(&path).cloned() } else { None };
            let entry_info = GitStatusEntry {
                path: path.clone(),
                staged,
                unstaged,
                is_untracked: is_untracked || (staged == '?' && unstaged == '?'),
                is_conflict,
                orig_path,
            };
            if is_conflict {
                info.merge_entries.push(entry_info);
            } else if staged != '.' && staged != '?' {
                info.staged_entries.push(entry_info);
            } else if (unstaged != '.' && unstaged != '?') || is_untracked {
                info.changed_entries.push(entry_info);
            }
        }
    }

    if let Ok(branches) = repo.branches(Some(BranchType::Local)) {
        info.branches = branches
            .filter_map(|b| b.ok())
            .filter_map(|(branch, _)| branch.name().ok().and_then(|n| n.map(str::to_owned)))
            .collect();
        info.branches.sort();
    }
    if let Ok(remotes) = repo.remotes() {
        info.remotes = remotes.iter().filter_map(|n| n.map(str::to_owned)).collect();
    }
    if let Ok(mut walk) = repo.revwalk() {
        if walk.push_head().is_ok() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let mut commits = Vec::new();
            for oid in walk.by_ref().take(8) {
                let Ok(oid) = oid else { break };
                let Ok(commit) = repo.find_commit(oid) else { continue };
                commits.push(RecentCommit {
                    hash: oid.to_string(),
                    short_hash: format!("{oid:.7}"),
                    subject: commit
                        .message()
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    author: commit.author().name().unwrap_or("").to_string(),
                    relative_date: relative_date(now, commit.time().seconds()),
                });
            }
            info.recent_commits = commits;
        }
    }
    if let Ok(mut walk) = repo.revwalk() {
        if walk.push_ref("refs/stash").is_ok() {
            info.stash_count = walk.count();
        }
    }

    info
}

fn status_char_index(s: Status) -> char {
    if s.contains(Status::INDEX_NEW) { 'A' }
    else if s.contains(Status::INDEX_MODIFIED) { 'M' }
    else if s.contains(Status::INDEX_DELETED) { 'D' }
    else if s.contains(Status::INDEX_RENAMED) { 'R' }
    else if s.contains(Status::INDEX_TYPECHANGE) { 'T' }
    else if s.contains(Status::CONFLICTED) { 'U' }
    else { '.' }
}

fn status_char_workdir(s: Status) -> char {
    if s.contains(Status::WT_NEW) { '?' }
    else if s.contains(Status::WT_MODIFIED) { 'M' }
    else if s.contains(Status::WT_DELETED) { 'D' }
    else if s.contains(Status::WT_RENAMED) { 'R' }
    else if s.contains(Status::WT_TYPECHANGE) { 'T' }
    else if s.contains(Status::CONFLICTED) { 'U' }
    else { '.' }
}

/// Human relative age in `git log --date=relative` style: the largest unit
/// that fits, singular when the count is 1 (months ≈ 30 days, years ≈ 365).
/// Future timestamps clamp to "0 seconds ago".
fn relative_date(now: i64, then: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;
    let secs = (now - then).max(0);
    let (n, unit) = if secs < MINUTE { (secs, "second") }
    else if secs < HOUR { (secs / MINUTE, "minute") }
    else if secs < DAY { (secs / HOUR, "hour") }
    else if secs < WEEK { (secs / DAY, "day") }
    else if secs < MONTH { (secs / WEEK, "week") }
    else if secs < YEAR { (secs / MONTH, "month") }
    else { (secs / YEAR, "year") };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// Commit/stash signature: git config `user.name` / `user.email` (local or
/// global), falling back to the app default when either is unset — e.g. fresh
/// repos on machines with no git identity configured.
fn signature(repo: &Repository) -> Result<git2::Signature<'static>, String> {
    let config = repo.config().map_err(|e| e.message().to_string())?;
    let name = config.get_string("user.name").ok();
    let email = config.get_string("user.email").ok();
    match (name, email) {
        (Some(name), Some(email)) => {
            git2::Signature::now(&name, &email).map_err(|e| e.message().to_string())
        }
        _ => git2::Signature::now("Muster", "muster@local").map_err(|e| e.message().to_string()),
    }
}

/// Loads staged diff content: `HEAD:path` vs `:path`.
///
/// Shells out to `git show` to read each side — cleaner than walking the index
/// via libgit2's `Index::get_path` (whose arity differs across git2 versions).
pub fn load_staged_diff(root: &str, path: &str) -> Result<(String, String), String> {
    let old = git_show(root, &format!("HEAD:{path}"))?;
    let new = git_show(root, &format!(":{path}"))?;
    Ok((old, new))
}

/// Loads unstaged diff content: `index:path` vs the worktree file.
pub fn load_unstaged_diff(root: &str, path: &str) -> Result<(String, String), String> {
    let old = git_show(root, &format!(":{path}"))?;
    let full = std::path::Path::new(root).join(path);
    let new = match std::fs::read(&full) {
        Ok(bytes) => {
            if bytes.contains(&0) {
                return Err("Binary file".into());
            }
            String::from_utf8(bytes).map_err(|_| "Binary file".to_string())?
        }
        Err(_) => String::new(), // file deleted from the worktree
    };
    Ok((old, new))
}

/// `git show <spec>` → UTF-8 text. Returns empty content if the spec resolves
/// to nothing, and `Binary file` if the bytes don't decode.
fn git_show(root: &str, spec: &str) -> Result<String, String> {
    let output = super::procs::quiet_command("git")
        .args(["show", spec])
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Ok(String::new());
    }
    let bytes = output.stdout;
    if bytes.contains(&0) {
        return Err("Binary file".into());
    }
    String::from_utf8(bytes).map_err(|_| "Binary file".to_string())
}

macro_rules! try_git {
    ($expr:expr; $msg:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return Err(format!("{}: {}", $msg, e.message())),
        }
    };
}

pub fn stage(repo_root: &str, path: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    let mut index = try_git!(repo.index(); "open index");
    try_git!(index.add_path(std::path::Path::new(path)); "stage file");
    try_git!(index.write(); "write index");
    Ok(())
}

pub fn stage_all(repo_root: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    let mut index = try_git!(repo.index(); "open index");
    try_git!(index.add_all(["*"].iter().map(std::path::Path::new), git2::IndexAddOption::DEFAULT, None); "stage all");
    try_git!(index.write(); "write index");
    Ok(())
}

pub fn unstage(repo_root: &str, path: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    let path = std::path::Path::new(path);
    if let Ok(head) = repo.head() {
        let tree = try_git!(head.peel_to_tree(); "head tree");
        let mut index = try_git!(repo.index(); "open index");
        try_git!(index.remove_path(path); "remove path");
        try_git!(index.write(); "write index");
        let mut builder = git2::build::CheckoutBuilder::new();
        builder.path(path).force();
        try_git!(repo.checkout_tree(tree.as_object(), Some(&mut builder)); "reset to head");
    } else {
        let mut index = try_git!(repo.index(); "open index");
        try_git!(index.remove_path(path); "remove path");
        try_git!(index.write(); "write index");
    }
    Ok(())
}

/// Unstage everything: reset the whole index to HEAD. On an unborn branch
/// there is no HEAD to reset to, so every index entry is removed instead
/// (`git rm --cached -r -f .` equivalent).
pub fn unstage_all(repo_root: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    if let Ok(head) = repo.head() {
        let object = try_git!(head.peel(git2::ObjectType::Any); "head object");
        try_git!(repo.reset_default(Some(&object), ["*"].iter().map(std::path::Path::new)); "reset index");
    } else {
        let mut index = try_git!(repo.index(); "open index");
        try_git!(index.remove_all(["*"].iter().map(std::path::Path::new), None); "clear index");
        try_git!(index.write(); "write index");
    }
    Ok(())
}

/// Per-file snapshot taken when a destructive confirmation opens: enough to
/// detect that the file was written to (or deleted) while the dialog was up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardEntry {
    pub path: String,
    pub exists: bool,
    pub size: u64,
    pub mtime_ms: i64,
}

/// Fingerprint of the repo + target files, re-validated before a discard
/// runs. Any drift aborts the operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitGuard {
    pub head_oid: Option<String>,
    pub branch: Option<String>,
    pub entries: Vec<GuardEntry>,
}

const GUARD_MISMATCH: &str = "Files changed while the confirmation was open";

fn head_fingerprint(repo: &Repository) -> (Option<String>, Option<String>) {
    match repo.head() {
        Ok(head) => {
            let oid = head.target().map(|o| o.to_string());
            let branch = if head.is_branch() {
                head.shorthand().map(str::to_owned)
            } else {
                None
            };
            (oid, branch)
        }
        Err(_) => (None, None),
    }
}

fn entry_fingerprint(repo_root: &str, path: &str) -> GuardEntry {
    let abs = std::path::Path::new(repo_root).join(path);
    match std::fs::metadata(&abs) {
        Ok(md) => {
            let mtime_ms = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(-1);
            GuardEntry { path: path.to_string(), exists: true, size: md.len(), mtime_ms }
        }
        Err(_) => GuardEntry { path: path.to_string(), exists: false, size: 0, mtime_ms: 0 },
    }
}

/// Snapshot HEAD oid, branch shorthand and per-path fs metadata (`paths` are
/// repo-relative). Untracked files are captured with exists=true, so a
/// rewrite while the confirmation is open is caught too.
pub fn guard(repo_root: &str, paths: &[String]) -> GitGuard {
    let (head_oid, branch) = repo_at(repo_root)
        .map(|r| head_fingerprint(&r))
        .unwrap_or((None, None));
    let entries = paths.iter().map(|p| entry_fingerprint(repo_root, p)).collect();
    GitGuard { head_oid, branch, entries }
}

/// Re-validate the guard against the live repo. `only` restricts the file
/// check to a single entry (single-file discard); `None` checks all of them.
fn validate_guard(repo_root: &str, guard: &GitGuard, only: Option<&str>) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    let (head_oid, branch) = head_fingerprint(&repo);
    if head_oid != guard.head_oid || branch != guard.branch {
        return Err(GUARD_MISMATCH.to_string());
    }
    for entry in &guard.entries {
        if let Some(only) = only {
            if entry.path != only {
                continue;
            }
        }
        let now = entry_fingerprint(repo_root, &entry.path);
        if now.exists != entry.exists || now.size != entry.size || now.mtime_ms != entry.mtime_ms {
            return Err(GUARD_MISMATCH.to_string());
        }
    }
    Ok(())
}

/// Discards one path: restores tracked files from HEAD, sends untracked
/// files to the Recycle Bin (they have nothing in HEAD to restore). Returns
/// a short human summary.
fn discard_file(repo_root: &str, path: &str) -> Result<String, String> {
    let repo = repo_at(repo_root)?;
    let status = repo
        .status_file(std::path::Path::new(path))
        .unwrap_or(git2::Status::empty());
    if status.contains(git2::Status::WT_NEW) {
        let abs = std::path::Path::new(repo_root).join(path);
        crate::services::explorer::trash(abs.to_string_lossy().as_ref())?;
        return Ok(format!("Moved {path} to Recycle Bin"));
    }
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.path(path).force();
    if let Ok(head) = repo.head() {
        let tree = try_git!(head.peel_to_tree(); "head tree");
        try_git!(repo.checkout_tree(tree.as_object(), Some(&mut builder)); "checkout tree");
    }
    Ok(format!("Discarded changes in {path}"))
}

pub fn discard_guarded(repo_root: &str, path: &str, guard: &GitGuard) -> Result<String, String> {
    validate_guard(repo_root, guard, Some(path))?;
    discard_file(repo_root, path)
}

/// Discard every file recorded in the guard. The entry list comes from the
/// guard itself, so files created after the guard was taken are not touched.
pub fn discard_all_guarded(repo_root: &str, guard: &GitGuard) -> Result<String, String> {
    validate_guard(repo_root, guard, None)?;
    let mut summaries = Vec::new();
    for entry in &guard.entries {
        summaries.push(discard_file(repo_root, &entry.path)?);
    }
    Ok(summaries.join("\n"))
}

pub fn commit(repo_root: &str, message: &str, include_all: bool, amend: bool) -> Result<Oid, String> {
    let repo = repo_at(repo_root)?;
    if include_all {
        let mut index = try_git!(repo.index(); "open index");
        try_git!(index.add_all(["*"].iter().map(std::path::Path::new), git2::IndexAddOption::DEFAULT, None); "stage all");
        try_git!(index.write(); "write index");
    }
    let mut index = try_git!(repo.index(); "open index");
    let tree_oid = try_git!(index.write_tree(); "write tree");
    let tree = try_git!(repo.find_tree(tree_oid); "find tree");
    let signature = signature(&repo)?;
    let head = repo.head();
    let parents: Vec<git2::Commit> = if amend {
        let head = head.map_err(|e| e.message().to_string())?;
        let commit = try_git!(head.peel_to_commit(); "head commit");
        vec![commit]
    } else if let Ok(h) = head {
        vec![try_git!(h.peel_to_commit(); "head commit")]
    } else {
        vec![]
    };
    let parent_oids: Vec<&git2::Commit> = parents.iter().collect();
    let update_ref = if amend || repo.head().is_ok() { Some("HEAD") } else { None };
    let oid = try_git!(
        repo.commit(update_ref, &signature, &signature, message, &tree, &parent_oids[..]);
        "commit"
    );
    Ok(oid)
}

pub fn switch_branch(repo_root: &str, name: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    try_git!(repo.set_head(&format!("refs/heads/{name}")); "set head");
    let head = try_git!(repo.head(); "head");
    let tree = try_git!(head.peel_to_tree(); "head tree");
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.force();
    try_git!(repo.checkout_tree(tree.as_object(), Some(&mut builder)); "checkout");
    Ok(())
}

pub fn create_branch(repo_root: &str, name: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    let head = try_git!(repo.head(); "head");
    let commit = try_git!(head.peel_to_commit(); "head commit");
    let _ = try_git!(repo.branch(name, &commit, false); "create branch");
    switch_branch(repo_root, name)
}

pub fn fetch(repo_root: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    let remotes = try_git!(repo.remotes(); "list remotes");
    // Fetch every remote with prune, tolerating individual failures: an
    // offline or unreachable remote must not block updates from the rest.
    // Only an across-the-board failure (or zero remotes) surfaces an error.
    let mut any_ok = false;
    let mut first_err: Option<String> = None;
    for name in remotes.iter().flatten() {
        let Ok(mut remote) = repo.find_remote(name) else { continue };
        let refspec = format!("+refs/heads/*:refs/remotes/{name}/*");
        let mut opts = git2::FetchOptions::new();
        opts.prune(git2::FetchPrune::On);
        match remote.fetch(&[&refspec], Some(&mut opts), None) {
            Ok(()) => any_ok = true,
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("fetch {name}: {}", e.message()));
                }
            }
        }
    }
    if any_ok {
        return Ok(());
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub fn pull(repo_root: &str) -> Result<(), String> {
    // Shell out to git for the merge step — git2's merge helpers are fiddly
    // and we want strict --ff-only semantics.
    super::procs::quiet_command("git")
        .args(["pull", "--ff-only"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| e.to_string())
        .and_then(|o| if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).to_string()) })
}

pub fn push(repo_root: &str, remote: &str) -> Result<(), String> {
    let repo = repo_at(repo_root)?;
    let mut r = try_git!(repo.find_remote(remote); "find remote");
    let head = try_git!(repo.head(); "head");
    let name = head.shorthand().unwrap_or("HEAD");
    try_git!(r.push(&[&format!("refs/heads/{name}:refs/heads/{name}")], None); "push");
    Ok(())
}

pub fn stash_all(repo_root: &str) -> Result<(), String> {
    let mut repo = repo_at(repo_root)?;
    let sig = signature(&repo)?;
    try_git!(repo.stash_save(&sig, "muster", Some(git2::StashFlags::INCLUDE_UNTRACKED)); "stash");
    Ok(())
}

pub fn stash_pop(repo_root: &str) -> Result<(), String> {
    let mut repo = repo_at(repo_root)?;
    try_git!(repo.stash_pop(0, None); "pop stash");
    Ok(())
}

pub fn init(repo_root: &str) -> Result<(), String> {
    Repository::init(repo_root).map(|_| ()).map_err(|e| e.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempRepo(std::path::PathBuf);

    impl TempRepo {
        /// Init a repo in a unique temp dir with one committed file.
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("muster-git-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let repo = Repository::init(&dir).unwrap();
            std::fs::write(dir.join("a.txt"), "one").unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("a.txt")).unwrap();
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("Test", "test@local").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();
            Self(dir)
        }

        fn root(&self) -> String {
            self.0.to_string_lossy().to_string()
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn guard_snapshots_head_branch_and_files() {
        let tmp = TempRepo::new();
        let g = guard(&tmp.root(), &["a.txt".to_string(), "missing.txt".to_string()]);
        assert!(g.head_oid.is_some());
        assert!(g.branch.is_some());
        assert_eq!(g.entries.len(), 2);
        assert!(g.entries[0].exists);
        assert_eq!(g.entries[0].size, 3);
        assert!(!g.entries[1].exists);
    }

    #[test]
    fn discard_guarded_restores_and_reports() {
        let tmp = TempRepo::new();
        let root = tmp.root();
        std::fs::write(tmp.0.join("a.txt"), "modified").unwrap();
        let g = guard(&root, &["a.txt".to_string()]);
        let summary = discard_guarded(&root, "a.txt", &g).unwrap();
        assert_eq!(summary, "Discarded changes in a.txt");
        assert_eq!(std::fs::read_to_string(tmp.0.join("a.txt")).unwrap(), "one");
    }

    #[test]
    fn discard_guarded_aborts_when_file_changed() {
        let tmp = TempRepo::new();
        let root = tmp.root();
        let g = guard(&root, &["a.txt".to_string()]);
        // An agent rewrites the file while the confirmation is open.
        std::fs::write(tmp.0.join("a.txt"), "agent write").unwrap();
        let err = discard_guarded(&root, "a.txt", &g).unwrap_err();
        assert_eq!(err, GUARD_MISMATCH);
        assert_eq!(std::fs::read_to_string(tmp.0.join("a.txt")).unwrap(), "agent write");
    }

    #[test]
    fn discard_guarded_aborts_when_file_deleted() {
        let tmp = TempRepo::new();
        let root = tmp.root();
        let g = guard(&root, &["a.txt".to_string()]);
        std::fs::remove_file(tmp.0.join("a.txt")).unwrap();
        let err = discard_guarded(&root, "a.txt", &g).unwrap_err();
        assert_eq!(err, GUARD_MISMATCH);
    }

    #[test]
    fn unstage_all_resets_index_to_head() {
        let tmp = TempRepo::new();
        let root = tmp.root();
        std::fs::write(tmp.0.join("a.txt"), "modified").unwrap();
        stage(&root, "a.txt").unwrap();
        unstage_all(&root).unwrap();
        let repo = Repository::open(&tmp.0).unwrap();
        let entry = repo
            .index()
            .unwrap()
            .get_path(std::path::Path::new("a.txt"), 0)
            .unwrap();
        let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
        let head_entry = head_tree.get_path(std::path::Path::new("a.txt")).unwrap();
        assert_eq!(entry.id, head_entry.id());
        // The worktree edit survives; only the index was reset.
        assert_eq!(std::fs::read_to_string(tmp.0.join("a.txt")).unwrap(), "modified");
    }

    #[test]
    fn unstage_all_clears_index_on_unborn_branch() {
        let dir = std::env::temp_dir().join(format!("muster-git-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Repository::init(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        let root = dir.to_string_lossy().to_string();
        stage(&root, "a.txt").unwrap();
        unstage_all(&root).unwrap();
        let mut index = repo.index().unwrap();
        index.read(false).unwrap();
        assert_eq!(index.len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discard_all_guarded_checks_every_entry() {
        let tmp = TempRepo::new();
        let root = tmp.root();
        std::fs::write(tmp.0.join("b.txt"), "two").unwrap();
        let g = guard(&root, &["a.txt".to_string(), "b.txt".to_string()]);
        // A file not in the guard must not be touched, and a guarded file
        // changing must abort the whole batch.
        std::fs::write(tmp.0.join("a.txt"), "modified").unwrap();
        let err = discard_all_guarded(&root, &g).unwrap_err();
        assert_eq!(err, GUARD_MISMATCH);
        assert_eq!(std::fs::read_to_string(tmp.0.join("a.txt")).unwrap(), "modified");
    }

    #[test]
    fn relative_date_picks_largest_unit() {
        const MIN: i64 = 60;
        const HOUR: i64 = 60 * MIN;
        const DAY: i64 = 24 * HOUR;
        let now = 1_000_000_000;
        let at = |ago: i64| relative_date(now, now - ago);
        assert_eq!(at(0), "0 seconds ago");
        assert_eq!(at(1), "1 second ago");
        assert_eq!(at(59), "59 seconds ago");
        assert_eq!(at(MIN), "1 minute ago");
        assert_eq!(at(59 * MIN), "59 minutes ago");
        assert_eq!(at(HOUR), "1 hour ago");
        assert_eq!(at(23 * HOUR), "23 hours ago");
        assert_eq!(at(DAY), "1 day ago");
        assert_eq!(at(6 * DAY), "6 days ago");
        assert_eq!(at(7 * DAY), "1 week ago");
        assert_eq!(at(29 * DAY), "4 weeks ago");
        assert_eq!(at(30 * DAY), "1 month ago");
        assert_eq!(at(364 * DAY), "12 months ago");
        assert_eq!(at(365 * DAY), "1 year ago");
        assert_eq!(at(3 * 365 * DAY), "3 years ago");
        // Future timestamps clamp to zero rather than wrapping.
        assert_eq!(relative_date(now, now + 10), "0 seconds ago");
    }

    #[test]
    fn signature_uses_git_config_identity() {
        let tmp = TempRepo::new();
        let repo = Repository::open(&tmp.0).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Alice").unwrap();
        config.set_str("user.email", "alice@example.com").unwrap();
        let sig = signature(&repo).unwrap();
        assert_eq!(sig.name(), Some("Alice"));
        assert_eq!(sig.email(), Some("alice@example.com"));
    }

    #[test]
    fn signature_falls_back_without_identity() {
        let tmp = TempRepo::new();
        let repo = Repository::open(&tmp.0).unwrap();
        // A machine-wide identity makes the fallback unreachable; skip there.
        let global = git2::Config::open_default()
            .ok()
            .and_then(|c| c.get_string("user.name").ok());
        if global.is_some() {
            return;
        }
        let sig = signature(&repo).unwrap();
        assert_eq!(sig.name(), Some("Muster"));
        assert_eq!(sig.email(), Some("muster@local"));
    }
}
