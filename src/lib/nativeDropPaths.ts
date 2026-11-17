/// Disk paths for OS file drops.
///
/// The page cannot read a dropped file's path from `DataTransfer` — but
/// WebView2 can: the FileList is posted to the host with
/// `chrome.webview.postMessageWithAdditionalObjects`, the Rust side resolves
/// each `ICoreWebView2File::Path` and answers on the `nativeDropPaths` event
/// under the correlation token we sent. See services/native_drop.rs.

import { events } from "./bindings";
import { api } from "./invoke";
import { formatFilePaths } from "./terminalPaste";

interface WebView2Bridge {
  postMessageWithAdditionalObjects?: (message: string, objects: ArrayLike<unknown>) => void;
}

/// Correlation tokens still waiting for the host's reply, mapped to the
/// session whose pane received the drop.
const pending = new Map<string, string>();
let nextToken = 1;
let listening = false;

/// Start listening for resolved drop paths. Call once at startup.
export function initNativeDropPaths(): void {
  if (listening) return;
  listening = true;
  events.nativeDropPaths.listen((event) => {
    const { token, paths } = event.payload;
    const sessionId = pending.get(token);
    pending.delete(token);
    if (sessionId && paths.length > 0) {
      api.sendText(sessionId, formatFilePaths(paths));
    }
  });
}

/// Ask the host to resolve disk paths for a drop, inserting them into
/// `sessionId` once they arrive. Returns false when the bridge is missing
/// (older WebView2 runtime), so the caller knows the drop did nothing.
export function requestDroppedPaths(sessionId: string, files: FileList): boolean {
  const bridge = (window as unknown as { chrome?: { webview?: WebView2Bridge } }).chrome?.webview;
  if (!bridge?.postMessageWithAdditionalObjects) return false;
  const token = `d${nextToken++}`;
  pending.set(token, sessionId);
  try {
    bridge.postMessageWithAdditionalObjects(`muster:files:${token}`, files);
    return true;
  } catch {
    pending.delete(token);
    return false;
  }
}

/// The files carried by a drag, if any. OS drops carry them; in-page drags
/// (FileTree, panes) never do — those travel as `application/x-muster-path`.
export function droppedFiles(dt: DataTransfer | null): FileList | null {
  return dt && dt.files.length > 0 ? dt.files : null;
}