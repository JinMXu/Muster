/// Diff-pane host registry — the "parking lot" for Monaco diff views.
///
/// React mounts/unmounts panes freely (tab switches, zoom, project switches),
/// but a Monaco DiffEditor is expensive to boot and a remount re-fetches the
/// diff. Host divs are therefore owned here, keyed by diff id, and panes just
/// attach/detach the host element — the same pattern as `terminalRegistry`.
/// The matching `DiffPane` components are rendered (via portals) once by
/// `DiffHosts` in App, so a parked diff stays mounted with its editor and
/// computed content intact; only its DOM moves. `pruneHosts` drops hosts
/// whose diff pane no longer exists (tab/pane/project closed), which also
/// unmounts its DiffPane.
const hosts = new Map<string, HTMLDivElement>();

/// Ids whose host is currently attached to a pane slot (i.e. actually on
/// screen). Drives DiffPane's `visible` prop: parked diffs pause polling.
const attachedIds = new Set<string>();

// Version counter + listeners so DiffHosts can re-render when hosts or
// attachment change (acquire/detach happen in effects, after App's render).
let version = 0;
const listeners = new Set<() => void>();
function bump(): void {
  version += 1;
  for (const l of listeners) l();
}

/// Pair with `hostsVersion` for useSyncExternalStore.
export function subscribeHosts(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function hostsVersion(): number {
  return version;
}

/// Get (or lazily create) the host element for a diff pane. Only called when
/// the pane is actually displayed, so "visited" diffs are the ones with hosts
/// — background tabs never materialize their editor until first shown.
export function acquireHost(diffId: string): HTMLDivElement {
  let el = hosts.get(diffId);
  if (!el) {
    el = document.createElement("div");
    el.className = "w-full h-full";
    hosts.set(diffId, el);
    bump();
  }
  return el;
}

/// Every visited diff (id + host element), for DiffHosts to portal into.
export function visitedHosts(): [string, HTMLDivElement][] {
  return [...hosts.entries()];
}

/// Ids currently attached to a pane slot.
export function attachedDiffIds(): ReadonlySet<string> {
  return attachedIds;
}

export function markAttached(diffId: string): void {
  attachedIds.add(diffId);
  bump();
}

export function markDetached(diffId: string): void {
  attachedIds.delete(diffId);
  bump();
}

/// Drop the host for a diff and detach it from the DOM. The DiffPane portal
/// into it unmounts on the next render once DiffHosts re-reads the registry.
export function releaseHost(diffId: string): void {
  const el = hosts.get(diffId);
  if (!el) return;
  el.remove();
  hosts.delete(diffId);
  attachedIds.delete(diffId);
  bump();
}

/// Drop every host whose diff id is not in `active`.
export function pruneHosts(active: ReadonlySet<string>): void {
  for (const id of [...hosts.keys()]) {
    if (!active.has(id)) releaseHost(id);
  }
}
