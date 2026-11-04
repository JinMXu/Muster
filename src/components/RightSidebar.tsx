import { useEffect, useState } from "react";
import type { AppStateView, RightPanel } from "../lib/types";
import { api } from "../lib/invoke";
import { useT } from "../lib/i18n/context";
import FileTree from "./FileTree";
import GitPanel from "./GitPanel";
import InfoPanel from "./InfoPanel";
import WindowControls from "./WindowControls";
import { IconFolder, IconGitBranch, IconInfo, IconPanelRightClose } from "./icons";

/// Right sidebar with Files / Git / Info tabs, anchored to the selected
/// project's directory (its custom_directory, or the cwd of its session).
/// All three panels stay mounted (hidden via CSS) so switching tabs doesn't
/// lose expansion state; hidden panels get `active={false}` so their polls
/// pause instead of running in the background.
///
/// The top row mirrors the main Header's height and hosts everything in one
/// strip, kero-style: compact panel chips on the left, the drag region in
/// the middle, and the window caption buttons flush to the top-right corner
/// (Header skips its own WindowControls while this panel is visible).
/// Clicking the active chip hides the panel (the Header button re-opens it).
export default function RightSidebar({
  state,
  width,
  onToggle,
}: {
  state: AppStateView | null;
  width: number;
  /// Collapse this panel (same backend command as Ctrl+Shift+B). Lives in the
  /// panel's top bar flush left of the caption buttons; the Header hosts the
  /// expand twin while the panel is hidden.
  onToggle: () => void;
}) {
  const [tab, setTab] = useState<RightPanel>(state?.panel_tab ?? "files");
  const { t } = useT();
  // Narrow panel: chips drop their labels (tooltips carry the names) so the
  // top bar still fits beside the caption buttons.
  const compact = width < 360;
  useEffect(() => {
    if (state?.panel_tab) setTab(state.panel_tab);
  }, [state?.panel_tab]);

  return (
    <aside
      className="bg-muster-panel flex flex-col flex-shrink-0"
      style={{ width }}
    >
      <div className="h-9 flex items-center flex-shrink-0 pl-2" data-tauri-drag-region>
        <Tab active={tab === "info"} onClick={() => api.togglePanel("info")} icon={<IconInfo size={14} />} label={t("rightSidebar.info")} compact={compact} />
        <Tab active={tab === "files"} onClick={() => api.togglePanel("files")} icon={<IconFolder size={14} />} label={t("rightSidebar.files")} compact={compact} />
        <Tab active={tab === "git"} onClick={() => api.togglePanel("git")} icon={<IconGitBranch size={14} />} label={t("rightSidebar.git")} compact={compact} />
        <div className="flex-1 self-stretch" data-tauri-drag-region />
        <button
          onClick={onToggle}
          title={t("header.toggleRightSidebarTooltip")}
          className="w-6 h-6 mr-1 rounded text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn flex items-center justify-center ui-fs-base active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          <IconPanelRightClose size={15} />
        </button>
        <WindowControls />
      </div>
      <div className="flex-1 min-h-0 overflow-hidden">
        <div className="h-full" style={{ display: tab === "files" ? "block" : "none" }}>
          <FileTree state={state} active={tab === "files"} />
        </div>
        <div className="h-full" style={{ display: tab === "git" ? "block" : "none" }}>
          <GitPanel state={state} active={tab === "git"} />
        </div>
        <div className="h-full" style={{ display: tab === "info" ? "block" : "none" }}>
          <InfoPanel state={state} active={tab === "info"} />
        </div>
      </div>
    </aside>
  );
}

function Tab({
  active,
  onClick,
  icon,
  label,
  compact,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  /// Narrow panel: icon-only chip; the label rides the tooltip.
  compact?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      className={`rounded-md ui-fs-xs flex items-center gap-1.5 flex-shrink-0 transition-colors duration-muster ease-muster ${
        compact ? "px-1.5" : "px-2"
      } h-6 ${
        active
          ? "bg-muster-selected text-muster-fg"
          : "text-muster-muted hover:bg-muster-hover hover:text-muster-fg"
      }`}
    >
      <span className={`flex items-center ${active ? "text-muster-accent" : ""}`}>{icon}</span>
      {!compact && <span>{label}</span>}
    </button>
  );
}
