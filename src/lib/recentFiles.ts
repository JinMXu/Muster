/// Cross-component UI state: recently opened files (drives the CommandPalette
/// "Recent" section) and recently closed tabs (drives Ctrl+Shift+T reopen).
///
/// The id→path/diff maps exist so a tab closed on the backend can still be
/// described synchronously: by the time `state-changed` reaches us the file /
/// diff is already dropped from the backend, so an async `file_info` lookup
/// would return nothing.

export interface RecentFile {
  path: string;
  name: string;
}

export interface DiffMeta {
  repoRoot: string;
  path: string;
  staged: boolean;
  oldRev: string | null;
  newRev: string | null;
  workdir: boolean;
}

const MAX_RECENT = 20;
const MAX_CLOSED = 10;

let recent: RecentFile[] = [];
const recentListeners = new Set<() => void>();

let closed: ClosedTab[] = [];

const filePaths = new Map<string, string>();
const diffMeta = new Map<string, DiffMeta>();

export type ClosedTabContent =
  | { kind: "session" }
  | { kind: "file"; path: string }
  | { kind: "diff"; repoRoot: string; path: string; staged?: boolean; oldRev?: string; newRev?: string; workdir?: boolean };

export interface ClosedTab {
  projectId: string | null;
  content: ClosedTabContent;
}

// ---- recent files ---------------------------------------------------------

export function pushRecentFile(path: string): void {
  const name = path.split(/[\\/]/).pop() ?? path;
  recent = [{ path, name }, ...recent.filter((r) => r.path !== path)].slice(0, MAX_RECENT);
  for (const cb of recentListeners) cb();
}

export function getRecentFiles(): RecentFile[] {
  return recent;
}

export function subscribeRecentFiles(cb: () => void): () => void {
  recentListeners.add(cb);
  return () => recentListeners.delete(cb);
}

// ---- tab content tracking (for reopening after close) ---------------------

export function trackFileTab(id: string | null, path: string): void {
  if (!id) return;
  filePaths.set(id, path);
  pushRecentFile(path);
}

export function trackDiffTab(id: string | null, meta: DiffMeta): void {
  if (!id) return;
  diffMeta.set(id, meta);
}

export function filePathOf(id: string): string | undefined {
  return filePaths.get(id);
}

export function diffMetaOf(id: string): DiffMeta | undefined {
  return diffMeta.get(id);
}

// ---- reveal-at-line (terminal link clicks, search results) ----------------
// `open_file_at` returns the tab id; the `file-reveal` event can race a newly
// mounted pane, so the target line is also parked here and consumed on mount.

const pendingReveals = new Map<string, number>();

export function setPendingReveal(id: string, line: number): void {
  pendingReveals.set(id, line);
}

export function takePendingReveal(id: string): number | null {
  const line = pendingReveals.get(id);
  if (line !== undefined) pendingReveals.delete(id);
  return line ?? null;
}

// ---- closed tab stack ------------------------------------------------------

export function pushClosedTab(tab: ClosedTab): void {
  closed = [tab, ...closed].slice(0, MAX_CLOSED);
}

export function popClosedTab(): ClosedTab | undefined {
  return closed.shift();
}
