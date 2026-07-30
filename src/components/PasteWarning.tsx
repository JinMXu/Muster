import { useT } from "../lib/i18n/context";

export default function PasteWarning({
  text,
  onConfirm,
  onCancel,
}: {
  text: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useT();
  const preview = text.length > 200 ? text.slice(0, 200) + "..." : text;

  return (
    <div
      className="absolute inset-0 z-50 bg-black/30 flex items-center justify-center"
      onClick={onCancel}
    >
      <div
        className="w-[420px] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] p-4 muster-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="ui-fs-base font-medium text-yellow-400 mb-1">
          {t("terminal.pasteWarningTitle")}
        </div>
        <div className="ui-fs-sm text-muster-muted mb-3">
          {t("terminal.pasteWarningBody")}
        </div>
        <div className="bg-white/[0.04] border border-white/[0.06] rounded-md px-2.5 py-1.5 mb-3 font-mono text-[11px] leading-relaxed text-muster-fg/70 max-h-24 overflow-y-auto whitespace-pre-wrap break-all">
          {preview}
        </div>
        <div className="flex justify-end gap-2 ui-fs-sm">
          <button
            className="px-2.5 py-1 rounded-md bg-white/[0.05] text-muster-muted hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
            onClick={onCancel}
          >
            {t("terminal.cancel")}
          </button>
          <button
            className="px-2.5 py-1 rounded-md bg-yellow-600/80 text-white hover:bg-yellow-600 active:scale-[.97] transition-transform duration-muster ease-muster"
            onClick={onConfirm}
          >
            {t("terminal.pasteAnyway")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function looksDangerousPaste(text: string): boolean {
  if (text.length > 1024 * 1024) return true; // >1MB is always suspicious
  const trimmed = text.trim();
  if (trimmed.length === 0) return false;

  // Multi-line text with common command indicators
  const lines = trimmed.split("\n");
  if (lines.length > 2) {
    const commandLines = lines.filter((line) => {
      const t = line.trim();
      if (t === "") return false;
      return (
        t.startsWith("$ ") ||
        t.startsWith("# ") ||
        t.startsWith("> ") ||
        t.startsWith("curl ") ||
        t.startsWith("wget ") ||
        t.startsWith("sudo ") ||
        t.startsWith("npm ") ||
        t.startsWith("pip ") ||
        t.startsWith("rm -") ||
        t.startsWith("git ") ||
        t.startsWith("chmod ") ||
        /^\s*(?:\w+\.)?exe\b/i.test(t) ||
        /^\s*\.\//.test(t) ||
        /^\s*\\/.test(t)
      );
    });
    if (commandLines.length > 0) return true;
  }

  // Hex-encoded or base64-encoded content (>500 chars) is suspicious
  if (trimmed.length > 500 && /^[0-9a-fA-F\s]+$/.test(trimmed)) return true;

  return false;
}
