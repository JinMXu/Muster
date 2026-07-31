import { useEffect, useState } from "react";
import type { AppStateView, RightPanel } from "../lib/types";
import { api } from "../lib/invoke";
import { useT } from "../lib/i18n/context";
import FileTree from "./FileTree";
import GitPanel from "./GitPanel";
import InfoPanel from "./InfoPanel";
import WindowControls from "./WindowControls";
import { IconFolder, IconGitBranch, IconInfo } from "./icons";

/// Right sidebar with Files / Git / Info tabs, anchored to the selected
/// project's directory (its custom_directory, or the cwd of its session).
/// All three panels stay mounted (hidden via CSS) so switching tabs doesn't
/// lose expansion state, git info, or kill/restart polling intervals.
///
/// The top row mirrors the main Header's height and hosts only the window
/// caption buttons, flush to the window's top-right corner (Header skips its
/// own WindowControls while this panel is visible); the panel tabs sit on
/// their own row below.
export default function RightSidebar({ state, width }: { state: AppStateView | null; width: number }) {
  const [tab, setTab] = useState<RightPanel>(state?.panel_tab ?? "files");
  const { t } = useT();
  useEffect(() => {
    if (state?.panel_tab) setTab(state.panel_tab);
  }, [state?.panel_tab]);

  return (
    <aside
      className="bg-muster-panel flex flex-col flex-shrink-0"
      style={{ width }}
    >
      <div className="h-9 flex items-center flex-shrink-0" data-tauri-drag-region>
        <div className="flex-1 self-stretch" data-tauri-drag-region />
        <WindowControls />
      </div>
      <div className="flex items-center gap-1 px-2 pb-1 flex-shrink-0">
        <Tab active={tab === "info"} onClick={() => api.togglePanel("info")} icon={<IconInfo size={13} />} label={t("rightSidebar.info")} />
        <Tab active={tab === "files"} onClick={() => api.togglePanel("files")} icon={<IconFolder size={13} />} label={t("rightSidebar.files")} />
        <Tab active={tab === "git"} onClick={() => api.togglePanel("git")} icon={<IconGitBranch size={13} />} label={t("rightSidebar.git")} />
      </div>
      <div className="flex-1 min-h-0 overflow-hidden">
        <div className="h-full" style={{ display: tab === "files" ? "block" : "none" }}>
          <FileTree state={state} />
        </div>
        <div className="h-full" style={{ display: tab === "git" ? "block" : "none" }}>
          <GitPanel state={state} />
        </div>
        <div className="h-full" style={{ display: tab === "info" ? "block" : "none" }}>
          <InfoPanel state={state} />
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
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex-1 px-2 py-1.5 rounded-md ui-fs-sm flex items-center justify-center gap-1 ${
        active ? "bg-white/[0.09] text-muster-fg" : "text-muster-muted hover:bg-muster-hover"
      }`}
    >
      <span className="flex items-center">{icon}</span>
      <span>{label}</span>
    </button>
  );
}
