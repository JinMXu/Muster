import { Fragment, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { api } from "../lib/invoke";
import type { PaneDropEdge } from "../lib/invoke";
import type { ProjectView, Uuid } from "../lib/types";
import { useT } from "../lib/i18n/context";
import { acquireHost, markAttached, markDetached } from "../lib/diffViewRegistry";
import TerminalPane from "./TerminalPane";
import FilePane from "./FilePane";

/// Render a niri-style layout: columns arranged left-to-right, with panes
/// stacked top-to-bottom inside each column. Each pane is the focused kind's
/// renderer alongside a header badge that points to focus state.
export default function PaneLayout({ project }: { project: ProjectView }) {
  const tab = project.tabs.find((t) => t.id === project.selected_tab_id);
  if (!tab) return null;

  // Local weight override for drag-resize: the backend no longer emits
  // full state on resize_pane_divider (to avoid serializing the entire
  // workspace on every mousemove). Instead we apply the delta locally
  // and clear the override when any other state change arrives.
  const [weightOverride, setWeightOverride] = useState<Map<string, number>>(new Map());
  useEffect(() => { setWeightOverride(new Map()); }, [project]);

  const getWeight = (id: string, original: number) => weightOverride.get(id) ?? original;

  const applyResize = (vertical: boolean, ci: number, index: number, deltaFrac: number, total: number) => {
    const delta = deltaFrac * total;
    setWeightOverride((prev) => {
      const next = new Map(prev);
      if (vertical) {
        const a = tab.columns[ci];
        const b = tab.columns[ci + 1];
        if (a && b) {
          next.set(a.id, getWeight(a.id, a.weight) + delta);
          next.set(b.id, getWeight(b.id, b.weight) - delta);
        }
      } else {
        const col = tab.columns[ci];
        if (col) {
          const a = col.panes[index];
          const b = col.panes[index + 1];
          if (a && b) {
            next.set(a.id, getWeight(a.id, a.weight) + delta);
            next.set(b.id, getWeight(b.id, b.weight) - delta);
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
        <div className="absolute inset-0 p-0">
          <Pane pane={focused} focused={true} paneKey={focused.id} tabId={tab.id} onTabDrop={moveTabIntoSplit} />
        </div>
      );
    }
  }

  const colTotal = tab.columns.reduce((sum, c) => sum + getWeight(c.id, c.weight), 0);

  return (
    <div className="absolute inset-0 flex flex-row gap-[2px]">
      {tab.columns.map((col, ci) => {
        const paneTotal = col.panes.reduce((sum, p) => sum + getWeight(p.id, p.weight), 0);
        let paneCum = 0;
        return (
          <div
            key={col.id}
            className="relative flex flex-col min-w-0 gap-[2px]"
            style={{ flexBasis: `${getWeight(col.id, col.weight) * 100}%` }}
          >
            {col.panes.map((pane, pi) => {
              paneCum += getWeight(pane.id, pane.weight);
              const dividerAt = paneCum;
              return (
                <Fragment key={pane.id}>
                  <div
                    className="min-h-0 bg-muster-bg"
                    style={{
                      flexBasis: `${getWeight(pane.id, pane.weight) * 100}%`,
                      zIndex: pane.id === tab.focused_pane_id ? 2 : 1,
                    }}
                  >
                    <Pane
                      pane={pane}
                      focused={pane.id === tab.focused_pane_id}
                      paneKey={pane.id}
                      tabId={tab.id}
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
      className={`absolute z-10 group ${
        vertical
          ? "top-0 bottom-0 w-[9px] -translate-x-1/2 cursor-col-resize"
          : "left-0 right-0 h-[9px] -translate-y-1/2 cursor-row-resize"
      }`}
      style={style}
    >
      <div
        className={`group-hover:bg-muster-accent group-active:bg-muster-accent ${
          vertical ? "w-[2px] h-full mx-auto" : "h-[2px] w-full my-auto"
        }`}
      />
    </div>
  );
}

/// Accent bar shown on the edge a dragged pane would land on.
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
  onTabDrop,
}: {
  pane: ProjectView["tabs"][number]["columns"][number]["panes"][number];
  focused: boolean;
  paneKey: Uuid;
  tabId: Uuid;
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

  return (
    <div
      className={`relative w-full h-full ${focused ? "z-10 muster-pane-focused" : ""}`}
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
{/* 6px grip along the top edge starts a pane-move drag. */}
        <div
          draggable
          title={t("pane.dragToMove")}
          onDragStart={(e) => {
            e.dataTransfer.setData("application/x-muster-pane", pane.id);
            e.dataTransfer.setData("application/x-muster-pane-source-tab", tabId);
            e.dataTransfer.effectAllowed = "move";
          }}
          className="absolute top-0 left-0 right-0 h-[6px] z-20 cursor-grab active:cursor-grabbing"
        />
      {pane.content.kind === "session" && (
        <TerminalPane sessionId={pane.content.id} focused={focused} paneKey={paneKey} />
      )}
      {pane.content.kind === "file" && (
        <FilePane fileId={pane.content.id} focused={focused} />
      )}
      {pane.content.kind === "diff" && (
        <DiffSlot diffId={pane.content.id} />
      )}
      {dropEdge && (
        <div
          className={`absolute pointer-events-none z-30 bg-muster-accent ${DROP_EDGE_CLASS[dropEdge]}`}
        />
      )}
    </div>
  );
}
