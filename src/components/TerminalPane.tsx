import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";
import { acquire } from "../lib/terminalRegistry";
import { api } from "../lib/invoke";
import { shellQuotePath } from "../lib/shellEscape";
import { openMenu } from "../lib/menuStore";
import { useT } from "../lib/i18n/context";
import { getTerminalSearchState, subscribeTerminalSearch } from "../lib/terminalSearch";
import PasteWarning, { looksDangerousPaste } from "./PasteWarning";
import TerminalSearchBar from "./TerminalSearchBar";

/// Re-sync a re-attached terminal's scroll viewport.
///
/// While a parked terminal's DOM element is detached (tab switch, zoom,
/// project switch), the browser destroys the `.xterm-viewport` scrolling
/// box, resetting its scrollTop to 0 — but xterm's internal ydisp keeps
/// pointing at the previous position (usually the bottom). Re-inserting the
/// element does not restore scrollTop, and xterm only re-applies it lazily;
/// any scroll event read in the meantime (a wheel tick, or one queued by the
/// detach itself) is interpreted by xterm's Viewport as a giant upward
/// scroll and slams the viewport to the top of the scrollback, after which
/// the terminal no longer follows output. Force the viewport's cached
/// metrics to be recomputed so it re-applies scrollTop from ydisp right now
/// — wherever ydisp is, so a user who scrolled up stays put.
function resyncViewport(term: Terminal): void {
  const viewport = (term as unknown as {
    viewport?: { reset(): void; syncScrollArea(immediate?: boolean): void };
  }).viewport;
  if (!viewport) return;
  viewport.reset(); // clear cached sizes/scrollTop so the sync isn't skipped
  viewport.syncScrollArea(true); // immediate: re-apply scrollTop = ydisp * rowHeight
}


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
  const [pasteWarning, setPasteWarning] = useState<string | null>(null);
  const { t } = useT();
  // Whether the scrollback search bar is open for THIS pane's session.
  const searchState = useSyncExternalStore(subscribeTerminalSearch, getTerminalSearchState);
  const searchOpen = searchState.sessionId === sessionId;

  const doPaste = async () => {
    const text = await navigator.clipboard.readText().catch(() => "");
    if (text && looksDangerousPaste(text)) {
      setPasteWarning(text);
      return;
    }
    if (text) api.sendText(sessionId, text);
  };

  useEffect(() => {
    const entry = acquire(sessionId);
    const container = containerRef.current!;
    if (!entry.term.element) {
      entry.term.open(container);
    } else if (entry.term.element.parentElement !== container) {
      container.appendChild(entry.term.element);
      resyncViewport(entry.term);
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
      // resize() rebuilds the viewport's scroll metrics; a scroll event
      // during that transient state can slam the viewport to the top of
      // the scrollback (same class of bug as the re-attach case handled
      // by resyncViewport above). If the user was following output, pin
      // the viewport back to the bottom after the resize so streaming
      // content stays visible.
      const wasAtBottom = entry.atBottom;
      try {
        entry.fit.fit();
        pushSize();
      } catch (_) {}
      if (wasAtBottom) {
        resyncViewport(entry.term);
        entry.term.scrollToBottom();
        entry.atBottom = true;
      }
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
    <>
    <div
      ref={containerRef}
      data-terminal-pane={sessionId}
      className={`relative w-full h-full overflow-hidden bg-muster-bg ${
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
        // Quote the path safely for the shell; no trailing newline so the
        // user can compose the rest of the command before running it.
        api.sendText(sessionId, shellQuotePath(path));
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
              action: doPaste,
            },
            { label: t("terminal.selectAll"), action: () => term.selectAll() },
            "sep",
            { label: t("terminal.splitRight"), action: () => api.split("right") },
            { label: t("terminal.splitDown"), action: () => api.split("bottom") },
          ],
        });
      }}
    />
    {searchOpen && <TerminalSearchBar sessionId={sessionId} nonce={searchState.nonce} />}
    {pasteWarning && (
      <PasteWarning
        text={pasteWarning}
        onConfirm={() => {
          api.sendText(sessionId, pasteWarning);
          setPasteWarning(null);
        }}
        onCancel={() => setPasteWarning(null)}
      />
    )}
    </>
  );
}
