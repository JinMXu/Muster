import { useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import {
  attachedDiffIds,
  hostsVersion,
  subscribeHosts,
  visitedHosts,
} from "../lib/diffViewRegistry";
import DiffPane from "./DiffPane";

/// Renders every visited diff pane exactly once, here at the app root,
/// portaled into registry-owned host elements (see diffViewRegistry). Pane
/// slots only attach/detach the host DOM, so the DiffPane — Monaco editor,
/// fetched diff, scroll position — survives tab/zoom/project switches
/// instead of remounting and re-fetching on every switch. `visible` (host
/// attached to a slot) lets DiffPane pause polling while parked.
export default function DiffHosts() {
  useSyncExternalStore(subscribeHosts, hostsVersion);
  const attached = attachedDiffIds();
  return (
    <>
      {visitedHosts().map(([id, host]) =>
        createPortal(<DiffPane diffId={id} visible={attached.has(id)} />, host, id)
      )}
    </>
  );
}
