import { useEffect, useMemo, useState } from "react";
import { DiffEditor } from "@monaco-editor/react";
import { api } from "../lib/invoke";
import { editorOptions, languageForPath, MONACO_THEME, ensureMonaco } from "../lib/monaco";
import { useSettings } from "../lib/settingsStore";
import type { DiffTabInfo } from "../lib/types";
import { useT } from "../lib/i18n/context";

/// Monaco diff view. Side-by-side by default, with a toolbar toggle for the
/// unified (inline) view and a reload button that re-reads the file from disk.
export default function DiffPane({ diffId, focused }: { diffId: string; focused: boolean }) {
  const [diff, setDiff] = useState<DiffTabInfo | null>(null);
  const [sideBySide, setSideBySide] = useState(true);
  const [monacoReady, setMonacoReady] = useState(false);
  const { t } = useT();
  // Same settings-driven options as the file editor (font size/family/wrap).
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
    let alive = true;
    const tick = () => api.diffInfo(diffId).then((d) => alive && setDiff(d));
    tick();
    const interval = setInterval(tick, 1000);
    return () => {
      alive = false;
      clearInterval(interval);
    };
  }, [diffId]);

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
          onClick={() => setSideBySide((v) => !v)}
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
