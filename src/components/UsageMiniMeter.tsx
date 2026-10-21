import { useUsageOverview } from "../hooks/useUsage";
import { useT } from "../lib/i18n/context";
import { formatTokens, tokenTotal } from "../lib/usage";
import { IconChartBar } from "./icons";

/// Compact sidebar entry point: today's tokens plus a live-run dot. The full
/// comparison view opens from here, the command palette, or Ctrl+Shift+U.
export default function UsageMiniMeter({ onOpen }: { onOpen: () => void }) {
  const { t } = useT();
  const overview = useUsageOverview();
  const tokens = overview ? tokenTotal(overview.today.tokens) : 0;
  const active = overview?.active_runs.length ?? 0;
  return (
    <button
      title={tokens ? t("usage.miniTitle", { tokens: formatTokens(tokens) }) : t("usage.title")}
      aria-label={t("usage.title")}
      onClick={onOpen}
      className="flex items-center gap-1 px-1.5 h-6 rounded-full bg-muster-hover/60 hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
    >
      <IconChartBar size={12} className="text-muster-accent" />
      <span className="ui-fs-xs tabular-nums text-muster-fg/80 leading-none font-medium">
        {tokens ? formatTokens(tokens) : "—"}
      </span>
      {active > 0 && <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 flex-shrink-0" />}
    </button>
  );
}
