import { useMemo } from "react";
import type { ToolKind, UsageSession } from "../lib/types";
import { useT } from "../lib/i18n/context";

const ALL_TOOLS: ToolKind[] = ["opencode", "claude_code", "codex", "kimi_code"];

const TOOL_COLORS: Record<ToolKind, string> = {
  opencode: "#a855f7",
  claude_code: "#d97757",
  codex: "#22c55e",
  kimi_code: "#3b82f6",
};

interface Day {
  key: string;
  label: string;
  tools: Record<ToolKind, number>;
  cost: number;
  total: number;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

/// Daily token usage bar chart, stacked per tool. Rendered as plain divs so
/// the day labels stay crisp; native `title` tooltips carry exact values.
export default function UsageChart({ sessions }: { sessions: UsageSession[] }) {
  const { t } = useT();

  const days = useMemo(() => {
    const map = new Map<string, Day>();
    for (const s of sessions) {
      const d = new Date(s.updated_at);
      const key = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
      let day = map.get(key);
      if (!day) {
        day = {
          key,
          label: `${d.getMonth() + 1}/${d.getDate()}`,
          tools: { opencode: 0, claude_code: 0, codex: 0, kimi_code: 0 },
          cost: 0,
          total: 0,
        };
        map.set(key, day);
      }
      const total =
        s.tokens.input + s.tokens.output + s.tokens.reasoning + s.tokens.cache_read + s.tokens.cache_write;
      day.tools[s.tool] += total;
      day.total += total;
      day.cost += s.cost_usd ?? 0;
    }
    return [...map.values()].sort((a, b) => a.key.localeCompare(b.key));
  }, [sessions]);

  const totals = useMemo(
    () => ({
      tokens: days.reduce((acc, d) => acc + d.total, 0),
      cost: days.reduce((acc, d) => acc + d.cost, 0),
    }),
    [days]
  );

  if (sessions.length === 0 || days.length === 0) {
    return (
      <div className="mt-3 bg-white/[0.02] border border-white/[0.05] rounded-lg px-3 py-4 text-center ui-fs-sm text-muster-muted/70">
        {t("usage.empty")}
      </div>
    );
  }

  const maxTotal = Math.max(...days.map((d) => d.total), 1);
  // Show at most ~6 x-axis labels.
  const labelStep = Math.max(1, Math.ceil(days.length / 6));

  return (
    <div className="mt-3 bg-white/[0.02] border border-white/[0.05] rounded-lg p-2">
      <div className="flex items-baseline justify-between px-1 mb-2">
        <span className="ui-fs-xs text-muster-muted uppercase tracking-wide">{t("usage.dailyTokens")}</span>
        <span className="ui-fs-sm text-muster-muted tabular-nums">
          {formatTokens(totals.tokens)} {t("usage.tokens")}
          {totals.cost > 0 && ` · $${totals.cost.toFixed(2)}`}
        </span>
      </div>

      <div className="flex items-end gap-[2px] h-28 px-1">
        {days.map((d) => (
          <div
            key={d.key}
            title={`${d.label}: ${formatTokens(d.total)} ${t("usage.tokens")}${d.cost > 0 ? ` · $${d.cost.toFixed(2)}` : ""}`}
            className="flex-1 flex flex-col justify-end h-full rounded-sm hover:bg-white/[0.03]"
          >
            {/* Render tools bottom-up (reverse order) so the stack reads top-down. */}
            {[...ALL_TOOLS].reverse().map((tk) => {
              const v = d.tools[tk];
              if (!v) return null;
              return (
                <div
                  key={tk}
                  className="w-full"
                  style={{ height: `${(v / maxTotal) * 100}%`, backgroundColor: TOOL_COLORS[tk] }}
                />
              );
            })}
          </div>
        ))}
      </div>

      <div className="flex mt-1 px-1">
        {days.map((d, i) => (
          <div
            key={d.key}
            className="flex-1 text-center ui-fs-2xs text-muster-muted/70 tabular-nums overflow-hidden"
            style={{ visibility: i % labelStep === 0 ? "visible" : "hidden" }}
          >
            {d.label}
          </div>
        ))}
      </div>
    </div>
  );
}
