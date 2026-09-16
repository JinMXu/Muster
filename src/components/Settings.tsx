import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/invoke";
import {
  createUpdater,
  type UpdaterController,
  type UpdaterPhase,
} from "../lib/updater";
import type { Settings as SettingsType, ThemeInfo } from "../lib/types";
import { useT, type TKey } from "../lib/i18n/context";
import {
  IconCheck,
  IconChevronDown,
  IconInfo,
  IconKeyboard,
  IconPlug,
  IconSettings,
  IconType,
  IconX,
  IconArrowUpRight,
} from "./icons";

export type SettingsTab = "general" | "font" | "shortcuts" | "integrations" | "about";

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
      { keys: "Ctrl+Shift+G", labelKey: "shortcuts.gitPanel" },
      { keys: "Ctrl+Shift+I", labelKey: "shortcuts.infoPanel" },
      { keys: "Ctrl+S", labelKey: "shortcuts.saveFile" },
      { keys: "Ctrl+K", labelKey: "shortcuts.clearTerminal" },
    ],
  },
];

const GITHUB_URL = "https://github.com/JinMXu/Muster";

export default function Settings({
  onClose,
  initialTab = "general",
}: {
  onClose: () => void;
  initialTab?: SettingsTab;
}) {
  const [s, setS] = useState<SettingsType | null>(null);
  const [themes, setThemes] = useState<ThemeInfo[]>([]);
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const { t } = useT();

  useEffect(() => {
    api.settings().then(setS);
    api.availableThemesWithInfo().then(setThemes);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  if (!s) return null;

  const update = (patch: Partial<SettingsType>) => setS({ ...s, ...patch });
  const save = () => {
    if (s) api.saveSettings(s).then(onClose);
  };

  return (
    <div
      className="fixed inset-0 z-40 bg-black/35 flex items-center justify-center"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        className="bg-muster-bg border border-white/[0.08] rounded-[10px] shadow-[0_12px_32px_rgba(0,0,0,0.5)] muster-pop flex flex-col overflow-hidden"
        style={{ width: 680, maxHeight: 600 }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-4 pb-3 border-b border-white/[0.06] flex-shrink-0">
          <h2 className="ui-fs-base font-semibold">{t("settings.title")}</h2>
          <button
            onClick={onClose}
            className="text-muster-muted hover:text-muster-fg transition-colors duration-muster ease-muster"
          >
            <IconX size={16} />
          </button>
        </div>

        {/* Body: sidebar + content */}
        <div className="flex flex-1 min-h-0">
          {/* Left sidebar */}
          <nav className="w-[160px] flex-shrink-0 py-3 px-2 border-r border-white/[0.06] flex flex-col gap-0.5">
            <TabButton
              active={tab === "general"}
              onClick={() => setTab("general")}
              icon={<IconSettings size={15} />}
              label={t("settings.tabGeneral")}
            />
            <TabButton
              active={tab === "font"}
              onClick={() => setTab("font")}
              icon={<IconType size={15} />}
              label={t("settings.tabFont")}
            />
            <TabButton
              active={tab === "shortcuts"}
              onClick={() => setTab("shortcuts")}
              icon={<IconKeyboard size={15} />}
              label={t("settings.tabShortcuts")}
            />
            <TabButton
              active={tab === "integrations"}
              onClick={() => setTab("integrations")}
              icon={<IconPlug size={15} />}
              label={t("settings.tabIntegrations")}
            />
            <TabButton
              active={tab === "about"}
              onClick={() => setTab("about")}
              icon={<IconInfo size={15} />}
              label={t("settings.tabAbout")}
            />
          </nav>

          {/* Content */}
          <div className="flex-1 overflow-y-auto px-5 py-4">
            {tab === "general" && (
              <GeneralTab s={s} update={update} themes={themes} t={t} />
            )}
            {tab === "font" && <FontTab s={s} update={update} t={t} />}
            {tab === "shortcuts" && <ShortcutsTab t={t} />}
            {tab === "integrations" && <IntegrationsTab t={t} />}
            {tab === "about" && <AboutTab t={t} />}
          </div>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-5 py-3 border-t border-white/[0.06] flex-shrink-0">
          <button
            onClick={() => {
              api.defaultSettings().then(setS);
            }}
            className="px-3 py-1.5 rounded-md bg-white/[0.05] ui-fs-base hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {t("settings.reset")}
          </button>
          <button
            onClick={save}
            className="px-3 py-1.5 rounded-md bg-muster-accent text-white ui-fs-base active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {t("settings.save")}
          </button>
        </div>
      </div>
    </div>
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
      className={`flex items-center gap-2 px-3 py-2 rounded-md ui-fs-sm transition-colors duration-muster ease-muster ${
        active
          ? "bg-white/[0.09] text-muster-fg"
          : "text-muster-muted hover:bg-muster-hover hover:text-muster-fg"
      }`}
    >
      <span className="flex items-center">{icon}</span>
      <span>{label}</span>
    </button>
  );
}

/** Titled group card: all rows inside share one bordered container. */
function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-5 last:mb-0">
      <div className="ui-fs-xs text-muster-muted uppercase tracking-wide mb-2 px-0.5">
        {title}
      </div>
      {/* No overflow-hidden here: dropdown popovers (theme picker) must be
          able to paint outside the card boundary. */}
      <div className="bg-white/[0.02] border border-white/[0.06] rounded-lg divide-y divide-white/[0.05]">
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
    <div className="flex items-center justify-between gap-6 px-3.5 py-2.5 min-h-[52px]">
      <div className="min-w-0">
        <div className="ui-fs-base text-muster-fg">{title}</div>
        {desc && (
          <div className="ui-fs-xs text-muster-muted mt-0.5">{desc}</div>
        )}
      </div>
      <div className="flex items-center flex-shrink-0">{children}</div>
    </div>
  );
}

function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={`relative w-9 h-5 rounded-full transition-colors duration-muster ease-muster flex-shrink-0 ${
        checked ? "bg-muster-accent" : "bg-white/[0.12]"
      }`}
    >
      <span
        className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform duration-muster ease-muster ${
          checked ? "translate-x-4" : "translate-x-0"
        }`}
      />
    </button>
  );
}

function Swatch({ background, accent }: { background: string; accent: string }) {
  return (
    <span
      className="w-4 h-4 rounded-full flex-shrink-0 border-2"
      style={{
        backgroundColor: `#${background}`,
        borderColor: `#${accent}`,
      }}
    />
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex bg-white/[0.05] rounded-md p-0.5">
      {options.map((opt) => (
        <button
          key={opt.value}
          onClick={() => onChange(opt.value)}
          className={`px-2.5 py-1 rounded ui-fs-sm transition-colors duration-muster ease-muster ${
            value === opt.value
              ? "bg-muster-accent text-white"
              : "text-muster-muted hover:text-muster-fg"
          }`}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

const BUILT_IN_THEMES = new Set([
  "Default Dark", "Default Light", "Dracula", "Tokyo Night", "Gruvbox Dark", "Monokai Pro",
]);

function ThemePicker({
  themes,
  dark,
  value,
  onChange,
}: {
  themes: ThemeInfo[];
  dark: boolean;
  value: string;
  onChange: (name: string) => void;
}) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const [dropUp, setDropUp] = useState(false);
  const [query, setQuery] = useState("");
  const ref = useRef<HTMLDivElement>(null);

  // Approximate rendered height of the popover (search input + list).
  const POPOVER_H = 280;

  const toggleOpen = () => {
    if (open) {
      setOpen(false);
      setQuery("");
      return;
    }
    // Open upward when there isn't enough room below inside the nearest
    // scrollable ancestor (the settings scroll area clips its content).
    const el = ref.current;
    if (el) {
      let p: HTMLElement | null = el.parentElement;
      while (p) {
        const s = getComputedStyle(p);
        if (/(auto|scroll)/.test(s.overflowY)) break;
        p = p.parentElement;
      }
      const rect = el.getBoundingClientRect();
      if (p) {
        const pr = p.getBoundingClientRect();
        const below = pr.bottom - rect.bottom;
        const above = rect.top - pr.top;
        setDropUp(below < POPOVER_H && above > below);
      } else {
        setDropUp(false);
      }
    }
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
        setQuery("");
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const filtered = themes.filter(
    (th) =>
      th.is_dark === dark &&
      th.name.toLowerCase().includes(query.toLowerCase())
  );
  const selected = themes.find((th) => th.name === value);

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={toggleOpen}
        className="w-[190px] flex items-center gap-2 bg-white/[0.05] px-2.5 py-1.5 rounded-md ui-fs-sm outline-none border border-transparent hover:border-white/[0.12] transition-colors"
      >
        {selected && <Swatch background={selected.background} accent={selected.accent} />}
        <span className="flex-1 text-left truncate">{value}</span>
        <span className={`text-muster-muted transition-transform duration-muster ease-muster ${open ? "rotate-180" : ""}`}>
          <IconChevronDown size={12} />
        </span>
      </button>
      {open && (
        <div
          className={`absolute z-50 right-0 w-[280px] bg-muster-bg border border-white/[0.1] rounded-md shadow-[0_8px_24px_rgba(0,0,0,0.4)] overflow-hidden ${
            dropUp ? "bottom-full mb-1" : "top-full mt-1"
          }`}
        >
          <div className="p-1.5 border-b border-white/[0.06]">
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("settings.themeSearchPlaceholder")}
              className="w-full bg-white/[0.05] px-2 py-1 rounded ui-fs-sm outline-none"
            />
          </div>
          <div className="max-h-[220px] overflow-y-auto py-1">
            {filtered.map((th, i) => {
              const showSeparator =
                i > 0 &&
                BUILT_IN_THEMES.has(th.name) &&
                !BUILT_IN_THEMES.has(filtered[i - 1].name);
              return (
                <div key={th.name}>
                  {showSeparator && (
                    <div className="my-1 border-t border-white/[0.06]" />
                  )}
                  <button
                    onClick={() => {
                      onChange(th.name);
                      setOpen(false);
                      setQuery("");
                    }}
                    className={`w-full flex items-center gap-2 px-2.5 py-1.5 ui-fs-sm text-left hover:bg-white/[0.06] transition-colors ${
                      th.name === value ? "text-muster-accent" : "text-muster-fg"
                    }`}
                  >
                    <Swatch background={th.background} accent={th.accent} />
                    <span className="flex-1 truncate">{th.name}</span>
                    {th.name === value && <IconCheck size={12} />}
                  </button>
                </div>
              );
            })}
            {filtered.length === 0 && (
              <div className="px-2.5 py-2 ui-fs-sm text-muster-muted">
                {t("settings.noThemesFound")}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
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

function GeneralTab({
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
      <Section title={t("settings.appearance")}>
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
        <Row title={t("settings.darkTheme")}>
          <ThemePicker
            themes={themes}
            dark
            value={s.theme_dark}
            onChange={(name) => update({ theme_dark: name })}
          />
        </Row>
        <Row title={t("settings.lightTheme")}>
          <ThemePicker
            themes={themes}
            dark={false}
            value={s.theme_light}
            onChange={(name) => update({ theme_light: name })}
          />
        </Row>
        <Row title={t("settings.language")}>
          <select
            value={s.language}
            onChange={(e) =>
              update({ language: e.target.value as SettingsType["language"] })
            }
            className="w-[190px] bg-white/[0.05] px-2.5 py-1.5 rounded-md ui-fs-sm outline-none border border-transparent focus:border-muster-accent/30 transition-colors"
          >
            <option value="system">{t("settings.languageSystem")}</option>
            <option value="en">{t("settings.languageEn")}</option>
            <option value="zh">{t("settings.languageZh")}</option>
          </select>
        </Row>
      </Section>

      <Section title={t("settings.behavior")}>
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

function FontTab({
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
      <Section title={t("settings.terminalFont")}>
        <Row title={t("settings.fontFamily")}>
          <input
            value={s.font_family}
            onChange={(e) => update({ font_family: e.target.value })}
            className="w-[220px] bg-white/[0.05] px-2.5 py-1.5 rounded-md ui-fs-sm outline-none border border-transparent focus:border-muster-accent/30 transition-colors"
            placeholder={t("settings.fontFamilyPlaceholder")}
          />
        </Row>
        <SliderRow
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
  return (
    <div>
      {SHORTCUT_GROUPS.map((group) => (
        <Section key={group.titleKey} title={t(group.titleKey)}>
          {group.items.map((item) => (
            <div
              key={item.keys}
              className="flex items-center justify-between gap-4 px-3.5 py-2"
            >
              <span className="ui-fs-sm text-muster-fg/80">
                {t(item.labelKey)}
              </span>
              <kbd className="bg-white/[0.06] rounded px-1.5 py-0.5 ui-fs-xs font-mono text-muster-muted whitespace-nowrap">
                {item.keys}
              </kbd>
            </div>
          ))}
        </Section>
      ))}
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
      <button
        onClick={onClick}
        disabled={busy}
        className="px-3 py-1.5 rounded-md bg-white/[0.06] ui-fs-sm hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster disabled:opacity-50 disabled:pointer-events-none"
      >
        {label}
      </button>
    </div>
  );
}

function AboutTab({ t }: { t: ReturnType<typeof useT>["t"] }) {
  const [version, setVersion] = useState<string | null>(null);
  const [phase, setPhase] = useState<UpdaterPhase>({ kind: "idle" });
  const ctrlRef = useRef<UpdaterController | null>(null);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
    ctrlRef.current = createUpdater(setPhase);
  }, []);

  const ctrl = ctrlRef.current;
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
        <img src="app-icon.png" alt="Muster" className="w-16 h-16 rounded-[14px]" />
        <div className="ui-fs-base font-semibold">Muster</div>
        <div className="ui-fs-xs text-muster-muted tabular-nums">
          {version ? t("settings.currentVersion", { version }) : "\u2026"}
        </div>
      </div>

      <Section title={t("settings.updates")}>
        <Row title={t("settings.updates")} desc={statusText ?? t("settings.updatesDesc")}>
          <div className={`ui-fs-xs ${statusError ? "text-red-400" : ""}`}>
            {phase.kind === "downloading" ? (
              <div className="w-[140px] h-1.5 rounded-full bg-white/[0.08] overflow-hidden">
                <div
                  className="h-full rounded-full bg-muster-accent transition-all duration-muster ease-muster"
                  style={{ width: `${phase.progress}%` }}
                />
              </div>
            ) : null}
          </div>
          {button ? (
            <button
              onClick={button.onClick}
              className={`ml-2 px-3 py-1.5 rounded-md ui-fs-sm active:scale-[.97] transition-transform duration-muster ease-muster ${
                button.primary
                  ? "bg-muster-accent text-white"
                  : "bg-white/[0.06] hover:bg-muster-hover-btn"
              }`}
            >
              {button.label}
            </button>
          ) : (
            <button
              onClick={() => ctrl?.checkForUpdates()}
              disabled={busy || !ctrl}
              className="ml-2 px-3 py-1.5 rounded-md bg-white/[0.06] ui-fs-sm hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster disabled:opacity-50 disabled:pointer-events-none"
            >
              {busy ? t("settings.checkingForUpdates") : t("settings.checkForUpdates")}
            </button>
          )}
        </Row>
      </Section>

      <Section title="Muster">
        <Row title={t("settings.githubRepo")} desc={t("settings.githubRepoDesc")}>
          <button
            onClick={() => openUrl(GITHUB_URL)}
            className="flex items-center gap-1 px-3 py-1.5 rounded-md bg-white/[0.06] ui-fs-sm hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {t("settings.openOnGithub")}
            <IconArrowUpRight size={12} />
          </button>
        </Row>
      </Section>
    </div>
  );
}
