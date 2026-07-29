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

/// A git diff opened as a tab. Old/new content is loaded via `git2` (staged)
/// or a worktree read (unstaged); both survive tab switches.
pub struct DiffTab {
    pub id: Uuid,
    pub repo_root: String,
    pub path: String,
    pub staged: bool,
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
            content: Mutex::new(DiffContent { loading: true, ..Default::default() }),
        };
        tab.reload();
        tab
    }

    pub fn name(&self) -> String {
        PathBuf::from(&self.path).file_name().and_then(|n| n.to_str()).map(str::to_owned).unwrap_or_default()
    }

    pub fn title(&self) -> String {
        if self.staged {
            format!("{} (Staged)", self.name())
        } else {
            self.name()
        }
    }

    pub fn info(&self) -> DiffTabInfo {
        let content = self.content.lock().clone();
        DiffTabInfo {
            id: self.id,
            repo_root: self.repo_root.clone(),
            path: self.path.clone(),
            staged: self.staged,
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

        let result = if staged {
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
    pub old: String,
    pub new: String,
    pub error: Option<String>,
    pub loading: bool,
}