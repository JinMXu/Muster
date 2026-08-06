import { useEffect, useState } from "react";
import type { GitStatusInfo } from "./types";
import { api } from "./invoke";

/// Shared git-status cache: one poller per repo root, fan-out to every
/// subscriber (GitPanel, FileTree decorations, ...), so multiple panels
/// never duplicate `git_status` calls. The poller stops when the last
/// subscriber for a root unsubscribes.
const POLL_MS = 3000;

type Listener = (info: GitStatusInfo) => void;

interface Entry {
  info: GitStatusInfo | null;
  /// Cached JSON of `info` — comparing against the previous poll costs one
  /// stringify of the new payload instead of two (the payload can be
  /// megabytes in a large repo).
  json: string;
  listeners: Set<Listener>;
  timer: ReturnType<typeof setInterval>;
}

const entries = new Map<string, Entry>();

async function refresh(root: string) {
  const e = entries.get(root);
  if (!e) return;
  try {
    const next = await api.gitStatus(root);
    // Skip the fan-out when nothing changed: listeners re-render on every
    // notification, and most polls return an identical snapshot. Only the
    // new payload is serialized; the previous one is kept cached on the entry.
    const json = JSON.stringify(next);
    if (e.info && json === e.json) return;
    e.json = json;
    e.info = next;
    for (const l of e.listeners) l(e.info);
  } catch {
    // Transient git error (lock, repo vanished): keep the last snapshot.
  }
}

/// Force an immediate re-fetch for a root (e.g. after `git init`); a no-op
/// when nothing is subscribed.
export function refreshGitStatus(root: string) {
  refresh(root);
}

/// Latest git status for a repo root, kept fresh by the shared poller.
/// Null until the first fetch resolves (or when `root` is null).
export function useGitStatus(root: string | null): GitStatusInfo | null {
  const [info, setInfo] = useState<GitStatusInfo | null>(null);
  useEffect(() => {
    if (!root) {
      setInfo(null);
      return;
    }
    let e = entries.get(root);
    if (!e) {
      e = { info: null, json: "", listeners: new Set(), timer: setInterval(() => refresh(root), POLL_MS) };
      entries.set(root, e);
      refresh(root);
    }
    const listener: Listener = (i) => setInfo(i);
    e.listeners.add(listener);
    if (e.info) setInfo(e.info);
    else setInfo(null);
    return () => {
      e.listeners.delete(listener);
      if (e.listeners.size === 0) {
        clearInterval(e.timer);
        entries.delete(root);
      }
    };
  }, [root]);
  return info;
}
