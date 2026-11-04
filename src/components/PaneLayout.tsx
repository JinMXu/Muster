import { Fragment, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { api } from "../lib/invoke";
import type { PaneDropEdge } from "../lib/invoke";
import type { ProjectView, Uuid } from "../lib/types";
import { useT } from "../lib/i18n/context";
import { useGitStatus } from "../lib/gitStatusStore";
import { useAgentStatus, agentLabel } from "../hooks/useAgentStatus";
import { acquireHost, markAttached, markDetached } from "../lib/diffViewRegistry";
import { BrandIcon, hasBrandIcon } from "./brandIcons";
import { IconButton } from "./ui/IconButton";
import { IconColumns, IconFile, IconMaximize2, IconTerminal } from "./icons";
import TerminalPane from "./TerminalPane";
import FilePane from "./FilePane";

/// Render a niri-style layout: columns arranged left-to-right, with panes
/// stacked top-to-bottom inside each column. Each pane is a rounded card;
/// when the tab is split, every card wears a thin header (agent · branch ·
/// change stats · zoom) so the splits stay tellable apart.
export default function PaneLayout({
  project,
  root,
}: {
  project: ProjectView;
  /** Anchored repo root of the project (drives the headers' git line). */
  root: string | null;
}) {
  const tab = project.tabs.find((t) => t.id === project.selected_tab_id);
  if (!tab) return null;

  // Local weight override for drag-resize: the backend no longer emits
  // full state on resize_pane_divider (to avoid serializing the entire
  // workspace on every mousemove). Instead we apply the delta locally
  // and clear the override when any other state change arrives.
  const [weightOverride, setWeightOverride] = useState<Map<string, number>>(new Map());
  useEffect(() => { setWeightOverride(new Map()); }, [project]);

  const getWeight = (id: string, original: number) => weightOverride.get(id) ?? original;

  // Render weights as a share of their actual sum, not of 1.0: snapshots
  // saved by versions that didn't renormalize after closing a pane (or any
  // future drift) still fill the container instead of leaving a dead strip.
  const pct = (weight: number, total: number) => `${(weight / (total || 1)) * 100}%`;

  const applyResize = (vertical: boolean, ci: number, index: number, deltaFrac: number, total: number) => {
    const delta = deltaFrac * total;
    setWeightOverride((prev) => {
      const next = new Map(prev);
      // Read from `prev` (the updater's own latest state), not the render-
      // closure `weightOverride`: consecutive drag deltas accumulate instead
      // of each overwriting the previous one.
      const w = (id: string, original: number) => prev.get(id) ?? original;
      if (vertical) {
        const a = tab.columns[ci];
        const b = tab.columns[ci + 1];
        if (a && b) {
          next.set(a.id, w(a.id, a.weight) + delta);
          next.set(b.id, w(b.id, b.weight) - delta);
        }
      } else {
        const col = tab.columns[ci];
        if (col) {
          const a = col.panes[index];
          const b = col.panes[index + 1];
          if (a && b) {
            next.set(a.id, w(a.id, a.weight) + delta);
            next.set(b.id, w(b.id, b.weight) - delta);
          }
        }
      }
      return next;
    });
  };

  /// A header tab dropped on a pane edge: move its panes into this tab,
  /// splitting each at the target pane's edge. When every pane has moved out,
  /// the backend closes the emptied source tab itself (move_pane_cross_tab),
  /// and selection stays on this tab.
  const moveTabIntoSplit = async (draggedTabId: Uuid, targetPaneId: Uuid, edge: PaneDropEdge) => {
    const source = project.tabs.find((t) => t.id === draggedTabId);
    if (!source || source.id === tab.id) return;
    for (const col of source.columns) {
      for (const p of col.panes) {
        if (await api.movePaneCrossTab(source.id, p.id, tab.id)) {
          await api.movePane(tab.id, p.id, targetPaneId, edge);
        }
      }
    }
  };

  if (tab.is_zoomed && tab.pane_count > 1) {
    const focused = tab.columns
      .flatMap((c) => c.panes)
      .find((p) => p.id === tab.focused_pane_id);
    if (focused) {
      return (
        <div className="absolute inset-0 p-1.5">
          <div className="w-full h-full rounded-[10px] border border-muster-hairline bg-muster-bg overflow-hidden muster-pane-focused">
            <Pane pane={focused} focused={true} paneKey={focused.id} tabId={tab.id} showHeader={false} root={root} onTabDrop={moveTabIntoSplit} />
          </div>
        </div>
      );
    }
  }

  const colTotal = tab.columns.reduce((sum, c) => sum + getWeight(c.id, c.weight), 0);

  return (
    /* Panes are rounded cards floating on the window wash, separated by
       see-through gaps instead of seams — the theme's glow shows through. */
    <div className="absolute inset-0 flex flex-row gap-1.5 p-1.5">
      {tab.columns.map((col, ci) => {
        const paneTotal = col.panes.reduce((sum, p) => sum + getWeight(p.id, p.weight), 0);
        let paneCum = 0;
        return (
          <div
            key={col.id}
            className="relative flex flex-col min-w-0 gap-1.5"
            style={{ flexBasis: pct(getWeight(col.id, col.weight), colTotal) }}
          >
            {col.panes.map((pane, pi) => {
              paneCum += getWeight(pane.id, pane.weight);
              const dividerAt = paneCum;
              const focused = pane.id === tab.focused_pane_id;
              return (
                <Fragment key={pane.id}>
                  <div
                    className={`min-h-0 rounded-[10px] border border-muster-hairline bg-muster-bg overflow-hidden ${
                      focused ? "muster-pane-focused" : ""
                    }`}
                    style={{
                      flexBasis: pct(getWeight(pane.id, pane.weight), paneTotal),
                    }}
                  >
                    <Pane
                      pane={pane}
                      focused={focused}
                      paneKey={pane.id}
                      tabId={tab.id}
                      showHeader={tab.pane_count > 1}
                      root={root}
                      onTabDrop={moveTabIntoSplit}
                    />
                  </div>
                  {pi + 1 < col.panes.length && (
                    <DividerHandle
                      vertical={false}
                      style={{ top: `${(dividerAt / paneTotal) * 100}%` }}
                      onDrag={(frac) => {
                        applyResize(false, ci, pi, frac, paneTotal);
                        api.resizePaneDivider(tab.id, false, ci, pi, frac * paneTotal);
                      }}
                    />
                  )}
                </Fragment>
              );
            })}
            {ci + 1 < tab.columns.length && (
              <DividerHandle
                vertical={true}
                style={{ left: "100%" }}
                onDrag={(frac) => {
                  applyResize(true, ci, ci, frac, colTotal);
                  api.resizePaneDivider(tab.id, true, ci, ci, frac * colTotal);
                }}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

/// Invisible drag handle overlaying a pane divider: a 2px accent line shown
/// only on hover, with a 9px hit area, absolutely positioned inside its
/// column (`left: 100%` hugs the column's right edge for column dividers). Pixel movement is converted to
/// a fraction of the parent's axis size and reported via `onDrag`, throttled
/// to one call per animation frame.
function DividerHandle({
  vertical,
  style,
  onDrag,
}: {
  vertical: boolean;
  style: CSSProperties;
  onDrag: (deltaFraction: number) => void;
}) {
  const drag = useRef<{ size: number; sent: number; latest: number; raf: boolean } | null>(null);

  const onMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    // Row dividers measure their column; column dividers live inside a
    // column but measure the whole tab row (the column's parent).
    const colEl = e.currentTarget.parentElement as HTMLElement;
    const host = vertical ? (colEl.parentElement as HTMLElement) : colEl;
    const rect = host.getBoundingClientRect();
    const pos = vertical ? e.clientX : e.clientY;
    drag.current = { size: vertical ? rect.width : rect.height, sent: pos, latest: pos, raf: false };
    document.body.style.userSelect = "none";

    const onMove = (ev: MouseEvent) => {
      const d = drag.current;
      if (!d) return;
      d.latest = vertical ? ev.clientX : ev.clientY;
      if (!d.raf) {
        d.raf = true;
        requestAnimationFrame(() => {
          const dd = drag.current;
          if (!dd) return;
          dd.raf = false;
          const px = dd.latest - dd.sent;
          if (px !== 0 && dd.size > 0) {
            dd.sent = dd.latest;
            onDrag(px / dd.size);
          }
        });
      }
    };
    const onUp = () => {
      drag.current = null;
      document.body.style.userSelect = "";
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <div
      onMouseDown={onMouseDown}
      className={`absolute z-10 flex items-center justify-center group ${
        vertical
          ? "top-0 bottom-0 w-[11px] -translate-x-1/2 cursor-col-resize"
          : "left-0 right-0 h-[11px] -translate-y-1/2 cursor-row-resize"
      }`}
      style={style}
    >
      {/* Invisible over the see-through gap; an accent bar lights up on
          hover / while dragging so the divider stays discoverable. */}
      <div
        className={`${
          vertical ? "w-[3px] h-10 my-auto" : "h-[3px] w-10 mx-auto"
        } rounded-full bg-muster-accent opacity-0 group-hover:opacity-60 group-active:opacity-90 transition-opacity duration-muster ease-muster`}
      />
    </div>
  );
}

/// Accent bar shown on the edge a dragged pane would land on.
/// Tailwind dot-class per agent status — mirrors AgentMiniBar's colors.
function statusDotClass(status: {
  state: string;
  attention: string | null;
}): string {
  if (status.attention === "approval") return "bg-rose-400 animate-pulse";
  if (status.state === "waiting") return "bg-amber-400 animate-pulse";
  if (status.state === "done") return "bg-sky-400";
  return "bg-emerald-400";
}

const DROP_EDGE_CLASS: Record<PaneDropEdge, string> = {
  left: "inset-y-0 left-0 w-[3px]",
  right: "inset-y-0 right-0 w-[3px]",
  top: "inset-x-0 top-0 h-[3px]",
  bottom: "inset-x-0 bottom-0 h-[3px]",
};

/// Slot that adopts the parked host element for a diff pane (see
/// diffViewRegistry). The DiffPane itself is rendered once by DiffHosts in
/// App, so tab/zoom/project switches only re-attach this DOM subtree instead
/// of remounting the Monaco diff editor — same idea as TerminalPane.
function DiffSlot({ diffId }: { diffId: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const host = acquireHost(diffId);
    ref.current!.appendChild(host);
    markAttached(diffId);
    return () => {
      host.remove();
      markDetached(diffId);
    };
  }, [diffId]);
  return <div ref={ref} className="w-full h-full" />;
}

function Pane({
  pane,
  focused,
  paneKey,
  tabId,
  showHeader,
  root,
  onTabDrop,
}: {
  pane: ProjectView["tabs"][number]["columns"][number]["panes"][number];
  focused: boolean;
  paneKey: Uuid;
  tabId: Uuid;
  /** Thin info header (agent · branch · stats · zoom) on split panes. */
  showHeader: boolean;
  root: string | null;
  /// A header tab dropped on this pane's edge: the tab's panes move into
  /// this tab as a split.
  onTabDrop: (draggedTabId: Uuid, targetPaneId: Uuid, edge: PaneDropEdge) => void;
}) {
  // Edge a pane-move drag is hovering over; null when no such drag is over
  // us. dragenter/dragleave nest (the pane content has children), so a
  // depth counter keeps the highlight from flickering while moving inside.
  const [dropEdge, setDropEdge] = useState<PaneDropEdge | null>(null);
  const dragDepth = useRef(0);
  const { t } = useT();
  // Header data: live agent status for session panes, project-level branch
  // and change stats (one shared poller per root behind these hooks).
  const agentStates = useAgentStatus();
  const status = pane.content.kind === "session" ? agentStates[pane.content.id] : undefined;
  const git = useGitStatus(showHeader ? root : null, showHeader);

  // Pane-move drags and header-tab drags both split into this pane.
  const acceptsDrag = (e: React.DragEvent<HTMLDivElement>) =>
    e.dataTransfer.types.includes("application/x-muster-pane") ||
    e.dataTransfer.types.includes("application/x-muster-tab");

  // Nearest edge of the pointer wins, so the highlight matches where the
  // backend will insert the pane.
  const edgeAt = (e: React.DragEvent<HTMLDivElement>): PaneDropEdge => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = (e.clientX - rect.left) / Math.max(rect.width, 1);
    const y = (e.clientY - rect.top) / Math.max(rect.height, 1);
    const distances: [PaneDropEdge, number][] = [
      ["left", x],
      ["right", 1 - x],
      ["top", y],
      ["bottom", 1 - y],
    ];
    return distances.reduce((a, b) => (b[1] < a[1] ? b : a))[0];
  };

  const startPaneDrag = (e: React.DragEvent) => {
    e.dataTransfer.setData("application/x-muster-pane", pane.id);
    e.dataTransfer.setData("application/x-muster-pane-source-tab", tabId);
    e.dataTransfer.effectAllowed = "move";
  };

  return (
    <div
      className="relative w-full h-full flex flex-col"
      onDragEnter={(e) => {
        if (!acceptsDrag(e)) return;
        dragDepth.current += 1;
      }}
      onDragLeave={() => {
        dragDepth.current = Math.max(0, dragDepth.current - 1);
        if (dragDepth.current === 0) setDropEdge(null);
      }}
      onDragOver={(e) => {
        // Only claim pane-move / tab drags; file-path drags fall through to
        // the terminal drop target inside.
        if (!acceptsDrag(e)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        setDropEdge(edgeAt(e));
      }}
      onDrop={(e) => {
        dragDepth.current = 0;
        setDropEdge(null);
        // A header tab dropped here splits its panes into this tab.
        const draggedTabId = e.dataTransfer.getData("application/x-muster-tab");
        if (draggedTabId) {
          // Dropping the tab already showing this pane is a no-op.
          if (draggedTabId === tabId) return;
          e.preventDefault();
          e.stopPropagation();
          onTabDrop(draggedTabId, pane.id, edgeAt(e));
          return;
        }
        const draggedId = e.dataTransfer.getData("application/x-muster-pane");
        if (!draggedId || draggedId === pane.id) return;
        e.preventDefault();
        e.stopPropagation();
        api.movePane(tabId, draggedId, pane.id, edgeAt(e));
      }}
    >
      {showHeader ? (
        /* Thin header on split panes; the whole strip is the pane-move
           drag region (replacing the headerless 6px top grip). */
        <div
          draggable
          title={t("pane.dragToMove")}
          onDragStart={startPaneDrag}
          className="relative flex items-center gap-1.5 h-6 px-2 flex-shrink-0 bg-muster-panel border-b border-muster-border cursor-grab active:cursor-grabbing"
        >
          {pane.content.kind === "session" ? (
            status?.agent ? (
              <>
                {/* Brand logo when Simple Icons has the agent; the status
                    dot then moves after the name (kero's pane title). */}
                {hasBrandIcon(status.agent) ? (
                  <BrandIcon
                    kind={status.agent}
                    size={13}
                    className={focused ? "text-muster-fg/90" : "text-muster-muted"}
                  />
                ) : (
                  <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${statusDotClass(status)}`} />
                )}
                <span className="ui-fs-xs truncate">{agentLabel(status.agent)}</span>
                {hasBrandIcon(status.agent) && (
                  <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${statusDotClass(status)}`} />
                )}
              </>
            ) : (
              <span className="text-muster-muted flex items-center">
                <IconTerminal size={11} />
              </span>
            )
          ) : (
            <span className="text-muster-muted flex items-center">
              {pane.content.kind === "file" ? <IconFile size={11} /> : <IconColumns size={11} />}
            </span>
          )}
          <span className="flex-1 min-w-0" />
          {git?.is_repo && git.branch && (
            <span className="ui-fs-2xs text-muster-muted truncate max-w-[30%]" title={git.branch}>
              {git.branch}
            </span>
          )}
          {git?.is_repo && git.line_additions > 0 && (
            <span className="ui-fs-2xs text-emerald-400/90 tabular-nums flex-shrink-0">
              +{git.line_additions > 999 ? `${(git.line_additions / 1000).toFixed(1)}k` : git.line_additions}
            </span>
          )}
          {git?.is_repo && git.line_deletions > 0 && (
            <span className="ui-fs-2xs text-red-400/90 tabular-nums flex-shrink-0">
              −{git.line_deletions > 999 ? `${(git.line_deletions / 1000).toFixed(1)}k` : git.line_deletions}
            </span>
          )}
          <IconButton
            size="xs"
            className="w-4 h-4"
            title={t("commandPalette.togglePaneZoom")}
            onClick={() => api.togglePaneZoom()}
          >
            <IconMaximize2 size={10} />
          </IconButton>
        </div>
      ) : (
        /* 6px grip along the top edge starts a pane-move drag. */
        <div
          draggable
          title={t("pane.dragToMove")}
          onDragStart={startPaneDrag}
          className="absolute top-0 left-0 right-0 h-[6px] z-20 cursor-grab active:cursor-grabbing"
        />
      )}
      <div className="flex-1 min-h-0">
        {pane.content.kind === "session" && (
          <TerminalPane sessionId={pane.content.id} focused={focused} paneKey={paneKey} tabId={tabId} />
        )}
        {pane.content.kind === "file" && (
          <FilePane fileId={pane.content.id} focused={focused} />
        )}
        {pane.content.kind === "diff" && (
          <DiffSlot diffId={pane.content.id} />
        )}
      </div>
      {/* A hint of dim on the pane the user is not in (kero's cardDim) —
          pointer-events-none, so clicks fall through to focus the pane. */}
      {!focused && (
        <div className="absolute inset-0 z-20 pointer-events-none bg-muster-veil" />
      )}
      {dropEdge && (
        <div
          className={`absolute pointer-events-none z-30 bg-muster-accent ${DROP_EDGE_CLASS[dropEdge]}`}
        />
      )}
    </div>
  );
}
