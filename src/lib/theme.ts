// Theme helpers — apply CSS custom properties and font fallback stacks.
import { applyTerminalTheme } from "./terminalRegistry";
import type { ThemeColors } from "./types";

/// Mix two 6-digit hex colors (no '#'); `amount` = how far toward `target`.
function mixHex(hex: string, target: string, amount: number): string {
  const ch = (h: string, i: number) => parseInt(h.slice(i, i + 2), 16);
  const mix = (a: number, b: number) =>
    Math.round(a + (b - a) * amount)
      .toString(16)
      .padStart(2, "0");
  return [0, 2, 4].map((i) => mix(ch(hex, i), ch(target, i))).join("");
}

function applyThemeColors(palette: {
  background: string;
  foreground: string;
  sidebar: string;
  accent: string;
  divider: string;
}) {
  const root = document.documentElement;
  // Layered, borderless scheme: the theme's background becomes L1 (content),
  // the window base (L0) and panel layer are derived from it.
  const l0 = mixHex(palette.background, "ffffff", 0.06);
  const panel = mixHex(l0, "ffffff", 0.03);
  const l1 = mixHex(palette.background, "000000", 0.12);
  root.style.setProperty("--muster-bg", `#${l1}`);
  root.style.setProperty("--muster-bg-float", `#${l0}`);
  root.style.setProperty("--muster-panel", `#${panel}`);
  root.style.setProperty("--muster-fg", `#${palette.foreground}`);
  root.style.setProperty("--muster-sidebar", `#${palette.sidebar}`);
  root.style.setProperty("--muster-accent", `#${palette.accent}`);
  root.style.setProperty("--muster-divider", `#${palette.divider}`);
}

/// Apply a resolved theme everywhere: CSS chrome variables + every parked
/// xterm terminal (and future ones).
export function applyFullTheme(colors: ThemeColors) {
  applyThemeColors(colors);
  applyTerminalTheme(colors);
}

export function applyThemeFonts(family: string) {
  if (!family) return;
  document.documentElement.style.setProperty("--muster-font", family);
}