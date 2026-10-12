// Preview-only mocks for the Tauri bridge. Aliased in vite.preview.config.ts.
export const invoke = (cmd: string, _args?: Record<string, unknown>): Promise<unknown> => {
  const table: Record<string, unknown> = {
    get_settings: {
      theme: "system",
      theme_dark: "Default Dark",
      theme_light: "Default Light",
      font_family: "",
      font_size: 13,
      ui_font_size: 12,
      font_thicken: false,
      editor_wrap_lines: false,
      diff_side_by_side: true,
      project_ports: false,
      language: "zh",
    },
    default_settings: {
      theme: "system",
      theme_dark: "Default Dark",
      theme_light: "Default Light",
      font_family: "",
      font_size: 13,
      ui_font_size: 12,
      font_thicken: false,
      editor_wrap_lines: false,
      diff_side_by_side: true,
      project_ports: false,
      language: "system",
    },
    available_themes_with_info: [
      { name: "Default Dark", is_dark: true, background: "1c2128", accent: "58a6ff" },
      { name: "Dracula", is_dark: true, background: "282a36", accent: "bd93f9" },
      { name: "Tokyo Night", is_dark: true, background: "1a1b26", accent: "7aa2f7" },
      { name: "Gruvbox Dark", is_dark: true, background: "282828", accent: "fabd2f" },
      { name: "Monokai Pro", is_dark: true, background: "2d2a2e", accent: "ffd866" },
      { name: "Default Light", is_dark: false, background: "f6f8fa", accent: "0969da" },
      { name: "GitHub Light", is_dark: false, background: "ffffff", accent: "0969da" },
    ],
    config_path: "C:\\Users\\xujin\\AppData\\Roaming\\muster\\config.toml",
    is_on_path: true,
    save_settings: null,
  };
  return Promise.resolve(table[cmd] ?? null);
};
