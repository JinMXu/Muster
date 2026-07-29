import { useEffect, useRef, useState } from "react";
import Editor from "@monaco-editor/react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/invoke";
import { editorOptions, languageForPath } from "../lib/monaco";
import { useSettings } from "../lib/settingsStore";
import type { FileTabInfo } from "../lib/types";
import { useT } from "../lib/i18n/context";

/// Monaco-based file editor pane. Edits are reported to the backend
/// (debounced) via `file_text_changed`; the dirty dot comes from `is_dirty`.
export default function FilePane({
  fileId,
  focused,
}: {
  fileId: string;
  focused: boolean;
}) {
  const [info, setInfo] = useState<FileTabInfo | null>(null);
  const [text, setText] = useState("");
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<string | null>(null);
  const editorRef = useRef<Parameters<NonNullable<React.ComponentProps<typeof Editor>["onMount"]>>[0] | null>(null);
  const { t } = useT();
  // Editor options follow the saved settings (font size/family/wrap); the
  // options prop diff goes through Monaco's updateOptions automatically.
  const settings = useSettings();
  const options = {
    ...editorOptions,
    fontSize: settings?.font_size ?? 13,
    fontFamily: settings?.font_family
      ? `${settings.font_family}, 'JetBrains Mono', Consolas, monospace`
      : editorOptions.fontFamily,
    wordWrap: (settings?.editor_wrap_lines ? "on" : "off") as "on" | "off",
  };

  // Monaco renders its own internal textarea, which the app's global
  // shortcut handler deliberately ignores — so Ctrl+S must be bound inside
  // the editor itself. Focus follows the pane's focus state the same way.
  useEffect(() => {
    if (focused) editorRef.current?.focus();
  }, [focused]);

  // Immediately push any debounce-pending edit to the backend and mark the
  // tab dirty. Called by the debounce timer and by the load effect's cleanup,
  // so edits made within the last 300ms aren't lost on unmount/fileId switch.
  const flushPending = () => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }
    if (pendingRef.current == null) return;
    const next = pendingRef.current;
    pendingRef.current = null;
    api.fileTextChanged(fileId, next);
    setInfo((prev) => (prev ? { ...prev, is_dirty: true } : prev));
  };

  useEffect(() => {
    let alive = true;
    api.fileInfo(fileId).then((i) => {
      if (!alive) return;
      setInfo(i);
      if (i && i.content_kind === "text") setText(i.text);
    });
    return () => {
      alive = false;
      flushPending();
    };
  }, [fileId]);

  // Clear the dirty indicator once the backend confirms a save.
  useEffect(() => {
    const unlisten = listen<{ id: string }>("file-saved", (event) => {
      if (event.payload.id !== fileId) return;
      setInfo((prev) => (prev ? { ...prev, is_dirty: false } : prev));
    });
    return () => {
      unlisten.then((u) => u());
    };
  }, [fileId]);

  const onText = (next: string) => {
    setText(next);
    if (!info || info.content_kind !== "text") return;
    pendingRef.current = next;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(flushPending, 300);
  };

  if (!info) {
    return <div className="w-full h-full flex items-center justify-center text-muster-muted ui-fs-base">{t("filePane.loading")}</div>;
  }
  if (info.content_kind === "image") {
    return (
      <div className="w-full h-full overflow-auto p-4">
        <img src={convertFileSrc(info.path)} alt={info.name} />
      </div>
    );
  }
  if (info.content_kind === "unavailable") {
    return <div className="w-full h-full flex items-center justify-center text-muster-muted ui-fs-base">{info.name}</div>;
  }

  return (
    <div className="w-full h-full flex flex-col bg-muster-bg">
      {info.is_dirty && (
        <div className="ui-fs-xs text-amber-400 px-3 py-1 border-b border-amber-400/30">
          {t("filePane.modified", { name: info.name })}
        </div>
      )}
      <div className="flex-1 min-h-0">
        <Editor
          value={text}
          onChange={(v) => onText(v ?? "")}
          language={languageForPath(info.path)}
          theme="muster-dark"
          options={options}
          onMount={(editor, monaco) => {
            editorRef.current = editor;
            editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
              api.saveFile(fileId);
            });
          }}
        />
      </div>
    </div>
  );
}
