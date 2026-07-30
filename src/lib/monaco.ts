// Monaco setup for Vite + Tauri: bundle the editor and its workers locally so
// nothing is fetched from a CDN (the app must work fully offline).
import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
// NOTE: monaco-editor's package.json `exports` map rewrites `monaco-editor/x`
// to `esm/vs/x.js`, so worker imports must NOT include the `esm/vs` prefix.
import editorWorker from "monaco-editor/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/language/json/json.worker?worker";
import cssWorker from "monaco-editor/language/css/css.worker?worker";
import htmlWorker from "monaco-editor/language/html/html.worker?worker";
import tsWorker from "monaco-editor/language/typescript/ts.worker?worker";
import type { ThemeColors } from "./types";

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

/// Static fallback theme matching the original L1 content layer. Replaced at
/// runtime by `applyMonacoTheme` once the real theme palette is loaded.
monaco.editor.defineTheme("muster-dark", {
  base: "vs-dark",
  inherit: true,
  rules: [],
  colors: {
    "editor.background": "#1c2128",
  },
});

/// Initial definition of the dynamic theme so an editor that mounts before
/// `applyMonacoTheme` runs doesn't fall back to a missing-theme error. The
/// real palette replaces this on the first `settingsStore.applyAll` call.
monaco.editor.defineTheme("muster-dynamic", {
  base: "vs-dark",
  inherit: true,
  rules: [],
  colors: {
    "editor.background": "#1c2128",
  },
});

/// Define a Monaco editor theme derived from the application's resolved theme
/// palette, then set it as the active theme on every open editor. Called from
/// `settingsStore.applyAll` whenever the theme changes.
///
/// The editor background follows the L1 content layer (theme background
/// darkened 12%), matching the pane it sits in so there is no visible seam.
/// Token colors inherit from `vs-dark` (dark themes) or `vs` (light themes)
/// so syntax highlighting stays readable without re-implementing a full
/// token-color palette per theme.
export function applyMonacoTheme(colors: ThemeColors, dark: boolean) {
  const bg = mixHex(colors.background, "000000", 0.12);
  const editorBg = `#${bg}`;
  monaco.editor.defineTheme("muster-dynamic", {
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
  // Switch every existing editor to the new theme.
  monaco.editor.setTheme("muster-dynamic");
}

/// Mix two 6-digit hex colors (no '#'); `amount` = how far toward `target`.
function mixHex(hex: string, target: string, amount: number): string {
  const ch = (h: string, i: number) => parseInt(h.slice(i, i + 2), 16);
  const mix = (a: number, b: number) =>
    Math.round(a + (b - a) * amount)
      .toString(16)
      .padStart(2, "0");
  return [0, 2, 4].map((i) => mix(ch(hex, i), ch(target, i))).join("");
}

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

/// Theme name used by FilePane / DiffPane — switches from the static fallback
/// to the dynamic theme once `applyMonacoTheme` has run at least once.
export const MONACO_THEME = "muster-dynamic";
