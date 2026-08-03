import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Editor from "@monaco-editor/react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/invoke";
import { editorOptions, languageForPath, MONACO_THEME, ensureMonaco } from "../lib/monaco";
import { useSettings } from "../lib/settingsStore";
import { setLatestText, clearLatestText } from "../lib/fileEdits";
import { takePendingReveal } from "../lib/recentFiles";
import { diffLines } from "../lib/diffLines";
import type { BlameLine, FileTabInfo } from "../lib/types";
import { useT } from "../lib/i18n/context";
import FileHistory from "./FileHistory";
import { IconGitBranch } from "./icons";

/// Monaco-based file editor pane. Edits are reported to the backend
/// (debounced) via `file_text_changed`; the dirty dot comes from `is_dirty`.
///
/// Files larger than this byte-count skip the periodic text-sync (avoiding
/// JSON-serialising multi-MB strings on the JS thread). The full text is
/// still sent on explicit save (Ctrl+S) and on tab close.
const MAX_SYNC_BYTES = 100_000;

type MonacoNS = typeof import("monaco-editor");
type EditorInst = Parameters<NonNullable<React.ComponentProps<typeof Editor>["onMount"]>>[0];

/// Strip a repo-root prefix from an absolute path, returning the repo-relative
/// path with forward slashes (case-insensitive — Windows drives are case-less).
function relPath(root: string, path: string): string {
  const r = root.replace(/[\\/]+$/, "");
  const lower = path.toLowerCase();
  const lowerRoot = r.toLowerCase();
  if (lower.startsWith(lowerRoot + "\\") || lower.startsWith(lowerRoot + "/")) {
    return path.slice(r.length + 1).replace(/\\/g, "/");
  }
  return path.replace(/\\/g, "/");
}

