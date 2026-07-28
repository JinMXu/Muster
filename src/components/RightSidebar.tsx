import { useEffect, useState } from "react";
import type { AppStateView, RightPanel } from "../lib/types";
import { api } from "../lib/invoke";
import { useT } from "../lib/i18n/context";
import FileTree from "./FileTree";
import GitPanel from "./GitPanel";
import InfoPanel from "./InfoPanel";
import { IconFolder, IconGitBranch, IconInfo } from "./icons";

/// Right sidebar with Files / Git / Info tabs, anchored to the selected
/// project's directory (its custom_directory, or the cwd of its session).
export default function RightSidebar({ state }: { state: AppStateView | null }) {
  const [tab, setTab] = useState<RightPanel>(state?.panel_tab ?? "files");
  const { t } = useT();
  useEffect(() => {
    if (state?.panel_tab) setTab(state.panel_tab);
  }, [state?.panel_tab]);

  return (
    <aside
      className="w-64 bg-muster-panel flex flex-col flex-shrink-0"
    >
      <div className="flex items-center gap-1 px-2 pt-3 pb-1">
        <Tab active={tab === "info"} onClick={() => api.togglePanel("info")} icon={<IconInfo size={13} />} label={t("rightSidebar.info")} />
        <Tab active={tab === "files"} onClick={() => api.togglePanel("files")} icon={<IconFolder size={13} />} label={t("rightSidebar.files")} />
        <Tab active={tab === "git"} onClick={() => api.togglePanel("git")} icon={<IconGitBranch size={13} />} label={t("rightSidebar.git")} />
      </div>
      <div className="flex-1 min-h-0 overflow-hidden">
        {tab === "files" && <FileTree state={state} />}
        {tab === "git" && <GitPanel state={state} />}
        {tab === "info" && <InfoPanel state={state} />}
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
      className={`flex-1 px-2 py-1.5 rounded-md text-[11px] flex items-center justify-center gap-1 ${
        active ? "bg-white/[0.09] text-muster-fg" : "text-muster-muted hover:bg-muster-hover"
      }`}
    >
      <span className="flex items-center">{icon}</span>
      <span>{label}</span>
    </button>
  );
}