import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { ProjectView, PtyProgress, TabView, Uuid } from "../lib/types";
import WindowControls from "./WindowControls";
import { IconColumns, IconMaximize2, IconPanelRight, IconPlus, IconX } from "./icons";
import { useT } from "../lib/i18n/context";
import { api } from "../lib/invoke";
import { agentLabel, useAgentStatus, type LiveAgentStatus } from "../hooks/useAgentStatus";

/// The session a tab's progress bar should reflect: the focused pane's
/// session, falling back to the first session pane in the tab.
function tabProgressSession(tab: TabView): Uuid | null {
  for (const col of tab.columns) {
    for (const pane of col.panes) {
      if (pane.id === tab.focused_pane_id && pane.content.kind === "session") return pane.content.id;
    }
  }
  for (const col of tab.columns) {
    for (const pane of col.panes) {
      if (pane.content.kind === "session") return pane.content.id;
    }
  }
  return null;
}

export default function Header({
  project,
  onNewSession,
  onCloseTab,
  onSelectTab,
  onMoveTab,
  onRenameTab,
  onTabMenu,
  togglePanel,
  panelVisible,
  isPaneZoomed,
  onExitZoom,
}: {
  project: ProjectView | null;
  onNewSession: () => void;
  onCloseTab: (id: Uuid) => void;
  onSelectTab: (id: Uuid) => void;
  onMoveTab: (from: Uuid, to: Uuid) => void;
  onRenameTab: (id: Uuid, name: string | null) => void;
  /// Right-click on a tab: App builds the menu; `requestRename` re-enters the
  /// tab's inline rename field so "Rename" shares the double-click flow.
  onTabMenu: (tab: ProjectView["tabs"][number], x: number, y: number, requestRename: () => void) => void;
  togglePanel: () => void;
  panelVisible: boolean;
  isPaneZoomed: boolean;
  onExitZoom: () => void;
}) {
  const { t } = useT();
  const [dragging, setDragging] = useState<Uuid | null>(null);
  // Coding-agent status per session, fed by the backend poller
  // (`agent-status-changed`). Drives the colored dot on agent tabs.
  const agents = useAgentStatus();
  // OSC 9;4 progress per session id, fed by the backend's `pty:progress`
  // event (emitted only when a session's progress value changes).
  const [progressBySession, setProgressBySession] = useState<Record<Uuid, PtyProgress>>({});
  // Pending "clear completed progress" timers, cleared on unmount.
  const progressTimersRef = useRef<Set<ReturnType<typeof setTimeout>>>(new Set());
  useEffect(() => {
    const unlisten = listen<PtyProgress>("pty:progress", (event) => {
      const p = event.payload;
      setProgressBySession((prev) => {
        const next = { ...prev };
        if (p.state === 0) delete next[p.id];
        else next[p.id] = p;
        return next;
      });
      // Completed progress (state 1 at 100%) lingers briefly, then clears —
      // shells don't always send the state-0 removal sequence.
      if (p.state === 1 && p.progress >= 100) {
        const timer = setTimeout(() => {
          progressTimersRef.current.delete(timer);
          setProgressBySession((prev) => {
            const cur = prev[p.id];
            if (!cur || cur.state !== 1 || cur.progress < 100) return prev;
            const next = { ...prev };
            delete next[p.id];
            return next;
          });
        }, 1000);
        progressTimersRef.current.add(timer);
      }
    });
    return () => {
      unlisten.then((f) => f());
      for (const timer of progressTimersRef.current) clearTimeout(timer);
      progressTimersRef.current.clear();
    };
  }, []);

  // Sessions that exit without sending a progress-removal sequence would
  // leave stale entries forever; prune entries whose session is gone.
  useEffect(() => {
    const alive = new Set<string>();
    for (const tab of project?.tabs ?? []) {
      for (const col of tab.columns) {
        for (const pane of col.panes) {
          if (pane.content.kind === "session") alive.add(pane.content.id);
        }
      }
    }
    setProgressBySession((prev) => {
      const keys = Object.keys(prev);
      if (keys.every((k) => alive.has(k))) return prev;
      const next: Record<Uuid, PtyProgress> = {};
      for (const k of keys) if (alive.has(k)) next[k] = prev[k];
      return next;
    });
  }, [project]);

  return (
    <header
      className="h-9 flex items-center pl-2"
      data-tauri-drag-region
    >
      <div
        className="flex-1 min-w-0 flex items-center gap-1 overflow-x-auto self-stretch"
        style={{ scrollbarWidth: "none" }}
        data-tauri-drag-region
      >
        {project?.tabs.map((tab) => {
          const sessionId = tabProgressSession(tab);
          return (
            <TabItem
              key={tab.id}
              tab={tab}
              isSelected={tab.id === project.selected_tab_id}
              isDragging={dragging === tab.id}
              progress={sessionId ? progressBySession[sessionId] ?? null : null}
              agent={sessionId ? agents[sessionId] ?? null : null}
              onSelect={() => onSelectTab(tab.id)}
              onClose={() => onCloseTab(tab.id)}
              onMove={(from) => onMoveTab(from, tab.id)}
              onDragStart={() => setDragging(tab.id)}
              onDragEnd={() => setDragging(null)}
              onRename={(name) => onRenameTab(tab.id, name)}
              onMenu={(x, y, requestRename) => onTabMenu(tab, x, y, requestRename)}
              onPaneDrop={(paneId, sourceTabId) =>
                api.movePaneCrossTab(sourceTabId, paneId, tab.id)
              }
            />
          );
        })}
        <button
          onClick={onNewSession}
          title={t("header.newSessionTooltip")}
          className="w-5 h-5 rounded text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn flex items-center justify-center ui-fs-base flex-shrink-0 active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          <IconPlus size={13} />
        </button>
      </div>
      <div className="flex items-center gap-1 flex-shrink-0 pr-2 self-stretch" data-tauri-drag-region>
        {isPaneZoomed && (
          <button
            onClick={onExitZoom}
            title={t("header.exitZoomTooltip")}
            className="w-5 h-5 rounded text-muster-accent hover:bg-muster-hover-btn flex items-center justify-center ui-fs-base active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            <IconMaximize2 size={13} />
          </button>
        )}
        {project && (
          <button
            onClick={togglePanel}
            title={t("header.toggleRightSidebarTooltip")}
            className={`w-5 h-5 rounded flex items-center justify-center ui-fs-base hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster ${
              panelVisible ? "text-muster-accent" : "text-muster-muted"
            }`}
          >
            <IconPanelRight size={13} />
          </button>
        )}
      </div>
      {/* While the right panel is visible it hosts the caption buttons flush
          to the window's top-right corner; otherwise they live here. */}
      {!panelVisible && <WindowControls />}
    </header>
  );
}

