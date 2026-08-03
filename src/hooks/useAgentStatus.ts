import { useMemo } from "react";
import { useTauriEvent } from "./useTauriEvent";
import type { AgentKind, AgentState, AgentStatusEvent, Uuid } from "../lib/types";

/// A live agent status: the hook only stores rows whose agent/state are
/// present (removal rows are deleted instead).
export interface LiveAgentStatus {
  id: Uuid;
  agent: AgentKind;
  state: AgentState;
}

/// Live coding-agent status per session, fed by the backend's
/// `agent-status-changed` event (emitted when a session's agent detection
/// changes). Returns a map of session id -> status; sessions without an
/// agent are simply absent.
export function useAgentStatus(): Record<Uuid, LiveAgentStatus> {
  const [event] = useTauriEvent<AgentStatusEvent | null>("agent-status-changed", null);
  return useMemo(() => {
    const map: Record<Uuid, LiveAgentStatus> = {};
    for (const row of event?.sessions ?? []) {
      if (row.agent && row.state) map[row.id] = row as LiveAgentStatus;
      else delete map[row.id];
    }
    return map;
  }, [event]);
}

/// Display name for an agent kind (brand names are not translated).
export function agentLabel(agent: AgentKind): string {
  switch (agent) {
    case "opencode":
      return "opencode";
    case "claude_code":
      return "Claude Code";
    case "codex":
      return "Codex";
    case "kimi_code":
      return "Kimi Code";
    case "aider":
      return "aider";
    case "gemini":
      return "Gemini CLI";
    case "goose":
      return "Goose";
  }
}
