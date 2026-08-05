use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::diff::{DiffTab, DiffTabInfo};
use super::file::{FileTab, FileTabInfo};
use super::pane::{FocusDirection, Pane, PaneColumn, PaneContent, PaneDropEdge, PaneTab, ResizeDirection};
use super::project::RightPanel;
use super::session::{SessionInfo, TerminalSession};
use crate::services::config::Settings;

/// One row in the left sidebar. Owns its tabs. Each tab is a niri-style pane layout.
pub struct Project {
    pub id: Uuid,
    pub custom_name: Option<String>,
    pub custom_directory: Option<String>,
    pub tabs: Vec<PaneTab>,
    pub selected_tab_id: Option<Uuid>,
    pub fallback_name: String,
}

impl Project {
    pub fn new(fallback_name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            custom_name: None,
            custom_directory: None,
            tabs: Vec::new(),
            selected_tab_id: None,
            fallback_name,
        }
    }

    pub fn name(&self, state: &AppState) -> String {
        if let Some(n) = &self.custom_name {
            if !n.is_empty() { return n.clone(); }
        }
        if let Some(tab) = self.selected_tab_view() {
            if let Some(PaneContent::Session(id)) = tab.focused_pane().map(|p| &p.content) {
                if let Some(s) = state.sessions.get(id) {
                    return s.title();
                }
            }
        }
        self.fallback_name.clone()
    }

    pub fn selected_tab_view(&self) -> Option<&PaneTab> {
        self.tabs.iter().find(|t| Some(t.id) == self.selected_tab_id)
    }

    pub fn selected_tab_mut(&mut self) -> Option<&mut PaneTab> {
        self.tabs.iter_mut().find(|t| Some(t.id) == self.selected_tab_id)
    }

    pub fn session_ids(&self) -> Vec<Uuid> {
        self.tabs
            .iter()
            .flat_map(|t| t.columns.iter().flat_map(|c| c.panes.iter()))
            .filter_map(|p| match &p.content { PaneContent::Session(id) => Some(*id), _ => None })
            .collect()
    }
}

/// Central state owned by Tauri managed state.
pub struct AppState {
    pub projects: Vec<Project>,
    pub selected_project_id: Option<Uuid>,
    pub is_left_sidebar_visible: bool,
    pub is_panel_visible: bool,
    pub panel_tab: RightPanel,
    pub sessions: HashMap<Uuid, Arc<TerminalSession>>,
    pub files: HashMap<Uuid, Arc<FileTab>>,
    pub diffs: HashMap<Uuid, Arc<DiffTab>>,
    pub settings: Arc<Mutex<Settings>>,
    pub project_counter: usize,
}

impl AppState {
    /// `settings` is shared across all windows' states so a settings change
    /// made in one window is visible in every other window.
    pub fn new(settings: Arc<Mutex<Settings>>) -> Self {
        Self {
            projects: Vec::new(),
            selected_project_id: None,
            is_left_sidebar_visible: true,
            is_panel_visible: false,
            panel_tab: RightPanel::Files,
            sessions: HashMap::new(),
            files: HashMap::new(),
            diffs: HashMap::new(),
            settings,
            project_counter: 0,
        }
    }

    // ---- Persistence --------------------------------------------------------

    /// Rebuild projects/tabs/splits/sessions from a saved snapshot. Sessions
    /// are created WITHOUT a PTY here — the caller (bootstrap) spawns them
    /// once the app handle is fully wired up.
    pub fn restore(&mut self, snapshot: &crate::models::project::SessionSnapshot) {
        use crate::models::project::PaneContentSnapshot;

        self.is_left_sidebar_visible = snapshot.is_left_sidebar_visible.unwrap_or(true);
        self.is_panel_visible = snapshot.is_right_panel_visible.unwrap_or(false);
        if let Some(t) = &snapshot.right_panel_tab {
            self.panel_tab = t.clone();
        }

        for proj_snap in &snapshot.projects {
            self.project_counter += 1;
            let mut project = Project::new(format!("Project {}", self.project_counter));
            project.custom_name = proj_snap.custom_name.clone();
            project.custom_directory = proj_snap.custom_directory.clone();

            for tab_snap in &proj_snap.tabs {
                let mut columns: Vec<PaneColumn> = Vec::new();
                for col_snap in &tab_snap.columns {
                    let mut panes: Vec<Pane> = Vec::new();
                    for pane_snap in &col_snap.panes {
                        let content = match &pane_snap.content {
                            PaneContentSnapshot::Session { working_directory } => {
                                let dir = if working_directory.is_empty() {
                                    proj_snap
                                        .custom_directory
                                        .clone()
                                        .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
                                        .unwrap_or_else(|| ".".into())
                                } else {
                                    working_directory.clone()
                                };
                                let session = Arc::new(TerminalSession::new(project.id, dir));
                                let sid = session.id;
                                self.sessions.insert(sid, session);
                                PaneContent::Session(sid)
                            }
                            PaneContentSnapshot::File { path } => {
                                if path.is_empty() {
                                    continue;
                                }
                                let file = Arc::new(FileTab::open(path));
                                let fid = file.id;
                                self.files.insert(fid, file);
                                PaneContent::File(fid)
                            }
                            PaneContentSnapshot::Diff { repo_root, path, staged, old_rev, new_rev, workdir } => {
                                let diff = if *workdir {
                                    match old_rev {
                                        Some(old) => Arc::new(DiffTab::new_checkpoint(
                                            repo_root.clone(),
                                            path.clone(),
                                            old.clone(),
                                        )),
                                        None => Arc::new(DiffTab::new_workdir(repo_root.clone(), path.clone())),
                                    }
                                } else {
                                    match (old_rev, new_rev) {
                                        (Some(old), Some(new)) => Arc::new(DiffTab::with_revs(
                                            repo_root.clone(),
                                            path.clone(),
                                            old.clone(),
                                            new.clone(),
                                        )),
                                        _ => Arc::new(DiffTab::new(repo_root.clone(), path.clone(), *staged)),
                                    }
                                };
                                let did = diff.id;
                                self.diffs.insert(did, diff);
                                PaneContent::Diff(did)
                            }
                        };
                        let mut pane = Pane::new(content);
                        pane.weight = pane_snap.weight;
                        panes.push(pane);
                    }
                    if !panes.is_empty() {
                        columns.push(PaneColumn { id: Uuid::new_v4(), panes, weight: col_snap.weight });
                    }
                }
                if columns.is_empty() {
                    continue;
                }
                let fc = tab_snap.focused_column.min(columns.len() - 1);
                let fr = tab_snap.focused_row.min(columns[fc].panes.len() - 1);
                let focused_pane_id = columns[fc].panes[fr].id;
                project.tabs.push(PaneTab {
                    id: Uuid::new_v4(),
                    custom_name: tab_snap.custom_name.clone(),
                    columns,
                    focused_pane_id,
                    is_zoomed: false,
                });
            }

            // A project whose tabs all failed to restore isn't worth keeping.
            if project.tabs.is_empty() {
                continue;
            }
            project.selected_tab_id = proj_snap
                .selected_tab_index
                .and_then(|i| project.tabs.get(i))
                .map(|t| t.id)
                .or_else(|| project.tabs.first().map(|t| t.id));
            self.projects.push(project);
        }

        self.selected_project_id = snapshot
            .selected_project_index
            .and_then(|i| self.projects.get(i))
            .map(|p| p.id)
            .or_else(|| self.projects.first().map(|p| p.id));
    }