function TabItem({
  tab,
  isSelected,
  isDragging,
  progress,
  agent,
  onSelect,
  onClose,
  onMove,
  onDragStart,
  onDragEnd,
  onRename,
  onMenu,
  onPaneDrop,
}: {
  tab: ProjectView["tabs"][number];
  isSelected: boolean;
  isDragging: boolean;
  /// OSC 9;4 progress of the tab's session, null when there is none.
  progress: PtyProgress | null;
  /// Coding agent detected in the tab's session, null when there is none.
  agent: LiveAgentStatus | null;
  onSelect: () => void;
  onClose: () => void;
  /// A tab was dropped on this tab to reorder: `from` is the dragged tab's
  /// id (this tab is the drop target).
  onMove: (from: Uuid) => void;
  onDragStart: () => void;
  onDragEnd: () => void;
  onRename: (name: string | null) => void;
  onMenu: (x: number, y: number, requestRename: () => void) => void;
  /// A pane dragged from another tab and dropped on this tab: moves the pane
  /// out of its source tab into this one as a new column.
  onPaneDrop: (paneId: Uuid, sourceTabId: Uuid) => void;
}) {
  const { t } = useT();
  const [hovering, setHovering] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState("");
  const [paneDragOver, setPaneDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const startRename = () => {
    setDraft(tab.custom_name ?? tab.display_title ?? "");
    setRenaming(true);
    setTimeout(() => inputRef.current?.focus(), 0);
  };
  const finish = (apply: boolean) => {
    if (apply) {
      const trimmed = draft.trim();
      onRename(trimmed.length ? trimmed : null);
    }
    setRenaming(false);
  };

  return (
    <div
      onClick={onSelect}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      onDoubleClick={startRename}
      draggable
      onDragStart={(e) => {
        // Carry the tab id: a drop on a pane edge (PaneLayout) splits the
        // tab's panes into that pane's tab; a drop on another tab reads the
        // id back to reorder.
        e.dataTransfer.setData("application/x-muster-tab", tab.id);
        e.dataTransfer.effectAllowed = "move";
        onDragStart();
      }}
      onDragEnd={onDragEnd}
      onDragOver={(e) => {
        // Tab reordering AND pane drops both accept the drag.
        if (
          e.dataTransfer.types.includes("application/x-muster-pane") ||
          e.dataTransfer.types.includes("application/x-muster-pane-source-tab")
        ) {
          e.preventDefault();
          setPaneDragOver(true);
        } else {
          e.preventDefault();
        }
      }}
      onDragLeave={() => setPaneDragOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setPaneDragOver(false);
        // Pane dropped from another tab → cross-tab move.
        const paneId = e.dataTransfer.getData("application/x-muster-pane");
        const sourceTabId = e.dataTransfer.getData("application/x-muster-pane-source-tab");
        if (paneId && sourceTabId && sourceTabId !== tab.id) {
          onPaneDrop(paneId, sourceTabId);
          return;
        }
        // Otherwise it's a tab-reorder drag: the dragged tab's id rides the
        // dataTransfer; this tab is the drop target.
        const draggedTabId = e.dataTransfer.getData("application/x-muster-tab");
        if (draggedTabId && draggedTabId !== tab.id) onMove(draggedTabId);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(e.clientX, e.clientY, startRename);
      }}
      className={`group relative rounded-md px-2 py-1 max-w-[220px] flex items-center gap-1 cursor-pointer whitespace-nowrap ui-fs-base flex-shrink-0 ${
        isSelected ? "bg-white/[0.09] text-muster-fg" : "text-muster-muted hover:bg-muster-hover"
      } ${isDragging ? "opacity-65" : ""} ${paneDragOver ? "ring-1 ring-inset ring-muster-accent" : ""}`}
    >
      {renaming ? (
        <input
          ref={inputRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => finish(true)}
          onKeyDown={(e) => {
            if (e.key === "Enter") finish(true);
            if (e.key === "Escape") finish(false);
          }}
          onClick={(e) => e.stopPropagation()}
          className="bg-transparent border border-muster-accent/60 rounded px-1 ui-fs-base outline-none"
        />
      ) : (
        <>
          <span className={`${isSelected ? "text-muster-accent" : "text-muster-muted/60"} ui-fs-xs`}>•</span>
          <span className="truncate">{tab.custom_name ?? tab.display_title ?? t("header.untitled")}</span>
          {agent && (
            <span
              title={
                agent.state === "waiting"
                  ? t("header.agentWaiting", { agent: agentLabel(agent.agent) })
                  : agent.state === "done"
                    ? t("header.agentDone", { agent: agentLabel(agent.agent) })
                    : t("header.agentWorking", { agent: agentLabel(agent.agent) })
              }
              className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                agent.state === "waiting"
                  ? "bg-amber-400 animate-pulse"
                  : agent.state === "done"
                    ? "bg-sky-400"
                    : "bg-emerald-400"
              }`}
            />
          )}
          {tab.pane_count > 1 && (
            <span className="flex items-center gap-0.5 ui-fs-2xs text-muster-muted/70">
              <IconColumns size={10} />
              {tab.pane_count}
            </span>
          )}
          {hovering ? (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onClose();
              }}
              className="ml-1 w-3.5 h-3.5 rounded flex items-center justify-center text-muster-muted hover:text-muster-fg"
            >
              <IconX size={11} />
            </button>
          ) : null}
        </>
      )}
      {progress && (
        <div className="absolute left-1 right-1 bottom-0 h-[2px] rounded-full bg-white/10 overflow-hidden pointer-events-none">
          <div
            className={`h-full rounded-full ${
              progress.state === 2
                ? "bg-red-500"
                : progress.state === 4
                  ? "bg-amber-400"
                  : "bg-muster-accent"
            } ${progress.state === 3 ? "w-full animate-pulse" : ""}`}
            style={progress.state === 3 ? undefined : { width: `${progress.progress}%` }}
          />
        </div>
      )}
    </div>
  );
}