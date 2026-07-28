import { useEffect, useState } from "react";
import type { AppStateView } from "./types";
import { api } from "./invoke";

/// Anchored right-panel context for the selected project.
/// - `root`: the panel anchor — the pinned custom_directory if set, else the
///   toplevel of the nearest git repository containing the session's cwd (so
///   `cd` inside a repo does NOT re-root the panels), else the live cwd.
/// - `cwd`: the session's live working directory (tracked via OSC 9;9).
export interface ProjectAnchor {
  root: string | null;
  cwd: string | null;
}

/// Cache of cwd → resolved repo root, so the 2s poll only hits the backend
/// when the cwd actually changes (module scope: shared by every consumer).
const rootCache = new Map<string, string>();

/// Resolve the selected project's anchored root and live cwd. Polls every 2s
/// because cwd changes don't emit state-changed.
export function useProjectCwd(state: AppStateView | null): ProjectAnchor {
  const project = state?.projects.find((p) => p.id === state.selected_project_id) ?? null;
  const projectId = project?.id ?? null;
  const pinned = project?.custom_directory ?? null;
  const [cwd, setCwd] = useState<string | null>(pinned);
  const [root, setRoot] = useState<string | null>(pinned);

  // Track the session's live cwd (pinned projects pin the cwd too).
  useEffect(() => {
    if (pinned) {
      setCwd(pinned);
      return;
    }
    if (!projectId) {
      setCwd(null);
      return;
    }
    let alive = true;
    const tick = () =>
      api.listAllSessions().then((ss) => {
        if (!alive) return;
        const s = ss.find((x) => x.project_id === projectId);
        setCwd(s?.working_directory ?? null);
      });
    tick();
    const i = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(i);
    };
  }, [projectId, pinned]);

  // Anchor the panels: the pinned directory wins; otherwise resolve the cwd
  // to its containing repo's toplevel, falling back to the cwd itself.
  useEffect(() => {
    if (pinned) {
      setRoot(pinned);
      return;
    }
    if (!cwd) {
      setRoot(null);
      return;
    }
    const cached = rootCache.get(cwd);
    if (cached !== undefined) {
      setRoot(cached);
      return;
    }
    let alive = true;
    api.resolveProjectRoot(cwd).then((r) => {
      rootCache.set(cwd, r);
      if (alive) setRoot(r);
    });
    return () => {
      alive = false;
    };
  }, [cwd, pinned]);

  return { root, cwd };
}
