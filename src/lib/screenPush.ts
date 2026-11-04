import { api } from "./invoke";
import { screenSessionIds, screenText } from "./terminalRegistry";

const PUSH_INTERVAL_MS = 3000;

/// Last text pushed per session — a screen that hasn't changed isn't pushed
/// again, so an idle workspace generates no IPC traffic at all.
const lastPushed = new Map<string, string>();

/// Push every open terminal's visible screen text to the backend, which
/// runs the per-agent screen-detect manifests over it (kero's engine, see
/// `services::screen_detect`). Started once per window; terminals with no
/// visible text are skipped — there is nothing to classify yet.
export function startScreenPush(): void {
  setInterval(() => {
    const live = screenSessionIds();
    for (const id of Object.keys(lastPushed)) {
      if (!live.includes(id)) lastPushed.delete(id);
    }
    for (const id of live) {
      const text = screenText(id);
      if (!text || text.trim() === "") continue;
      if (lastPushed.get(id) === text) continue;
      const first = !lastPushed.has(id);
      lastPushed.set(id, text);
      console.debug(
        `[screenPush] ${id.slice(0, 8)} push ${text.length} chars (first=${first})`,
      );
      void api.setSessionScreen(id, text).catch(() => {
        // A push racing the pane's close is harmless; the next tick moves on.
      });
    }
  }, PUSH_INTERVAL_MS);
}
