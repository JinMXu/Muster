import type { AppStateView, Uuid } from "./types";

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
