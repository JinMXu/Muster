import { useEffect } from "react";

import { events } from "../lib/bindings";
import { api } from "../lib/invoke";
import { formatFilePaths } from "../lib/terminalPaste";
import { setExternalDropTarget } from "../lib/externalDropStore";

/// Route OS-originated file drags (Explorer) onto the terminal pane under
/// the cursor. The events come from the native chained drop target
/// (services/native_drop.rs) as window-global payloads with physical
/// coordinates, so we hit-test `[data-terminal-pane]` ourselves. In-page
/// drags (FileTree → pane) keep using HTML5 dataTransfer and never produce
/// these events, so the two paths can't double-insert.
export function useNativeFileDrop() {
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    events.nativeFileDrop.listen((event) => {
      const p = event.payload;
      if (p.kind === "leave") {
        setExternalDropTarget(null);
      } else if (p.kind === "drop") {
        const sessionId = paneAt(p);
        setExternalDropTarget(null);
        if (sessionId && p.paths.length > 0) {
          // Same insertable form as paste: quoted paths, no trailing
          // newline, so the user composes the rest of the command.
          api.sendText(sessionId, formatFilePaths(p.paths));
        }
      } else {
        // enter/over carry the hover position only; feed the ring highlight.
        setExternalDropTarget(paneAt(p));
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
      setExternalDropTarget(null);
    };
  }, []);
}

/// Physical → CSS px (÷ devicePixelRatio), then the pane element under the
/// point. A hidden tab's pane can't be hit — invisible means unclickable,
/// which is the behavior we want.
function paneAt(position: { x: number; y: number }): string | null {
  const el = document.elementFromPoint(
    position.x / window.devicePixelRatio,
    position.y / window.devicePixelRatio,
  );
  return el?.closest("[data-terminal-pane]")?.getAttribute("data-terminal-pane") ?? null;
}
