import { useEffect, useMemo, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "../lib/invoke";
import {
  createUpdater,
  type UpdaterController,
  type UpdaterPhase,
} from "../lib/updater";
import type { Settings as SettingsType, ShellOption, ThemeInfo } from "../lib/types";
import { useT, type TKey } from "../lib/i18n/context";
import { mixHex } from "../lib/colorUtils";
import { cn } from "./ui/cn";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { Toggle } from "./ui/Toggle";
import { Segmented } from "./ui/Segmented";
import { Modal } from "./ui/Modal";
import {
  IconArrowUpRight,
  IconCheck,
  IconFolder,
  IconInfo,
  IconKeyboard,
  IconPlug,
  IconSearch,
  IconSettings,
  IconTerminal,
  IconType,
  IconX,
} from "./icons";

export type SettingsTab =
  | "appearance"
  | "terminal"
  | "editor"
  | "shortcuts"
  | "integrations"
  | "about";

interface ShortcutItem {
  keys: string;
  labelKey: TKey;
}

const SHORTCUT_GROUPS: { titleKey: TKey; items: ShortcutItem[] }[] = [
  {
    titleKey: "shortcuts.groupWindow",
    items: [
      { keys: "Ctrl+N", labelKey: "shortcuts.newProject" },
      { keys: "Ctrl+Shift+N", labelKey: "shortcuts.newWindow" },
      { keys: "Ctrl+1 ~ Ctrl+9", labelKey: "shortcuts.switchProject" },
      { keys: "Ctrl+Alt+[ / ]", labelKey: "shortcuts.prevNextProject" },
      { keys: "Ctrl+,", labelKey: "shortcuts.openSettings" },
      { keys: "Ctrl+/", labelKey: "shortcuts.showShortcuts" },
    ],
  },
  {
    titleKey: "shortcuts.groupTabs",
    items: [
      { keys: "Ctrl+T", labelKey: "shortcuts.newSession" },
      { keys: "Ctrl+W", labelKey: "shortcuts.closeTab" },
      { keys: "Ctrl+Shift+W", labelKey: "shortcuts.closePane" },
      { keys: "Ctrl+Shift+T", labelKey: "shortcuts.reopenClosedTab" },
      { keys: "Ctrl+Tab", labelKey: "shortcuts.tabSwitcher" },
      { keys: "Ctrl+Shift+[ / ]", labelKey: "shortcuts.prevNextTab" },
      { keys: "Ctrl+D", labelKey: "shortcuts.splitRight" },
      { keys: "Ctrl+Shift+D", labelKey: "shortcuts.splitDown" },
      { keys: "Ctrl+[ / ]", labelKey: "shortcuts.prevNextPane" },
      { keys: "Ctrl+Alt+\u2190->\u2191\u2193", labelKey: "shortcuts.focusPane" },
      { keys: "Ctrl+Alt+Shift+\u2190->\u2191\u2193", labelKey: "shortcuts.resizePane" },
      { keys: "Ctrl+Shift+Enter", labelKey: "shortcuts.toggleZoom" },
    ],
  },
  {
    titleKey: "shortcuts.groupPanels",
    items: [
      { keys: "Ctrl+P", labelKey: "shortcuts.commandPalette" },
      { keys: "Ctrl+B", labelKey: "shortcuts.toggleSidebar" },
      { keys: "Ctrl+Shift+B", labelKey: "shortcuts.toggleRightPanel" },
      { keys: "Ctrl+Shift+E", labelKey: "shortcuts.filesPanel" },
      { keys: "Ctrl+Shift+F", labelKey: "shortcuts.searchPanel" },
      { keys: "Ctrl+F", labelKey: "shortcuts.terminalSearch" },
      { keys: "Ctrl+Shift+A", labelKey: "shortcuts.agentBar" },
      { keys: "Ctrl+Shift+U", labelKey: "shortcuts.usagePanel" },
      { keys: "Ctrl+Shift+G", labelKey: "shortcuts.gitPanel" },
      { keys: "Ctrl+Shift+I", labelKey: "shortcuts.infoPanel" },
      { keys: "Ctrl+S", labelKey: "shortcuts.saveFile" },
      { keys: "Ctrl+K", labelKey: "shortcuts.clearTerminal" },
    ],
  },
];

const GITHUB_URL = "https://github.com/JinMXu/Muster";
const ISSUES_URL = `${GITHUB_URL}/issues`;
const RELEASES_URL = `${GITHUB_URL}/releases`;
const LICENSE_URL = `${GITHUB_URL}/blob/main/LICENSE`;

const TABS: { id: SettingsTab; labelKey: TKey; icon: React.ReactNode }[] = [
  { id: "appearance", labelKey: "settings.tabAppearance", icon: <IconSettings size={15} /> },
  { id: "terminal", labelKey: "settings.tabTerminal", icon: <IconTerminal size={15} /> },
  { id: "editor", labelKey: "settings.tabEditor", icon: <IconType size={15} /> },
  { id: "shortcuts", labelKey: "settings.tabShortcuts", icon: <IconKeyboard size={15} /> },
  { id: "integrations", labelKey: "settings.tabIntegrations", icon: <IconPlug size={15} /> },
  { id: "about", labelKey: "settings.tabAbout", icon: <IconInfo size={15} /> },
];

/// OS/arch for the About tab. WebView2 exposes the Chromium User-Agent
/// Client Hints API; fall back to parsing the UA string for anything else.
async function osInfo(): Promise<{ os: string; arch: string }> {
  try {
    const uad = (
      navigator as Navigator & {
        userAgentData?: {
          getHighEntropyValues: (hints: string[]) => Promise<Record<string, string>>;
        };
      }
    ).userAgentData;
    if (uad?.getHighEntropyValues) {
      const v = await uad.getHighEntropyValues([
        "platform",
        "platformVersion",
        "architecture",
        "bitness",
      ]);
      const os = [v.platform ?? "Windows", v.platformVersion ? `v${v.platformVersion}` : null]
        .filter(Boolean)
        .join(" ");
      const arch = [v.architecture ?? "", v.bitness ? `${v.bitness}-bit` : ""]
        .filter(Boolean)
        .join(" ");
      if (os || arch) return { os: os || "—", arch: arch || "—" };
    }
  } catch {
    // fall through to UA sniffing
  }
  const nt = navigator.userAgent.match(/Windows NT ([\d.]+)/);
  return { os: nt ? `Windows NT ${nt[1]}` : navigator.platform, arch: "x64" };
}

export default function Settings({
  onClose,
  initialTab = "appearance",
}: {
  onClose: () => void;
  initialTab?: SettingsTab;
}) {
  // Settings apply live: every control change persists immediately (and
  // re-applies across windows via the settings-changed event). `draft` is
  // simply the current settings; there is no Save step.
  const [draft, setDraft] = useState<SettingsType | null>(null);
  const [themes, setThemes] = useState<ThemeInfo[]>([]);
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [version, setVersion] = useState("");
  // Updater lives here (not in AboutTab) so its phase survives tab switches.
  const [phase, setPhase] = useState<UpdaterPhase>({ kind: "idle" });
  const ctrlRef = useRef<UpdaterController | null>(null);
  // Debounced persistence (see update() below): the last draft state and its
  // timer, flushed on dialog close.
  const pendingRef = useRef<SettingsType | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  const { t } = useT();

  useEffect(() => {
    api.settings().then(setDraft);
    api.availableThemesWithInfo().then(setThemes);
    getVersion().then(setVersion).catch(() => {});
    ctrlRef.current = createUpdater(setPhase);
    return () => {
      // Close (unmount) flushes a pending debounced write.
      if (saveTimerRef.current !== null) window.clearTimeout(saveTimerRef.current);
      const p = pendingRef.current;
      pendingRef.current = null;
      if (p) api.saveSettings(p);
    };
  }, []);

  if (!draft) return null;

  // Persisting runs through a short debounce: sliders fire onChange
  // continuously while dragging and text inputs per keystroke, and each
  // save rewrites the TOML, re-themes every window and rebuilds the tray.
  // The pending write flushes when the dialog closes.
  const update = (patch: Partial<SettingsType>) => {
    if (!draft) return;
    const next = { ...draft, ...patch };
    setDraft(next);
    pendingRef.current = next;
    if (saveTimerRef.current !== null) window.clearTimeout(saveTimerRef.current);
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      const p = pendingRef.current;
      pendingRef.current = null;
      if (p) api.saveSettings(p);
    }, 250);
  };
  const reset = () => api.defaultSettings().then((s) => {
    if (saveTimerRef.current !== null) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    pendingRef.current = null;
    setDraft(s);
    api.saveSettings(s);
  });
  const revealConfig = () => {
    api.configPath().then((p) => revealItemInDir(p)).catch(() => {});
  };

  return (
    <Modal
      onClose={onClose}
      z="z-40"
      labelledBy="settings-title"
      /* Fixed size on purpose: the dialog must not resize when switching
         tabs — only the content area scrolls. */
      className="flex flex-col overflow-hidden w-[800px] h-[min(620px,calc(100vh-48px))]"
    >
      {/* Header */}
      <div className="flex items-center justify-between pl-5 pr-3 py-3 border-b border-muster-border flex-shrink-0">
        <h2 id="settings-title" className="ui-fs-base font-semibold">
          {t("settings.title")}
        </h2>
        <IconButton size="md" onClick={onClose} aria-label={t("common.close")}>
          <IconX size={16} />
        </IconButton>
      </div>

      {/* Body: sidebar + content */}
      <div className="flex flex-1 min-h-0">
        <nav className="w-[192px] flex-shrink-0 py-3 px-2.5 border-r border-muster-border flex flex-col">
          <div className="flex flex-col gap-0.5">
            {TABS.map((item) => (
              <TabButton
                key={item.id}
                active={tab === item.id}
                onClick={() => setTab(item.id)}
                icon={item.icon}
                label={t(item.labelKey)}
              />
            ))}
          </div>
          <div className="mt-auto flex flex-col gap-1">
            <button
              onClick={revealConfig}
              className="flex items-center gap-2 px-3 py-1.5 rounded-md ui-fs-xs text-muster-muted hover:text-muster-fg hover:bg-muster-hover transition-colors duration-muster ease-muster outline-none"
            >
              <span className="flex items-center">
                <IconFolder size={13} />
              </span>
              {t("settings.openConfigFile")}
            </button>
            <div className="px-3 pb-1 ui-fs-2xs text-muster-muted tabular-nums select-none">
              Muster{version ? ` · v${version}` : ""}
            </div>
          </div>
        </nav>

        {/* Content */}
        <div
          key={tab}
          className="flex-1 min-w-0 overflow-y-auto overflow-x-hidden px-6 py-5 muster-fade"
        >
          <h3 className="ui-fs-md font-semibold mb-4">
            {t(TABS.find((it) => it.id === tab)?.labelKey ?? "settings.title")}
          </h3>
          {tab === "appearance" && (
            <AppearanceTab s={draft} update={update} themes={themes} t={t} />
          )}
          {tab === "terminal" && <TerminalTab s={draft} update={update} t={t} />}
          {tab === "editor" && <EditorTab s={draft} update={update} t={t} />}
          {tab === "shortcuts" && <ShortcutsTab t={t} />}
          {tab === "integrations" && <IntegrationsTab t={t} />}
          {tab === "about" && (
            <AboutTab t={t} version={version} phase={phase} ctrl={ctrlRef.current} />
          )}
        </div>
      </div>

      {/* Footer: settings apply live, so only the defaults escape hatch remains */}
      <div className="flex items-center justify-end px-5 py-3 border-t border-muster-border flex-shrink-0">
        <Button size="md" onClick={reset}>
          {t("settings.reset")}
        </Button>
      </div>
    </Modal>
  );
}

