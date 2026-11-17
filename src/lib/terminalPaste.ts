/// Shared paste-payload resolution for terminal panes: text first, then
/// Explorer file copies. Files copied in Explorer reach the clipboard as
/// CF_HDROP with no `text/plain` counterpart, so `readText()` returns ""
/// for them and the native `read_clipboard_files` command is the only way
/// to see them. See docs/superpowers/specs/2026-11-17-clipboard-file-paste-design.md.

import { api } from "./invoke";
import { shellQuotePath } from "./shellEscape";

export type PastePayload =
  | { kind: "text"; text: string }
  | { kind: "files"; text: string }
  | { kind: "none" };

/// Join file paths into one insertable string: each path single-quoted for
/// PowerShell (same convention as the FileTree drag-drop path), multiple
/// files space-separated (Windows Terminal behavior).
export function formatFilePaths(paths: string[]): string {
  return paths.map(shellQuotePath).join(" ");
}

/// Text wins when present; files only surface when the clipboard carries no
/// text at all (a file copy never does). Callers treat `files` like any
/// other paste: paths are opaque data, so no dangerous-paste dialog.
export async function getPastePayload(): Promise<PastePayload> {
  const text = await navigator.clipboard.readText().catch(() => "");
  if (text) return { kind: "text", text };
  const files = await api.readClipboardFiles().catch(() => [] as string[]);
  if (files.length > 0) return { kind: "files", text: formatFilePaths(files) };
  return { kind: "none" };
}
