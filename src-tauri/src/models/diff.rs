use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Diff content sides. Each side is loaded lazily via the git module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffContent {
    pub old: String,
    pub new: String,
    pub error: Option<String>,
    pub loading: bool,
}

/// A git diff opened as a tab. Content is loaded via `git2` (staged), a
/// worktree read (unstaged), two arbitrary commits (`rev`), or a commit vs
/// the live worktree (`workdir`); both survive tab switches.
pub struct DiffTab {
    pub id: Uuid,
    pub repo_root: String,
    pub path: String,
    pub staged: bool,
    /// `(old_rev, new_rev)` when this tab compares two commits instead of the
    /// working tree / index. Empty revs mean "file didn't exist there".
    pub rev: Option<(String, String)>,
    /// When true the new side is the live worktree file: `rev` holds the old
    /// side's rev (or is `None` to diff against HEAD).
    pub workdir: bool,
    pub content: Mutex<DiffContent>,
}

impl DiffTab {
    pub fn new(repo_root: String, path: String, staged: bool) -> Self {
        let id = Uuid::new_v4();
        let tab = Self {
            id,
            repo_root,
            path,
            staged,
            rev: None,
            workdir: false,
            content: Mutex::new(DiffContent { loading: true, ..Default::default() }),
        };
        tab.reload();
        tab
    }

    pub fn with_revs(repo_root: String, path: String, old_rev: String, new_rev: String) -> Self {
        let id = Uuid::new_v4();
        let tab = Self {
            id,
            repo_root,
            path,
            staged: false,
            rev: Some((old_rev, new_rev)),
            workdir: false,
            content: Mutex::new(DiffContent { loading: true, ..Default::default() }),
        };
        tab.reload();
        tab
    }

    /// Diff of `path` against its HEAD version (new side = worktree).
    pub fn new_workdir(repo_root: String, path: String) -> Self {
        let id = Uuid::new_v4();
        let tab = Self {
            id,
            repo_root,
            path,
            staged: false,
            rev: None,
            workdir: true,
            content: Mutex::new(DiffContent { loading: true, ..Default::default() }),
        };
        tab.reload();
        tab
    }

    /// Diff of `path` between `old_rev` and the current worktree (checkpoint).
    pub fn new_checkpoint(repo_root: String, path: String, old_rev: String) -> Self {
        let id = Uuid::new_v4();
        let tab = Self {
            id,
            repo_root,
            path,
            staged: false,
            rev: Some((old_rev, String::new())),
            workdir: true,
            content: Mutex::new(DiffContent { loading: true, ..Default::default() }),
        };
        tab.reload();
        tab
    }

    pub fn name(&self) -> String {
        PathBuf::from(&self.path).file_name().and_then(|n| n.to_str()).map(str::to_owned).unwrap_or_default()
    }

    pub fn title(&self) -> String {
        if self.workdir {
            let short = |r: &str| -> String {
                let t: String = r.chars().take(7).collect();
                if t.is_empty() { "HEAD".into() } else { t }
            };
            if let Some((old, _)) = &self.rev {
                format!("{} (vs {})", self.name(), short(old))
            } else {
                format!("{} (vs HEAD)", self.name())
            }
        } else if let Some((old, new)) = &self.rev {
            let short = |r: &str| -> String {
                let t: String = r.chars().take(7).collect();
                if t.is_empty() { "∅".into() } else { t }
            };
            format!("{} ({}..{})", self.name(), short(old), short(new))
        } else if self.staged {
            format!("{} (Staged)", self.name())
        } else {
            self.name()
        }
    }

    pub fn info(&self) -> DiffTabInfo {
        let content = self.content.lock().clone();
        let (old_rev, new_rev) = self.rev.clone().map(|(o, n)| (Some(o), Some(n))).unwrap_or((None, None));
        DiffTabInfo {
            id: self.id,
            repo_root: self.repo_root.clone(),
            path: self.path.clone(),
            staged: self.staged,
            old_rev,
            new_rev,
            old: content.old,
            new: content.new,
            error: content.error,
            loading: content.loading,
        }
    }

    /// Re-load both sides from disk / git, computing the diff content.
    pub fn reload(&self) {
        let mut content = self.content.lock();
        content.loading = true;
        content.error = None;
        drop(content);

        let root = self.repo_root.clone();
        let path = self.path.clone();
        let staged = self.staged;
        let rev = self.rev.clone();
        let workdir = self.workdir;

        let result = if workdir {
            let old_rev = match &rev {
                Some((old, _)) => old.clone(),
                None => "HEAD".to_string(),
            };
            crate::services::git::load_workdir_diff(&root, &path, &old_rev)
        } else if let Some((old, new)) = rev {
            crate::services::git::load_commit_diff(&root, &path, &old, &new)
        } else if staged {
            crate::services::git::load_staged_diff(&root, &path)
        } else {
            crate::services::git::load_unstaged_diff(&root, &path)
        };
        let mut content = self.content.lock();
        content.loading = false;
        match result {
            Ok((old, new)) => {
                content.old = old;
                content.new = new;
            }
            Err(e) => {
                content.error = Some(e);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffTabInfo {
    pub id: Uuid,
    pub repo_root: String,
    pub path: String,
    pub staged: bool,
    /// Non-null when this diff compares two commits.
    pub old_rev: Option<String>,
    pub new_rev: Option<String>,
    pub old: String,
    pub new: String,
    pub error: Option<String>,
    pub loading: bool,
}