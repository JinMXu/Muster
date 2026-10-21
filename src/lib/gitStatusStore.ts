import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { GitStatusInfo } from "./types";
import { api } from "./invoke";

/// Shared git-status cache: one poller per repo root, fan-out to every
/// subscriber (GitPanel, FileTree decorations, ...), so multiple panels
/// never duplicate `git_status` calls.
///
/// The poll is deliberately cheap: each tick asks for the lightweight
/// index/HEAD revision and only requests the full status payload when it
/// moved. Full scans also run on `fs-changed` / `file-saved` (worktree edits
/// don't move the revision), on window focus, and as a slow fallback.
///
/// Panels pass `active` for whether they are currently visible; a root with
/// only hidden subscribers polls at the idle cadence and never forces a
/// refresh on file events.
const POLL_ACTIVE_MS = 8000;
const POLL_IDLE_MS = 30000;
/// Fallback full scan (catches worktree edits the file watcher didn't see,
/// e.g. changes in directories the tree hasn't loaded).
const FULL_REFRESH_ACTIVE_MS = 30000;
const FULL_REFRESH_IDLE_MS = 120000;
const FS_DEBOUNCE_MS = 600;
/// Minimum gap between forced scans while a burst of file events streams in.
const FORCED_REFRESH_GAP_MS = 1000;
/// A panel becoming visible refreshes if the snapshot is older than this
/// (matches the old always-on 8s cadence, without scanning on every switch).
const ACTIVATE_REFRESH_AFTER_MS = 8000;

type Listener = (info: GitStatusInfo) => void;

interface Entry {
  info: GitStatusInfo | null;
  /// Cached JSON of `info` — comparing against the previous full response
  /// costs one stringify of the new payload instead of two.
  json: string;
  revision: string;
  listeners: Set<Listener>;
  /// Number of subscribers that are currently visible.
  active: number;
  timer: ReturnType<typeof setInterval> | null;
  intervalMs: number;
  lastFull: number;
  refreshing: boolean;
  pendingFull: boolean;
  pendingForce: boolean;
}

const entries = new Map<string, Entry>();
const forcedTimers = new Map<string, ReturnType<typeof setTimeout>>();
let globalListenersReady = false;

function hasActive(e: Entry): boolean {
  return e.active > 0;
}

function fullRefreshInterval(e: Entry): number {
  return hasActive(e) ? FULL_REFRESH_ACTIVE_MS : FULL_REFRESH_IDLE_MS;
}

async function fullRefresh(root: string, force: boolean) {
  const e = entries.get(root);
  if (!e) return;
  if (e.refreshing) {
    e.pendingFull = true;
    e.pendingForce = e.pendingForce || force;
    return;
  }
  e.refreshing = true;
  try {
    const next = await api.gitStatus(root, force);
    const cur = entries.get(root);
    if (!cur) return;
    cur.lastFull = Date.now();
    cur.revision = next.revision;
    // Skip the fan-out when nothing changed: listeners re-render on every
    // notification, and most refreshes return an identical snapshot.
    const json = JSON.stringify(next);
    if (cur.info && json === cur.json) return;
    cur.json = json;
    cur.info = next;
    for (const l of cur.listeners) l(next);
  } catch {
    // Transient git error (lock, repo vanished): keep the last snapshot.
  } finally {
    const cur = entries.get(root);
    if (cur) {
      cur.refreshing = false;
      if (cur.pendingFull) {
        const forceNext = cur.pendingForce;
        cur.pendingFull = false;
        cur.pendingForce = false;
        void fullRefresh(root, forceNext);
      }
    }
  }
}

async function tick(root: string) {
  const e = entries.get(root);
  if (!e || e.refreshing || document.hidden) return;
  try {
    if (Date.now() - e.lastFull >= fullRefreshInterval(e)) {
      await fullRefresh(root, false);
      return;
    }
    const revision = await api.gitStatusRevision(root);
    const cur = entries.get(root);
    if (!cur || document.hidden || revision === cur.revision) return;
    await fullRefresh(root, false);
  } catch {
    // Revision probe failed; the fallback full scan still runs on cadence.
  }
}

