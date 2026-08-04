import { useEffect, useRef, useState } from "react";
import { api } from "../lib/invoke";
import { formatTokens } from "../lib/format";
import type { Settings as SettingsType, ThemeInfo, UsageSummary } from "../lib/types";
import { useT, type TKey } from "../lib/i18n/context";
import {
  IconChartBar,
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconSettings,
  IconKeyboard,
  IconType,
  IconPlug,
  IconX,
} from "./icons";

export type SettingsTab = "general" | "font" | "shortcuts" | "integrations";

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
      { keys: "Ctrl+Shift+U", labelKey: "shortcuts.usagePanel" },
    ],
  },
];

export default function Settings({
  onClose,
  onOpenUsage,
  initialTab = "general",
}: {
  onClose: () => void;
  onOpenUsage: () => void;
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
          </nav>

          {/* Content */}
          <div className="flex-1 overflow-y-auto px-5 py-4">
            {tab === "general" && (
              <GeneralTab s={s} update={update} themes={themes} t={t} />
            )}
            {tab === "font" && <FontTab s={s} update={update} t={t} />}
            {tab === "shortcuts" && <ShortcutsTab t={t} />}
            {tab === "integrations" && (
              <IntegrationsTab onOpenUsage={onOpenUsage} t={t} />
            )}
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

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-4">
      <div className="ui-fs-xs text-muster-muted uppercase tracking-wide mb-1.5">
        {label}
      </div>
      {children}
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
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const ref = useRef<HTMLDivElement>(null);

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
        onClick={() => setOpen(!open)}
        className="w-full flex items-center gap-2 bg-white/[0.05] px-2.5 py-1.5 rounded-md ui-fs-base outline-none border border-transparent hover:border-white/[0.12] transition-colors"
      >
        {selected && <Swatch background={selected.background} accent={selected.accent} />}
        <span className="flex-1 text-left truncate">{value}</span>
        <span className={`text-muster-muted transition-transform duration-muster ease-muster ${open ? "rotate-180" : ""}`}>
          <IconChevronDown size={12} />
        </span>
      </button>
      {open && (
        <div className="absolute z-50 top-full left-0 right-0 mt-1 bg-muster-bg border border-white/[0.1] rounded-md shadow-[0_8px_24px_rgba(0,0,0,0.4)] overflow-hidden">
          <div className="p-1.5 border-b border-white/[0.06]">
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search..."
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
                No themes found
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

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
  const appearanceOptions: {
    value: "system" | "light" | "dark";
    label: string;
  }[] = [
    { value: "system", label: t("settings.themeSystem") },
    { value: "light", label: t("settings.themeLight") },
    { value: "dark", label: t("settings.themeDark") },
  ];

  return (
    <div>
      <Field label={t("settings.appearance")}>
        <div className="flex gap-2">
          {appearanceOptions.map((opt) => (
            <button
              key={opt.value}
              onClick={() => update({ theme: opt.value })}
              className={`flex-1 py-2.5 rounded-lg border ui-fs-sm transition-all duration-muster ease-muster ${
                s.theme === opt.value
                  ? "border-muster-accent bg-muster-accent/10 text-muster-fg"
                  : "border-white/[0.08] text-muster-muted hover:border-white/[0.15] hover:text-muster-fg"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </Field>

      <Field label={t("settings.language")}>
        <select
          value={s.language}
          onChange={(e) =>
            update({ language: e.target.value as SettingsType["language"] })
          }
          className="w-full bg-white/[0.05] px-2.5 py-1.5 rounded-md ui-fs-base outline-none border border-transparent focus:border-muster-accent/30 transition-colors"
        >
          <option value="system">{t("settings.languageSystem")}</option>
          <option value="en">{t("settings.languageEn")}</option>
          <option value="zh">{t("settings.languageZh")}</option>
        </select>
      </Field>

      <Field label={t("settings.darkTheme")}>
        <ThemePicker
          themes={themes}
          dark
          value={s.theme_dark}
          onChange={(name) => update({ theme_dark: name })}
        />
      </Field>

      <Field label={t("settings.lightTheme")}>
        <ThemePicker
          themes={themes}
          dark={false}
          value={s.theme_light}
          onChange={(name) => update({ theme_light: name })}
        />
      </Field>
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
      <Field label={t("settings.fontFamily")}>
        <input
          value={s.font_family}
          onChange={(e) => update({ font_family: e.target.value })}
          className="w-full bg-white/[0.05] px-2.5 py-1.5 rounded-md ui-fs-base outline-none border border-transparent focus:border-muster-accent/30 transition-colors"
          placeholder={t("settings.fontFamilyPlaceholder")}
        />
      </Field>

      <Field label={t("settings.fontSize")}>
        <div className="flex items-center gap-3">
          <input
            type="range"
            min={8}
            max={32}
            value={s.font_size}
            onChange={(e) => update({ font_size: Number(e.target.value) })}
            className="flex-1"
          />
          <span className="ui-fs-base w-10 text-right tabular-nums text-muster-muted">
            {s.font_size}px
          </span>
        </div>
      </Field>

      <Field label={t("settings.uiFontSize")}>
        <div className="flex items-center gap-3">
          <input
            type="range"
            min={10}
            max={16}
            step={0.5}
            value={s.ui_font_size}
            onChange={(e) => update({ ui_font_size: Number(e.target.value) })}
            className="flex-1"
          />
          <span className="ui-fs-base w-10 text-right tabular-nums text-muster-muted">
            {s.ui_font_size}px
          </span>
        </div>
      </Field>

      <div className="space-y-3 mt-2">
        <div className="flex items-center justify-between bg-white/[0.03] rounded-lg px-3 py-2.5">
          <span className="ui-fs-base text-muster-fg">
            {t("settings.thickenFont")}
          </span>
          <Toggle
            checked={s.font_thicken}
            onChange={(v) => update({ font_thicken: v })}
          />
        </div>
        <div className="flex items-center justify-between bg-white/[0.03] rounded-lg px-3 py-2.5">
          <span className="ui-fs-base text-muster-fg">
            {t("settings.wrapLines")}
          </span>
          <Toggle
            checked={s.editor_wrap_lines}
            onChange={(v) => update({ editor_wrap_lines: v })}
          />
        </div>
      </div>
    </div>
  );
}

function ShortcutsTab({ t }: { t: ReturnType<typeof useT>["t"] }) {
  return (
    <div>
      {SHORTCUT_GROUPS.map((group) => (
        <div key={group.titleKey} className="mb-4 last:mb-0">
          <div className="ui-fs-xs text-muster-muted uppercase tracking-wide mb-2">
            {t(group.titleKey)}
          </div>
          <div className="space-y-0.5">
            {group.items.map((item) => (
              <div
                key={item.keys}
                className="flex items-center justify-between py-1.5 px-2 rounded-md hover:bg-white/[0.03] transition-colors"
              >
                <span className="ui-fs-base text-muster-fg/80">
                  {t(item.labelKey)}
                </span>
                <kbd className="bg-white/[0.06] rounded px-1.5 py-0.5 ui-fs-sm font-mono text-muster-muted whitespace-nowrap">
                  {item.keys}
                </kbd>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function IntegrationsTab({
  onOpenUsage,
  t,
}: {
  onOpenUsage: () => void;
  t: ReturnType<typeof useT>["t"];
}) {
  return (
    <div>
      <Integrations t={t} />
      <UsageEntry onOpen={onOpenUsage} t={t} />
    </div>
  );
}

function UsageEntry({
  onOpen,
  t,
}: {
  onOpen: () => void;
  t: ReturnType<typeof useT>["t"];
}) {
  const [summary, setSummary] = useState<UsageSummary | null>(null);

  useEffect(() => {
    api.usage.summary().then(setSummary).catch(() => {});
  }, []);

  const totals = (summary?.tools ?? []).reduce(
    (acc, ts) => ({
      tokens: acc.tokens + ts.total_tokens,
      sessions: acc.sessions + ts.session_count,
    }),
    { tokens: 0, sessions: 0 }
  );

  return (
    <div className="mt-2">
      <div className="ui-fs-xs text-muster-muted uppercase tracking-wide mb-1.5">
        {t("settings.openUsage")}
      </div>
      <button
        onClick={onOpen}
        className="w-full flex items-center gap-3 bg-white/[0.03] border border-white/[0.06] rounded-lg px-3 py-2.5 text-left hover:bg-white/[0.06] hover:border-white/[0.1] active:scale-[.99] transition-all duration-muster ease-muster"
      >
        <span className="w-8 h-8 shrink-0 rounded-md bg-muster-accent/15 text-muster-accent flex items-center justify-center">
          <IconChartBar size={16} />
        </span>
        <span className="flex-1 min-w-0">
          <span className="block ui-fs-base text-muster-fg">
            {t("settings.openUsage")}
          </span>
          <span className="block ui-fs-xs text-muster-muted mt-0.5 tabular-nums">
            {totals.sessions > 0
              ? t("settings.usageTotals", {
                  tokens: formatTokens(totals.tokens),
                  sessions: totals.sessions,
                })
              : t("settings.usageEmpty")}
          </span>
        </span>
        <span className="text-muster-muted shrink-0">
          <IconChevronRight size={14} />
        </span>
      </button>
    </div>
  );
}

function Integrations({ t }: { t: ReturnType<typeof useT>["t"] }) {
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(
    null
  );
  const [pathResult, setPathResult] = useState<{
    ok: boolean;
    text: string;
  } | null>(null);
  const [onPath, setOnPath] = useState<boolean | null>(null);

  useEffect(() => {
    api.isOnPath().then(setOnPath).catch(() => setOnPath(false));
  }, []);

  const installExplorer = () => {
    setResult(null);
    api
      .installExplorerContextMenu()
      .then(() =>
        setResult({ ok: true, text: t("settings.integrationsInstalled") })
      )
      .catch((e) => setResult({ ok: false, text: String(e) }));
  };

  const togglePath = () => {
    setPathResult(null);
    if (onPath) {
      api
        .removeFromPath()
        .then(() => {
          setOnPath(false);
          setPathResult({ ok: true, text: t("settings.integrationsInstalled") });
        })
        .catch((e) => setPathResult({ ok: false, text: String(e) }));
    } else {
      api
        .addToPath()
        .then(() => {
          setOnPath(true);
          setPathResult({ ok: true, text: t("settings.integrationsInstalled") });
        })
        .catch((e) => setPathResult({ ok: false, text: String(e) }));
    }
  };

  return (
    <div className="mb-2">
      <div className="ui-fs-xs text-muster-muted uppercase tracking-wide mb-1.5">
        {t("settings.integrationsTitle")}
      </div>
      <div className="space-y-2">
        <div className="flex items-center gap-2 bg-white/[0.03] rounded-lg px-3 py-2.5">
          <button
            onClick={installExplorer}
            className="px-3 py-1.5 rounded-md bg-white/[0.05] ui-fs-sm hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {t("settings.installExplorerMenu")}
          </button>
          {result && (
            <span
              className={`ui-fs-sm ${result.ok ? "text-green-400" : "text-red-400"}`}
            >
              {result.text}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2 bg-white/[0.03] rounded-lg px-3 py-2.5">
          <button
            onClick={togglePath}
            className="px-3 py-1.5 rounded-md bg-white/[0.05] ui-fs-sm hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {onPath ? t("settings.removeFromPath") : t("settings.addToPath")}
          </button>
          {onPath !== null && !pathResult && (
            <span
              className={`ui-fs-sm ${onPath ? "text-green-400" : "text-muster-muted"}`}
            >
              {onPath
                ? t("settings.onPathInstalled")
                : t("settings.pathAvailable")}
            </span>
          )}
          {pathResult && (
            <span
              className={`ui-fs-sm ${pathResult.ok ? "text-green-400" : "text-red-400"}`}
            >
              {pathResult.text}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
