import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdaterPhase =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }
  | { kind: "available"; version: string }
  | { kind: "downloading"; progress: number; total?: number }
  | { kind: "ready" }
  | { kind: "error"; stage: "check" | "install"; message: string };

export interface UpdaterController {
  checkForUpdates: () => Promise<void>;
  downloadAndInstall: () => Promise<void>;
  restartToUpdate: () => void;
}

/**
 * Drive the Tauri updater plugin through the full lifecycle:
 * check -> download/install (with progress) -> relaunch.
 * `onPhase` receives every state transition so UI can render it.
 */
export function createUpdater(onPhase: (p: UpdaterPhase) => void): UpdaterController {
  let pending: Update | null = null;

  async function checkForUpdates() {
    pending = null;
    onPhase({ kind: "checking" });
    try {
      const update = await check();
      if (update) {
        pending = update;
        onPhase({ kind: "available", version: update.version });
      } else {
        onPhase({ kind: "none" });
      }
    } catch (e) {
      onPhase({ kind: "error", stage: "check", message: String(e) });
    }
  }

  async function downloadAndInstall() {
    if (!pending) return;
    let total: number | undefined;
    let received = 0;
    onPhase({ kind: "downloading", progress: 0 });
    try {
      await pending.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength;
            break;
          case "Progress":
            received += event.data.chunkLength;
            onPhase({
              kind: "downloading",
              progress: total ? Math.min(100, (received / total) * 100) : 0,
              total,
            });
            break;
          case "Finished":
            onPhase({ kind: "downloading", progress: 100, total });
            break;
        }
      });
      onPhase({ kind: "ready" });
    } catch (e) {
      onPhase({ kind: "error", stage: "install", message: String(e) });
    }
  }

  function restartToUpdate() {
    // On Windows the NSIS installer relaunches the app itself when
    // possible; relaunch() is the safe fallback for MSI and no-op cases.
    relaunch().catch(() => {});
  }

  return { checkForUpdates, downloadAndInstall, restartToUpdate };
}
