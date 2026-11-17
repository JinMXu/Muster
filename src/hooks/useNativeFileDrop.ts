import { useEffect } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { api } from "../lib/invoke";
import { formatFilePaths } from "../lib/terminalPaste";
import { setExternalDropTarget } from "../lib/externalDropStore";

/// Route OS-originated file drags (Explorer) onto the terminal pane under
/// the cursor. Tauri's `dragDropEnabled` intercepts those before any HTML5
/// drop event fires, so this can't live in TerminalPane's onDrop — events
/// arrive window-global with physical coordinates and we hit-test
/// `[data-terminal-pane]` ourselves. In-page drags (FileTree → pane) keep
/// using HTML5 dataTransfer and never produce these events, so the two
/// paths can't double-insert.
export function useNativeFileDrop() {
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "leave") {
          setExternalDropTarget(null);
        } else if (p.type === "drop") {
          const sessionId = paneAt(p.position);
          setExternalDropTarget(null);
          if (sessionId && p.paths.length > 0) {
            // Same insertable form as paste: quoted paths, no trailing
            // newline, so the user composes the rest of the command.
            api.sendText(sessionId, formatFilePaths(p.paths));
          }
        } else {
          // enter/over: only carry a position; hover feedback via the store.
          setExternalDropTarget(paneAt(p.position));
        }
      })
      .then((fn) => {
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
