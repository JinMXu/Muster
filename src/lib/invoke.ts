import { invoke } from "@tauri-apps/api/core";
import type {
  AppStateView,
  BlameLine,
  DirEntry,
  DiffTabInfo,
  DirtyFile,
  FileCommit,
  FileTabInfo,
  GitStatusInfo,
  GitGuard,
  ListenPort,
  ProcessInfo,
  RightPanel,
  SearchMatch,
  SessionInfo,
  Settings,
  ThemeColors,
  ToolKind,
  UsageSession,
  UsageSummary,
  Uuid,
} from "./types";
import { setPendingReveal, trackDiffTab, trackFileTab } from "./recentFiles";

// Import the pane enums' string forms directly (Rust side listens for the
// serde-converted lowercase form).
export type PaneDropEdge = "left" | "right" | "top" | "bottom";
export type FocusDirection = "up" | "down" | "left" | "right" | "next" | "previous";
export type ResizeDirection = "up" | "down" | "left" | "right";

const c = <T>(name: string, args?: Record<string, unknown>): Promise<T> => invoke<T>(name, args);

export const api = {
  state: () => c<AppStateView>("get_state"),
  settings: () => c<Settings>("get_settings"),
  defaultSettings: () => c<Settings>("default_settings"),
  saveSettings: (s: Settings) => c<void>("save_settings", { settings: s }),
  availableThemes: () => c<string[]>("available_themes"),
  themeColors: (name: string, dark: boolean) => c<ThemeColors>("theme_colors", { name, dark }),
  sessionInfo: (id: Uuid) => c<SessionInfo | null>("session_info", { id }),
  listAllSessions: () => c<SessionInfo[]>("list_all_sessions"),
  fileInfo: (id: Uuid) => c<FileTabInfo | null>("file_info", { id }),
  diffInfo: (id: Uuid) => c<DiffTabInfo | null>("diff_info", { id }),
  focusAgentSession: (sessionId: Uuid) => c<boolean>("focus_agent_session", { sessionId }),

  newWindow: () => c<void>("new_window"),

  newProject: (directory?: string) => c<Uuid>("new_project", { directory: directory ?? null }),
  closeProject: (id: Uuid) => c<void>("close_project", { id }),
  selectProject: (id: Uuid) => c<void>("select_project", { id }),
  selectProjectByIndex: (idx: number) => c<void>("select_project_by_index", { idx }),
  selectNextProject: () => c<void>("select_next_project"),
  selectPreviousProject: () => c<void>("select_previous_project"),
  moveProject: (from: Uuid, to: Uuid) => c<void>("move_project", { from, to }),
  renameProject: (id: Uuid, name: string | null) => c<void>("rename_project", { id, name }),
  setProjectDirectory: (id: Uuid, directory: string | null) =>
    c<void>("set_project_directory", { id, directory }),

  spawnSession: () => c<SessionInfo>("spawn_session", { directory: null }),
  sendText: (id: Uuid, text: string) => c<void>("send_text", { id, text }),
  resizeTerminal: (id: Uuid, cols: number, rows: number) =>
    c<void>("resize_terminal", { id, cols, rows }),
  clearTerminal: (id: Uuid) => c<void>("clear_terminal", { id }),

  sessionProcesses: (sessionId: Uuid, shellPid: number) =>
    c<ProcessInfo[]>("session_processes", { sessionId, shellPid }),
  killProcess: (sessionId: Uuid, shellPid: number, pid: number) =>
    c<void>("kill_process", { sessionId, shellPid, pid }),
  sessionPorts: (pids: number[], projectRoot: string | null) =>
    c<ListenPort[]>("session_ports", { pids, projectRoot }),

  closeSelectedTab: () => c<void>("close_selected_tab"),
  closeTab: (tabId: Uuid) => c<void>("close_tab", { tabId }),
  selectTab: (id: Uuid) => c<void>("select_tab", { id }),
  selectNextTab: () => c<void>("select_next_tab"),
  selectPreviousTab: () => c<void>("select_previous_tab"),
  moveTab: (from: Uuid, to: Uuid) => c<void>("move_tab", { from, to }),
  renameTab: (id: Uuid, name: string | null) => c<void>("rename_tab", { id, name }),
  closeOtherTabs: (tabId: Uuid) => c<void>("close_other_tabs", { tabId }),
  closeTabsToRight: (tabId: Uuid) => c<void>("close_tabs_to_right", { tabId }),
  closeAllTabs: () => c<void>("close_all_tabs"),
  paneContextPath: (tabId: Uuid, paneId: Uuid) =>
    c<string | null>("pane_context_path", { tabId, paneId }),

  split: (edge: PaneDropEdge) => c<void>("split", { edge }),
  focusPane: (direction: FocusDirection) => c<void>("focus_pane", { direction }),
  resizePane: (direction: ResizeDirection) => c<void>("resize_pane", { direction }),
  resizePaneDivider: (tabId: Uuid, vertical: boolean, columnIndex: number, index: number, delta: number) =>
    c<void>("resize_pane_divider", { tabId, vertical, columnIndex, index, delta }),
  movePane: (tabId: Uuid, paneId: Uuid, targetPaneId: Uuid, edge: PaneDropEdge) =>
    c<void>("move_pane", { tabId, paneId, targetPaneId, edge }),
  movePaneCrossTab: (sourceTabId: Uuid, paneId: Uuid, targetTabId: Uuid) =>
    c<boolean>("move_pane_cross_tab", { sourceTabId, paneId, targetTabId }),
  togglePaneZoom: () => c<void>("toggle_pane_zoom"),
  equalizePanes: () => c<void>("equalize_panes"),

  toggleLeftSidebar: () => c<void>("toggle_left_sidebar"),
  toggleRightPanel: () => c<void>("toggle_right_panel"),
  togglePanel: (panel: RightPanel) => c<void>("toggle_panel", { panel }),

  openFile: (path: string, toSide: boolean) =>
    c<Uuid | null>("open_file", { path, toSide }).then((id) => {
      trackFileTab(id, path);
      return id;
    }),
  openFileAt: (path: string, line: number) =>
    c<Uuid | null>("open_file_at", { path, line }).then((id) => {
      trackFileTab(id, path);
      if (id) setPendingReveal(id, line);
      return id;
    }),
  fileTextChanged: (id: Uuid, text: string) => c<void>("file_text_changed", { id, text }),
  saveSelectedFile: () => c<void>("save_selected_file"),
  saveFile: (id: Uuid, text?: string) => c<void>("save_file", { id, text: text ?? null }),
  tabDirtyFiles: (tabId: Uuid) => c<DirtyFile[]>("tab_dirty_files", { tabId }),
  projectDirtyFiles: (projectId: Uuid) => c<DirtyFile[]>("project_dirty_files", { projectId }),
  openDiff: (repoRoot: string, path: string, staged: boolean) =>
    c<Uuid | null>("open_diff", { repoRoot, path, staged }).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged, oldRev: null, newRev: null, workdir: false });
      return id;
    }),
  openCommitDiff: (repoRoot: string, path: string, oldRev: string, newRev: string) =>
    c<Uuid | null>("open_commit_diff", { repoRoot, path, oldRev, newRev }).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged: false, oldRev, newRev, workdir: false });
      return id;
    }),
  openWorkdirDiff: (repoRoot: string, path: string) =>
    c<Uuid | null>("open_workdir_diff", { repoRoot, path }).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged: false, oldRev: null, newRev: null, workdir: true });
      return id;
    }),
  openCheckpointDiff: (repoRoot: string, path: string, oldRev: string) =>
    c<Uuid | null>("open_checkpoint_diff", { repoRoot, path, oldRev }).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged: false, oldRev, newRev: null, workdir: true });
      return id;
    }),
  reloadDiff: (id: Uuid) => c<void>("reload_diff", { id }),

  listDirectory: (path: string) => c<DirEntry[]>("list_directory", { path }),
  trashFile: (path: string) => c<void>("trash_file", { path }),
  createFile: (parentDir: string, name: string, isDirectory: boolean) =>
    c<string>("create_file", { parentDir, name, isDirectory }),
  renamePath: (from: string, to: string) => c<string>("rename_path", { from, to }),
  watchDirectories: (paths: string[]) => c<void>("watch_directories", { paths }),
  searchFiles: (root: string, query: string, caseSensitive: boolean) =>
    c<SearchMatch[]>("search_files", { root, query, caseSensitive }),
  listProjectFiles: (root: string) => c<string[]>("list_project_files", { root }),

  gitStatus: (repoRoot: string) => c<GitStatusInfo>("git_status", { repoRoot }),
  gitGuard: (repoRoot: string, paths: string[]) => c<GitGuard>("git_guard", { repoRoot, paths }),
  resolveProjectRoot: (cwd: string) => c<string>("resolve_project_root", { cwd }),
  git: {
    stage: (repoRoot: string, path: string) => c<void>("git_stage", { repoRoot, path }),
    stageAll: (repoRoot: string) => c<void>("git_stage_all", { repoRoot }),
    unstage: (repoRoot: string, path: string) => c<void>("git_unstage", { repoRoot, path }),
    unstageAll: (repoRoot: string) => c<void>("git_unstage_all", { repoRoot }),
    discardGuarded: (repoRoot: string, path: string, guard: GitGuard) =>
      c<string>("git_discard_guarded", { repoRoot, path, guard }),
    discardAllGuarded: (repoRoot: string, guard: GitGuard) =>
      c<string>("git_discard_all_guarded", { repoRoot, guard }),
    commit: (repoRoot: string, message: string, includeAll: boolean, amend: boolean) =>
      c<string>("git_commit", { repoRoot, message, includeAll, amend }),
    switchBranch: (repoRoot: string, name: string) => c<void>("git_switch_branch", { repoRoot, name }),
    createBranch: (repoRoot: string, name: string) => c<void>("git_create_branch", { repoRoot, name }),
    fetch: (repoRoot: string) => c<void>("git_fetch", { repoRoot }),
    pull: (repoRoot: string) => c<void>("git_pull", { repoRoot }),
    push: (repoRoot: string, remote: string) => c<void>("git_push", { repoRoot, remote }),
    stashAll: (repoRoot: string) => c<void>("git_stash_all", { repoRoot }),
    stashPop: (repoRoot: string) => c<void>("git_stash_pop", { repoRoot }),
    init: (repoRoot: string) => c<void>("git_init", { repoRoot }),
    fileHistory: (repoRoot: string, path: string) =>
      c<FileCommit[]>("git_file_history", { repoRoot, path }),
    headContent: (repoRoot: string, path: string) =>
      c<string | null>("git_head_content", { repoRoot, path }),
    blame: (repoRoot: string, path: string) =>
      c<BlameLine[]>("git_blame", { repoRoot, path }),
    headOid: (repoRoot: string) => c<string | null>("git_head_oid", { repoRoot }),
    checkpointChanges: (repoRoot: string, checkpoint: string) =>
      c<string[]>("git_checkpoint_changes", { repoRoot, checkpoint }),
  },
  usage: {
    summary: () => c<UsageSummary>("usage_summary"),
    sessions: (opts?: { tool?: ToolKind; since?: number; limit?: number }) =>
      c<UsageSession[]>("usage_sessions", opts ?? {}),
    refresh: () => c<void>("usage_refresh"),
  },
  installExplorerContextMenu: () => c<void>("install_explorer_context_menu"),
  addToPath: () => c<void>("add_to_path"),
  removeFromPath: () => c<void>("remove_from_path"),
  isOnPath: () => c<boolean>("is_on_path"),
};