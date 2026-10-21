import { addCollection } from "@iconify/react";
import { useSyncExternalStore } from "react";

/// File-type icons for the file tree, backed by a generated subset of the
/// vscode-icons set (@iconify-json/vscode-icons, MIT) rendered through
/// @iconify/react.
///
/// The subset (only the names referenced below, ~100 KB) is loaded lazily in
/// one dynamic-import chunk; `scripts/build-file-icon-subset.mjs` regenerates
/// it before dev/build. Unknown icons must NEVER hit iconify's API fallback,
/// so rendering gates on `iconLoaded` + a loaded flag and falls back to the
/// app's generic file icon instead.

const PREFIX = "vscode-icons";

let loaded = false;
const waiters = new Set<() => void>();
let loading: Promise<void> | null = null;

function notifyWaiters() {
  loaded = true;
  for (const w of waiters) w();
}

function loadCollection(): Promise<void> {
  if (!loading) {
    loading = import("../assets/vscode-icons-subset.json")
      .then((m) => {
        // Cast via addCollection's own parameter type — the JSON module has
        // no type declaration of its own.
        addCollection(m.default as Parameters<typeof addCollection>[0]);
        notifyWaiters();
      })
      .catch((e) => {
        // Stay on the generic-file fallback; allow a later retry.
        loading = null;
        console.warn("file icons: failed to load vscode-icons collection", e);
      });
  }
  return loading;
}

function subscribeLoaded(cb: () => void) {
  waiters.add(cb);
  if (!loading) loadCollection();
  return () => {
    waiters.delete(cb);
  };
}

/// True once the vscode-icons collection is registered with iconify.
/// Re-renders subscribers when loading finishes (first FileTree mount kicks
/// it off).
function useIconsLoaded(): boolean {
  return useSyncExternalStore(subscribeLoaded, () => loaded);
}

/// Exact file names (lowercased), checked before the extension map: build
/// files, dotfiles and manifests whose meaning the extension alone loses.
const NAME_ICONS: Record<string, string> = {
  dockerfile: "file-type-docker",
  ".dockerignore": "file-type-docker",
  ".env": "file-type-dotenv",
  ".gitignore": "file-type-git",
  ".gitattributes": "file-type-git",
  ".gitmodules": "file-type-git",
  ".gitconfig": "file-type-git",
  ".gitkeep": "file-type-git",
  makefile: "file-type-makefile",
  "package.json": "file-type-node",
  "package-lock.json": "file-type-node",
  "pnpm-lock.yaml": "file-type-node",
  "yarn.lock": "file-type-node",
  "tsconfig.json": "file-type-typescript",
  "jsconfig.json": "file-type-jsconfig",
  "cargo.toml": "file-type-rust",
  "cargo.lock": "file-type-rust",
  "go.mod": "file-type-go",
  "go.sum": "file-type-go",
};

/// Extension (lowercased, no dot) → vscode-icons icon name. Curated to the
/// types that actually appear in dev workspaces; anything else falls back to
/// `default-file`.
const EXT_ICONS: Record<string, string> = {
  // Web / scripting
  js: "file-type-js",
  mjs: "file-type-js",
  cjs: "file-type-js",
  jsx: "file-type-reactjs",
  ts: "file-type-typescript",
  mts: "file-type-typescript",
  cts: "file-type-typescript",
  tsx: "file-type-reactts",
  vue: "file-type-vue",
  svelte: "file-type-svelte",
  html: "file-type-html",
  htm: "file-type-html",
  css: "file-type-css",
  scss: "file-type-scss",
  sass: "file-type-sass",
  less: "file-type-less",
  svg: "file-type-svg",
  // Data / config
  json: "file-type-json",
  jsonc: "file-type-json",
  json5: "file-type-json5",
  yaml: "file-type-yaml",
  yml: "file-type-yaml",
  toml: "file-type-toml",
  ini: "file-type-ini",
  cfg: "file-type-ini",
  conf: "file-type-ini",
  xml: "file-type-xml",
  sql: "file-type-sql",
  // Languages
  py: "file-type-python",
  pyi: "file-type-python",
  rs: "file-type-rust",
  go: "file-type-go",
  c: "file-type-c",
  h: "file-type-cheader",
  cpp: "file-type-cpp",
  cc: "file-type-cpp",
  cxx: "file-type-cpp",
  hpp: "file-type-cppheader",
  hh: "file-type-cppheader",
  cs: "file-type-csharp",
  fs: "file-type-fsharp",
  fsx: "file-type-fsharp",
  vb: "file-type-vb",
  java: "file-type-java",
  kt: "file-type-kotlin",
  scala: "file-type-scala",
  swift: "file-type-swift",
  dart: "file-type-dartlang",
  php: "file-type-php",
  rb: "file-type-ruby",
  lua: "file-type-lua",
  hs: "file-type-haskell",
  ex: "file-type-elixir",
  exs: "file-type-elixir",
  erl: "file-type-erlang",
  clj: "file-type-clojure",
  // Shell / system
  sh: "file-type-shell",
  bash: "file-type-shell",
  zsh: "file-type-shell",
  fish: "file-type-shell",
  ps1: "file-type-powershell",
  psm1: "file-type-powershell",
  bat: "file-type-bat",
  cmd: "file-type-bat",
  // Documents
  md: "file-type-markdown",
  markdown: "file-type-markdown",
  txt: "file-type-text",
  log: "file-type-text",
  pdf: "file-type-pdf",
  // Media
  png: "file-type-image",
  jpg: "file-type-image",
  jpeg: "file-type-image",
  gif: "file-type-image",
  webp: "file-type-image",
  bmp: "file-type-image",
  ico: "file-type-image",
  mp4: "file-type-video",
  mov: "file-type-video",
  mkv: "file-type-video",
  avi: "file-type-video",
  mp3: "file-type-audio",
  wav: "file-type-audio",
  flac: "file-type-audio",
  ogg: "file-type-audio",
  woff: "file-type-font",
  woff2: "file-type-font",
  ttf: "file-type-font",
  otf: "file-type-font",
  // Archives / binaries
  zip: "file-type-zip",
  "7z": "file-type-zip",
  rar: "file-type-zip",
  gz: "file-type-zip",
  tar: "file-type-zip",
  xz: "file-type-zip",
  exe: "file-type-binary",
  dll: "file-type-binary",
  so: "file-type-binary",
  dylib: "file-type-binary",
  bin: "file-type-binary",
  wasm: "file-type-wasm",
};

/// vscode-icons icon name for a file (or `DEFAULT_ICON`). `name` is the file
/// name with extension, exact-match names win over the extension.
export function fileIconName(name: string): string {
  const lower = name.toLowerCase();
  const byName = NAME_ICONS[lower];
  if (byName) return `${PREFIX}:${byName}`;
  const dot = lower.lastIndexOf(".");
  const ext = dot > 0 ? lower.slice(dot + 1) : "";
  const byExt = ext ? EXT_ICONS[ext] : undefined;
  return `${PREFIX}:${byExt ?? "default-file"}`;
}

/// True once the vscode-icons collection is registered with iconify. Subscribing
/// also kicks off the (single) load.
export function useIconsReady(): boolean {
  return useSyncExternalStore(subscribeLoaded, () => loaded);
}
