use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RightPanel {
    #[default]
    Files,
    Git,
    Info,
}

/// Tolerant deserialization: snapshots saved by versions that had other tabs
/// (e.g. an early Search panel writing `right_panel_tab: "search"`) must not
/// fail the whole restore — unknown values simply fall back to Files.
impl<'de> Deserialize<'de> for RightPanel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "git" => RightPanel::Git,
            "info" => RightPanel::Info,
            _ => RightPanel::Files,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    #[default]
    System,
    Light,
    Dark,
}

/// Snapshot of open projects + metadata, saved so a relaunch restores the layout.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub projects: Vec<ProjectSnapshot>,
    pub selected_project_index: Option<usize>,
    pub is_left_sidebar_visible: Option<bool>,
    pub is_right_panel_visible: Option<bool>,
    pub right_panel_tab: Option<RightPanel>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    pub custom_name: Option<String>,
    pub custom_directory: Option<String>,
    pub tabs: Vec<TabSnapshot>,
    pub selected_tab_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub columns: Vec<ColumnSnapshot>,
    pub focused_column: usize,
    pub focused_row: usize,
    pub custom_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSnapshot {
    pub panes: Vec<PaneSnapshot>,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "lowercase")]
pub enum PaneContentSnapshot {
    Session { working_directory: String },
    File { path: String },
    Diff {
        repo_root: String,
        path: String,
        staged: bool,
        /// Two-commit diffs persist their revs so a restart keeps the right
        /// content. `#[serde(default)]` keeps old snapshots loadable.
        #[serde(default)]
        old_rev: Option<String>,
        #[serde(default)]
        new_rev: Option<String>,
        /// The new side is the live worktree (vs HEAD, or vs `old_rev` when
        /// set). `#[serde(default)]` keeps old snapshots loadable.
        #[serde(default)]
        workdir: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub content: PaneContentSnapshot,
    pub weight: f32,
}