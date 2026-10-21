import type { TokenUsage } from "./types";

export type UsageRange = "today" | "7d" | "30d" | "all";

export function tokenTotal(t: TokenUsage): number {
  return t.input + t.output + t.reasoning + t.cache_read + t.cache_write;
}

export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function rangeBounds(range: UsageRange): { since: number | null; until: number | null } {
  const now = Date.now();
  switch (range) {
    case "today":
      return { since: startOfToday(), until: null };
    case "7d":
      return { since: now - 7 * 86_400_000, until: null };
    case "30d":
      return { since: now - 30 * 86_400_000, until: null };
    case "all":
      return { since: null, until: null };
  }
}

export function formatTokens(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0";
  if (n < 1_000) return String(Math.round(n));
  if (n < 1_000_000) return `${(n / 1_000).toFixed(n < 10_000 ? 1 : 0)}K`;
  if (n < 1_000_000_000) return `${(n / 1_000_000).toFixed(n < 10_000_000 ? 1 : 0)}M`;
  return `${(n / 1_000_000_000).toFixed(1)}B`;
}

export function formatPercent(x: number): string {
  return `${Math.round(x * 100)}%`;
}

export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "—";
  if (ms < 60_000) return `${Math.round(ms / 1_000)}s`;
  if (ms < 3_600_000) return `${Math.round(ms / 60_000)}m`;
  const h = Math.floor(ms / 3_600_000);
  const m = Math.round((ms % 3_600_000) / 60_000);
  return m ? `${h}h ${m}m` : `${h}h`;
}

export function formatTimestamp(ms: number): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
