import type { Uuid } from "../lib/types";
import type { LiveAgentStatus } from "./useAgentStatus";
import {
  useAllAgentStatus,
  type AllAgentStatusRow,
} from "../lib/agentStatusStore";

/// A cross-window agent row, including the session's display title and the
/// project name (only present in the `all-agent-status` snapshot payload).
export type AllAgentStatus = AllAgentStatusRow;

/// Cross-window coding-agent status, fed by the backend's global
/// `all-agent-status` event (the FULL current snapshot, all windows). Shared
/// single listener in `lib/agentStatusStore`.
export function useAllAgents(): Record<Uuid, AllAgentStatus> {
  return useAllAgentStatus();
}
