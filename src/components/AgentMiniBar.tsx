import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/invoke";
import { agentLabel } from "../hooks/useAgentStatus";
import { useAllAgents, type AllAgentStatus } from "../hooks/useAllAgents";
import { useT } from "../lib/i18n/context";
import type { AgentState, Uuid } from "../lib/types";
import { IconTerminal, IconX } from "./icons";

/// Cross-window coding-agent overview capsule that lives in the left
/// sidebar's footer. Click opens a small popover listing every detected agent
/// across every window, sorted by attention (done → waiting → working); click
/// a row jumps the user to that agent's pane, in whichever window it lives.
/// Hidden entirely when no agents are running.

/// Priority used to sort the popover: higher first.
const PRIORITY: Record<AgentState, number> = {
  waiting: 3,
  done: 2,
  working: 1,
};

/// Tailwind dot-class per state — mirrors the colors used in `Header.tsx`.
const DOT_CLASS: Record<AgentState, string> = {
  working: "bg-emerald-400",
  waiting: "bg-amber-400 animate-pulse",
  done: "bg-sky-400",
};

export default function AgentMiniBar({
  open,
  onToggle,
  onClose,
}: {
  /// Whether the popover is currently open. Owned by App so a global
  /// `Ctrl+Shift+A` shortcut can toggle it without prop-drilling a ref.
  open: boolean;
  /// Toggles the open state (used by the capsule click).
  onToggle: () => void;
  /// Closes the popover (used by Esc / outside-click / row-activation).
  onClose: () => void;
}) {
  const { t } = useT();
  const agents = useAllAgents();
  const [selected, setSelected] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  /// Sorted list of detected-agent rows. `done` and `waiting` rise above
  /// `working` so the rows needing attention surface first.
  const rows: AllAgentStatus[] = useMemo(() => {
    return Object.values(agents).sort((a, b) => {
      const p = PRIORITY[b.state] - PRIORITY[a.state];
      if (p !== 0) return p;
      // Tiebreak: agent label, then session id (stable per session).
      const al = agentLabel(a.agent);
      const bl = agentLabel(b.agent);
      return al !== bl ? al.localeCompare(bl) : a.id.localeCompare(b.id);
    });
  }, [agents]);

  const counts = useMemo(() => {
    let done = 0, waiting = 0, working = 0;
    for (const r of rows) {
      if (r.state === "done") done++;
      else if (r.state === "waiting") waiting++;
      else if (r.state === "working") working++;
    }
    return { done, waiting, working, total: rows.length };
  }, [rows]);

  // No agents at all → the capsule is invisible. If the popover was open,
  // close it so it doesn't dangle unanchored.
  useEffect(() => {
    if (open && counts.total === 0) onClose();
  }, [open, counts.total, onClose]);

  const activate = (idx: number) => {
    const r = rows[idx];
    if (!r) return;
    api.focusAgentSession(r.id).then(() => {});
    onClose();
  };
  const move = (delta: number) => {
    if (!rows.length) return;
    setSelected((s) => (s + delta + rows.length) % rows.length);
  };

  // Keyboard nav while the popover is open. Just like SearchPanel / Command-
  // Palette, ArrowUp/Down move, Enter activates, Esc closes.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      // Capture-phase so we beat the global allocator's keydown listener
      // (the App's NAV_MAP only blocks INPUT/TEXTAREA, but the popover's
      // own listener should always win for arrow keys etc.)
      if (e.key === "ArrowDown") { e.preventDefault(); e.stopPropagation(); move(1); }
      else if (e.key === "ArrowUp") { e.preventDefault(); e.stopPropagation(); move(-1); }
      else if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); activate(selected); }
      else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // Rows + onClose are dependencies so the listener capture stays fresh
    // when the agent set changes (cheap: re-subscribing at most once per
    // poll, when agents change).
  }, [open, selected, rows, onClose]);

  // Reset selection to the top whenever the popover reopens or the row
  // membership changes, so keyboard quick-fire always hits the first row.
  useEffect(() => { if (open) setSelected(0); }, [open, rows]);

  if (counts.total === 0) return null;

  // The states present, in display order (done → waiting → working). Each
  // shows a colored dot so the capsule reads the same colors as the tab dots.
  const present: AgentState[] = [];
  if (counts.done) present.push("done");
  if (counts.waiting) present.push("waiting");
  if (counts.working) present.push("working");

  // One tooltip variant per state mix.
  const tooltip = (() => {
    if (counts.done && counts.waiting && counts.working) {
      return t("agents.tooltipMixed", {
        done: counts.done, waiting: counts.waiting, working: counts.working });
    }
    if (counts.done) return t("agents.tooltipDone", { n: counts.done });
    if (counts.waiting) return t("agents.tooltipWaiting", { n: counts.waiting });
    return t("agents.tooltipRunning", { n: counts.working });
  })();

  return (
    <div className="relative">
      <button
        title={tooltip}
        onClick={onToggle}
        className="flex items-center gap-1 px-1.5 h-6 rounded-full bg-muster-hover/60 hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
        aria-label={t("agents.title")}
      >
        <span className="flex items-center gap-0.5">
          {present.map((s) => (
            <span key={s} className={`w-1.5 h-1.5 rounded-full ${DOT_CLASS[s]}`} />
          ))}
        </span>
        <span className="ui-fs-xs tabular-nums text-muster-fg/80 leading-none font-medium">
          {counts.total}
        </span>
      </button>

      {open && (
        <>
          {/* Outside-click backdrop (non-dimmed: a small corner popover
              shouldn't darken the whole screen like the big modals do). */}
          <div className="fixed inset-0 z-40" onClick={onClose} />
          <div
            className="absolute bottom-full left-0 mb-1 z-50 w-[280px] max-w-[80vw] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] muster-pop flex flex-col"
            ref={listRef}
          >
            <div className="flex items-center gap-2 px-3 h-9 flex-shrink-0 border-b border-white/[0.08]">
              <span className="text-muster-accent flex items-center">
                <IconTerminal size={13} />
              </span>
              <span className="flex-1 ui-fs-sm font-medium">{t("agents.title")}</span>
              <span className="ui-fs-xs text-muster-muted tabular-nums">{counts.total}</span>
              <button
                onClick={onClose}
                title={t("common.close")}
                className="px-1 rounded text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg flex items-center"
              >
                <IconX size={13} />
              </button>
            </div>
            <div className="flex-1 min-h-0 overflow-y-auto p-1.5 max-h-[60vh]">
              {rows.map((r, i) => (
                <AgentRow
                  key={r.id}
                  row={r}
                  selected={i === selected}
                  onSelect={() => setSelected(i)}
                  onActivate={() => activate(i)}
                />
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function AgentRow({
  row,
  selected,
  onSelect,
  onActivate,
}: {
  row: AllAgentStatus;
  selected: boolean;
  onSelect: () => void;
  onActivate: () => void;
}) {
  const { t } = useT();
  const stateLabel =
    row.state === "working"
      ? t("agents.state.working")
      : row.state === "waiting"
        ? t("agents.state.waiting")
        : t("agents.state.done");
  return (
    <div
      onClick={onActivate}
      onMouseEnter={onSelect}
      title={t("agents.jumpTo")}
      className={`flex items-center gap-2 px-2 h-9 rounded cursor-pointer ui-fs-sm ${
        selected ? "bg-muster-selected text-muster-fg" : "hover:bg-muster-hover text-muster-fg/85"
      }`}
    >
      <span className={`w-2 h-2 rounded-full flex-shrink-0 ${DOT_CLASS[row.state]}`} />
      <div className="flex-1 min-w-0 flex flex-col">
        <span className="truncate font-medium">
          {agentLabel(row.agent)}
          <span className="ml-1.5 ui-fs-xs font-normal text-muster-muted/80">{stateLabel}</span>
        </span>
        <span className="truncate ui-fs-xs text-muster-muted/70">
          {row.title || "—"}
        </span>
      </div>
      {row.project && (
        <span className="flex-shrink-0 truncate ui-fs-2xs text-muster-muted/60 max-w-[80px]" title={row.project}>
          {row.project}
        </span>
      )}
    </div>
  );
}