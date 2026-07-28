import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { acquire } from "../lib/terminalRegistry";
import { api } from "../lib/invoke";
import { openMenu } from "../lib/menuStore";
import { useT } from "../lib/i18n/context";

/// One terminal pane: attaches the parked xterm instance for `sessionId`
/// (owned by `terminalRegistry`) into its container div. The instance
/// outlives this component — tab switches, zoom, and project switches only
/// re-attach the DOM element, so the visible buffer is never lost.
export default function TerminalPane({
  sessionId,
  focused,
  paneKey,
}: {
  sessionId: string;
  focused: boolean;
  paneKey: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  // File-drop highlight: dragenter/dragleave nest (xterm children), so a
  // depth counter keeps the ring from flickering while moving inside.
  const [dropActive, setDropActive] = useState(false);
  const dragDepth = useRef(0);
  const { t } = useT();

  useEffect(() => {
    const entry = acquire(sessionId);
    const container = containerRef.current!;
    if (!entry.term.element) {
      entry.term.open(container);
    } else if (entry.term.element.parentElement !== container) {
      container.appendChild(entry.term.element);
    }

    const pushSize = () => {
      const { cols, rows } = entry.term;
      if (cols && rows) {
        invoke("resize_terminal", { id: sessionId, cols, rows }).catch(() => {});
      }
    };

    // Fit once attached (element may have zero size on first mount).
    const raf = requestAnimationFrame(() => {
      try {
        entry.fit.fit();
        pushSize();
      } catch (_) {
        /* container zero-sized */
      }
    });

    const ro = new ResizeObserver(() => {
      try {
        entry.fit.fit();
        pushSize();
      } catch (_) {}
    });
    ro.observe(container);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      // Deliberately NOT disposing the terminal — it stays parked in the
      // registry until the session itself is closed (see pruneSessions).
    };
  }, [sessionId]);

  useEffect(() => {
    if (focused) {
      acquire(sessionId).term.focus();
    }
  }, [focused, paneKey, sessionId]);

  return (
    <div
      ref={containerRef}
      className={`w-full h-full overflow-hidden bg-muster-bg ${
        dropActive ? "ring-1 ring-inset ring-muster-accent" : ""
      }`}
      onDragEnter={(e) => {
        if (!e.dataTransfer.types.includes("application/x-muster-path")) return;
        dragDepth.current += 1;
        setDropActive(true);
      }}
      onDragLeave={() => {
        dragDepth.current = Math.max(0, dragDepth.current - 1);
        if (dragDepth.current === 0) setDropActive(false);
      }}
      onDragOver={(e) => {
        // Only claim drags carrying a file path; pane-move drags fall
        // through to the PaneLayout drop target around us.
        if (e.dataTransfer.types.includes("application/x-muster-path")) {
          e.preventDefault();
          e.dataTransfer.dropEffect = "copy";
        }
      }}
      onDrop={(e) => {
        const path = e.dataTransfer.getData("application/x-muster-path");
        dragDepth.current = 0;
        setDropActive(false);
        if (!path) return;
        e.preventDefault();
        e.stopPropagation();
        // Quote paths with spaces; no trailing newline so the user can
        // compose the rest of the command before running it.
        api.sendText(sessionId, path.includes(" ") ? `"${path}"` : path);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        const { term } = acquire(sessionId);
        openMenu({
          x: e.clientX,
          y: e.clientY,
          items: [
            {
              label: t("terminal.copy"),
              action: () => {
                const sel = term.getSelection();
                if (sel) navigator.clipboard.writeText(sel);
              },
            },
            {
              label: t("terminal.paste"),
              action: async () => {
                const text = await navigator.clipboard.readText().catch(() => "");
                if (text) api.sendText(sessionId, text);
              },
            },
            { label: t("terminal.selectAll"), action: () => term.selectAll() },
            "sep",
            { label: t("terminal.splitRight"), action: () => api.split("right") },
            { label: t("terminal.splitDown"), action: () => api.split("bottom") },
          ],
        });
      }}
    />
  );
}
