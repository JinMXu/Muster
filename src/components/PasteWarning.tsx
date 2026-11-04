import { useT } from "../lib/i18n/context";
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";

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
    <Modal onClose={onCancel} fixed={false} width={420} className="p-4">
      <div className="ui-fs-base font-medium text-yellow-400 mb-1">
        {t("terminal.pasteWarningTitle")}
      </div>
      <div className="ui-fs-sm text-muster-muted mb-3">
        {t("terminal.pasteWarningBody")}
      </div>
      <div className="bg-muster-fill-weak border border-muster-border rounded-md px-2.5 py-1.5 mb-3 font-mono text-[11px] leading-relaxed text-muster-fg/70 max-h-24 overflow-y-auto whitespace-pre-wrap break-all">
        {preview}
      </div>
      <div className="flex justify-end gap-2 ui-fs-sm">
        <Button size="sm" onClick={onCancel}>
          {t("terminal.cancel")}
        </Button>
        <Button size="sm" variant="warning" onClick={onConfirm}>
          {t("terminal.pasteAnyway")}
        </Button>
      </div>
    </Modal>
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
