// TypeScript mirrors of Rust structs sent over the Tauri bridge.

export type Uuid = string;

export type RightPanel = "files" | "git" | "info";

export type AppTheme = "system" | "light" | "dark";

export interface Settings {
  theme: AppTheme;
  theme_dark: string;
  theme_light: string;
  font_family: string;
  font_size: number;
  font_thicken: boolean;
  editor_wrap_lines: boolean;
  language: "system" | "en" | "zh";
}

export type PaneContentKind = "session" | "file" | "diff";

export interface PaneContent {
  kind: PaneContentKind;
  id: Uuid;
}

export interface Pane {
  id: Uuid;
  content: PaneContent;
  weight: number;
}

export interface PaneColumn {
  id: Uuid;
  panes: Pane[];
  weight: number;
}

export interface TabView {
  id: Uuid;
  custom_name: string | null;
  display_title: string | null;
  columns: PaneColumn[];
  focused_pane_id: Uuid;
  is_zoomed: boolean;
  pane_count: number;
}

export interface ProjectView {
  id: Uuid;
  name: string;
  custom_name: string | null;
  custom_directory: string | null;
  tabs: TabView[];
  selected_tab_id: Uuid | null;
  session_count: number;
}

export interface AppStateView {
  projects: ProjectView[];
  selected_project_id: Uuid | null;
  is_left_sidebar_visible: boolean;
  is_panel_visible: boolean;
  panel_tab: RightPanel;
  has_split_panes: boolean;
  is_pane_zoomed: boolean;
}

export interface SessionInfo {
  id: Uuid;
  project_id: Uuid;
  title: string;
  working_directory: string;
  shell_name: string;
  has_exited: boolean;
  pid: number | null;
}

/// Payload of the `pty:progress` event (OSC 9;4 ConEmu progress sequences).
export interface PtyProgress {
  id: Uuid;
  /// 0=remove, 1=normal, 2=error, 3=indeterminate, 4=warning
  state: number;
  progress: number;
}

export interface DirtyFile {
  id: Uuid;
  name: string;
}

export interface FileTabInfo {
  id: Uuid;
  path: string;
  name: string;
  content_kind: "text" | "image" | "unavailable";
  text: string;
  is_dirty: boolean;
}

export interface DiffTabInfo {
  id: Uuid;
  repo_root: string;
  path: string;
  staged: boolean;
  old: string;
  new: string;
  error: string | null;
  loading: boolean;
}

export interface GitStatusEntry {
  path: string;
  staged: string;
  unstaged: string;
  is_untracked: boolean;
  is_conflict: boolean;
  orig_path: string | null;
}

export interface RecentCommit {
  hash: string;
  short_hash: string;
  subject: string;
  author: string;
  relative_date: string;
}

export interface GuardEntry {
  path: string;
  exists: boolean;
  size: number;
  mtime_ms: number;
}

export interface GitGuard {
  head_oid: string | null;
  branch: string | null;
  entries: GuardEntry[];
}

export interface GitStatusInfo {
  is_repo: boolean;
  repo_root: string;
  root_path: string;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  has_upstream: boolean;
  merge_entries: GitStatusEntry[];
  staged_entries: GitStatusEntry[];
  changed_entries: GitStatusEntry[];
  branches: string[];
  remotes: string[];
  recent_commits: RecentCommit[];
  stash_count: number;
  error: string | null;
}

export interface DirEntry {
  name: string;
  path: string;
  is_directory: boolean;
}

/// One row of the Info panel's PROCESSES section (mirrors Rust ProcessInfo).
export interface ProcessInfo {
  pid: number;
  name: string;
  cpu: number;
  mem_bytes: number;
  exe: string;
}

/// One row of the Info panel's PORTS section (mirrors Rust ListenPort).
export interface ListenPort {
  port: number;
  pid: number;
  process_name: string;
}

export interface ThemeColors {
  name: string;
  background: string;
  foreground: string;
  cursor: string;
  accent: string;
  selection_bg: string;
  selection_fg: string;
  sidebar: string;
  divider: string;
  palette: string[];
}

/// Which CLI tool a usage record came from (mirrors Rust ToolKind).
export type ToolKind = "opencode" | "claude_code" | "codex" | "kimi_code";

/// Normalized token breakdown shared across all four tools.
export interface TokenUsage {
  input: number;
  output: number;
  reasoning: number;
  cache_read: number;
  cache_write: number;
}

/// One session's usage (mirrors Rust UsageSession).
export interface UsageSession {
  tool: ToolKind;
  session_id: string;
  title: string;
  cwd: string;
  model: string;
  started_at: number;
  updated_at: number;
  tokens: TokenUsage;
  cost_usd: number | null;
}

/// Per-tool aggregate for the summary cards (mirrors Rust ToolSummary).
export interface ToolSummary {
  tool: ToolKind;
  total_tokens: number;
  tokens: TokenUsage;
  session_count: number;
  cost_usd: number | null;
  last_updated: number;
}

/// Top-level summary payload (mirrors Rust UsageSummary).
export interface UsageSummary {
  tools: ToolSummary[];
}