function scheduleTimer(root: string) {
  const e = entries.get(root);
  if (!e) return;
  const intervalMs = hasActive(e) ? POLL_ACTIVE_MS : POLL_IDLE_MS;
  if (e.timer && e.intervalMs === intervalMs) return;
  if (e.timer) clearInterval(e.timer);
  e.intervalMs = intervalMs;
  e.timer = setInterval(() => void tick(root), intervalMs);
}

/// Debounced forced refresh after a file event. Changes that arrive while a
/// scan is cooling down get a trailing scan, so the last write is never lost.
function scheduleForcedRefresh(root: string) {
  const e = entries.get(root);
  if (!e || !hasActive(e) || document.hidden) return;
  const prev = forcedTimers.get(root);
  if (prev) clearTimeout(prev);
  const run = () => {
    forcedTimers.delete(root);
    const cur = entries.get(root);
    if (!cur || !hasActive(cur)) return;
    const wait = FORCED_REFRESH_GAP_MS - (Date.now() - cur.lastFull);
    if (wait > 0) {
      forcedTimers.set(root, setTimeout(run, wait));
      return;
    }
    void fullRefresh(root, true);
  };
  forcedTimers.set(root, setTimeout(run, FS_DEBOUNCE_MS));
}

function normalizePath(p: string): string {
  return p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
}

function dirInsideRoot(dir: string, root: string): boolean {
  const d = normalizePath(dir);
  const r = normalizePath(root);
  return d === r || d.startsWith(`${r}\\`);
}

function forceActiveRoots() {
  for (const [root, e] of entries) {
    if (hasActive(e)) scheduleForcedRefresh(root);
  }
}

function ensureGlobalListeners() {
  if (globalListenersReady) return;
  globalListenersReady = true;
  void listen<{ dir: string }>("fs-changed", (event) => {
    const dir = event.payload.dir;
    for (const [root, e] of entries) {
      if (hasActive(e) && dirInsideRoot(dir, root)) scheduleForcedRefresh(root);
    }
  });
  void listen("file-saved", forceActiveRoots);
  window.addEventListener("focus", forceActiveRoots);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) forceActiveRoots();
  });
}

/// Force an immediate re-fetch for a root (e.g. after `git init`); a no-op
/// when nothing is subscribed.
export function refreshGitStatus(root: string) {
  void fullRefresh(root, true);
}

/// Latest git status for a repo root, kept fresh by the shared poller.
/// Null until the first fetch resolves (or when `root` is null). `active` is
/// false for panels that are mounted but not visible.
export function useGitStatus(root: string | null, active = true): GitStatusInfo | null {
  const [info, setInfo] = useState<GitStatusInfo | null>(null);
  useEffect(() => {
    if (!root) {
      setInfo(null);
      return;
    }
    let e = entries.get(root);
    if (!e) {
      e = {
        info: null,
        json: "",
        // Sentinel: the first revision probe always differs until a full
        // snapshot has landed.
        revision: "\u0000",
        listeners: new Set(),
        active: 0,
        timer: null,
        intervalMs: 0,
        lastFull: 0,
        refreshing: false,
        pendingFull: false,
        pendingForce: false,
      };
      entries.set(root, e);
      ensureGlobalListeners();
      void fullRefresh(root, true);
    }
    const entry = e;
    const listener: Listener = (i) => setInfo(i);
    entry.listeners.add(listener);
    if (active) entry.active += 1;
    if (entry.info) setInfo(entry.info);
    else setInfo(null);
    scheduleTimer(root);
    if (active && Date.now() - entry.lastFull > ACTIVATE_REFRESH_AFTER_MS) {
      void fullRefresh(root, true);
    }
    return () => {
      const cur = entries.get(root);
      if (!cur) return;
      cur.listeners.delete(listener);
      if (active) cur.active = Math.max(0, cur.active - 1);
      if (cur.listeners.size === 0) {
        if (cur.timer) clearInterval(cur.timer);
        cur.timer = null;
        entries.delete(root);
        const pending = forcedTimers.get(root);
        if (pending) {
          clearTimeout(pending);
          forcedTimers.delete(root);
        }
      } else {
        scheduleTimer(root);
      }
    };
  }, [root, active]);
  return info;
}
