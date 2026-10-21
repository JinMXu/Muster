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

const EMPTY: ProjectAnchor = { root: null, cwd: null };
const POLL_MS = 5000;

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

/// One shared poller per selected project. Files, Git and Info panels all
/// call `useProjectCwd` while mounted; without sharing, each held its own
/// 5s `list_all_sessions` interval and `session-cwd-changed` listener.
interface Entry {
  projectId: string;
  pinned: string | null;
  anchor: ProjectAnchor;
  listeners: Set<() => void>;
  timer: ReturnType<typeof setInterval> | null;
  unlisten: UnlistenFn | null;
  disposed: boolean;
  pending: boolean;
  /// Increments per root resolution, so a late response for an old cwd is
  /// dropped instead of overwriting the new anchor.
  rootSeq: number;
}

const entries = new Map<string, Entry>();
let visibilityBound = false;

function notify(e: Entry) {
  for (const l of e.listeners) l();
}

function setAnchor(e: Entry, cwd: string | null, root: string | null) {
  if (e.anchor.cwd === cwd && e.anchor.root === root) return;
  e.anchor = { cwd, root };
  notify(e);
}

async function resolveRoot(e: Entry) {
  if (e.pinned) {
    setAnchor(e, e.pinned, e.pinned);
    return;
  }
  const cwd = e.anchor.cwd;
  if (!cwd) {
    setAnchor(e, null, null);
    return;
  }
  const cached = rootCache.get(cwd);
  if (cached !== undefined) {
    setAnchor(e, cwd, cached);
    return;
  }
  const seq = ++e.rootSeq;
  const root = await api.resolveProjectRoot(cwd).catch(() => cwd);
  if (e.disposed || e.pinned || seq !== e.rootSeq) return;
  cacheRoot(cwd, root);
  setAnchor(e, cwd, root);
}

async function tick(e: Entry) {
  if (e.pinned || document.hidden || e.disposed || e.pending) return;
  e.pending = true;
  try {
    const sessions = await api.listAllSessions().catch(() => null);
    if (!sessions || e.disposed || e.pinned) return;
    const cwd = sessions.find((s) => s.project_id === e.projectId)?.working_directory ?? null;
    if (cwd === e.anchor.cwd) return;
    setAnchor(e, cwd, e.anchor.root);
    void resolveRoot(e);
  } finally {
    e.pending = false;
  }
}

function ensureVisibilityListener() {
  if (visibilityBound) return;
  visibilityBound = true;
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) return;
    for (const e of entries.values()) void tick(e);
  });
}

function startEntry(e: Entry) {
  if (e.pinned) return;
  void tick(e);
  e.timer = setInterval(() => void tick(e), POLL_MS);
  listen("session-cwd-changed", () => void tick(e)).then((u) => {
    if (e.disposed) u();
    else e.unlisten = u;
  });
}

function stopEntry(e: Entry) {
  if (e.timer) clearInterval(e.timer);
  e.timer = null;
  e.unlisten?.();
  e.unlisten = null;
}

function disposeEntry(e: Entry) {
  e.disposed = true;
  stopEntry(e);
}

function acquire(projectId: string, pinned: string | null): Entry {
  let e = entries.get(projectId);
  if (!e) {
    e = {
      projectId,
      pinned,
      anchor: pinned ? { cwd: pinned, root: pinned } : EMPTY,
      listeners: new Set(),
      timer: null,
      unlisten: null,
      disposed: false,
      pending: false,
      rootSeq: 0,
    };
    entries.set(projectId, e);
    ensureVisibilityListener();
    startEntry(e);
    return e;
  }
  if (e.pinned !== pinned) {
    stopEntry(e);
    e.pinned = pinned;
    e.rootSeq += 1;
    if (pinned) {
      setAnchor(e, pinned, pinned);
    } else {
      startEntry(e);
      if (e.anchor.cwd) void resolveRoot(e);
    }
  }
  return e;
}

/// Resolve the selected project's anchored root and live cwd. A single
/// module-level poller (5s fallback + `session-cwd-changed` events) serves
/// every subscribed panel.
export function useProjectCwd(state: AppStateView | null): ProjectAnchor {
  const project = state?.projects.find((p) => p.id === state.selected_project_id) ?? null;
  const projectId = project?.id ?? null;
  const pinned = project?.custom_directory ?? null;
  const [anchor, setAnchor] = useState<ProjectAnchor>(EMPTY);

  useEffect(() => {
    if (!projectId) {
      setAnchor(EMPTY);
      return;
    }
    const e = acquire(projectId, pinned);
    const listener = () => setAnchor(e.anchor);
    e.listeners.add(listener);
    listener();
    return () => {
      const cur = entries.get(projectId);
      if (!cur) return;
      cur.listeners.delete(listener);
      if (cur.listeners.size === 0) {
        disposeEntry(cur);
        entries.delete(projectId);
      }
    };
  }, [projectId, pinned]);

  return anchor;
}
