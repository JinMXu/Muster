import { useEffect, useMemo, useRef, useState } from "react";
import { DiffEditor, type MonacoDiffEditor } from "@monaco-editor/react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../lib/invoke";
import { editorOptions, languageForPath, MONACO_THEME, ensureMonaco } from "../lib/monaco";
import { useSettings, updateSettings } from "../lib/settingsStore";
import type { DiffTabInfo } from "../lib/types";
import { useT } from "../lib/i18n/context";

/// Absolute path of the file a diff describes (diff paths are repo-relative).
function diffAbsPath(diff: DiffTabInfo): string {
  if (/^[A-Za-z]:[\\/]/.test(diff.path)) return diff.path;
  const root = diff.repo_root.replace(/[\\/]+$/, "");
  return `${root}\\${diff.path.replace(/\//g, "\\")}`;
}

/// Parent directory of a path, normalized for case-insensitive comparison
/// with the backend's `fs-changed` payload.
function parentDirLower(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const idx = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  const dir = idx > 0 ? trimmed.slice(0, idx) : trimmed;
  return dir.replace(/\//g, "\\").toLowerCase();
}

/// Monaco diff view. Side-by-side by default, with a toolbar toggle for the
/// unified (inline) view and a reload button that re-reads the file from disk.
/// The view-mode preference is persisted via settingsStore.
///
/// Rendered once per visited diff by DiffHosts (see diffViewRegistry), not by
/// the pane slot, so tab/zoom/project switches keep it mounted. `visible` is
/// false while parked (host element detached): change listeners pause and
/// state is kept as-is until the pane is shown again.
export default function DiffPane({ diffId, visible }: { diffId: string; visible: boolean }) {
  const [diff, setDiff] = useState<DiffTabInfo | null>(null);
  const [monacoReady, setMonacoReady] = useState(false);
  const editorRef = useRef<MonacoDiffEditor | null>(null);
  const { t } = useT();
  // Same settings-driven options as the file editor (font size/family/wrap).
  const settings = useSettings();
  const sideBySide = settings?.diff_side_by_side ?? true;
  const options = useMemo(() => ({
    ...editorOptions,
    fontSize: settings?.font_size ?? 13,
    fontFamily: settings?.font_family
      ? `${settings.font_family}, 'JetBrains Mono', Consolas, monospace`
      : editorOptions.fontFamily,
    wordWrap: (settings?.editor_wrap_lines ? "on" : "off") as "on" | "off",
  }), [settings?.font_size, settings?.font_family, settings?.editor_wrap_lines]);

  // Refresh target for the change listeners: the file's directory and
  // whether this diff is pinned to two commits (fixed content, no reload).
  const refreshTargetRef = useRef<{ dir: string; fixed: boolean } | null>(null);
  useEffect(() => {
    refreshTargetRef.current = diff
      ? { dir: parentDirLower(diffAbsPath(diff)), fixed: !!diff.old_rev && !!diff.new_rev }
      : null;
  }, [diff]);

  useEffect(() => {
    if (!visible) return; // parked: keep the fetched diff, do no work
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const load = () =>
      api.diffInfo(diffId).then((d) => {
        if (!alive) return;
        // Skip the state update (and the re-render it causes) when the
        // content is unchanged.
        setDiff((prev) =>
          prev && d && prev.old === d.old && prev.new === d.new &&
          prev.loading === d.loading && prev.error === d.error
            ? prev
            : d
        );
      }).catch(() => {});
    // The backend only recomputes diff content on an explicit reload, so
    // refresh on file events instead of re-transferring both sides on a timer.
    const reload = () => {
      timer = null;
      api
        .reloadDiff(diffId)
        .then(() => { if (alive) load(); })
        .catch(() => {});
    };
    const schedule = () => {
      if (document.hidden || timer) return;
      timer = setTimeout(reload, 700);
    };
    const onFsChanged = (event: { payload: { dir: string } }) => {
      const target = refreshTargetRef.current;
      if (!target || target.fixed) return;
      const dir = event.payload.dir.replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase();
      if (dir === target.dir) schedule();
    };
    const onFileSaved = () => {
      if (refreshTargetRef.current && !refreshTargetRef.current.fixed) schedule();
    };
    load();
    const unlisteners: Promise<UnlistenFn>[] = [
      listen<{ dir: string }>("fs-changed", onFsChanged),
      listen("file-saved", onFileSaved),
    ];
    return () => {
      alive = false;
      if (timer) clearTimeout(timer);
      for (const p of unlisteners) p.then((u) => u()).catch(() => {});
    };
  }, [diffId, visible]);

  // The host element is detached while parked, so Monaco's automaticLayout
  // can miss the 0→real-size transition when it is re-attached; force a
  // layout pass when the pane is shown again.
  useEffect(() => {
    if (visible) editorRef.current?.layout();
  }, [visible]);

  useEffect(() => { ensureMonaco().then(() => setMonacoReady(true)); }, []);

  const reload = async () => {
    await api.reloadDiff(diffId);
    setDiff(await api.diffInfo(diffId));
  };

  if (!diff || !monacoReady) return <div className="text-muster-muted ui-fs-base">{t("diffPane.loading")}</div>;
  if (diff.loading) return <div className="text-muster-muted ui-fs-base anim-pulse">{t("diffPane.computingDiff")}</div>;
  if (diff.error) return <div className="text-red-400 ui-fs-base">{diff.error}</div>;

  return (
    <div className="w-full h-full flex flex-col bg-muster-bg">
      <div className="flex items-center gap-2 border-b border-white/[0.08] px-3 py-1 ui-fs-base text-muster-muted">
        <span className="flex-1 truncate">
          {diff.path}
          {diff.staged ? t("diffPane.stagedSuffix") : ""}
          {diff.old_rev && diff.new_rev ? (
            <span className="text-muster-muted/80 font-mono ui-fs-sm">
              {" "}
              ({diff.old_rev.slice(0, 7)}..{diff.new_rev.slice(0, 7)})
            </span>
          ) : null}
        </span>
        <button
          onClick={() => updateSettings({ diff_side_by_side: !sideBySide })}
          className="px-1.5 py-0.5 rounded hover:bg-muster-hover-btn text-muster-muted hover:text-muster-fg active:scale-[.97] transition-transform duration-muster ease-muster"
          title={t("diffPane.toggleView")}
        >
          {sideBySide ? t("diffPane.split") : t("diffPane.unified")}
        </button>
        <button
          onClick={reload}
          className="px-1.5 py-0.5 rounded hover:bg-muster-hover-btn text-muster-muted hover:text-muster-fg active:scale-[.97] transition-transform duration-muster ease-muster"
          title={t("diffPane.reloadHint")}
        >
          {t("diffPane.reload")}
        </button>
      </div>
      <div className="flex-1 min-h-0">
        <DiffEditor
          original={diff.old}
          modified={diff.new}
          language={languageForPath(diff.path)}
          theme={MONACO_THEME}
          onMount={(editor) => { editorRef.current = editor; }}
          options={{
            ...options,
            readOnly: true,
            renderSideBySide: sideBySide,
          }}
        />
      </div>
    </div>
  );
}
