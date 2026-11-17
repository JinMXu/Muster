import { describe, expect, it } from "vitest";
import { resolvePanelSessions } from "./sessionUtils";
import type { PaneContent, ProjectView, SessionInfo } from "./types";

function session(id: string, project_id = "p1"): SessionInfo {
  return {
    id,
    project_id,
    title: id,
    working_directory: `D:\\${id}`,
    shell_name: "PowerShell",
    has_exited: false,
    pid: 1,
    agent: null,
    agent_command: null,
  };
}

const term = (id: string): PaneContent => ({ kind: "session", id });
const file = (id: string): PaneContent => ({ kind: "file", id });

/// A project whose selected tab lays `columns` out left-to-right, each a
/// top-to-bottom stack of pane contents; pane ids are `pane-<content id>`.
function project(columns: PaneContent[][], focused: string | null): ProjectView {
  return {
    id: "p1",
    name: "p1",
    custom_name: null,
    custom_directory: null,
    selected_tab_id: "t1",
    session_count: 0,
    tabs: [
      {
        id: "t1",
        custom_name: null,
        display_title: null,
        focused_pane_id: focused ? `pane-${focused}` : "",
        is_zoomed: false,
        pane_count: columns.flat().length,
        columns: columns.map((panes, i) => ({
          id: `c${i}`,
          weight: 1,
          panes: panes.map((content) => ({ id: `pane-${content.id}`, content, weight: 1 })),
        })),
      },
    ],
  };
}

const ids = (list: SessionInfo[]) => list.map((s) => s.id);

describe("resolvePanelSessions", () => {
  it("returns one card per session pane in layout order and selects the focused one", () => {
    const sessions = [session("b"), session("a"), session("c")];
    const r = resolvePanelSessions(project([[term("a")], [term("b"), term("c")]], "b"), sessions);
    expect(ids(r.cards)).toEqual(["a", "b", "c"]);
    expect(r.selected?.id).toBe("b");
    expect(r.missing).toEqual([]);
  });

  it("falls back to the tab's first session when a file pane is focused", () => {
    const sessions = [session("a"), session("b")];
    const r = resolvePanelSessions(project([[term("a")], [file("f"), term("b")]], "f"), sessions);
    expect(ids(r.cards)).toEqual(["a", "b"]);
    expect(r.selected?.id).toBe("a");
  });

  it("reports panes the session poll has not seen yet", () => {
    const r = resolvePanelSessions(project([[term("a")], [term("new")]], "new"), [session("a")]);
    expect(ids(r.cards)).toEqual(["a"]);
    expect(r.selected?.id).toBe("a");
    expect(r.missing).toEqual(["new"]);
  });

  it("falls back to any project session when the tab has no terminal pane", () => {
    const sessions = [session("other", "p2"), session("x")];
    const r = resolvePanelSessions(project([[file("f")]], "f"), sessions);
    expect(r.selected?.id).toBe("x");
    expect(ids(r.cards)).toEqual(["x"]);
  });

  it("is empty without a project", () => {
    expect(resolvePanelSessions(null, [session("a")])).toEqual({ cards: [], selected: null, missing: [] });
  });
});
