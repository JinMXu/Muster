import { useEffect, useRef } from "react";
import type { PaneContentKind, Uuid } from "../lib/types";
import { Modal } from "./ui/Modal";
import { MenuItem } from "./ui/MenuList";

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
    <Modal
      onClose={onClose}
      fixed={false}
      align="top"
      topOffset={80}
      scrim="dim"
      className="w-[440px] max-w-[90vw]"
    >
      <div ref={listRef} className="max-h-80 overflow-y-auto p-1.5">
        {tabs.map((tab, i) => (
          <MenuItem
            key={tab.id}
            dataIdx={i}
            height="md"
            selected={i === index}
            onClick={() => onSelect(tab.id)}
          >
            <span
              className={`w-6 text-center ui-fs-sm font-mono ${
                i === index ? "text-muster-accent" : "text-muster-muted"
              }`}
            >
              {tab.kind ? KIND_ICON[tab.kind] : "·"}
            </span>
            <span className="flex-1 truncate">{tab.title}</span>
            {tab.subtitle && (
              <span className="max-w-[45%] truncate ui-fs-sm text-muster-muted/80" dir="rtl">
                {tab.subtitle}
              </span>
            )}
          </MenuItem>
        ))}
      </div>
    </Modal>
  );
}
