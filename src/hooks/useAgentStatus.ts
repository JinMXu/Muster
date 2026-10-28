import type { AgentKind, Uuid } from "../lib/types";
import {
  useAgentStatus as useSharedAgentStatus,
  type LiveAgentStatus,
} from "../lib/agentStatusStore";

/// A live agent status: the hook only stores rows whose agent/state are
/// present (removal rows are deleted instead).
export type { LiveAgentStatus };

/// Live coding-agent status per session, fed by the backend's
/// `agent-status-changed` event (emitted when a session's agent detection
/// changes). Returns a map of session id -> status; sessions without an
/// agent are simply absent.
///
/// Backed by a single shared listener (see `lib/agentStatusStore`): every
/// consumer of this hook used to register its own Tauri listener, and each
/// event then fanned out into one setState per consumer.
export function useAgentStatus(): Record<Uuid, LiveAgentStatus> {
  return useSharedAgentStatus();
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
    case "atomcode":
      return "AtomCode";
    case "copilot":
      return "GitHub Copilot";
    case "cursor":
      return "Cursor";
    case "kiro":
      return "Kiro";
    case "amp":
      return "Amp";
    case "droid":
      return "Factory Droid";
    case "crush":
      return "Crush";
    case "cline":
      return "Cline";
    case "continue_cli":
      return "Continue";
    case "kilo":
      return "Kilo Code";
    case "grok":
      return "Grok Build";
    case "open_hands":
      return "OpenHands";
    case "pi":
      return "Pi";
    case "vibe":
      return "Mistral Vibe";
    case "qwen":
      return "Qwen Code";
    case "qoder":
      return "Qoder";
    case "trae":
      return "Trae CLI";
    case "code_buddy":
      return "CodeBuddy";
    case "comate":
      return "Comate";
  }
}
