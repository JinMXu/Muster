import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppStateView } from "./types";
import { api } from "./invoke";

/// Anchored right-panel context for the selected project.
/// - `root`: the panel anchor - the pinned custom_directory if set, else the
///   toplevel of the nearest git repository containing the session's cwd (so
///   `cd` inside a repo does NOT re-root the panels), else the live cwd.
///   When the session `cd`s into a linked git worktree, the root follows the
///   worktree's own toplevel (so Files/Git/Info panels re-root there).
/// - `cwd`: the session's live working directory (tracked via OSC 9;9,
///   with immediate event-driven updates - no poll wait).
export interface ProjectAnchor {
  root: string | null;
  cwd: string | null;
}

/// Cache of cwd -> resolved repo root, so the poll only hits the backend
/// when the cwd actually changes (module scope: shared by every consumer).
/// Bounded to 50 entries (LRU) to prevent unbounded growth over long sessions.
const rootCache = new Map<string, string>();
const ROOT_CACHE_MAX = 50;
function cacheRoot(cwd: string, root: string) {
  if (rootCache.size >= ROOT_CACHE_MAX) {
    const first = rootCache.keys().next().value;
    if (first !== undefined) rootCache.delete(first);
  }
  rootCache.set(cwd, root);
}

/// Resolve the selected project's anchored root and live cwd. Listens for the
/// `session-cwd-changed` event (emitted immediately when the shell's cwd
/// changes, e.g. after a `cd` into a worktree) and falls back to a 5s poll.
export function useProjectCwd(state: AppStateView | null): ProjectAnchor {
  const project = state?.projects.find((p) => p.id === state.selected_project_id) ?? null;
  const projectId = project?.id ?? null;
  const pinned = project?.custom_directory ?? null;
  const [cwd, setCwd] = useState<string | null>(pinned);
  const [root, setRoot] = useState<string | null>(pinned);

  // Track the session's live cwd (pinned projects pin the cwd too).
  // Uses an immediate event listener PLUS a 5s fallback poll.
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
    // Event-driven: re-read sessions immediately when any session's cwd
    // changes, so worktree re-rooting is instant (no poll delay).
    let unlisten: UnlistenFn | null = null;
    listen("session-cwd-changed", () => {
      if (alive) tick();
    }).then((u) => {
      if (!alive) { u(); return; }
      unlisten = u;
    });
    const i = setInterval(tick, 5000);
    return () => {
      alive = false;
      clearInterval(i);
      unlisten?.();
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
      cacheRoot(cwd, r);
      if (alive) setRoot(r);
    });
    return () => {
      alive = false;
    };
  }, [cwd, pinned]);

  return { root, cwd };
}
