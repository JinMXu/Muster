// Curated theme allowlist, shared by generate-theme-catalog.mjs (upstream
// regeneration) and trim-theme-catalog.mjs (offline re-trim). Kept in its
// own module because generate-theme-catalog.mjs runs its download pipeline
// at import time and must not be imported by other scripts.
//
// 10 dark + 10 light, chosen for popularity and visual range. Names must
// match the Ghostty theme files exactly. The hand-written defaults in
// src-tauri/src/theme/catalog.rs (Default Dark/Light) are always available
// regardless of this list.
export const CURATED_THEMES = new Set([
  // dark
  "Dracula",
  "Gruvbox Dark",
  "Monokai Pro",
  "Catppuccin Mocha",
  "TokyoNight Storm",
  "Nord",
  "Kanagawa Wave",
  "Rose Pine",
  "Ayu Mirage",
  "GitHub Dark Default",
  // light
  "Catppuccin Latte",
  "iTerm2 Solarized Light",
  "GitHub Light Default",
  "Gruvbox Light",
  "Rose Pine Dawn",
  "Ayu Light",
  "Everforest Light Med",
  "Flexoki Light",
  "Atom One Light",
  "TokyoNight Day",
]);
