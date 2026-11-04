import { useUsageOverview } from "../hooks/useUsage";
import { useT } from "../lib/i18n/context";
import { formatTokens, tokenTotal } from "../lib/usage";
import { IconChartBar } from "./icons";

/// Compact sidebar entry point: today's tokens plus a live-run dot. The full
/// comparison view opens from here, the command palette, or Ctrl+Shift+U.
/// With nothing to show it renders as a plain icon button — the old
/// "icon + —" placeholder pill read as noise.
export default function UsageMiniMeter({ onOpen }: { onOpen: () => void }) {
  const { t } = useT();
  const overview = useUsageOverview();
  const tokens = overview ? tokenTotal(overview.today.tokens) : 0;
  const active = overview?.active_runs.length ?? 0;
  if (!tokens && !active) {
    return (
      <button
        title={t("usage.title")}
        aria-label={t("usage.title")}
        onClick={onOpen}
        className="w-6 h-6 rounded-md text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn flex items-center justify-center ui-fs-base active:scale-[.97] transition-transform duration-muster ease-muster"
      >
        <IconChartBar size={15} />
      </button>
    );
  }
  return (
    <button
      title={t("usage.miniTitle", { tokens: formatTokens(tokens) })}
      aria-label={t("usage.title")}
      onClick={onOpen}
      className="flex items-center gap-1 px-1.5 h-6 rounded-md bg-muster-hover/60 hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
    >
      <IconChartBar size={14} className="text-muster-accent" />
      {tokens > 0 && (
        <span className="ui-fs-xs tabular-nums text-muster-fg/80 leading-none font-medium">
          {formatTokens(tokens)}
        </span>
      )}
      {active > 0 && <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 flex-shrink-0" />}
    </button>
  );
}
