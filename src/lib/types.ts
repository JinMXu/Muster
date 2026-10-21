// Data-transfer types are generated from the Rust DTOs by Specta (see
// src-tauri/src/specta_export.rs). Regenerate with
// `cargo test --lib specta_export::tests::export_bindings`; CI checks that
// the committed bindings are up to date.
import type {
  AgentAttention,
  AgentKind,
  AgentState,
  AgentStatusChanged,
  AgentStatusRow,
  AllAgentStatus,
  AppStateView,
  BlameLine,
  DiffTabInfo,
  DirEntry,
  DirtyFileInfo,
  FileCommit,
  FileReveal,
  FileSaved,
  FsChanged,
  GitGuard,
  GitStatusEntry,
  GitStatusInfo,
  GuardEntry,
  ListenPort,
  NotificationActivate,
  OpenSettings,
  Pane,
  PaneColumn,
  PaneContent,
  ProcessInfo,
  ProjectView,
  PtyData,
  PtyExit,
  PtyProgress,
  RecentCommit,
  RightPanel,
  SearchMatch,
  SessionAgentStatus,
  SessionCwdChanged,
  SessionInfo,
  SettingsChanged,
  ShellOption,
  StateChanged,
  TabFocused,
  TabView,
  ThemeColors,
  ThemeInfo,
} from "./bindings";

export type {
  AgentAttention,
  AgentKind,
  AgentState,
  AgentStatusChanged,
  AgentStatusRow,
  AllAgentStatus,
  AppStateView,
  BlameLine,
  DiffTabInfo,
  DirEntry,
  DirtyFileInfo,
  FileCommit,
  FileReveal,
  FileSaved,
  FsChanged,
  GitGuard,
  GitStatusEntry,
  GitStatusInfo,
  GuardEntry,
  ListenPort,
  NotificationActivate,
  OpenSettings,
  Pane,
  PaneColumn,
  PaneContent,
  ProcessInfo,
  ProjectView,
  PtyData,
  PtyExit,
  PtyProgress,
  RecentCommit,
  RightPanel,
  SearchMatch,
  SessionAgentStatus,
  SessionCwdChanged,
  SessionInfo,
  SettingsChanged,
  ShellOption,
  StateChanged,
  TabFocused,
  TabView,
  ThemeColors,
  ThemeInfo,
};

/// UI-side aliases/types that have no single Rust DTO.
export type Uuid = string;
export type AppTheme = "system" | "light" | "dark";
export type PaneContentKind = PaneContent["kind"];

/// The close-confirmation rows use the Rust `DirtyFileInfo` name; the UI has
/// historically called them `DirtyFile`.
export type DirtyFile = DirtyFileInfo;

export interface Settings {
  theme: AppTheme;
  theme_dark: string;
  theme_light: string;
  font_family: string;
  font_size: number;
  ui_font_size: number;
  font_thicken: boolean;
  editor_wrap_lines: boolean;
  diff_side_by_side: boolean;
  project_ports: boolean;
  /** Terminal right-click: "menu" opens the context menu, "smart" copies the
   * selection (or pastes when nothing is selected) Windows Terminal style. */
  terminal_right_click: "menu" | "smart";
  /** Default shell for new sessions: "auto" or an id from list_shells. */
  default_shell: string;
  language: "system" | "en" | "zh";
}

export interface FileTabInfo {
  id: Uuid;
  path: string;
  name: string;
  content_kind: "text" | "image" | "unavailable";
  text: string;
  is_dirty: boolean;
}

/// Payload of the `agent-status-changed` event (the generated
/// `AgentStatusChanged`); kept as an alias for the existing hooks.
export type AgentStatusEvent = AgentStatusChanged;
