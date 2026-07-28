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

/// Editor chrome matching the L1 content layer (#1c2128 default). Token
/// colors inherit from vs-dark; only the background is pinned so the editor
/// blends into the pane without a visible seam.
monaco.editor.defineTheme("muster-dark", {
  base: "vs-dark",
  inherit: true,
  rules: [],
  colors: {
    "editor.background": "#1c2128",
  },
});

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