    pub fn selected_project(&self) -> Option<&Project> {
        self.projects.iter().find(|p| Some(p.id) == self.selected_project_id)
    }

    fn project_index(&self, id: Uuid) -> Option<usize> {
        self.projects.iter().position(|p| p.id == id)
    }

    pub fn selected_tab(&self) -> Option<&PaneTab> {
        self.selected_project().and_then(|p| p.selected_tab_view())
    }

    fn selected_tab_mut(&mut self) -> Option<&mut PaneTab> {
        self.selected_project_mut().and_then(|p| p.selected_tab_mut())
    }

    pub fn selected_project_mut(&mut self) -> Option<&mut Project> {
        let id = self.selected_project_id?;
        self.projects.iter_mut().find(|p| p.id == id)
    }

    /// Best-effort directory for a new session in the current project.
    fn compute_session_dir(&self) -> Option<String> {
        let project = self.selected_project()?;
        if let Some(d) = &project.custom_directory {
            return Some(d.clone());
        }
        if let Some(t) = project.selected_tab_view() {
            if let Some(pane) = t.focused_pane() {
                if let PaneContent::Session(id) = &pane.content {
                    if let Some(s) = self.sessions.get(id) {
                        return Some(s.current_directory());
                    }
                }
            }
        }
        let sids = project.session_ids();
        if let Some(first) = sids.first() {
            if let Some(s) = self.sessions.get(first) {
                return Some(s.current_directory());
            }
        }
        None
    }

    // ---- Projects ----------------------------------------------------------

