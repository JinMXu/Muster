// Theme helpers - apply CSS custom properties and font fallback stacks.
import { applyTerminalTheme } from "./terminalRegistry";
import { mixHex } from "./colorUtils";
import type { ThemeColors } from "./types";

function channels(hex: string): [number, number, number] {
  const h = hex.replace(/^#/, "").slice(0, 6);
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

/** Rough chroma: max minus min channel. Zero on grays. */
function chroma(hex: string): number {
  const [r, g, b] = channels(hex);
  return Math.max(r, g, b) - Math.min(r, g, b);
}

function isDarkBg(hex: string): boolean {
  const [r, g, b] = channels(hex);
  return 0.299 * r + 0.587 * g + 0.114 * b < 128;
}

function applyThemeColors(palette: {
  background: string;
  foreground: string;
  cursor: string;
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
  // The accent follows the theme's cursor color when it carries a hue (as
  // kero does), so each theme tints the UI in its own color instead of
  // every theme falling back to the palette's blue.
  const accent =
    chroma(palette.cursor) > 24 ? palette.cursor : palette.accent;
  // A glow of the accent washes the window base from the top right (kero's
  // look), the strongest visual cue that separates themes.
  const [ar, ag, ab] = channels(accent);
  const glowAlpha = isDarkBg(palette.background) ? 0.16 : 0.1;
  root.style.setProperty("--muster-bg", `#${l1}`);
  root.style.setProperty("--muster-bg-float", `#${l0}`);
  root.style.setProperty("--muster-panel", `#${panel}`);
  root.style.setProperty("--muster-fg", `#${palette.foreground}`);
  root.style.setProperty("--muster-sidebar", `#${palette.sidebar}`);
  root.style.setProperty("--muster-accent", `#${accent}`);
  root.style.setProperty("--muster-divider", `#${palette.divider}`);
  root.style.setProperty(
    "--muster-glow",
    `rgba(${ar}, ${ag}, ${ab}, ${glowAlpha})`,
  );
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