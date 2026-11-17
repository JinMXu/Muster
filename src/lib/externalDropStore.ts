//! Module-level store for which terminal pane an OS file drag (Explorer)
//! currently hovers over. Tauri's native drag-drop events are window-global
//! (payload is just paths + position), so the hit-testing lives in
//! useNativeFileDrop and the result is shared with TerminalPane through this
//! tiny store — same shape as terminalSearch.

interface ExternalDropState {
  sessionId: string | null;
}

let state: ExternalDropState = { sessionId: null };
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

export function setExternalDropTarget(sessionId: string | null): void {
  if (state.sessionId === sessionId) return;
  state = { sessionId };
  emit();
}

export function getExternalDropTarget(): ExternalDropState {
  return state;
}

export function subscribeExternalDrop(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
