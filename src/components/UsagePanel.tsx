import { useEffect, useState, useCallback } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../lib/invoke";
import { formatTokens } from "../lib/format";
import type { ToolKind, ToolSummary, UsageSession, UsageSummary } from "../lib/types";
import { useT } from "../lib/i18n/context";

type TimeRange = "today" | "week" | "month" | "all";

const TOOL_COLORS: Record<ToolKind, string> = {
  opencode: "#a855f7",
  claude_code: "#d97757",
  codex: "#22c55e",
  kimi_code: "#3b82f6",
};

const TOOL_LABELS: Record<ToolKind, string> = {
  opencode: "OpenCode",
  claude_code: "Claude Code",
  codex: "Codex",
  kimi_code: "Kimi Code",
};

const ALL_TOOLS: ToolKind[] = ["opencode", "claude_code", "codex", "kimi_code"];

function formatCost(c: number | null): string {
  if (c === null) return "-";
  return "$" + c.toFixed(2);
}

function sinceForRange(range: TimeRange): number | undefined {
  if (range === "all") return undefined;
  const now = Date.now();
  if (range === "today") return now - 24 * 60 * 60 * 1000;
  if (range === "week") return now - 7 * 24 * 60 * 60 * 1000;
  if (range === "month") return now - 30 * 24 * 60 * 60 * 1000;
  return undefined;
}

