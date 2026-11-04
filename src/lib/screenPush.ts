import { api } from "./invoke";
import { screenDiag, screenSessionIds, screenText } from "./terminalRegistry";

const PUSH_INTERVAL_MS = 3000;

/// Last text pushed per session — a screen that hasn't changed isn't pushed
/// again, so an idle workspace generates no IPC traffic at all. During the
/// viewport bug hunt the diag snapshot also participates: a viewport move
/// without a text change (the scroll-follow failure mode) must still push.
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
      const diag = screenDiag(id);
      const last = lastPushed.get(id);
      const lastDiag = last?.split("\n##DIAG##")[1] ?? null;
      const lastText = last?.split("\n##DIAG##")[0] ?? null;
      if (lastText === text && lastDiag === diag) continue;
      const first = !lastPushed.has(id);
      lastPushed.set(id, `${text}\n##DIAG##${diag}`);
      console.debug(
        `[screenPush] ${id.slice(0, 8)} push ${text.length} chars (first=${first}) ${diag ?? ""}`,
      );
      void api.setSessionScreen(id, text, diag).catch(() => {
        // A push racing the pane's close is harmless; the next tick moves on.
      });
    }
  }, PUSH_INTERVAL_MS);
}