    pub fn new_project(&mut self, directory: Option<String>) -> Uuid {
        self.project_counter += 1;
        let fallback = format!("Project {}", self.project_counter);
        let mut project = Project::new(fallback);
        if let Some(dir) = &directory {
            project.custom_directory = Some(dir.clone());
            project.custom_name = Some(
                std::path::Path::new(dir)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| project.fallback_name.clone()),
            );
        }
        let pid = project.id;
        self.insert_project(project);
        self.spawn_session_in_selected(directory);
        pid
    }

    fn insert_project(&mut self, project: Project) {
        let new_id = project.id;
        if let Some(sel_id) = self.selected_project_id {
            if let Some(idx) = self.projects.iter().position(|p| p.id == sel_id) {
                self.projects.insert(idx + 1, project);
            } else {
                self.projects.push(project);
            }
        } else {
            self.projects.push(project);
        }
        self.selected_project_id = Some(new_id);
    }

    pub fn close_project(&mut self, id: Uuid) {
        if let Some(idx) = self.project_index(id) {
            let sids: Vec<Uuid> = self.projects[idx].session_ids();
            for sid in sids {
                if let Some(s) = self.sessions.remove(&sid) {
                    s.terminate();
                }
            }
            // Also clear any opened files/diffs that lived in this project's tabs.
            for tab in &self.projects[idx].tabs {
                for pane in tab.all_panes() {
                    match &pane.content {
                        PaneContent::File(fid) => { self.files.remove(fid); }
                        PaneContent::Diff(did) => { self.diffs.remove(did); }
                        _ => {}
                    }
                }
            }
            self.projects.remove(idx);
            if self.selected_project_id == Some(id) {
                let next = idx.min(self.projects.len().saturating_sub(1));
                self.selected_project_id = self.projects.get(next).map(|p| p.id);
            }
            if self.projects.is_empty() {
                self.is_panel_visible = false;
            }
        }
    }

    pub fn move_project(&mut self, from: Uuid, to: Uuid) {
        if from == to { return }
        let Some(fi) = self.project_index(from) else { return };
        let Some(ti) = self.project_index(to) else { return };
        let removed = self.projects.remove(fi);
        self.projects.insert(ti, removed);
    }

    pub fn rename_project(&mut self, id: Uuid, name: Option<String>) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
            p.custom_name = name.filter(|n| !n.is_empty());
        }
    }

    pub fn set_project_directory(&mut self, id: Uuid, directory: Option<String>) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
            p.custom_directory = directory.filter(|d| !d.is_empty());
        }
    }

    pub fn select_project(&mut self, id: Uuid) {
        if self.projects.iter().any(|p| p.id == id) {
            self.selected_project_id = Some(id);
        }
    }
    pub fn select_project_by_index(&mut self, idx: usize) {
        if let Some(p) = self.projects.get(idx) {
            self.selected_project_id = Some(p.id);
        }
    }
    pub fn select_next_project(&mut self) { self.shift_project(1); }
    pub fn select_previous_project(&mut self) { self.shift_project(-1); }
    fn shift_project(&mut self, delta: i32) {
        if self.projects.is_empty() { return }
        let cur = self
            .project_index(self.selected_project_id.unwrap_or_default())
            .unwrap_or(0);
        let next = ((cur as i32 + delta).rem_euclid(self.projects.len() as i32)) as usize;
        self.selected_project_id = Some(self.projects[next].id);
    }

    // ---- Sessions / Tabs --------------------------------------------------

    pub fn spawn_session_in_selected(&mut self, directory: Option<String>) -> Option<Uuid> {
        let project_id = self.selected_project_id?;
        let initial_dir = directory
            .or_else(|| self.compute_session_dir())
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            });
        let session = Arc::new(TerminalSession::new(project_id, initial_dir));
        let session_id = session.id;
        self.insert_session_tab_after_selected(project_id, PaneContent::Session(session_id));
        self.sessions.insert(session_id, session);
        Some(session_id)
    }

    fn insert_session_tab_after_selected(&mut self, project_id: Uuid, content: PaneContent) {
        let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) else { return };
        let insert_at_idx = project
            .selected_tab_id
            .and_then(|sid| project.tabs.iter().position(|t| t.id == sid));
        let tab = PaneTab::new(content);
        let tab_id = tab.id;
        if let Some(i) = insert_at_idx {
            project.tabs.insert(i + 1, tab);
        } else {
            project.tabs.push(tab);
        }
        project.selected_tab_id = Some(tab_id);
    }

    pub fn close_selected_tab(&mut self) {
        let Some(p_idx) = self.projects.iter().position(|p| Some(p.id) == self.selected_project_id) else { return };
        if self.projects[p_idx].tabs.is_empty() {
            let pid = self.projects[p_idx].id;
            self.close_project(pid);
            return;
        }
        let Some(tab_id) = self.projects[p_idx].selected_tab_id else { return };
        self.remove_tab(p_idx, tab_id);
    }

    /// Remove one tab from the project at `p_idx`: terminate its sessions,
    /// drop its files/diffs, and reassign the project's selection if the
    /// removed tab was selected. Shared by all the close-tab variants.
    fn remove_tab(&mut self, p_idx: usize, tab_id: Uuid) {
        let Some(i) = self.projects[p_idx].tabs.iter().position(|t| t.id == tab_id) else { return };
        let removed_tab = self.projects[p_idx].tabs.remove(i);
        for pane in removed_tab.all_panes() {
            match &pane.content {
                PaneContent::Session(id) => { if let Some(s) = self.sessions.remove(id) { s.terminate(); } }
                PaneContent::File(id) => { self.files.remove(id); }
                PaneContent::Diff(id) => { self.diffs.remove(id); }
            }
        }
        let p = &mut self.projects[p_idx];
        if p.selected_tab_id == Some(tab_id) {
            let neighbor = i.min(p.tabs.len().saturating_sub(1));
            p.selected_tab_id = p.tabs.get(neighbor).map(|t| t.id);
        }
    }

    /// Close a specific tab by id, wherever it lives. Selection is only
    /// reassigned when the closed tab was the selected one (see remove_tab),
    /// so closing a background tab keeps the current selection untouched.
    pub fn close_tab(&mut self, tab_id: Uuid) {
        let Some(p_idx) = self.project_index_of_tab(tab_id) else { return };
        self.remove_tab(p_idx, tab_id);
    }

    fn project_index_of_tab(&self, tab_id: Uuid) -> Option<usize> {
        self.projects.iter().position(|p| p.tabs.iter().any(|t| t.id == tab_id))
    }

    pub fn close_other_tabs(&mut self, tab_id: Uuid) {
        let Some(p_idx) = self.project_index_of_tab(tab_id) else { return };
        let ids: Vec<Uuid> = self.projects[p_idx]
            .tabs
            .iter()
            .filter(|t| t.id != tab_id)
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.remove_tab(p_idx, id);
        }
        // The surviving tab becomes the selected one.
        self.projects[p_idx].selected_tab_id = Some(tab_id);
    }

    pub fn close_tabs_to_right(&mut self, tab_id: Uuid) {
        let Some(p_idx) = self.project_index_of_tab(tab_id) else { return };
        let Some(pos) = self.projects[p_idx].tabs.iter().position(|t| t.id == tab_id) else { return };
        let ids: Vec<Uuid> = self.projects[p_idx].tabs.iter().skip(pos + 1).map(|t| t.id).collect();
        for id in ids {
            self.remove_tab(p_idx, id);
        }
    }

    /// Close every tab of the selected project, keeping the (now empty)
    /// project itself alive.
    pub fn close_all_tabs(&mut self) {
        let Some(p_idx) = self.projects.iter().position(|p| Some(p.id) == self.selected_project_id) else { return };
        let ids: Vec<Uuid> = self.projects[p_idx].tabs.iter().map(|t| t.id).collect();
        for id in ids {
            self.remove_tab(p_idx, id);
        }
    }

    pub fn split(&mut self, edge: PaneDropEdge) {
        self.split_with_dir(edge, None);
    }

    /// Split the selected tab's focused pane with a fresh terminal session.
    /// The new session starts in `dir` when given, otherwise in the focused
    /// pane's current directory (see `compute_session_dir`). Returns the new
    /// session's id (None when the tab can't be split).
    pub fn split_with_dir(&mut self, edge: PaneDropEdge, dir: Option<String>) -> Option<Uuid> {
        let can_split = self.selected_tab().map(|t| t.can_split()).unwrap_or(false);
        if !can_split {
            return None;
        }
        let project_id = self.selected_project_id?;
        let dir = dir.or_else(|| self.compute_session_dir()).unwrap_or_else(|| {
            dirs::home_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| ".".into())
        });
        let session = Arc::new(TerminalSession::new(project_id, dir));
        let session_id = session.id;
        let pane = Pane::new(PaneContent::Session(session_id));
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) {
            if let Some(tab) = project.selected_tab_mut() {
                tab.split(pane, edge);
            }
        }
        self.sessions.insert(session_id, session);
        Some(session_id)
    }

    // ---- Pane navigation ---------------------------------------------------

    pub fn focus_pane(&mut self, dir: FocusDirection) { if let Some(t) = self.selected_tab_mut() { t.focus(dir); } }
    pub fn resize_pane(&mut self, dir: ResizeDirection) { if let Some(t) = self.selected_tab_mut() { t.resize(dir); } }
    pub fn resize_pane_divider(&mut self, tab_id: Uuid, vertical: bool, column_index: usize, index: usize, delta: f32) {
        for p in &mut self.projects {
            if let Some(t) = p.tabs.iter_mut().find(|t| t.id == tab_id) {
                t.resize_divider(vertical, column_index, index, delta);
                return;
            }
        }
    }
    pub fn toggle_pane_zoom(&mut self) { if let Some(t) = self.selected_tab_mut() { t.toggle_zoom(); } }
    pub fn equalize_panes(&mut self) { if let Some(t) = self.selected_tab_mut() { t.equalize(); } }

    /// Drag & drop rearrange: move `pane_id` to `edge` of `target_pane_id`
    /// within the tab `tab_id` (intra-tab only).
    pub fn move_pane(&mut self, tab_id: Uuid, pane_id: Uuid, target_pane_id: Uuid, edge: PaneDropEdge) {
        for p in &mut self.projects {
            if let Some(t) = p.tabs.iter_mut().find(|t| t.id == tab_id) {
                t.move_pane(pane_id, target_pane_id, edge);
                return;
            }
        }
    }

    /// Drag & drop a pane across tabs: detach `pane_id` from `source_tab_id`
    /// and add it as a new column in `target_tab_id`. When the move empties
    /// the source tab, that tab is closed — every pane was detached first, so
    /// closing it terminates nothing (see remove_tab). Refused when the same
    /// tab is both source and target (use `move_pane` instead). Returns true
    /// when a move actually happened.
    pub fn move_pane_cross_tab(&mut self, source_tab_id: Uuid, pane_id: Uuid, target_tab_id: Uuid) -> bool {
        if source_tab_id == target_tab_id {
            return false;
        }
        // Detach from source tab, allowing the move to take its last pane.
        let detached: Option<Pane> = {
            let mut found: Option<Pane> = None;
            for p in &mut self.projects {
                if let Some(t) = p.tabs.iter_mut().find(|t| t.id == source_tab_id) {
                    found = t.detach_pane_allowing_empty(pane_id);
                    break;
                }
            }
            found
        };
        let Some(pane) = detached else { return false };
        // A move that emptied the source tab closes it rather than leaving a
        // pane-less tab behind.
        if let Some(p_idx) = self.project_index_of_tab(source_tab_id) {
            let emptied = self.projects[p_idx]
                .tabs
                .iter()
                .any(|t| t.id == source_tab_id && t.all_panes().is_empty());
            if emptied {
                self.remove_tab(p_idx, source_tab_id);
            }
        }
        // Insert into target tab.
        let inserted = {
            let mut ok = false;
            for p in &mut self.projects {
                if let Some(t) = p.tabs.iter_mut().find(|t| t.id == target_tab_id) {
                    t.add_pane_as_column(pane);
                    ok = true;
                    // Switch the project so the user lands where the pane was
                    // dropped, not left looking at the source.
                    self.selected_project_id = Some(p.id);
                    break;
                }
            }
            ok
        };
        if !inserted {
            // Should be impossible since the pane came from somewhere.
            return false;
        }
        true
    }

    // ---- Sidebar / panel ---------------------------------------------------

    pub fn toggle_left_sidebar(&mut self) { self.is_left_sidebar_visible = !self.is_left_sidebar_visible; }
    pub fn toggle_right_panel(&mut self) { self.is_panel_visible = !self.is_panel_visible; }
    pub fn toggle_panel(&mut self, panel: RightPanel) {
        if self.is_panel_visible && self.panel_tab == panel {
            self.is_panel_visible = false;
        } else {
            self.panel_tab = panel;
            self.is_panel_visible = true;
        }
    }

    // ---- Tab navigation ----------------------------------------------------

    pub fn select_next_tab(&mut self) {
        let Some(p) = self.selected_project_mut() else { return };
        if let Some(sel) = p.selected_tab_id {
            if let Some(i) = p.tabs.iter().position(|t| t.id == sel) {
                let next = (i + 1) % p.tabs.len();
                p.selected_tab_id = p.tabs.get(next).map(|t| t.id);
            }
        }
    }
    pub fn select_previous_tab(&mut self) {
        let Some(p) = self.selected_project_mut() else { return };
        if let Some(sel) = p.selected_tab_id {
            if let Some(i) = p.tabs.iter().position(|t| t.id == sel) {
                let next = (i as i32 - 1).rem_euclid(p.tabs.len() as i32) as usize;
                p.selected_tab_id = p.tabs.get(next).map(|t| t.id);
            }
        }
    }
    pub fn select_tab(&mut self, id: Uuid) {
        if let Some(p) = self.selected_project_mut() {
            if p.tabs.iter().any(|t| t.id == id) { p.selected_tab_id = Some(id); }
        }
    }

    /// Locate the pane hosting `session_id` across every project/tab, select
    /// its project and tab, and focus that pane. Returns true when the session
    /// was found in this AppState. Used by the cross-window agent mini-bar to
    /// jump the user to the agent's pane (possibly in another window).
    pub fn focus_session(&mut self, session_id: Uuid) -> bool {
        for project in &mut self.projects {
            for tab in &mut project.tabs {
                for col in &mut tab.columns {
                    for pane in &col.panes {
                        if let PaneContent::Session(id) = &pane.content {
                            if *id == session_id {
                                self.selected_project_id = Some(project.id);
                                project.selected_tab_id = Some(tab.id);
                                tab.focused_pane_id = pane.id;
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
    pub fn move_tab(&mut self, from: Uuid, to: Uuid) {
        if from == to { return }
        let Some(p) = self.selected_project_mut() else { return };
        let Some(fi) = p.tabs.iter().position(|t| t.id == from) else { return };
        let Some(ti) = p.tabs.iter().position(|t| t.id == to) else { return };
        let moved = p.tabs.remove(fi);
        p.tabs.insert(ti, moved);
    }
    pub fn rename_tab(&mut self, id: Uuid, name: Option<String>) {
        if let Some(p) = self.selected_project_mut() {
            if let Some(t) = p.tabs.iter_mut().find(|t| t.id == id) {
                t.custom_name = name.filter(|n| !n.is_empty());
            }
        }
    }

    // ---- Files / Diffs -----------------------------------------------------

    pub fn open_file(&mut self, path: &str, to_side: bool) -> Option<Uuid> {
        let file = Arc::new(FileTab::open(path));
        let id = file.id;

        let project_id = self.selected_project_id?;
        // Reuse an existing pane for the same path? One lookup: compute the
        // candidate pane and focus it while the project is borrowed.
        let existing_pane_id = self
            .projects
            .iter_mut()
            .find(|p| p.id == project_id)
            .and_then(|p| {
                let pane_id = p
                    .selected_tab_view()
                    .and_then(|t| t.all_panes().iter().find(|pane| matches!(&pane.content, PaneContent::File(fid) if *fid == id)).map(|p| p.id));
                if let Some(pane_id) = pane_id {
                    if let Some(t) = p.selected_tab_mut() {
                        t.focused_pane_id = pane_id;
                    }
                }
                pane_id
            });
        if existing_pane_id.is_some() {
            self.files.insert(id, file);
            return Some(id);
        }

        let content = PaneContent::File(id);
        if to_side {
            let can_split = self.selected_tab().map(|t| t.can_split()).unwrap_or(false);
            let p = self.projects.iter_mut().find(|p| p.id == project_id)?;
            if let Some(t) = p.selected_tab_mut() {
                if can_split {
                    t.split(Pane::new(content), PaneDropEdge::Right);
                    self.files.insert(id, file);
                    return Some(id);
                }
            }
            // Fallback: open in its own tab.
            self.append_tab(project_id, content);
            self.files.insert(id, file);
            return Some(id);
        }
        self.append_tab(project_id, content);
        self.files.insert(id, file);
        Some(id)
    }

    fn append_tab(&mut self, project_id: Uuid, content: PaneContent) {
        let tab = PaneTab::new(content);
        let tab_id = tab.id;
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
            p.tabs.push(tab);
            p.selected_tab_id = Some(tab_id);
        }
    }

    pub fn save_selected_file(&self) -> Result<Option<Uuid>, String> {
        let Some(tab) = self.selected_tab() else { return Ok(None) };
        let Some(pane) = tab.focused_pane() else { return Ok(None) };
        if let PaneContent::File(id) = &pane.content {
            if let Some(f) = self.files.get(id) {
                f.save()?;
                return Ok(Some(*id));
            }
        }
        Ok(None)
    }

    pub fn save_file(&self, id: Uuid) -> Result<(), String> {
        if let Some(f) = self.files.get(&id) { return f.save(); }
        Ok(())
    }

    /// Unsaved files within a tab — drives the close-confirmation dialog.
    pub fn tab_dirty_files(&self, tab_id: Uuid) -> Vec<DirtyFileInfo> {
        let mut out = Vec::new();
        for p in &self.projects {
            let Some(tab) = p.tabs.iter().find(|t| t.id == tab_id) else { continue };
            for pane in tab.all_panes() {
                if let PaneContent::File(fid) = &pane.content {
                    if let Some(f) = self.files.get(fid) {
                        if *f.is_dirty.lock() {
                            out.push(DirtyFileInfo { id: *fid, name: f.name() });
                        }
                    }
                }
            }
        }
        out
    }

    /// Unsaved files across every tab of a project.
    pub fn project_dirty_files(&self, project_id: Uuid) -> Vec<DirtyFileInfo> {
        let mut out = Vec::new();
        if let Some(p) = self.projects.iter().find(|p| p.id == project_id) {
            for tab in &p.tabs {
                for pane in tab.all_panes() {
                    if let PaneContent::File(fid) = &pane.content {
                        if let Some(f) = self.files.get(fid) {
                            if *f.is_dirty.lock() {
                                out.push(DirtyFileInfo { id: *fid, name: f.name() });
                            }
                        }
                    }
                }
            }
        }
        out
    }

    pub fn open_diff(&mut self, repo_root: &str, path: &str, staged: bool) -> Option<Uuid> {
        let diff = Arc::new(DiffTab::new(repo_root.to_string(), path.to_string(), staged));
        let id = diff.id;
        let project_id = self.selected_project_id?;
        self.append_tab(project_id, PaneContent::Diff(id));
        self.diffs.insert(id, diff);
        Some(id)
    }

    /// Open a diff of `path` between two arbitrary commits (empty rev = the
    /// file didn't exist on that side). Appended as a new pane/tab like the
    /// working-tree diffs.
    pub fn open_commit_diff(&mut self, repo_root: &str, path: &str, old_rev: &str, new_rev: &str) -> Option<Uuid> {
        let diff = Arc::new(DiffTab::with_revs(repo_root.to_string(), path.to_string(), old_rev.to_string(), new_rev.to_string()));
        let id = diff.id;
        let project_id = self.selected_project_id?;
        self.append_tab(project_id, PaneContent::Diff(id));
        self.diffs.insert(id, diff);
        Some(id)
    }

    /// Open a diff of `path` against its HEAD version (new side = worktree).
    pub fn open_workdir_diff(&mut self, repo_root: &str, path: &str) -> Option<Uuid> {
        let diff = Arc::new(DiffTab::new_workdir(repo_root.to_string(), path.to_string()));
        let id = diff.id;
        let project_id = self.selected_project_id?;
        self.append_tab(project_id, PaneContent::Diff(id));
        self.diffs.insert(id, diff);
        Some(id)
    }

    /// Open a diff of `path` between `old_rev` and the current worktree
    /// (the checkpoint panel's "changes since checkpoint" view).
    pub fn open_checkpoint_diff(&mut self, repo_root: &str, path: &str, old_rev: &str) -> Option<Uuid> {
        let diff = Arc::new(DiffTab::new_checkpoint(repo_root.to_string(), path.to_string(), old_rev.to_string()));
        let id = diff.id;
        let project_id = self.selected_project_id?;
        self.append_tab(project_id, PaneContent::Diff(id));
        self.diffs.insert(id, diff);
        Some(id)
    }

    // ---- Info payloads for the frontend ----------------------------------

    pub fn session_info(&self, id: Uuid) -> Option<SessionInfo> { self.sessions.get(&id).map(|s| SessionInfo::from(s.as_ref())) }
    pub fn file_info(&self, id: Uuid) -> Option<FileTabInfo> { self.files.get(&id).map(|f| FileTabInfo::from(f.as_ref())) }
    pub fn diff_info(&self, id: Uuid) -> Option<DiffTabInfo> {
        self.diffs.get(&id).map(|d| d.info())
    }

    /// Filesystem path behind a pane, for the Reveal/Copy Path context-menu
    /// items: session → its current working directory, file → its path,
    /// diff → the repo root.
    pub fn pane_context_path(&self, tab_id: Uuid, pane_id: Uuid) -> Option<String> {
        for p in &self.projects {
            let Some(tab) = p.tabs.iter().find(|t| t.id == tab_id) else { continue };
            let pane = tab.all_panes().into_iter().find(|pn| pn.id == pane_id)?;
            return match &pane.content {
                PaneContent::Session(id) => self.sessions.get(id).map(|s| s.current_directory()),
                PaneContent::File(id) => self.files.get(id).map(|f| f.path.lock().clone()),
                PaneContent::Diff(id) => self.diffs.get(id).map(|d| d.repo_root.clone()),
            };
        }
        None
    }
    pub fn list_all_sessions(&self) -> Vec<SessionInfo> {
        self.sessions.values().map(|s| SessionInfo::from(s.as_ref())).collect()
    }
}

/// One unsaved file entry shown in the close-confirmation dialog.
#[derive(Debug, Clone, Serialize)]
pub struct DirtyFileInfo {
    pub id: Uuid,
    pub name: String,
}

/// Frontend-facing serialized projection of the live state.
#[derive(Debug, Clone, Serialize)]
pub struct AppStateView {
    pub projects: Vec<ProjectView>,
    pub selected_project_id: Option<Uuid>,
    pub is_left_sidebar_visible: bool,
    pub is_panel_visible: bool,
    pub panel_tab: RightPanel,
    pub has_split_panes: bool,
    pub is_pane_zoomed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectView {
    pub id: Uuid,
    pub name: String,
    pub custom_name: Option<String>,
    pub custom_directory: Option<String>,
    pub tabs: Vec<TabView>,
    pub selected_tab_id: Option<Uuid>,
    pub session_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TabView {
    pub id: Uuid,
    pub custom_name: Option<String>,
    pub display_title: Option<String>,
    pub columns: Vec<PaneColumn>,
    pub focused_pane_id: Uuid,
    pub is_zoomed: bool,
    pub pane_count: usize,
}

impl AppState {
    pub fn view(&self) -> AppStateView {
        let projects = self
            .projects
            .iter()
            .map(|p| ProjectView {
                id: p.id,
                name: p.name(self),
                custom_name: p.custom_name.clone(),
                custom_directory: p.custom_directory.clone(),
                tabs: p.tabs.iter().map(|t| self.tab_view(t)).collect(),
                selected_tab_id: p.selected_tab_id,
                session_count: p.session_ids().len(),
            })
            .collect();
        let has_split = self.selected_tab().map(|t| t.has_multiple_panes()).unwrap_or(false);
        let is_zoom = self.selected_tab().map(|t| t.is_zoomed).unwrap_or(false);
        AppStateView {
            projects,
            selected_project_id: self.selected_project_id,
            is_left_sidebar_visible: self.is_left_sidebar_visible,
            is_panel_visible: self.is_panel_visible,
            panel_tab: self.panel_tab.clone(),
            has_split_panes: has_split,
            is_pane_zoomed: is_zoom,
        }
    }

    fn tab_view(&self, t: &PaneTab) -> TabView {
        TabView {
            id: t.id,
            custom_name: t.custom_name.clone(),
            display_title: t.focused_pane().and_then(|pane| match &pane.content {
                PaneContent::Session(id) => self.sessions.get(id).map(|s| s.title()),
                PaneContent::File(id) => self.files.get(id).map(|f| f.name()),
                PaneContent::Diff(id) => self.diffs.get(id).map(|d| d.title()),
            }),
            columns: t.columns.clone(),
            focused_pane_id: t.focused_pane_id,
            is_zoomed: t.is_zoomed,
            pane_count: t.all_panes().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::project::{
        ColumnSnapshot, PaneContentSnapshot, PaneSnapshot, ProjectSnapshot, SessionSnapshot, TabSnapshot,
    };

    fn fresh_state() -> AppState {
        AppState::new(Arc::new(Mutex::new(Settings::default())))
    }

    fn session_pane(wd: &str, weight: f32) -> PaneSnapshot {
        PaneSnapshot {
            content: PaneContentSnapshot::Session { working_directory: wd.to_string() },
            weight,
        }
    }

    #[test]
    fn restore_preserves_layout_and_selection() {
        let snapshot = SessionSnapshot {
            projects: vec![ProjectSnapshot {
                custom_name: Some("proj".into()),
                custom_directory: Some("C:\\work".into()),
                tabs: vec![
                    TabSnapshot {
                        custom_name: Some("term".into()),
                        columns: vec![
                            ColumnSnapshot {
                                panes: vec![session_pane("C:\\work\\a", 0.7), session_pane("", 0.3)],
                                weight: 0.6,
                            },
                            ColumnSnapshot {
                                panes: vec![PaneSnapshot {
                                    content: PaneContentSnapshot::File { path: "C:\\work\\f.txt".into() },
                                    weight: 1.0,
                                }],
                                weight: 0.4,
                            },
                        ],
                        focused_column: 1,
                        focused_row: 0,
                    },
                    TabSnapshot {
                        custom_name: None,
                        columns: vec![ColumnSnapshot {
                            panes: vec![PaneSnapshot {
                                content: PaneContentSnapshot::Diff {
                                    repo_root: "C:\\definitely-not-a-repo".into(),
                                    path: "x.rs".into(),
                                    staged: true,
                                    old_rev: None,
                                    new_rev: None,
                                    workdir: false,
                                },
                                weight: 1.0,
                            }],
                            weight: 1.0,
                        }],
                        focused_column: 0,
                        focused_row: 0,
                    },
                ],
                selected_tab_index: Some(1),
            }],
            selected_project_index: Some(0),
            is_left_sidebar_visible: Some(false),
            is_right_panel_visible: Some(true),
            right_panel_tab: Some(RightPanel::Git),
        };

        let mut state = fresh_state();
        state.restore(&snapshot);

        // Project and selection.
        assert_eq!(state.projects.len(), 1);
        let project = &state.projects[0];
        assert_eq!(project.custom_name.as_deref(), Some("proj"));
        assert_eq!(project.custom_directory.as_deref(), Some("C:\\work"));
        assert_eq!(state.selected_project_id, Some(project.id));
        assert!(!state.is_left_sidebar_visible);
        assert!(state.is_panel_visible);
        assert_eq!(state.panel_tab, RightPanel::Git);

        // Tab strip.
        assert_eq!(project.tabs.len(), 2);
        assert_eq!(project.selected_tab_id, Some(project.tabs[1].id));
        assert_eq!(project.tabs[0].custom_name.as_deref(), Some("term"));

        // Columns, weights, and pane kinds.
        let tab0 = &project.tabs[0];
        assert_eq!(tab0.columns.len(), 2);
        assert_eq!(tab0.columns[0].weight, 0.6);
        assert_eq!(tab0.columns[1].weight, 0.4);
        assert_eq!(tab0.columns[0].panes[0].weight, 0.7);
        assert_eq!(tab0.columns[0].panes[1].weight, 0.3);
        assert!(matches!(tab0.columns[0].panes[0].content, PaneContent::Session(_)));
        assert!(matches!(tab0.columns[0].panes[1].content, PaneContent::Session(_)));
        assert!(matches!(tab0.columns[1].panes[0].content, PaneContent::File(_)));
        // Focus lands on the snapshot's focused column/row.
        assert_eq!(tab0.focused_pane_id, tab0.columns[1].panes[0].id);

        // Backing resources exist and sessions keep their working directory.
        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.files.len(), 1);
        assert_eq!(state.diffs.len(), 1);
        let PaneContent::Session(s0) = tab0.columns[0].panes[0].content else { panic!() };
        let PaneContent::Session(s1) = tab0.columns[0].panes[1].content else { panic!() };
        assert_eq!(state.sessions[&s0].current_directory(), "C:\\work\\a");
        // An empty working directory falls back to the project directory.
        assert_eq!(state.sessions[&s1].current_directory(), "C:\\work");

        let diff = state.diffs.values().next().unwrap();
        assert_eq!(diff.repo_root, "C:\\definitely-not-a-repo");
        assert!(diff.staged);
    }

    #[test]
    fn move_pane_cross_tab_closes_emptied_source_tab() {
        let tab = |wd: &str| TabSnapshot {
            custom_name: None,
            columns: vec![ColumnSnapshot {
                panes: vec![session_pane(wd, 1.0)],
                weight: 1.0,
            }],
            focused_column: 0,
            focused_row: 0,
        };
        let snapshot = SessionSnapshot {
            projects: vec![ProjectSnapshot {
                custom_name: None,
                custom_directory: None,
                tabs: vec![tab("C:\\work\\a"), tab("C:\\work\\b")],
                selected_tab_index: Some(1),
            }],
            selected_project_index: Some(0),
            is_left_sidebar_visible: None,
            is_right_panel_visible: None,
            right_panel_tab: None,
        };
        let mut state = fresh_state();
        state.restore(&snapshot);

        let project = &state.projects[0];
        let source_tab = project.tabs[0].id;
        let target_tab = project.tabs[1].id;
        let pane = project.tabs[0].columns[0].panes[0].id;
        let PaneContent::Session(session_id) = project.tabs[0].columns[0].panes[0].content else {
            panic!()
        };

        // Moving the source tab's only pane succeeds and closes the tab.
        assert!(state.move_pane_cross_tab(source_tab, pane, target_tab));
        let project = &state.projects[0];
        assert_eq!(project.tabs.len(), 1);
        assert_eq!(project.tabs[0].id, target_tab);
        assert_eq!(project.tabs[0].all_panes().len(), 2);
        // Selection stays on the target tab.
        assert_eq!(project.selected_tab_id, Some(target_tab));
        // Closing the emptied source tab terminated nothing: the moved pane's
        // session is still alive.
        assert_eq!(state.sessions.len(), 2);
        assert!(state.sessions.contains_key(&session_id));
    }

    #[test]
    fn restore_clamps_invalid_indices() {
        let snapshot = SessionSnapshot {
            projects: vec![ProjectSnapshot {
                custom_name: None,
                custom_directory: None,
                tabs: vec![TabSnapshot {
                    custom_name: None,
                    columns: vec![ColumnSnapshot {
                        panes: vec![session_pane("C:\\work", 1.0), session_pane("C:\\work", 1.0)],
                        weight: 1.0,
                    }],
                    focused_column: 99,
                    focused_row: 99,
                }],
                selected_tab_index: Some(99),
            }],
            selected_project_index: Some(99),
            is_left_sidebar_visible: None,
            is_right_panel_visible: None,
            right_panel_tab: None,
        };

        let mut state = fresh_state();
        state.restore(&snapshot);

        let project = &state.projects[0];
        assert_eq!(state.selected_project_id, Some(project.id));
        assert_eq!(project.selected_tab_id, Some(project.tabs[0].id));
        let tab = &project.tabs[0];
        // Focus clamped to the last available pane.
        assert_eq!(tab.focused_pane_id, tab.columns[0].panes[1].id);
        // Missing visibility flags fall back to defaults.
        assert!(state.is_left_sidebar_visible);
        assert!(!state.is_panel_visible);
    }

    #[test]
    fn restore_drops_projects_with_no_restorable_tabs() {
        let snapshot = SessionSnapshot {
            projects: vec![
                ProjectSnapshot {
                    custom_name: Some("empty".into()),
                    custom_directory: None,
                    tabs: vec![TabSnapshot {
                        custom_name: None,
                        columns: vec![ColumnSnapshot {
                            // An empty file path is skipped, leaving no panes.
                            panes: vec![PaneSnapshot {
                                content: PaneContentSnapshot::File { path: String::new() },
                                weight: 1.0,
                            }],
                            weight: 1.0,
                        }],
                        focused_column: 0,
                        focused_row: 0,
                    }],
                    selected_tab_index: None,
                },
                ProjectSnapshot {
                    custom_name: Some("good".into()),
                    custom_directory: None,
                    tabs: vec![TabSnapshot {
                        custom_name: None,
                        columns: vec![ColumnSnapshot {
                            panes: vec![session_pane("C:\\work", 1.0)],
                            weight: 1.0,
                        }],
                        focused_column: 0,
                        focused_row: 0,
                    }],
                    selected_tab_index: None,
                },
            ],
            // Points at the dropped project; falls back to the first survivor.
            selected_project_index: Some(0),
            is_left_sidebar_visible: None,
            is_right_panel_visible: None,
            right_panel_tab: None,
        };

        let mut state = fresh_state();
        state.restore(&snapshot);

        assert_eq!(state.projects.len(), 1);
        assert_eq!(state.projects[0].custom_name.as_deref(), Some("good"));
        assert_eq!(state.selected_project_id, Some(state.projects[0].id));
        assert!(state.files.is_empty());
        assert_eq!(state.sessions.len(), 1);
    }

    #[test]
    fn restore_empty_snapshot_leaves_state_empty() {
        let mut state = fresh_state();
        state.restore(&SessionSnapshot::default());

        assert!(state.projects.is_empty());
        assert_eq!(state.selected_project_id, None);
        assert!(state.sessions.is_empty());
    }

    #[test]
    fn right_panel_unknown_value_falls_back_to_files() {
        // Snapshots written by builds with more tabs (e.g. an early Search
        // panel) must not fail restore — unknown panel tabs map to Files.
        let json = r#"{"projects":[],"right_panel_tab":"search"}"#;
        let snapshot: SessionSnapshot = serde_json::from_str(json).expect("snapshot parses");
        assert_eq!(snapshot.right_panel_tab, Some(RightPanel::Files));

        // Known values still deserialize correctly.
        assert_eq!(
            serde_json::from_str::<SessionSnapshot>(r#"{"projects":[],"right_panel_tab":"git"}"#)
                .unwrap()
                .right_panel_tab,
            Some(RightPanel::Git)
        );
    }
}
