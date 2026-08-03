//! Module-level store for the terminal scrollback search bar (Ctrl+F in a
//! terminal pane). The bar lives inside TerminalPane but is opened/closed from
//! App's global keyboard handler, so the two share this tiny store instead of
//! threading props through PaneLayout.
//!
//! `nonce` increments on every open/close so TerminalPane can re-focus the
//! input even when the same session is searched again.

export interface TerminalSearchState {
  sessionId: string | null;
  nonce: number;
}

let state: TerminalSearchState = { sessionId: null, nonce: 0 };
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

export function openTerminalSearch(sessionId: string): void {
  state = { sessionId, nonce: state.nonce + 1 };
  emit();
}

export function closeTerminalSearch(): void {
  state = { sessionId: null, nonce: state.nonce + 1 };
  emit();
}

export function getTerminalSearchState(): TerminalSearchState {
  return state;
}

export function subscribeTerminalSearch(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
