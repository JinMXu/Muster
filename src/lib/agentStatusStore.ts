import { useSyncExternalStore } from "react";
import { events } from "./bindings";
import type { AgentAttention, AgentKind, AgentState, AgentStatusRow, Uuid } from "./types";

/// A live agent status row (agent and state both present).
export interface LiveAgentStatus {
  id: Uuid;
  agent: AgentKind;
  state: AgentState;
  attention: AgentAttention | null;
}

/// A cross-window agent row with display title and project name, only
/// present in the `all-agent-status` snapshot payload.
export interface AllAgentStatusRow extends LiveAgentStatus {
  title: string;
  project: string;
}

type StatusMap = Record<Uuid, LiveAgentStatus>;
type AllMap = Record<Uuid, AllAgentStatusRow>;

/// Shared single-listener stores for the two agent-status feeds.
///
/// `agent-status-changed` is emitted per window with INCREMENTAL rows
/// (upserts, and null-agent rows for removals), so the per-window map
/// accumulates — rebuilding it from the latest event alone (the old
/// per-component hooks) made concurrent agents in one window overwrite each
/// other's rows. `all-agent-status` carries the full cross-window snapshot,
/// so that map is replaced wholesale.
///
/// One Tauri listener per feed for the whole window no matter how many
/// components subscribe (Header, InfoPanel and every TerminalPane used to
/// register their own, multiplying setState per event).

interface Cell<T> {
  get: () => T;
  subscribe: (listener: () => void) => () => void;
  set: (next: T) => void;
}

function createCell<T>(initial: T): Cell<T> {
  let snapshot = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    set(next) {
      if (next === snapshot) return;
      snapshot = next;
      for (const l of listeners) l();
    },
  };
}

const EMPTY: StatusMap = {};
const ALL_EMPTY: AllMap = {};

const windowCell = createCell<StatusMap>(EMPTY);
const allCell = createCell<AllMap>(ALL_EMPTY);

let started = false;

/// Register the two feed listeners once, on first subscription. They stay
/// registered for the lifetime of the webview: quiet polls emit nothing, so
/// an idle listener costs nothing, and re-registering per mount/unmount only
/// invites missed events.
function ensureFeeds() {
  if (started) return;
  started = true;

  events.agentStatusChanged.listen((event) => {
    const next = { ...windowCell.get() };
    for (const row of event.payload.sessions) {
      if (row.agent && row.state) {
        next[row.id] = {
          id: row.id,
          agent: row.agent,
          state: row.state,
          attention: row.attention,
        };
      } else {
        delete next[row.id];
      }
    }
    windowCell.set(next);
  });

  events.allAgentStatus.listen((event) => {
    const next: AllMap = {};
    for (const row of event.payload.sessions) {
      if (row.agent && row.state) {
        next[row.id] = {
          id: row.id,
          agent: row.agent,
          state: row.state,
          attention: row.attention,
          title: row.title ?? "",
          project: row.project ?? "",
        };
      }
    }
    allCell.set(next);
  });
}

/// Live coding-agent status per session in THIS window, fed by the backend's
/// incremental `agent-status-changed` event. Sessions without an agent are
/// simply absent from the map.
export function useAgentStatus(): StatusMap {
  return useSyncExternalStore(
    (listener) => {
      ensureFeeds();
      return windowCell.subscribe(listener);
    },
    windowCell.get,
    windowCell.get
  );
}

/// Cross-window agent status (every window's agents, with title/project),
/// fed by the full `all-agent-status` snapshot event.
export function useAllAgentStatus(): AllMap {
  return useSyncExternalStore(
    (listener) => {
      ensureFeeds();
      return allCell.subscribe(listener);
    },
    allCell.get,
    allCell.get
  );
}