function formatTime(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (sameDay) return `${hh}:${mm}`;
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${mo}/${dd} ${hh}:${mm}`;
}

export default function UsagePanel({ onClose }: { onClose: () => void }) {
  const { t } = useT();
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [sessions, setSessions] = useState<UsageSession[]>([]);
  const [range, setRange] = useState<TimeRange>("week");
  const [toolFilter, setToolFilter] = useState<ToolKind | "all">("all");
  const [sortBy, setSortBy] = useState<"time" | "tokens">("time");

  const load = useCallback(async () => {
    const since = sinceForRange(range);
    const [sum, sess] = await Promise.all([
      api.usage.summary(),
      api.usage.sessions({ since, limit: 500 }),
    ]);
    setSummary(sum);
    setSessions(sess);
  }, [range]);

  // Initial load + reload when range changes.
  useEffect(() => {
    load();
  }, [load]);

  // Listen for background-scan completion.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    // Async listen vs. unmount race: if cleanup ran before the promise
    // resolved, unlisten the moment the handle arrives.
    let cancelled = false;
    listen("usage-updated", () => { load(); }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  // Esc to close.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const handleRefresh = useCallback(async () => {
    await api.usage.refresh();
    await load();
  }, [load]);

  const filteredSessions = sessions
    .filter((s) => toolFilter === "all" || s.tool === toolFilter)
    .sort((a, b) => {
      if (sortBy === "tokens") {
        const totalA = a.tokens.input + a.tokens.output + a.tokens.reasoning + a.tokens.cache_read + a.tokens.cache_write;
        const totalB = b.tokens.input + b.tokens.output + b.tokens.reasoning + b.tokens.cache_read + b.tokens.cache_write;
        return totalB - totalA;
      }
      return b.updated_at - a.updated_at;
    });

  // Build a map for quick card lookup.
  const summaryMap = new Map<ToolKind, ToolSummary>();
  summary?.tools.forEach((ts) => summaryMap.set(ts.tool, ts));

  if (!summary) return null;

  const ranges: TimeRange[] = ["today", "week", "month", "all"];

  return (
    <div className="absolute inset-0 z-40 bg-black/35" onClick={onClose}>
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[760px] max-h-[80vh]">
        <div
          className="bg-muster-bg border border-white/[0.08] rounded-[10px] shadow-[0_12px_32px_rgba(0,0,0,0.5)] px-5 py-4 muster-pop flex flex-col max-h-[80vh]"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header: title + range + refresh */}
          <div className="flex items-center justify-between mb-4">
            <h2 className="ui-fs-base font-semibold">{t("usage.title")}</h2>
            <div className="flex items-center gap-3">
              <div className="flex gap-1">
                {ranges.map((r) => (
                  <button
                    key={r}
                    onClick={() => setRange(r)}
                    className={`px-2 py-1 rounded ui-fs-sm transition-colors ${
                      range === r
                        ? "bg-muster-accent text-white"
                        : "bg-white/[0.05] text-muster-muted hover:bg-muster-hover-btn"
                    }`}
                  >
                    {t(`usage.${r}`)}
                  </button>
                ))}
              </div>
              <button
                onClick={handleRefresh}
                className="px-2 py-1 rounded bg-white/[0.05] ui-fs-sm text-muster-muted hover:bg-muster-hover-btn transition-colors"
                title={t("usage.refresh")}
              >
                &#8635; {t("usage.refresh")}
              </button>
            </div>
          </div>

          {/* Summary cards */}
          <div className="grid grid-cols-4 gap-3 mb-4">
            {ALL_TOOLS.map((tk) => {
              const ts = summaryMap.get(tk);
              const color = TOOL_COLORS[tk];
              const found = ts && ts.session_count > 0;
              return (
                <div
                  key={tk}
                  className="bg-white/[0.03] border border-white/[0.06] rounded-lg px-3 py-2.5"
                >
                  <div className="flex items-center gap-1.5 mb-1.5">
                    <span
                      className="inline-block w-2 h-2 rounded-full"
                      style={{ backgroundColor: color }}
                    />
                    <span className="ui-fs-sm font-medium text-muster-muted truncate">
                      {TOOL_LABELS[tk]}
                    </span>
                  </div>
                  {found ? (
                    <>
                      <div className="text-lg font-semibold tabular-nums">
                        {formatTokens(ts!.total_tokens)}
                      </div>
                      <div className="ui-fs-xs text-muster-muted mb-1">
                        {t("usage.tokens")}
                      </div>
                      <div className="ui-fs-sm tabular-nums text-muster-muted">
                        {formatCost(ts!.cost_usd)} &middot; {ts!.session_count}{" "}
                        {ts!.session_count === 1 ? t("usage.session") : t("usage.sessions")}
                      </div>
                    </>
                  ) : (
                    <div className="ui-fs-sm text-muster-muted/60 py-2">
                      {t("usage.notFound")}
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {/* Sessions table */}
          <div className="flex items-center justify-between mb-2">
            <span className="ui-fs-sm font-medium text-muster-muted uppercase tracking-wide">
              {t("usage.sessions")}
            </span>
            <div className="flex items-center gap-2">
              <select
                value={toolFilter}
                onChange={(e) => setToolFilter(e.target.value as ToolKind | "all")}
                className="bg-white/[0.05] border border-white/[0.06] rounded ui-fs-sm px-2 py-1 text-muster-muted outline-none"
              >
                <option value="all">{t("usage.allTools")}</option>
                {ALL_TOOLS.map((tk) => (
                  <option key={tk} value={tk}>{TOOL_LABELS[tk]}</option>
                ))}
              </select>
              <select
                value={sortBy}
                onChange={(e) => setSortBy(e.target.value as "time" | "tokens")}
                className="bg-white/[0.05] border border-white/[0.06] rounded ui-fs-sm px-2 py-1 text-muster-muted outline-none"
              >
                <option value="time">{t("usage.sortByTime")}</option>
                <option value="tokens">{t("usage.sortByTokens")}</option>
              </select>
            </div>
          </div>

          <div className="overflow-y-auto flex-1 -mx-1 px-1">
            {filteredSessions.length === 0 ? (
              <div className="text-center ui-fs-sm text-muster-muted/60 py-8">
                {t("usage.noSessions")}
              </div>
            ) : (
              <table className="w-full ui-fs-sm">
                <thead>
                  <tr className="text-left text-muster-muted/70 border-b border-white/[0.06]">
                    <th className="py-1.5 pr-3 font-normal">{t("usage.time")}</th>
                    <th className="py-1.5 pr-3 font-normal">{t("usage.tool")}</th>
                    <th className="py-1.5 pr-3 font-normal">{t("usage.model")}</th>
                    <th className="py-1.5 pr-3 font-normal text-right">{t("usage.tokens")}</th>
                    <th className="py-1.5 font-normal text-right">{t("usage.cost")}</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredSessions.map((s) => {
                    const total = s.tokens.input + s.tokens.output + s.tokens.reasoning + s.tokens.cache_read + s.tokens.cache_write;
                    return (
                      <tr key={`${s.tool}-${s.session_id}`} className="border-b border-white/[0.03] hover:bg-white/[0.02]">
                        <td className="py-1.5 pr-3 tabular-nums text-muster-muted">
                          {formatTime(s.updated_at)}
                        </td>
                        <td className="py-1.5 pr-3">
                          <span className="inline-flex items-center gap-1">
                            <span
                              className="inline-block w-1.5 h-1.5 rounded-full"
                              style={{ backgroundColor: TOOL_COLORS[s.tool] }}
                            />
                            {TOOL_LABELS[s.tool]}
                          </span>
                        </td>
                        <td className="py-1.5 pr-3 text-muster-muted truncate max-w-[160px]">
                          {s.model || "-"}
                        </td>
                        <td className="py-1.5 pr-3 text-right tabular-nums">
                          {formatTokens(total)}
                        </td>
                        <td className="py-1.5 text-right tabular-nums text-muster-muted">
                          {formatCost(s.cost_usd)}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
