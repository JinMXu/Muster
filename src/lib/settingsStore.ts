import { useSyncExternalStore } from "react";
import { api } from "./invoke";
import { applyFullTheme, applyThemeFonts } from "./theme";
import { applyTerminalFont } from "./terminalRegistry";
import { applyMonacoTheme, ensureMonaco } from "./monaco";
import type { Settings as SettingsType } from "./types";

/// Module-level settings store (same useSyncExternalStore pattern as
/// menuStore). Loads the persisted settings once at app start, applies every
/// setting to its consumer (theme, terminal font, UI font), and re-applies on
/// demand after the Settings modal saves. Monaco panes subscribe via
/// `useSettings` and recompute their editor options themselves.
let current: SettingsType | null = null;
const listeners = new Set<() => void>();

const notify = () => listeners.forEach((l) => l());

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
let systemListenerRegistered = false;

/// Push every setting to its consumer. "system" appearance follows the OS via
/// matchMedia; the change listener is registered at most once and re-applies
/// the current settings whenever the OS preference flips.
async function applyAll(s: SettingsType): Promise<void> {
  const dark = s.theme === "dark" || (s.theme === "system" && darkQuery.matches);
  const colors = await api.themeColors(dark ? s.theme_dark : s.theme_light, dark);
  applyFullTheme(colors);
  // Keep native chrome (select popups, scrollbars) on the same light/dark
  // scheme as the theme, or dropdown options render light-on-white.
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
  if (s.theme === "system" && !systemListenerRegistered) {
    systemListenerRegistered = true;
    darkQuery.addEventListener("change", () => {
      if (current) applyAll(current);
    });
  }
  applyTerminalFont({ family: s.font_family, size: s.font_size, thicken: s.font_thicken });
  applyThemeFonts(s.font_family);
  ensureMonaco().then(() => applyMonacoTheme(colors, dark));
  document.documentElement.style.setProperty("--ui-font-scale", String((s.ui_font_size ?? 12) / 12));
}

/// Load settings from the backend, store them, and apply everything.
export async function initSettings(): Promise<void> {
  const s = await api.settings();
  current = s;
  notify();
  await applyAll(s);
}

/// Re-read after the Settings modal saved — same as init.
export function reloadSettings(): Promise<void> {
  return initSettings();
}

/// Persist a setting change made outside the Settings modal (e.g. the diff
/// viewer's side-by-side toggle): patch the cached settings, notify
/// subscribers, and save to disk.
export async function updateSettings(patch: Partial<SettingsType>): Promise<void> {
  if (!current) return;
  current = { ...current, ...patch };
  notify();
  await api.saveSettings(current);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useSettings(): SettingsType | null {
  return useSyncExternalStore(subscribe, () => current);
}