/* ---------- shared building blocks ---------- */

function TabButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`relative flex items-center gap-2.5 px-3 py-2 rounded-md ui-fs-sm transition-colors duration-muster ease-muster ${
        active
          ? "bg-muster-selected text-muster-fg"
          : "text-muster-muted hover:bg-muster-hover hover:text-muster-fg"
      }`}
    >
      {active && (
        <span className="absolute left-0 top-1/2 -translate-y-1/2 w-[2.5px] h-4 rounded-full bg-muster-accent" />
      )}
      <span className="flex items-center">{icon}</span>
      <span>{label}</span>
    </button>
  );
}

/** Group card: all rows inside share one bordered container. The title is
 * optional — tabs whose page title already says it all go without. */
function Section({
  title,
  children,
}: {
  title?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-5 last:mb-0">
      {title && (
        <div className="ui-fs-xs text-muster-muted uppercase tracking-wide mb-2 px-0.5">
          {title}
        </div>
      )}
      {/* No overflow-hidden here: dropdown popovers (theme picker) must be
          able to paint outside the card boundary. */}
      <div className="bg-muster-card border border-muster-border rounded-lg divide-y divide-muster-border">
        {children}
      </div>
    </div>
  );
}

/** Standard settings row: title + optional description on the left, control
 * on the right. Keep all controls visually aligned via this component. */
