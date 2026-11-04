import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Terminal } from "@xterm/xterm";
import { useAgentStatus, agentLabel } from "../hooks/useAgentStatus";
import { acquire, resyncViewport } from "../lib/terminalRegistry";
import { api } from "../lib/invoke";
import { shellQuotePath } from "../lib/shellEscape";
import { openMenu } from "../lib/menuStore";
import { useT } from "../lib/i18n/context";
import { getTerminalSearchState, subscribeTerminalSearch } from "../lib/terminalSearch";
import { useSettings } from "../lib/settingsStore";
import PasteWarning, { looksDangerousPaste } from "./PasteWarning";
import TerminalSearchBar from "./TerminalSearchBar";

/// One terminal pane: attaches the parked xterm instance for `sessionId`
/// (owned by `terminalRegistry`) into its container div. The instance
/// outlives this component — tab switches, zoom, and project switches only
/// re-attach the DOM element, so the visible buffer is never lost.
export default function TerminalPane({
  sessionId,
  focused,
  paneKey,
  tabId,
}: {
  sessionId: string;
  focused: boolean;
  paneKey: string;
  tabId: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  // File-drop highlight: dragenter/dragleave nest (xterm children), so a
  // depth counter keeps the ring from flickering while moving inside.
  const [dropActive, setDropActive] = useState(false);
  const dragDepth = useRef(0);
  const [pasteWarning, setPasteWarning] = useState<string | null>(null);
  const { t } = useT();
  const settings = useSettings();
  const agents = useAgentStatus();
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
      console.log(`[term] ${sessionId.slice(0, 8)} pushSize ${cols}x${rows} fitted=${entry.fitted} atBottom=${entry.atBottom}`);
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
      // The first fit reflows whatever the restore flush wrote at the
      // 80x24 spawn size; that transient fires scroll events which would
      // otherwise leave the viewport stuck above the live prompt (the
      // covered-input-box bug on restored sessions). Pin to the bottom:
      // the user cannot have scrolled within one frame of mounting.
      entry.fitted = true;
      entry.atBottom = true;
      entry.term.scrollToBottom();
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
      onContextMenu={async (e) => {
        e.preventDefault();
        const { term } = acquire(sessionId);
        // Windows Terminal-style smart right-click: copy when there is a
        // selection, paste when there isn't. Shift+right-click (or the
        // "menu" setting) always opens the full menu, so split/close stay
        // reachable.
        const smart = !e.shiftKey && settings?.terminal_right_click === "smart";
        if (smart) {
          const sel = term.getSelection();
          if (sel) {
            navigator.clipboard.writeText(sel);
            term.clearSelection();
          } else {
            doPaste();
          }
          return;
        }
        // A restored shell has no agent process yet; offer to replay the
        // invocation that was detected before the relaunch.
        const info = await api.sessionInfo(sessionId).catch(() => null);
        const resumableAgent = info?.agent && info.agent_command && !agents[sessionId]
          ? info.agent
          : null;
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
            ...(resumableAgent
              ? [
                  "sep" as const,
                  {
                    label: t("terminal.restartAgent", { agent: agentLabel(resumableAgent) }),
                    action: () => {
                      api.restartSessionAgent(sessionId).catch(() => {});
                    },
                  },
                ]
              : []),
            { label: t("terminal.closePane"), action: () => api.closePane(tabId, paneKey) },
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
