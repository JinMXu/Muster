import type { AppStateView, ProjectView, SessionInfo, Uuid } from "./types";

/// Resolve the focused pane's session ID from the selected tab of the
/// selected project. Returns null if no terminal session is focused.
export function getFocusedSessionId(state: AppStateView | null): Uuid | null {
  if (!state) return null;
  const project = state.projects.find((p) => p.id === state.selected_project_id);
  if (!project) return null;
  const tab = project.tabs.find((t) => t.id === project.selected_tab_id);
  if (!tab) return null;
  for (const col of tab.columns) {
    for (const pane of col.panes) {
      if (pane.id === tab.focused_pane_id && pane.content.kind === "session") {
        return pane.content.id;
      }
    }
  }
  return null;
}

/// Resolve the focused pane's file ID from the selected tab of the
/// selected project. Returns null if no file pane is focused.
export function getFocusedFileId(state: AppStateView | null): Uuid | null {
  if (!state) return null;
  const project = state.projects.find((p) => p.id === state.selected_project_id);
  if (!project) return null;
  const tab = project.tabs.find((t) => t.id === project.selected_tab_id);
  if (!tab) return null;
  for (const col of tab.columns) {
    for (const pane of col.panes) {
      if (pane.id === tab.focused_pane_id && pane.content.kind === "file") {
        return pane.content.id;
      }
    }
  }
  return null;
}

/// The sessions the Info panel shows for a project.
export interface PanelSessions {
  /// One per terminal pane of the selected tab, in layout order (columns
  /// left-to-right, panes top-to-bottom). When the tab has no resolvable
  /// session pane, just `selected` (if any).
  cards: SessionInfo[];
  /// The focused pane's session; else the tab's first; else any of the
  /// project's sessions. Drives the panel's directory/process/port rows.
  selected: SessionInfo | null;
  /// Session ids of panes not yet in `sessions` (a pane opened since the
  /// last poll) — the caller should poll again.
  missing: Uuid[];
}

/// Resolve the selected tab's session panes of `project` against the polled
/// `sessions` list.
export function resolvePanelSessions(project: ProjectView | null, sessions: SessionInfo[]): PanelSessions {
  const tab = project?.tabs.find((t) => t.id === project.selected_tab_id) ?? null;
  const byId = new Map(sessions.map((s) => [s.id, s]));
  const cards: SessionInfo[] = [];
  const missing: Uuid[] = [];
  let focused: SessionInfo | null = null;
  for (const col of tab?.columns ?? []) {
    for (const pane of col.panes) {
      if (pane.content.kind !== "session") continue;
      const s = byId.get(pane.content.id);
      if (!s) {
        missing.push(pane.content.id);
        continue;
      }
      cards.push(s);
      if (pane.id === tab?.focused_pane_id) focused = s;
    }
  }
  const selected = focused ?? cards[0] ?? sessions.find((s) => s.project_id === project?.id) ?? null;
  if (cards.length === 0 && selected) cards.push(selected);
  return { cards, selected, missing };
}
