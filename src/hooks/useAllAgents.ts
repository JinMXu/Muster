import { useMemo } from "react";
import { useTauriEvent } from "./useTauriEvent";
import type { AgentStatusEvent, Uuid } from "../lib/types";
import type { LiveAgentStatus } from "./useAgentStatus";

/// A cross-window agent row, including the session's display title and the
/// project name (only present in the `all-agent-status` snapshot payload).
export interface AllAgentStatus extends LiveAgentStatus {
  /// Session display title (e.g. cwd basename), or "" when the backend
  /// couldn't resolve it (shouldn't happen for live sessions).
  title: string;
  /// Project display name for "where" in the popover, or "".
  project: string;
}

/// Cross-window coding-agent status, fed by the backend's global
/// `all-agent-status` event. Unlike `useAgentStatus` (per-window), this
/// reaches every window so an agent mini-bar in any window can show the
/// complete picture.
///
/// The event carries the FULL current snapshot (all detected agents across
/// all windows), so the whole map is replaced on each event — no
/// accumulation, no stale entries when an agent disappears.
export function useAllAgents(): Record<Uuid, AllAgentStatus> {
  const [event] = useTauriEvent<AgentStatusEvent | null>("all-agent-status", null);
  return useMemo(() => {
    const map: Record<Uuid, AllAgentStatus> = {};
    for (const row of event?.sessions ?? []) {
      // The snapshot only carries live rows (agent + state both non-null);
      // a session that finished-and-was-seen or closed is simply absent.
      if (row.agent && row.state) {
        map[row.id] = {
          id: row.id,
          agent: row.agent,
          state: row.state,
          title: row.title ?? "",
          project: row.project ?? "",
        };
      }
    }
    return map;
  }, [event]);
}