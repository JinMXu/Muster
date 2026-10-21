import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../lib/invoke";
import { events } from "../lib/bindings";
import { useTauriEventOf } from "../hooks/useTauriEvent";
import { useT, type TKey } from "../lib/i18n/context";
import { agentLabel } from "../hooks/useAgentStatus";
import {
  formatDuration,
  formatPercent,
  formatTimestamp,
  formatTokens,
  rangeBounds,
  type UsageRange,
} from "../lib/usage";
import type {
  AgentCoverage,
  AppStateView,
  UsageCompare,
  UsageRunView,
} from "../lib/types";
import { IconChartBar, IconRefresh, IconX } from "./icons";

type Tab = "compare" | "runs" | "coverage";

const RANGE_KEY: Record<UsageRange, TKey> = {
  today: "usage.range.today",
  "7d": "usage.range.7d",
  "30d": "usage.range.30d",
  all: "usage.range.all",
};

const TAB_KEY: Record<Tab, TKey> = {
  compare: "usage.tab.compare",
  runs: "usage.tab.runs",
  coverage: "usage.tab.coverage",
};

const CONF_KEY: Record<UsageRunView["confidence"], TKey> = {
  high: "usage.confidence.high",
  medium: "usage.confidence.medium",
  low: "usage.confidence.low",
};

export default function UsagePanel({
  open,
  onClose,
  state,
}: {
  open: boolean;
  onClose: () => void;
  state: AppStateView | null;
}) {
  const { t } = useT();
  const [range, setRange] = useState<UsageRange>("7d");
  const [projectId, setProjectId] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("compare");
  const [compare, setCompare] = useState<UsageCompare | null>(null);
  const [runs, setRuns] = useState<UsageRunView[]>([]);
  const [loading, setLoading] = useState(false);
  const [event] = useTauriEventOf(events.usageUpdated, null);

  const load = useCallback(async () => {
    const { since, until } = rangeBounds(range);
    setLoading(true);
    try {
      const [nextCompare, nextRuns] = await Promise.all([
        api.usageCompare(since, until, projectId),
        api.usageRuns(since, until, projectId, 200),
      ]);
      setCompare(nextCompare);
      setRuns(nextRuns);
    } catch {
      /* backend scan may still be warming up */
    } finally {
      setLoading(false);
    }
  }, [range, projectId]);

  useEffect(() => {
    if (!open) return;
    void load();
  }, [open, load, event?.revision]);

  const refresh = async () => {
    setLoading(true);
    try {
      await api.usageRefresh();
    } catch {
      /* ignore */
    }
    await load();
  };

  const coverage = useMemo(() => {
    const list = [...(compare?.coverage ?? [])];
    list.sort(
      (a, b) =>
        Number(b.has_adapter) - Number(a.has_adapter) ||
        b.events - a.events ||
        agentLabel(a.agent).localeCompare(agentLabel(b.agent))
    );
    return list;
  }, [compare]);

  if (!open) return null;

  const total = compare?.totals;
  const isEmpty = !compare || compare.rows.length === 0;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div
        className="w-[min(1100px,96vw)] h-[min(720px,90vh)] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_18px_48px_rgba(0,0,0,0.55)] muster-pop flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2.5 px-4 h-12 flex-shrink-0 border-b border-white/[0.08]">
          <IconChartBar size={16} className="text-muster-accent flex-shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="ui-fs-base font-medium">{t("usage.title")}</div>
            <div className="ui-fs-xs text-muster-muted/80">
              {compare?.updated_at
                ? t("usage.updated", { time: formatTimestamp(compare.updated_at) })
                : t("usage.loading")}
            </div>
          </div>
          <button
            onClick={refresh}
            title={t("usage.refresh")}
            className="px-1.5 py-1 rounded text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            <IconRefresh size={13} className={loading ? "animate-spin" : ""} />
          </button>
          <button
            onClick={onClose}
            title={t("common.close")}
            className="px-1.5 py-1 rounded text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg"
          >
            <IconX size={14} />
          </button>
        </div>

        <div className="flex items-center gap-3 px-4 py-2 flex-shrink-0 border-b border-white/[0.08]">
          <div className="flex items-center rounded-md bg-muster-hover/50 p-0.5">
            {(["today", "7d", "30d", "all"] as UsageRange[]).map((r) => (
              <button
                key={r}
                onClick={() => setRange(r)}
                className={`px-2 py-1 rounded ui-fs-xs transition-colors duration-muster ease-muster ${
                  range === r
                    ? "bg-muster-selected text-muster-fg"
                    : "text-muster-muted hover:text-muster-fg"
                }`}
              >
                {t(RANGE_KEY[r])}
              </button>
            ))}
          </div>
          <select
            value={projectId ?? ""}
            onChange={(e) => setProjectId(e.target.value || null)}
            className="bg-muster-hover/50 border border-white/[0.08] rounded px-2 py-1 ui-fs-xs text-muster-fg/80 outline-none max-w-[220px]"
          >
            <option value="">{t("usage.allProjects")}</option>
            {(state?.projects ?? []).map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <div className="flex-1" />
          <div className="flex items-center rounded-md bg-muster-hover/50 p-0.5">
            {(["compare", "runs", "coverage"] as Tab[]).map((x) => (
              <button
                key={x}
                onClick={() => setTab(x)}
                className={`px-2.5 py-1 rounded ui-fs-xs transition-colors duration-muster ease-muster ${
                  tab === x
                    ? "bg-muster-selected text-muster-fg"
                    : "text-muster-muted hover:text-muster-fg"
                }`}
              >
                {t(TAB_KEY[x])}
              </button>
            ))}
          </div>
        </div>

        <div className="flex-1 min-h-0 overflow-auto">
          {tab === "compare" && (
            <CompareTable
              rows={compare?.rows ?? []}
              loading={loading}
              empty={isEmpty}
            />
          )}
          {tab === "runs" && <RunsTable runs={runs} onClose={onClose} />}
          {tab === "coverage" && <CoverageGrid coverage={coverage} />}
        </div>

        <div className="flex items-center gap-4 px-4 h-9 flex-shrink-0 border-t border-white/[0.08] ui-fs-xs text-muster-muted/80">
          <span>
            {t("usage.summary", {
              runs: total?.runs ?? 0,
              events: total?.events ?? 0,
            })}
          </span>
          <span className="flex-1" />
          <span>{t("usage.localOnly")}</span>
        </div>
      </div>
    </div>
  );
}

