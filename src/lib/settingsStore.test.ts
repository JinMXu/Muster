import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  settings: vi.fn(),
  themeColors: vi.fn(),
  applyFullTheme: vi.fn(),
  applyThemeFonts: vi.fn(),
  applyTerminalFont: vi.fn(),
  applyMonacoTheme: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(name, handler);
    return () => {
      mocks.listeners.delete(name);
    };
  }),
}));

vi.mock("./invoke", () => ({
  api: {
    settings: mocks.settings,
    themeColors: mocks.themeColors,
    saveSettings: vi.fn(),
  },
}));

vi.mock("./theme", () => ({
  applyFullTheme: mocks.applyFullTheme,
  applyThemeFonts: mocks.applyThemeFonts,
}));

vi.mock("./terminalRegistry", () => ({
  applyTerminalFont: mocks.applyTerminalFont,
}));

vi.mock("./monaco", () => ({
  applyMonacoTheme: mocks.applyMonacoTheme,
}));

function settings(theme: string) {
  return {
    theme,
    theme_dark: "muster-dark",
    theme_light: "muster-light",
    font_family: "JetBrains Mono",
    font_size: 13,
    font_thicken: false,
    ui_font_size: 12,
  };
}

async function loadStore() {
  vi.resetModules();
  return import("./settingsStore");
}

async function flush() {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
}

describe("settingsStore", () => {
  beforeEach(() => {
    mocks.listeners.clear();
    mocks.settings.mockReset();
    mocks.themeColors.mockReset();
    mocks.applyFullTheme.mockReset();
    mocks.applyThemeFonts.mockReset();
    mocks.applyTerminalFont.mockReset();
    mocks.applyMonacoTheme.mockReset();
    mocks.themeColors.mockResolvedValue({
      background: "000000",
      foreground: "ffffff",
      cursor: "ffffff",
      selection_bg: "444444",
      accent: "8888ff",
      divider: "333333",
      palette: Array(16).fill("000000"),
    });
  });

  it("reloads and reapplies settings when another window saves", async () => {
    mocks.settings.mockResolvedValue(settings("dark"));
    const store = await loadStore();
    await store.initSettings();
    await flush();

    expect(mocks.settings).toHaveBeenCalledTimes(1);
    expect(mocks.applyFullTheme).toHaveBeenCalledTimes(1);

    const onChanged = mocks.listeners.get("settings-changed");
    expect(onChanged).toBeDefined();

    mocks.settings.mockResolvedValue(settings("light"));
    onChanged?.({ payload: null });
    await flush();

    expect(mocks.settings).toHaveBeenCalledTimes(2);
    expect(mocks.applyFullTheme).toHaveBeenCalledTimes(2);
  });
});
