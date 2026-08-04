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
  ui_font_size: number;
  font_thicken: boolean;
  editor_wrap_lines: boolean;
  project_ports: boolean;
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
  /// Non-null when this diff compares two commits (File History).
  old_rev: string | null;
  new_rev: string | null;
  old: string;
  new: string;
  error: string | null;
  loading: boolean;
}

/// One commit that touched a file (mirrors Rust services::git::FileCommit).
export interface FileCommit {
  hash: string;
  short_hash: string;
  parent: string | null;
  subject: string;
  author: string;
  relative_date: string;
  date_ms: number;
}

/// One blame-annotated line (mirrors Rust services::git::BlameLine).
export interface BlameLine {
  line: number;
  short_hash: string;
  author: string;
  date: string;
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
  is_worktree: boolean;
}

export interface DirEntry {
  name: string;
  path: string;
  is_directory: boolean;
}

/// One matching line of one file (mirrors Rust services::search::SearchMatch).
/// `match_start` / `match_len` are char indices into `line_text`.
export interface SearchMatch {
  path: string;
  rel_path: string;
  line: number;
  column: number;
  line_text: string;
  match_start: number;
  match_len: number;
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

export interface ThemeInfo {
  name: string;
  is_dark: boolean;
  background: string;
  accent: string;
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

/// A coding-agent CLI detected in a session's process tree (mirrors Rust
/// services::agents::AgentKind).
export type AgentKind =
  | "opencode"
  | "claude_code"
  | "codex"
  | "kimi_code"
  | "aider"
  | "gemini"
  | "goose";

/// Two-state heuristic: working (output recently) or waiting (quiet).
export type AgentState = "working" | "waiting" | "done";

/// One row of the `agent-status-changed` event payload. `agent`/`state` are
/// null for a removal (the session's agent disappeared) — the UI drops the
/// status dot in that case. For the global `all-agent-status` snapshot, each
/// row additionally carries `title`/`project` so the mini-bar popover can
/// show "what" and "where" without another IPC round-trip.
export interface AgentStatusRow {
  id: Uuid;
  agent: AgentKind | null;
  state: AgentState | null;
  title?: string;
  project?: string;
}

/// Payload of the `agent-status-changed` event (Rust AgentStatusEvent).
export interface AgentStatusEvent {
  sessions: AgentStatusRow[];
}