function CompareTable({
  rows,
  loading,
  empty,
}: {
  rows: UsageCompare["rows"];
  loading: boolean;
  empty: boolean;
}) {
  const { t } = useT();
  if (loading && empty) {
    return <div className="p-6 ui-fs-sm text-muster-muted">{t("usage.loading")}</div>;
  }
  if (empty) {
    return (
      <div className="h-full flex items-center justify-center p-8 text-center ui-fs-sm text-muster-muted">
        <div className="max-w-[420px]">{t("usage.empty")}</div>
      </div>
    );
  }
  return (
    <table className="w-full text-left ui-fs-sm border-collapse">
      <thead className="sticky top-0 z-10 bg-muster-bg">
        <tr className="ui-fs-2xs uppercase tracking-wide text-muster-muted/80">
          <th className="px-4 py-2 font-medium">{t("usage.col.agentModel")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.runs")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.attribution")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.tokens")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.tokensPerRun")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.freshPerRun")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.cacheHit")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.tokensPerTurn")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.outputPerTurn")}</th>
          <th className="px-4 py-2 font-medium text-right">{t("usage.col.avgDuration")}</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr
            key={`${r.agent}-${r.model ?? "unknown"}`}
            className="border-t border-white/[0.05] hover:bg-muster-hover/40"
          >
            <td className="px-4 py-2">
              <div className="flex items-center gap-2 min-w-0">
                <span className="font-medium truncate">{agentLabel(r.agent)}</span>
                <span className="ui-fs-xs text-muster-muted/80 truncate">
                  {r.model ?? "—"}
                </span>
              </div>
            </td>
            <td className="px-2 py-2 text-right tabular-nums">{r.runs || "—"}</td>
            <td className="px-2 py-2 text-right tabular-nums">
              {formatPercent(r.attributed_ratio)}
            </td>
            <td
              className="px-2 py-2 text-right tabular-nums"
              title={`input ${formatTokens(r.tokens.input)} · output ${formatTokens(
                r.tokens.output
              )} · reasoning ${formatTokens(r.tokens.reasoning)} · cache read ${formatTokens(
                r.tokens.cache_read
              )}`}
            >
              {formatTokens(r.total_tokens)}
            </td>
            <td className="px-2 py-2 text-right tabular-nums">
              {r.runs ? formatTokens(r.tokens_per_run) : "—"}
            </td>
            <td className="px-2 py-2 text-right tabular-nums">
              {r.runs ? formatTokens(r.fresh_input_per_run) : "—"}
            </td>
            <td className="px-2 py-2 text-right tabular-nums">{formatPercent(r.cache_hit)}</td>
            <td className="px-2 py-2 text-right tabular-nums">
              {formatTokens(r.tokens_per_turn)}
            </td>
            <td className="px-2 py-2 text-right tabular-nums">
              {formatTokens(r.output_per_turn)}
            </td>
            <td className="px-4 py-2 text-right tabular-nums">
              {r.runs ? formatDuration(r.avg_duration_ms) : "—"}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function RunsTable({ runs, onClose }: { runs: UsageRunView[]; onClose: () => void }) {
  const { t } = useT();
  if (runs.length === 0) {
    return (
      <div className="h-full flex items-center justify-center p-8 text-center ui-fs-sm text-muster-muted">
        <div className="max-w-[420px]">{t("usage.runs.empty")}</div>
      </div>
    );
  }
  return (
    <table className="w-full text-left ui-fs-sm border-collapse">
      <thead className="sticky top-0 z-10 bg-muster-bg">
        <tr className="ui-fs-2xs uppercase tracking-wide text-muster-muted/80">
          <th className="px-4 py-2 font-medium">{t("usage.col.time")}</th>
          <th className="px-2 py-2 font-medium">{t("usage.col.agentModel")}</th>
          <th className="px-2 py-2 font-medium">{t("usage.col.project")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.duration")}</th>
          <th className="px-2 py-2 font-medium text-right">{t("usage.col.tokens")}</th>
          <th className="px-4 py-2 font-medium text-right">{t("usage.col.confidence")}</th>
        </tr>
      </thead>
      <tbody>
        {runs.map((r) => {
          const running = r.ended_at == null;
          const duration = (r.ended_at ?? Date.now()) - r.started_at;
          return (
            <tr
              key={r.id}
              onClick={() => {
                api.focusAgentSession(r.session_id).catch(() => {});
                onClose();
              }}
              className="border-t border-white/[0.05] hover:bg-muster-hover/40 cursor-pointer"
              title={t("usage.jumpToSession")}
            >
              <td className="px-4 py-2 whitespace-nowrap text-muster-muted/80">
                {formatTimestamp(r.started_at)}
              </td>
              <td className="px-2 py-2">
                <div className="flex items-center gap-2 min-w-0">
                  <span className="truncate">{agentLabel(r.agent)}</span>
                  <span className="ui-fs-xs text-muster-muted/80 truncate">
                    {r.model ?? "—"}
                  </span>
                  {running && (
                    <span className="ui-fs-2xs px-1 rounded bg-emerald-400/15 text-emerald-300 flex-shrink-0">
                      {t("usage.running")}
                    </span>
                  )}
                </div>
              </td>
              <td className="px-2 py-2 text-muster-muted/80 truncate max-w-[220px]">
                {r.project}
              </td>
              <td className="px-2 py-2 text-right tabular-nums">
                {formatDuration(duration)}
              </td>
              <td className="px-2 py-2 text-right tabular-nums">
                {formatTokens(
                  r.tokens.input +
                    r.tokens.output +
                    r.tokens.reasoning +
                    r.tokens.cache_read +
                    r.tokens.cache_write
                )}
              </td>
              <td className="px-4 py-2 text-right">
                <span className="ui-fs-xs text-muster-muted/80">
                  {t(CONF_KEY[r.confidence])}
                </span>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function CoverageGrid({ coverage }: { coverage: AgentCoverage[] }) {
  const { t } = useT();
  return (
    <div className="p-4">
      <div className="ui-fs-xs text-muster-muted/80 mb-3">{t("usage.coverage.note")}</div>
      <div className="grid grid-cols-2 md:grid-cols-3 gap-2">
        {coverage.map((c) => {
          const status = !c.has_adapter
            ? t("usage.coverage.unavailable")
            : c.events
              ? t("usage.coverage.native")
              : t("usage.coverage.noData");
          return (
            <div
              key={c.agent}
              className="rounded border border-white/[0.06] bg-muster-panel/40 px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="ui-fs-sm font-medium truncate">{agentLabel(c.agent)}</span>
                <span
                  className={`ui-fs-2xs px-1.5 py-0.5 rounded flex-shrink-0 ${
                    c.has_adapter
                      ? c.events
                        ? "bg-emerald-400/15 text-emerald-300"
                        : "bg-amber-400/15 text-amber-300"
                      : "bg-white/[0.06] text-muster-muted"
                  }`}
                >
                  {status}
                </span>
              </div>
              <div className="mt-1 ui-fs-xs text-muster-muted/80 tabular-nums">
                {c.has_adapter
                  ? t("usage.coverage.stats", {
                      events: c.events,
                      tokens: formatTokens(c.total_tokens),
                    })
                  : "—"}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
