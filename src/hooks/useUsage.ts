import { useEffect, useState } from "react";
import { api } from "../lib/invoke";
import { events } from "../lib/bindings";
import type { UsageOverview } from "../lib/types";
import { startOfToday } from "../lib/usage";
import { useTauriEventOf } from "./useTauriEvent";

/// Today's usage overview, refreshed when the backend finishes a scan.
export function useUsageOverview(active = true): UsageOverview | null {
  const [event] = useTauriEventOf(events.usageUpdated, null);
  const [data, setData] = useState<UsageOverview | null>(null);
  useEffect(() => {
    if (!active) return;
    let alive = true;
    api
      .usageOverview(startOfToday())
      .then((d) => {
        if (alive) setData(d);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [active, event?.revision]);
  return data;
}
