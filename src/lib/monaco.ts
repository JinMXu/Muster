// Monaco setup for Vite + Tauri: bundle the editor and its workers locally so
// nothing is fetched from a CDN (the app must work fully offline).
//
// Monaco is lazily loaded: the ~5MB editor package is only imported when a
// FilePane or DiffPane first mounts, not on app startup.

import type { ThemeColors } from "./types";
import { mixHex } from "./colorUtils";

// ── Pure utilities (no monaco import needed) ──────────────────────────

/// Map a file extension to a Monaco language id (defaults to plaintext).
const LANGUAGE_BY_EXT: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  rs: "rust",
  py: "python",
  toml: "ini",
  md: "markdown",
  css: "css",
  html: "html",
  htm: "html",
  yml: "yaml",
  yaml: "yaml",
  xml: "xml",
  ps1: "powershell",
  bat: "bat",
  cmd: "bat",
  sh: "shell",
};

export function languageForPath(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? path;
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  return LANGUAGE_BY_EXT[ext] ?? "plaintext";
}

export const editorOptions = {
  minimap: { enabled: false },
  fontSize: 12.5,
  fontFamily: "'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace",
  automaticLayout: true,
  scrollBeyondLastLine: false,
} as const;

export const MONACO_THEME = "muster-dynamic";

// ── Lazy Monaco loader ────────────────────────────────────────────────

type MonacoNS = typeof import("monaco-editor");
let monacoInstance: MonacoNS | null = null;
let monacoPromise: Promise<MonacoNS> | null = null;

export function ensureMonaco(): Promise<MonacoNS> {
  if (monacoInstance) return Promise.resolve(monacoInstance);
  if (monacoPromise) return monacoPromise;
  monacoPromise = (async () => {
    const monaco = await import("monaco-editor");
    const { loader } = await import("@monaco-editor/react");
    // NOTE: monaco-editor's package.json `exports` map rewrites `monaco-editor/x`
    // to `esm/vs/x.js`, so worker imports must NOT include the `esm/vs` prefix.
    const editorWorker = (await import("monaco-editor/editor/editor.worker?worker")).default;
    const jsonWorker = (await import("monaco-editor/language/json/json.worker?worker")).default;
    const cssWorker = (await import("monaco-editor/language/css/css.worker?worker")).default;
    const htmlWorker = (await import("monaco-editor/language/html/html.worker?worker")).default;
    const tsWorker = (await import("monaco-editor/language/typescript/ts.worker?worker")).default;

    self.MonacoEnvironment = {
      getWorker(_workerId: string, label: string) {
        switch (label) {
          case "json":
            return new jsonWorker();
          case "css":
          case "scss":
          case "less":
            return new cssWorker();
          case "html":
          case "handlebars":
          case "razor":
            return new htmlWorker();
          case "typescript":
          case "javascript":
            return new tsWorker();
          default:
            return new editorWorker();
        }
      },
    };

    loader.config({ monaco });

    monaco.editor.defineTheme("muster-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [],
      colors: { "editor.background": "#1c2128" },
    });

    monaco.editor.defineTheme("muster-dynamic", {
      base: "vs-dark",
      inherit: true,
      rules: [],
      colors: { "editor.background": "#1c2128" },
    });

    monacoInstance = monaco;
    return monaco;
  })();
  return monacoPromise;
}

// ── Theme application (no-op until monaco is loaded) ──────────────────

export function applyMonacoTheme(colors: ThemeColors, dark: boolean) {
  if (!monacoInstance) return;
  const bg = mixHex(colors.background, "000000", 0.12);
  const editorBg = `#${bg}`;
  monacoInstance.editor.defineTheme("muster-dynamic", {
    base: dark ? "vs-dark" : "vs",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": editorBg,
      "editor.foreground": `#${colors.foreground}`,
      "editorLineNumber.foreground": `#${mixHex(colors.foreground, colors.background, 0.4)}`,
      "editorLineNumber.activeForeground": `#${colors.foreground}`,
      "editor.selectionBackground": `#${colors.selection_bg}`,
      "editor.inactiveSelectionBackground": `#${mixHex(colors.selection_bg, colors.background, 0.5)}`,
      "editor.lineHighlightBackground": `#${mixHex(bg, colors.foreground, 0.04)}`,
      "editorCursor.foreground": `#${colors.accent}`,
      "editorIndentGuide.background": `#${mixHex(colors.foreground, colors.background, 0.8)}`,
      "editorIndentGuide.activeBackground": `#${mixHex(colors.foreground, colors.background, 0.6)}`,
      "editorGutter.background": editorBg,
      "editorWidget.background": `#${mixHex(bg, "000000", 0.2)}`,
      "editorWidget.border": `#${colors.divider}`,
      "editorSuggestWidget.background": `#${mixHex(bg, "000000", 0.2)}`,
      "editorSuggestWidget.selectedBackground": `#${mixHex(colors.accent, "000000", 0.7)}`,
      "scrollbarSlider.background": `#${mixHex(colors.foreground, colors.background, 0.8)}40`,
      "scrollbarSlider.hoverBackground": `#${mixHex(colors.foreground, colors.background, 0.7)}60`,
      "scrollbarSlider.activeBackground": `#${mixHex(colors.foreground, colors.background, 0.6)}80`,
    },
  });
  monacoInstance.editor.setTheme("muster-dynamic");
}