function Row({
  title,
  desc,
  children,
}: {
  title: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-3.5 py-3 min-h-[56px]">
      <div className="min-w-0">
        <div className="ui-fs-base text-muster-fg">{title}</div>
        {desc && (
          <div className="ui-fs-xs text-muster-muted mt-1 leading-relaxed [word-break:break-word]">
            {desc}
          </div>
        )}
      </div>
      <div className="flex items-center flex-shrink-0">{children}</div>
    </div>
  );
}

/** One row of theme tiles (dark or light), previewed the way kero draws
 * them: a mini terminal window on the theme's wash — "Aa" in the theme's
 * foreground, a selection chip, a cursor block and its first eight ANSI
 * colors — so the grid reads at a glance like the terminal will. */
function ThemeGrid({
  themes,
  dark,
  value,
  onChange,
  t,
}: {
  themes: ThemeInfo[];
  dark: boolean;
  value: string;
  onChange: (name: string) => void;
  t: ReturnType<typeof useT>["t"];
}) {
  const list = themes.filter((th) => th.is_dark === dark);
  return (
    <div className="px-3.5 py-3">
      <div className="ui-fs-xs text-muster-muted mb-2">
        {dark ? t("settings.darkTheme") : t("settings.lightTheme")}
      </div>
      <div className="grid grid-cols-5 gap-1.5">
        {list.map((th) => {
          const selected = th.name === value;
          const [bg, fg, cursor, selBg, ...ansi] = th.preview;
          // The wash behind the mini window: one step away from the theme's
          // own background, with a glow of the cursor color from the corner.
          const wash = dark ? mixHex(th.background, "000000", 0.14) : mixHex(th.background, "ffffff", 0.1);
          const [cr, cg, cb] = [0, 2, 4].map((i) =>
            parseInt(cursor.replace(/^#/, "").slice(i, i + 2), 16),
          );
          return (
            <button
              key={th.name}
              onClick={() => onChange(th.name)}
              title={th.name}
              className={cn(
                "relative rounded-lg p-1.5 outline-none transition-colors duration-muster ease-muster",
                selected
                  ? "ring-1 ring-muster-accent"
                  : "ring-1 ring-transparent hover:ring-muster-border-strong",
              )}
              style={{
                background: `radial-gradient(120% 90% at 100% 0%, rgba(${cr}, ${cg}, ${cb}, 0.3) 0%, transparent 60%), #${wash}`,
              }}
            >
              {/* The mini terminal window */}
              <span
                className="relative flex h-[58px] flex-col justify-between rounded-md px-1.5 py-1.5"
                style={{ background: `#${bg}` }}
              >
                <span className="flex items-center gap-1">
                  <span
                    className="text-[12px] leading-none font-semibold"
                    style={{ color: `#${fg}` }}
                  >
                    Aa
                  </span>
                  <span className="flex-1" />
                  <span
                    className="rounded-[3px] px-1 text-[8px] leading-[12px] font-mono"
                    style={{ background: `#${selBg}`, color: `#${fg}` }}
                  >
                    ~/
                  </span>
                  <span
                    className="h-[10px] w-[3px] rounded-[1px]"
                    style={{ background: `#${cursor}` }}
                  />
                </span>
                <span className="flex gap-[3px]">
                  {ansi.map((c, i) => (
                    <span
                      key={i}
                      className="h-[7px] w-[7px] rounded-full"
                      style={{ background: `#${c}` }}
                    />
                  ))}
                </span>
                {selected && (
                  <span className="absolute -top-0.5 -right-0.5 w-3.5 h-3.5 rounded-full bg-muster-accent text-white flex items-center justify-center">
                    <IconCheck size={9} />
                  </span>
                )}
              </span>
              <span
                className={cn(
                  "block pt-1 pb-0.5 text-center ui-fs-2xs truncate",
                  selected ? "text-muster-fg font-medium" : "text-muster-muted",
                )}
              >
                {th.name}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/// Common monospace families offered in the font dropdown (bundled default
/// first, then Windows built-ins and popular coding/CJK fonts). The input
/// stays free-typable for anything not on the list.
const MONO_FONT_PRESETS = [
  "JetBrains Mono",
  "Cascadia Code",
  "Cascadia Mono",
  "Consolas",
  "Courier New",
  "Sarasa Mono SC",
  "Maple Mono",
  "LXGW WenKai Mono",
  "Fira Code",
  "Source Code Pro",
  "IBM Plex Mono",
  "DejaVu Sans Mono",
];

/// Numeric setting as a hand-filled input. Typing stays in local text state
/// (so "1" on the way to "14" isn't clamped mid-keystroke) and commits —
/// rounded and clamped to [min, max] — on blur or Enter.
function NumberRow({
  title,
  value,
  min,
  max,
  suffix,
  onChange,
}: {
  title: string;
  value: number;
  min: number;
  max: number;
  suffix: string;
  onChange: (v: number) => void;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const v = Math.round(Number(text));
    if (Number.isFinite(v)) onChange(Math.min(max, Math.max(min, v)));
    else setText(String(value));
  };
  return (
    <Row title={title}>
      <div className="flex items-center gap-1.5">
        <input
          type="number"
          min={min}
          max={max}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && commit()}
          className="w-[72px] bg-muster-hover px-2.5 py-1.5 rounded-md ui-fs-sm outline-none border border-transparent focus:border-muster-accent/30 transition-colors tabular-nums"
        />
        <span className="ui-fs-sm text-muster-muted">{suffix}</span>
      </div>
    </Row>
  );
}

function SliderRow({
  title,
  value,
  min,
  max,
  step = 1,
  suffix,
  onChange,
}: {
  title: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  suffix: string;
  onChange: (v: number) => void;
}) {
  return (
    <Row title={title}>
      <div className="flex items-center gap-3 w-[220px]">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          className="flex-1"
        />
        <span className="ui-fs-sm w-12 text-right tabular-nums text-muster-muted">
          {value}
          {suffix}
        </span>
      </div>
    </Row>
  );
}

/* ---------- tabs ---------- */

function AppearanceTab({
  s,
  update,
  themes,
  t,
}: {
  s: SettingsType;
  update: (patch: Partial<SettingsType>) => void;
  themes: ThemeInfo[];
  t: ReturnType<typeof useT>["t"];
}) {
  return (
    <div>
      <Section>
        <Row title={t("settings.appearanceMode")}>
          <Segmented
            value={s.theme}
            options={[
              { value: "system", label: t("settings.themeSystem") },
              { value: "light", label: t("settings.themeLight") },
              { value: "dark", label: t("settings.themeDark") },
            ]}
            onChange={(v) => update({ theme: v })}
          />
        </Row>
        <ThemeGrid
          themes={themes}
          dark
          value={s.theme_dark}
          onChange={(name) => update({ theme_dark: name })}
          t={t}
        />
        <ThemeGrid
          themes={themes}
          dark={false}
          value={s.theme_light}
          onChange={(name) => update({ theme_light: name })}
          t={t}
        />
        <Row title={t("settings.language")}>
          <select
            value={s.language}
            onChange={(e) =>
              update({ language: e.target.value as SettingsType["language"] })
            }
            className="w-[190px] bg-muster-hover px-2.5 py-1.5 rounded-md ui-fs-sm outline-none border border-transparent focus:border-muster-accent/30 transition-colors"
          >
            <option value="system">{t("settings.languageSystem")}</option>
            <option value="en">{t("settings.languageEn")}</option>
            <option value="zh">{t("settings.languageZh")}</option>
          </select>
        </Row>
      </Section>
    </div>
  );
}

function TerminalTab({
  s,
  update,
  t,
}: {
  s: SettingsType;
  update: (patch: Partial<SettingsType>) => void;
  t: ReturnType<typeof useT>["t"];
}) {
  // Available shells for the default-shell picker, resolved live on open
  // (a shell installed after Muster started shows up on the next open).
  const [shells, setShells] = useState<ShellOption[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    api.listShells().then((list) => {
      if (!cancelled) setShells(list);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return (
    <div>
      <Section>
        <Row title={t("settings.defaultShell")} desc={t("settings.defaultShellDesc")}>
          <select
            value={s.default_shell}
            onChange={(e) => update({ default_shell: e.target.value })}
            title={shells?.find((sh) => sh.id === s.default_shell)?.path || undefined}
            className="w-[190px] bg-muster-hover px-2.5 py-1.5 rounded-md ui-fs-sm outline-none border border-transparent focus:border-muster-accent/30 transition-colors"
          >
            <option value="auto">{t("settings.shellAuto")}</option>
            {(shells ?? [])
              .filter((sh) => sh.id !== "auto" && (sh.available || sh.id === s.default_shell))
              .map((sh) => (
                <option key={sh.id} value={sh.id}>
                  {sh.available ? sh.name : `${sh.name} (${t("settings.shellNotInstalled")})`}
                </option>
              ))}
          </select>
        </Row>
        <Row title={t("settings.rightClick")} desc={t("settings.rightClickDesc")}>
          <Segmented
            value={s.terminal_right_click}
            options={[
              { value: "menu", label: t("settings.rightClickMenu") },
              { value: "smart", label: t("settings.rightClickSmart") },
            ]}
            onChange={(v) => update({ terminal_right_click: v })}
          />
        </Row>
        <Row title={t("settings.projectPorts")}>
          <Toggle
            checked={s.project_ports}
            onChange={(v) => update({ project_ports: v })}
          />
        </Row>
      </Section>
    </div>
  );
}

function EditorTab({
  s,
  update,
  t,
}: {
  s: SettingsType;
  update: (patch: Partial<SettingsType>) => void;
  t: ReturnType<typeof useT>["t"];
}) {
  return (
    <div>
      {/* Live preview: reacts to family/size/thicken as they are edited. */}
      <div className="mb-5 rounded-lg border border-muster-border bg-muster-well px-4 py-3">
        <div className="ui-fs-xs text-muster-muted mb-1.5">
          {t("settings.fontPreview")}
        </div>
        <div
          className="font-mono whitespace-nowrap overflow-hidden text-ellipsis"
          style={{
            fontFamily: s.font_family ? `'${s.font_family}', monospace` : undefined,
            fontSize: s.font_size,
            fontWeight: s.font_thicken ? 600 : 400,
          }}
        >
          ABCabc 0123456789 -&gt; =&gt; != {"{()[]}"} $VAR ~/src #
        </div>
      </div>

      <Section title={t("settings.terminalFont")}>
        <Row title={t("settings.fontFamily")}>
          {/* Datalist dropdown: common monospace presets on click, but a
              family outside the list can still be typed by hand. */}
          <input
            list="muster-mono-fonts"
            value={s.font_family}
            onChange={(e) => update({ font_family: e.target.value })}
            className="w-[220px] bg-muster-hover px-2.5 py-1.5 rounded-md ui-fs-sm outline-none border border-transparent focus:border-muster-accent/30 transition-colors"
            placeholder={t("settings.fontFamilyPlaceholder")}
          />
          <datalist id="muster-mono-fonts">
            {MONO_FONT_PRESETS.map((f) => (
              <option key={f} value={f} />
            ))}
          </datalist>
        </Row>
        <NumberRow
          title={t("settings.fontSize")}
          value={s.font_size}
          min={8}
          max={32}
          suffix="px"
          onChange={(v) => update({ font_size: v })}
        />
        <SliderRow
          title={t("settings.uiFontSize")}
          value={s.ui_font_size}
          min={10}
          max={16}
          step={0.5}
          suffix="px"
          onChange={(v) => update({ ui_font_size: v })}
        />
        <Row title={t("settings.thickenFont")}>
          <Toggle
            checked={s.font_thicken}
            onChange={(v) => update({ font_thicken: v })}
          />
        </Row>
      </Section>

      <Section title={t("settings.editor")}>
        <Row title={t("settings.wrapLines")}>
          <Toggle
            checked={s.editor_wrap_lines}
            onChange={(v) => update({ editor_wrap_lines: v })}
          />
        </Row>
        <Row title={t("settings.diffSideBySide")}>
          <Toggle
            checked={s.diff_side_by_side}
            onChange={(v) => update({ diff_side_by_side: v })}
          />
        </Row>
      </Section>
    </div>
  );
}

function ShortcutsTab({ t }: { t: ReturnType<typeof useT>["t"] }) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();

  const groups = useMemo(
    () =>
      q
        ? SHORTCUT_GROUPS.map((g) => ({
            ...g,
            items: g.items.filter(
              (it) =>
                t(it.labelKey).toLowerCase().includes(q) ||
                it.keys.toLowerCase().includes(q),
            ),
          })).filter((g) => g.items.length > 0)
        : SHORTCUT_GROUPS,
    [q, t],
  );

  return (
    <div>
      <div className="relative mb-4">
        <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muster-muted flex items-center">
          <IconSearch size={13} />
        </span>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("settings.searchShortcuts")}
          className="w-full bg-muster-hover border border-transparent focus:border-muster-accent/30 pl-8 pr-3 py-1.5 rounded-md ui-fs-sm outline-none transition-colors"
        />
      </div>

      {groups.map((group) => (
        <Section key={group.titleKey} title={t(group.titleKey)}>
          {group.items.map((item) => (
            <div
              key={item.keys}
              className="flex items-center justify-between gap-4 px-3.5 py-2"
            >
              <span className="ui-fs-sm text-muster-fg/80">
                {t(item.labelKey)}
              </span>
              <kbd className="bg-muster-fill rounded px-1.5 py-0.5 ui-fs-xs font-mono text-muster-muted whitespace-nowrap">
                {item.keys}
              </kbd>
            </div>
          ))}
        </Section>
      ))}

      {groups.length === 0 && (
        <div className="py-12 text-center ui-fs-sm text-muster-muted">
          {t("settings.noShortcutsFound")}
        </div>
      )}
    </div>
  );
}

function IntegrationsTab({ t }: { t: ReturnType<typeof useT>["t"] }) {
  const [explorerResult, setExplorerResult] = useState<
    { ok: boolean; text: string } | null
  >(null);
  const [pathResult, setPathResult] = useState<
    { ok: boolean; text: string } | null
  >(null);
  const [onPath, setOnPath] = useState<boolean | null>(null);

  useEffect(() => {
    api.isOnPath().then(setOnPath).catch(() => setOnPath(false));
  }, []);

  const installExplorer = () => {
    setExplorerResult(null);
    api
      .installExplorerContextMenu()
      .then(() =>
        setExplorerResult({ ok: true, text: t("settings.integrationsInstalled") })
      )
      .catch((e) => setExplorerResult({ ok: false, text: String(e) }));
  };

  const togglePath = () => {
    setPathResult(null);
    const action = onPath ? api.removeFromPath() : api.addToPath();
    action
      .then(() => {
        setOnPath(!onPath);
        setPathResult({ ok: true, text: t("settings.integrationsInstalled") });
      })
      .catch((e) => setPathResult({ ok: false, text: String(e) }));
  };

  return (
    <div>
      <Section title={t("settings.systemIntegration")}>
        <Row title={t("settings.explorerMenu")} desc={t("settings.explorerMenuDesc")}>
          <ActionStatus
            label={t("settings.install")}
            result={explorerResult}
            onClick={installExplorer}
            t={t}
          />
        </Row>
        <Row title={t("settings.pathIntegration")} desc={t("settings.pathDesc")}>
          <ActionStatus
            label={onPath ? t("settings.removeFromPath") : t("settings.addToPath")}
            busy={onPath === null}
            result={pathResult}
            onClick={togglePath}
            t={t}
          />
        </Row>
      </Section>
    </div>
  );
}

function ActionStatus({
  label,
  busy = false,
  result,
  onClick,
  t,
}: {
  label: string;
  busy?: boolean;
  result: { ok: boolean; text: string } | null;
  onClick: () => void;
  t: ReturnType<typeof useT>["t"];
}) {
  return (
    <div className="flex items-center gap-2">
      {result && (
        <span
          className={`ui-fs-xs max-w-[140px] truncate ${
            result.ok ? "text-green-400" : "text-red-400"
          }`}
          title={result.text}
        >
          {result.ok ? t("settings.integrationsInstalled") : result.text}
        </span>
      )}
      <Button size="md" onClick={onClick} disabled={busy}>
        {label}
      </Button>
    </div>
  );
}

function LinkRow({
  title,
  desc,
  url,
  t,
}: {
  title: string;
  desc: string;
  url: string;
  t: ReturnType<typeof useT>["t"];
}) {
  return (
    <Row title={title} desc={desc}>
      <Button size="md" onClick={() => openUrl(url)} className="gap-1">
        {t("settings.openOnGithub")}
        <IconArrowUpRight size={12} />
      </Button>
    </Row>
  );
}

function AboutTab({
  t,
  version,
  phase,
  ctrl,
}: {
  t: ReturnType<typeof useT>["t"];
  version: string;
  phase: UpdaterPhase;
  ctrl: UpdaterController | null;
}) {
  const [os, setOs] = useState("");
  const [arch, setArch] = useState("");
  const [configFile, setConfigFile] = useState("");

  useEffect(() => {
    osInfo().then((i) => {
      setOs(i.os);
      setArch(i.arch);
    });
    api.configPath().then(setConfigFile).catch(() => {});
  }, []);

  const busy = phase.kind === "checking" || phase.kind === "downloading";

  // Status line + action button per updater phase.
  let statusText: string | null = null;
  let statusError = false;
  let button: { label: string; onClick: () => void; primary?: boolean } | null =
    null;
  switch (phase.kind) {
    case "idle":
      break;
    case "checking":
      statusText = t("settings.checkingForUpdates");
      break;
    case "none":
      statusText = t("settings.updateNone");
      break;
    case "available":
      statusText = `${t("settings.updateAvailable")} · v${phase.version}`;
      button = { label: t("settings.downloadUpdate"), onClick: () => ctrl?.downloadAndInstall(), primary: true };
      break;
    case "downloading":
      statusText = t("settings.updateDownloading");
      break;
    case "ready":
      statusText = t("settings.updateReady");
      button = { label: t("settings.restartToUpdate"), onClick: () => ctrl?.restartToUpdate(), primary: true };
      break;
    case "error":
      statusText = phase.stage === "check" ? t("settings.updateError") : t("settings.updateInstallError");
      statusError = true;
      break;
  }

  return (
    <div>
      {/* App identity */}
      <div className="flex flex-col items-center gap-1.5 pt-3 pb-6">
        <img src="app-icon.png" alt="Muster" className="w-16 h-16 rounded-[14px] shadow-pop-sm" />
        <div className="ui-fs-md font-semibold mt-1">Muster</div>
        <div className="ui-fs-xs text-muster-muted">{t("settings.tagline")}</div>
        <div className="ui-fs-xs text-muster-muted/80 tabular-nums mt-0.5">
          v{version || "…"}
        </div>
      </div>

      <Section title={t("settings.updates")}>
        <Row title={t("settings.updates")} desc={t("settings.updatesDesc")}>
          <div className="flex items-center gap-2.5">
            {phase.kind === "downloading" ? (
              <div className="w-[120px] h-1.5 rounded-full bg-muster-hover-btn overflow-hidden">
                <div
                  className="h-full rounded-full bg-muster-accent transition-all duration-muster ease-muster"
                  style={{ width: `${phase.progress}%` }}
                />
              </div>
            ) : statusText ? (
              <span
                className={`ui-fs-xs max-w-[200px] truncate ${statusError ? "text-red-400" : "text-muster-muted"}`}
                title={statusText}
              >
                {statusText}
              </span>
            ) : null}
            {button ? (
              <Button
                size="md"
                variant={button.primary ? "primary" : "default"}
                onClick={button.onClick}
              >
                {button.label}
              </Button>
            ) : (
              <Button
                size="md"
                loading={busy}
                onClick={() => ctrl?.checkForUpdates()}
                disabled={busy || !ctrl}
              >
                {busy ? t("settings.checkingForUpdates") : t("settings.checkForUpdates")}
              </Button>
            )}
          </div>
        </Row>
      </Section>

      <Section title={t("settings.links")}>
        <LinkRow title={t("settings.githubRepo")} desc={t("settings.githubRepoDesc")} url={GITHUB_URL} t={t} />
        <LinkRow title={t("settings.issues")} desc={t("settings.issuesDesc")} url={ISSUES_URL} t={t} />
        <LinkRow title={t("settings.releases")} desc={t("settings.releasesDesc")} url={RELEASES_URL} t={t} />
        <LinkRow title={t("settings.license")} desc={t("settings.licenseDesc")} url={LICENSE_URL} t={t} />
      </Section>

      <Section title={t("settings.systemInfo")}>
        <Row title={t("settings.versionLabel")}>
          <span className="ui-fs-sm text-muster-muted tabular-nums">
            v{version || "…"}
          </span>
        </Row>
        <Row title={t("settings.osLabel")}>
          <span className="ui-fs-sm text-muster-muted">{os || "…"}</span>
        </Row>
        <Row title={t("settings.archLabel")}>
          <span className="ui-fs-sm text-muster-muted">{arch || "…"}</span>
        </Row>
        <Row title={t("settings.configFile")} desc={configFile || undefined}>
          <Button
            size="md"
            onClick={() => configFile && revealItemInDir(configFile)}
            disabled={!configFile}
          >
            {t("common.revealInExplorer")}
          </Button>
        </Row>
      </Section>

      <div className="pt-1 pb-2 text-center ui-fs-xs text-muster-muted/70 select-none">
        {t("settings.copyright")}
      </div>
    </div>
  );
}
