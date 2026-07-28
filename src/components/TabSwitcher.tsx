import { useEffect, useRef } from "react";
import type { PaneContentKind, Uuid } from "../lib/types";

export interface SwitcherTab {
  id: Uuid;
  title: string;
  subtitle: string | null;
  kind: PaneContentKind | null;
}

const KIND_ICON: Record<PaneContentKind, string> = {
  session: ">_",
  file: "▤",
  diff: "±",
};

/// Centered Ctrl+Tab overlay listing the current project's tabs. Purely
/// presentational — the parent owns open/cycling state and commits on Ctrl
/// release. Clicking a row selects it immediately; clicking the backdrop or
/// pressing Escape closes without switching.
export default function TabSwitcher({
  tabs,
  index,
  onSelect,
  onClose,
}: {
  tabs: SwitcherTab[];
  index: number;
  onSelect: (id: Uuid) => void;
  onClose: () => void;
}) {
  const listRef = useRef<HTMLDivElement>(null);

  // Keep the highlighted row in view while cycling past the visible window.
  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-idx="${index}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [index]);

  return (
    <div
      className="absolute inset-0 z-50 bg-black/15 flex justify-center items-start"
      onClick={onClose}
    >
      <div
        className="mt-20 w-[440px] max-w-[90vw] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] muster-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <div ref={listRef} className="max-h-80 overflow-y-auto p-1.5">
          {tabs.map((tab, i) => (
            <button
              key={tab.id}
              data-idx={i}
              onClick={() => onSelect(tab.id)}
              className={`w-full flex items-center gap-2 px-2.5 h-8 rounded-md text-[12.5px] text-left ${
                i === index ? "bg-white/[0.09] text-muster-fg" : "text-muster-muted"
              }`}
            >
              <span
                className={`w-6 text-center text-[11px] font-mono ${
                  i === index ? "text-muster-accent" : "text-muster-muted"
                }`}
              >
                {tab.kind ? KIND_ICON[tab.kind] : "·"}
              </span>
              <span className="flex-1 truncate">{tab.title}</span>
              {tab.subtitle && (
                <span className="max-w-[45%] truncate text-[11px] text-muster-muted/80" dir="rtl">
                  {tab.subtitle}
                </span>
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
