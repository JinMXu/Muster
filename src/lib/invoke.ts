import { commands } from "./bindings";
import type {
  BlameLine,
  DiffTabInfo,
  DirEntry,
  FileCommit,
  FileTabInfo,
  GitGuard,
  GitStatusInfo,
  RightPanel,
  SearchMatch,
  SessionAgentStatus,
  SessionInfo,
  Settings,
  Uuid,
} from "./types";
import { setPendingReveal, trackDiffTab, trackFileTab } from "./recentFiles";

// Command argument enums are generated from the Rust definitions; re-export
// them here so existing call sites keep importing from `lib/invoke`.
export type { FocusDirection, PaneDropEdge, ResizeDirection } from "./bindings";
import type { FocusDirection, PaneDropEdge, ResizeDirection } from "./bindings";

/// Commands that return `()` surface as `null` from the generated bindings;
/// normalize them to the `Promise<void>` the app's `api` has always exposed.
const done = (p: Promise<unknown>): Promise<void> => p.then(() => {});

export const api = {
  state: () => commands.getState(),
  // Settings has serde defaults (so the generated fields are optional while
  // the persisted config always contains every field). Keep the stricter
  // hand-written shape at this boundary.
  settings: () => commands.getSettings() as unknown as Promise<Settings>,
  configPath: () => commands.configPath(),
  takePendingAction: () => commands.takePendingAction(),
  defaultSettings: () => commands.defaultSettings() as unknown as Promise<Settings>,
  saveSettings: (s: Settings) => done(commands.saveSettings(s)),
  availableThemes: () => commands.availableThemes(),
  availableThemesWithInfo: () => commands.availableThemesWithInfo(),
  themeColors: (name: string, dark: boolean) => commands.themeColors(name, dark),
  sessionInfo: (id: Uuid) =>
    commands.sessionInfo(id) as unknown as Promise<SessionInfo | null>,
  listAllSessions: () => commands.listAllSessions(),
  agentStatusSnapshot: () => commands.agentStatusSnapshot() as Promise<SessionAgentStatus[]>,
  fileInfo: (id: Uuid) =>
    commands.fileInfo(id) as unknown as Promise<FileTabInfo | null>,
  diffInfo: (id: Uuid) =>
    commands.diffInfo(id) as unknown as Promise<DiffTabInfo | null>,
  focusAgentSession: (sessionId: Uuid) => commands.focusAgentSession(sessionId),

  newWindow: () => done(commands.newWindow()),
  newProject: (directory?: string) => commands.newProject(directory ?? null),
  closeProject: (id: Uuid) => done(commands.closeProject(id)),
  selectProject: (id: Uuid) => done(commands.selectProject(id)),
  selectProjectByIndex: (idx: number) => done(commands.selectProjectByIndex(idx)),
  selectNextProject: () => done(commands.selectNextProject()),
  selectPreviousProject: () => done(commands.selectPreviousProject()),
  moveProject: (from: Uuid, to: Uuid) => done(commands.moveProject(from, to)),
  renameProject: (id: Uuid, name: string | null) => done(commands.renameProject(id, name)),
  setProjectDirectory: (id: Uuid, directory: string | null) =>
    done(commands.setProjectDirectory(id, directory)),

  spawnSession: () => commands.spawnSession(null),
  listShells: () => commands.listShells(),
  sendText: (id: Uuid, text: string) => commands.sendText(id, text),
  terminalAck: (id: Uuid, bytes: number) => commands.terminalAck(id, bytes),
  resizeTerminal: (id: Uuid, cols: number, rows: number) =>
    done(commands.resizeTerminal(id, cols, rows)),
  clearTerminal: (id: Uuid) => commands.clearTerminal(id),

  sessionProcesses: (sessionId: Uuid, shellPid: number) =>
    commands.sessionProcesses(sessionId, shellPid),
  killProcess: (sessionId: Uuid, shellPid: number, pid: number) =>
    done(commands.killProcess(sessionId, shellPid, pid)),
  sessionPorts: (pids: number[], projectRoot: string | null) =>
    commands.sessionPorts(pids, projectRoot),

  closeSelectedTab: () => done(commands.closeSelectedTab()),
  closeTab: (tabId: Uuid) => done(commands.closeTab(tabId)),
  selectTab: (id: Uuid) => done(commands.selectTab(id)),
  selectNextTab: () => done(commands.selectNextTab()),
  selectPreviousTab: () => done(commands.selectPreviousTab()),
  moveTab: (from: Uuid, to: Uuid) => done(commands.moveTab(from, to)),
  renameTab: (id: Uuid, name: string | null) => done(commands.renameTab(id, name)),
  closeOtherTabs: (tabId: Uuid) => done(commands.closeOtherTabs(tabId)),
  closeTabsToRight: (tabId: Uuid) => done(commands.closeTabsToRight(tabId)),
  closeAllTabs: () => done(commands.closeAllTabs()),
  paneContextPath: (tabId: Uuid, paneId: Uuid) => commands.paneContextPath(tabId, paneId),

  split: (edge: PaneDropEdge) => done(commands.split(edge)),
  focusPane: (direction: FocusDirection) => done(commands.focusPane(direction)),
  resizePane: (direction: ResizeDirection) => done(commands.resizePane(direction)),
  resizePaneDivider: (
    tabId: Uuid,
    vertical: boolean,
    columnIndex: number,
    index: number,
    delta: number
  ) => done(commands.resizePaneDivider(tabId, vertical, columnIndex, index, delta)),
  movePane: (tabId: Uuid, paneId: Uuid, targetPaneId: Uuid, edge: PaneDropEdge) =>
    done(commands.movePane(tabId, paneId, targetPaneId, edge)),
  movePaneCrossTab: (sourceTabId: Uuid, paneId: Uuid, targetTabId: Uuid) =>
    commands.movePaneCrossTab(sourceTabId, paneId, targetTabId),
  closePane: (tabId: Uuid, paneId: Uuid) => commands.closePane(tabId, paneId),
  togglePaneZoom: () => done(commands.togglePaneZoom()),
  equalizePanes: () => done(commands.equalizePanes()),

  toggleLeftSidebar: () => done(commands.toggleLeftSidebar()),
  toggleRightPanel: () => done(commands.toggleRightPanel()),
  togglePanel: (panel: RightPanel) => done(commands.togglePanel(panel)),

  openFile: (path: string, toSide: boolean) =>
    commands.openFile(path, toSide).then((id) => {
      trackFileTab(id, path);
      return id;
    }),
  openFileAt: (path: string, line: number) =>
    commands.openFileAt(path, line).then((id) => {
      trackFileTab(id, path);
      if (id) setPendingReveal(id, line);
      return id;
    }),
  fileTextChanged: (id: Uuid, text: string) => commands.fileTextChanged(id, text),
  saveSelectedFile: () => done(commands.saveSelectedFile()),
  saveFile: (id: Uuid, text?: string) => done(commands.saveFile(id, text ?? null)),
  tabDirtyFiles: (tabId: Uuid) => commands.tabDirtyFiles(tabId),
  projectDirtyFiles: (projectId: Uuid) => commands.projectDirtyFiles(projectId),
  openDiff: (repoRoot: string, path: string, staged: boolean) =>
    commands.openDiff(repoRoot, path, staged).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged, oldRev: null, newRev: null, workdir: false });
      return id;
    }),
  openCommitDiff: (repoRoot: string, path: string, oldRev: string, newRev: string) =>
    commands.openCommitDiff(repoRoot, path, oldRev, newRev).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged: false, oldRev, newRev, workdir: false });
      return id;
    }),
  openWorkdirDiff: (repoRoot: string, path: string) =>
    commands.openWorkdirDiff(repoRoot, path).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged: false, oldRev: null, newRev: null, workdir: true });
      return id;
    }),
  openCheckpointDiff: (repoRoot: string, path: string, oldRev: string) =>
    commands.openCheckpointDiff(repoRoot, path, oldRev).then((id) => {
      trackDiffTab(id, { repoRoot, path, staged: false, oldRev, newRev: null, workdir: true });
      return id;
    }),
  reloadDiff: (id: Uuid) => done(commands.reloadDiff(id)),

  listDirectory: (path: string) => commands.listDirectory(path) as Promise<DirEntry[]>,
  trashFile: (path: string) => done(commands.trashFile(path)),
  createFile: (parentDir: string, name: string, isDirectory: boolean) =>
    commands.createFile(parentDir, name, isDirectory),
  renamePath: (from: string, to: string) => commands.renamePath(from, to),
  watchDirectories: (paths: string[]) => commands.watchDirectories(paths),
  searchFiles: (root: string, query: string, caseSensitive: boolean) =>
    commands.searchFiles(root, query, caseSensitive) as Promise<SearchMatch[]>,
  listProjectFiles: (root: string) => commands.listProjectFiles(root),

  gitStatus: (repoRoot: string, force = false) =>
    commands.gitStatus(repoRoot, force) as Promise<GitStatusInfo>,
  gitGuard: (repoRoot: string, paths: string[]) => commands.gitGuard(repoRoot, paths),
  gitStatusRevision: (repoRoot: string) => commands.gitStatusRevision(repoRoot),
  resolveProjectRoot: (cwd: string) => commands.resolveProjectRoot(cwd),
  git: {
    stage: (repoRoot: string, path: string) => done(commands.gitStage(repoRoot, path)),
    stageAll: (repoRoot: string) => done(commands.gitStageAll(repoRoot)),
    unstage: (repoRoot: string, path: string) => done(commands.gitUnstage(repoRoot, path)),
    unstageAll: (repoRoot: string) => done(commands.gitUnstageAll(repoRoot)),
    discardGuarded: (repoRoot: string, path: string, guard: GitGuard) =>
      commands.gitDiscardGuarded(repoRoot, path, guard),
    discardAllGuarded: (repoRoot: string, guard: GitGuard) =>
      commands.gitDiscardAllGuarded(repoRoot, guard),
    commit: (repoRoot: string, message: string, includeAll: boolean, amend: boolean) =>
      commands.gitCommit(repoRoot, message, includeAll, amend),
    switchBranch: (repoRoot: string, name: string) =>
      done(commands.gitSwitchBranch(repoRoot, name)),
    createBranch: (repoRoot: string, name: string) =>
      done(commands.gitCreateBranch(repoRoot, name)),
    fetch: (repoRoot: string) => done(commands.gitFetch(repoRoot)),
    pull: (repoRoot: string) => done(commands.gitPull(repoRoot)),
    push: (repoRoot: string, remote: string) => done(commands.gitPush(repoRoot, remote)),
    stashAll: (repoRoot: string) => done(commands.gitStashAll(repoRoot)),
    stashPop: (repoRoot: string) => done(commands.gitStashPop(repoRoot)),
    init: (repoRoot: string) => done(commands.gitInit(repoRoot)),
    fileHistory: (repoRoot: string, path: string) =>
      commands.gitFileHistory(repoRoot, path) as Promise<FileCommit[]>,
    headContent: (repoRoot: string, path: string) => commands.gitHeadContent(repoRoot, path),
    blame: (repoRoot: string, path: string) =>
      commands.gitBlame(repoRoot, path) as Promise<BlameLine[]>,
    headOid: (repoRoot: string) => commands.gitHeadOid(repoRoot),
    checkpointChanges: (repoRoot: string, checkpoint: string) =>
      commands.gitCheckpointChanges(repoRoot, checkpoint),
  },
  installExplorerContextMenu: () => done(commands.installExplorerContextMenu()),
  addToPath: () => done(commands.addToPath()),
  removeFromPath: () => done(commands.removeFromPath()),
  isOnPath: () => commands.isOnPath(),
};
