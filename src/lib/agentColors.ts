import type { AgentAttention, AgentState } from "./types";
import type { TKey } from "./i18n/context";

/// The agent-state dot vocabulary, shared by the tab strip (Header), the
/// sidebar's project rows, the agent mini bar, the command palette and the
/// Info panel — one definition so a state reads the same color everywhere
/// (five components used to carry their own copy).
export type DotKind = AgentState | "approval";

/// Tailwind dot-class per kind. `approval` outranks any state: the backend
/// reports it as `waiting` plus an attention flag.
export const DOT_CLASS: Record<DotKind, string> = {
  approval: "bg-rose-400 animate-pulse",
  working: "bg-emerald-400",
  waiting: "bg-amber-400 animate-pulse",
  done: "bg-sky-400",
};

/// Which dot a live agent status shows.
export function dotKind(r: {
  state: AgentState;
  attention: AgentAttention | null;
}): DotKind {
  return r.attention === "approval" ? "approval" : r.state;
}

/// Translation key for each kind's state label (the agent mini bar and the
/// Info panel's state pill share it).
export const STATE_LABEL_KEY: Record<DotKind, TKey> = {
  approval: "agents.state.approval",
  working: "agents.state.working",
  waiting: "agents.state.waiting",
  done: "agents.state.done",
};