export default function FilePane({
  fileId,
  focused,
}: {
  fileId: string;
  focused: boolean;
}) {
  const [info, setInfo] = useState<FileTabInfo | null>(null);
  const [text, setText] = useState("");
  const [monacoReady, setMonacoReady] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<string | null>(null);
  const editorRef = useRef<EditorInst | null>(null);
  const monacoRef = useRef<MonacoNS | null>(null);
  const latestTextRef = useRef("");
  const decorationsRef = useRef<ReturnType<EditorInst["createDecorationsCollection"]> | null>(null);
  const pendingRevealRef = useRef<number | null>(null);
  const { t } = useT();

  // ---- git context (inline diff + blame + history) -----------------------
  const [repoRoot, setRepoRoot] = useState<string | null>(null);
  /// HEAD content of the file; `null` when untracked at HEAD / not loaded.
  const [headText, setHeadText] = useState<string | null>(null);
  const [diffOn, setDiffOn] = useState(false);
  const [blameOn, setBlameOn] = useState(false);
  const [blame, setBlame] = useState<BlameLine[] | null>(null);
  const [cursorLine, setCursorLine] = useState(1);
  const [showHistory, setShowHistory] = useState(false);

  // Editor options follow the saved settings (font size/family/wrap).
  const settings = useSettings();
  const options = useMemo(() => ({
    ...editorOptions,
    fontSize: settings?.font_size ?? 13,
    fontFamily: settings?.font_family
      ? `${settings.font_family}, 'JetBrains Mono', Consolas, monospace`
      : editorOptions.fontFamily,
    wordWrap: (settings?.editor_wrap_lines ? "on" : "off") as "on" | "off",
  }), [settings?.font_size, settings?.font_family, settings?.editor_wrap_lines]);

  useEffect(() => {
    if (focused) editorRef.current?.focus();
  }, [focused]);

  useEffect(() => { ensureMonaco().then(() => setMonacoReady(true)); }, []);

  const flushPending = (forceSync = false) => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }
    if (pendingRef.current == null) return;
    const next = pendingRef.current;
    pendingRef.current = null;
    setLatestText(fileId, next);
    const bytes = new TextEncoder().encode(next).length;
    if (forceSync || bytes <= MAX_SYNC_BYTES) {
      api.fileTextChanged(fileId, next);
    }
    setInfo((prev) => (prev ? { ...prev, is_dirty: true } : prev));
  };

  useEffect(() => {
    let alive = true;
    api.fileInfo(fileId).then((i) => {
      if (!alive) return;
      setInfo(i);
      if (i && i.content_kind === "text") {
        setText(i.text);
        // Open-at-line from a terminal link / search result: the backend
        // event can race this pane's mount, so consume the parked target.
        const line = takePendingReveal(fileId);
        if (line != null) pendingRevealRef.current = line;
      }
    });
    return () => {
      alive = false;
      flushPending(true);
      clearLatestText(fileId);
    };
  }, [fileId]);

  // Resolve the repo root + HEAD content for git features. Only for text
  // files; `info.path` is stable across the dirty-dot re-renders.
  useEffect(() => {
    if (!info || info.content_kind !== "text") {
      setRepoRoot(null);
      setHeadText(null);
      setBlame(null);
      return;
    }
    let alive = true;
    api.resolveProjectRoot(info.path).then((root) => {
      if (!alive) return;
      setRepoRoot(root);
      const rel = relPath(root, info.path);
      api.git.headContent(root, rel).then(
        (head) => alive && setHeadText(head),
        () => alive && setHeadText(null)
      );
    });
    return () => { alive = false; };
  }, [info?.path, info?.content_kind]);

  // Clear the dirty indicator once the backend confirms a save.
  useEffect(() => {
    const unlisten = listen<{ id: string }>("file-saved", (event) => {
      if (event.payload.id !== fileId) return;
      setInfo((prev) => (prev ? { ...prev, is_dirty: false } : prev));
    });
    return () => { unlisten.then((u) => u()); };
  }, [fileId]);

  // Open-file-at-line: scroll + focus the requested line once mounted.
  useEffect(() => {
    const unlisten = listen<{ id: string; line: number }>("file-reveal", (event) => {
      if (event.payload.id !== fileId) return;
      const editor = editorRef.current;
      if (editor) {
        const line = Math.max(1, event.payload.line);
        editor.revealLineInCenter(line);
        editor.setPosition({ lineNumber: line, column: 1 });
        editor.focus();
      } else {
        pendingRevealRef.current = event.payload.line;
      }
    });
    return () => { unlisten.then((u) => u()); };
  }, [fileId]);

  // Inline diff markers: LCS of HEAD content vs the LIVE buffer, so unsaved
  // edits stay accurate while typing.
  const markers = useMemo(() => {
    if (!diffOn || headText === null) return null;
    return diffLines(headText, text);
  }, [diffOn, headText, text]);

  const applyDecorations = useCallback(() => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    if (!editor || !monaco) return;
    if (!decorationsRef.current) decorationsRef.current = editor.createDecorationsCollection();
    const coll = decorationsRef.current;
    const lineCount = editor.getModel()?.getLineCount() ?? 0;
    const decos: Parameters<EditorInst["deltaDecorations"]>[1] = [];
    if (markers) {
      for (const line of markers.added) {
        if (line < 1 || line > lineCount) continue;
        decos.push({
          range: new monaco.Range(line, 1, line, 1),
          options: {
            isWholeLine: true,
            linesDecorationsClassName: "muster-diff-added",
            lineNumberClassName: "muster-diff-added-ln",
          },
        });
      }
      for (const line of markers.removed) {
        if (line < 1 || line > lineCount) continue;
        decos.push({
          range: new monaco.Range(line, 1, line, 1),
          options: {
            isWholeLine: true,
            linesDecorationsClassName: "muster-diff-removed",
            lineNumberClassName: "muster-diff-removed-ln",
          },
        });
      }
    }
    coll.set(decos);
  }, [markers]);

  useEffect(() => { applyDecorations(); }, [applyDecorations]);

  // Blame: reload when toggled on or when the path/root changes.
  const loadBlame = useCallback(() => {
    if (!blameOn || !repoRoot || !info || info.content_kind !== "text") {
      setBlame(null);
      return;
    }
    let alive = true;
    api.git.blame(repoRoot, relPath(repoRoot, info.path)).then(
      (b) => alive && setBlame(b),
      () => alive && setBlame(null)
    );
    return () => { alive = false; };
  }, [blameOn, repoRoot, info?.path, info?.content_kind]);
  useEffect(loadBlame, [loadBlame]);

  const blameEntry = blameOn && blame ? (blame.find((b) => b.line === cursorLine) ?? null) : null;

  const onText = (next: string) => {
    setText(next);
    latestTextRef.current = next;
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
  if (!monacoReady) {
    return <div className="w-full h-full flex items-center justify-center text-muster-muted ui-fs-base">{t("filePane.loading")}</div>;
  }

  const rel = repoRoot ? relPath(repoRoot, info.path) : info.path.replace(/\\/g, "/");
  const canGit = repoRoot !== null;
  const tracked = canGit && headText !== null;

  return (
    <div className="relative w-full h-full flex flex-col bg-muster-bg">
      <div className="flex-shrink-0 flex items-center gap-0.5 px-2 h-8 border-b border-white/[0.08] ui-fs-xs text-muster-muted">
        <button
          onClick={() => setDiffOn(!diffOn)}
          disabled={!tracked}
          title={tracked ? t("filePane.inlineDiffToggle") : t("filePane.inlineDiffUnavailable")}
          className={`px-1.5 h-5 rounded font-medium flex items-center disabled:opacity-30 enabled:hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster ${
            diffOn ? "bg-muster-accent/20 text-muster-accent" : "text-muster-muted"
          }`}
        >
          Δ
        </button>
        <button
          onClick={() => api.openWorkdirDiff(repoRoot!, rel)}
          disabled={!tracked}
          title={t("filePane.diffVsHead")}
          className="px-1.5 h-5 rounded font-medium flex items-center disabled:opacity-30 enabled:hover:bg-muster-hover-btn enabled:hover:text-muster-fg active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          ↔
        </button>
        <button
          onClick={() => setBlameOn(!blameOn)}
          disabled={!tracked}
          title={t("filePane.blameToggle")}
          className={`px-1.5 h-5 rounded font-medium flex items-center disabled:opacity-30 enabled:hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster ${
            blameOn ? "bg-muster-accent/20 text-muster-accent" : "text-muster-muted"
          }`}
        >
          B
        </button>
        <button
          onClick={() => setShowHistory(true)}
          disabled={!canGit}
          title={t("filePane.history")}
          className="px-1 h-5 rounded flex items-center disabled:opacity-30 enabled:hover:bg-muster-hover-btn enabled:hover:text-muster-fg active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          <IconGitBranch size={12} />
        </button>
        <span className="flex-1" />
        {diffOn && tracked && (
          <span className="ui-fs-2xs text-muster-muted/70 truncate">
            {markers && markers.added.length + markers.removed.length > 0
              ? t("filePane.inlineDiffCount", { n: markers.added.length + markers.removed.length })
              : t("filePane.inlineDiffClean")}
          </span>
        )}
      </div>
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
          theme={MONACO_THEME}
          options={options}
          onMount={(editor, monaco) => {
            editorRef.current = editor;
            monacoRef.current = monaco;
            editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
              api.saveFile(fileId, latestTextRef.current);
            });
            editor.onDidChangeCursorPosition((e) => setCursorLine(e.position.lineNumber));
            applyDecorations();
            if (pendingRevealRef.current != null) {
              const line = Math.max(1, pendingRevealRef.current);
              pendingRevealRef.current = null;
              editor.revealLineInCenter(line);
              editor.setPosition({ lineNumber: line, column: 1 });
              editor.focus();
            }
          }}
        />
      </div>
      {blameEntry && (
        <div
          className="flex-shrink-0 flex items-center gap-2 px-3 h-7 border-t border-white/[0.08] ui-fs-xs text-muster-muted cursor-pointer hover:text-muster-fg"
          title={t("filePane.blameOpenHistory")}
          onClick={() => canGit && setShowHistory(true)}
        >
          <span className="font-mono text-muster-accent/80">{blameEntry.short_hash}</span>
          <span className="truncate">{blameEntry.author}</span>
          <span className="truncate">{blameEntry.date}</span>
        </div>
      )}
      {showHistory && canGit && (
        <FileHistory repoRoot={repoRoot!} path={rel} onClose={() => setShowHistory(false)} />
      )}
    </div>
  );
}
