import { useEffect, useState } from "react";
import { api } from "../lib/invoke";
import { formatTokens } from "../lib/format";
import type { Settings as SettingsType, UsageSummary } from "../lib/types";
import { useT } from "../lib/i18n/context";
import { IconChartBar, IconChevronRight } from "./icons";

/// Modal settings window: font / colors / appearance / editor / terminal.
export default function Settings({ onClose, onOpenUsage }: { onClose: () => void; onOpenUsage: () => void }) {
  const [s, setS] = useState<SettingsType | null>(null);
  const [themes, setThemes] = useState<string[]>([]);
  const { t } = useT();

  useEffect(() => {
    api.settings().then(setS);
    api.availableThemes().then(setThemes);
  }, []);

  if (!s) return null;

  const update = (patch: Partial<SettingsType>) => {
    setS({ ...s, ...patch });
  };
  const save = () => {
    if (s) api.saveSettings(s).then(onClose);
  };

  return (
    <div className="absolute inset-0 z-40 bg-black/35" onClick={onClose}>
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[440px]">
        <div
          className="bg-muster-bg border border-white/[0.08] rounded-[10px] shadow-[0_12px_32px_rgba(0,0,0,0.5)] px-5 py-4 muster-pop"
          onClick={(e) => e.stopPropagation()}
        >
        <h2 className="text-sm font-semibold mb-3">{t("settings.title")}</h2>

        <Field label={t("settings.appearance")}>
          {(["system", "light", "dark"] as const).map((th) => (
            <label key={th} className="flex items-center gap-1.5 text-xs mr-3">
              <input
                type="radio"
                checked={s.theme === th}
                onChange={() => update({ theme: th })}
              />
              {th === "system" ? t("settings.themeSystem") : th === "light" ? t("settings.themeLight") : t("settings.themeDark")}
            </label>
          ))}
        </Field>

        <Field label={t("settings.language")}>
          <select
            value={s.language}
            onChange={(e) => update({ language: e.target.value as SettingsType["language"] })}
            className="flex-1 bg-white/[0.05] px-2 py-1 rounded text-xs outline-none"
          >
            <option value="system">{t("settings.languageSystem")}</option>
            <option value="en">{t("settings.languageEn")}</option>
            <option value="zh">{t("settings.languageZh")}</option>
          </select>
        </Field>

        <Field label={t("settings.fontFamily")}>
          <input
            value={s.font_family}
            onChange={(e) => update({ font_family: e.target.value })}
            className="flex-1 bg-white/[0.05] px-2 py-1 rounded text-xs outline-none"
            placeholder={t("settings.fontFamilyPlaceholder")}
          />
        </Field>

        <Field label={t("settings.fontSize")}>
          <input
            type="range"
            min={8}
            max={32}
            value={s.font_size}
            onChange={(e) => update({ font_size: Number(e.target.value) })}
          />
          <span className="text-xs ml-2 w-8">{s.font_size}</span>
        </Field>

        <div className="flex items-center gap-2 mb-3">
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={s.font_thicken}
              onChange={(e) => update({ font_thicken: e.target.checked })}
            />
            {t("settings.thickenFont")}
          </label>
        </div>

        <Field label={t("settings.darkTheme")}>
          <select
            value={s.theme_dark}
            onChange={(e) => update({ theme_dark: e.target.value })}
            className="flex-1 bg-white/[0.05] px-2 py-1 rounded text-xs outline-none"
          >
            {themes.map((th) => (
              <option key={th} value={th}>{th}</option>
            ))}
          </select>
        </Field>

        <Field label={t("settings.lightTheme")}>
          <select
            value={s.theme_light}
            onChange={(e) => update({ theme_light: e.target.value })}
            className="flex-1 bg-white/[0.05] px-2 py-1 rounded text-xs outline-none"
          >
            {themes.map((th) => (
              <option key={th} value={th}>{th}</option>
            ))}
          </select>
        </Field>

        <div className="flex items-center gap-2 mb-3">
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={s.editor_wrap_lines}
              onChange={(e) => update({ editor_wrap_lines: e.target.checked })}
            />
            {t("settings.wrapLines")}
          </label>
        </div>

        <Integrations />

        <UsageEntry onOpen={onOpenUsage} />

        <div className="flex justify-end gap-2">
          <button
            onClick={() => {
              // Fill the form with factory defaults; they only take effect
              // (and persist) once the user clicks Save.
              api.defaultSettings().then(setS);
            }}
            className="px-3 py-1.5 rounded-md bg-white/[0.05] text-xs hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {t("settings.reset")}
          </button>
          <button
            onClick={save}
            className="px-3 py-1.5 rounded-md bg-muster-accent text-white text-xs active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {t("settings.save")}
          </button>
        </div>
        </div>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-3">
      <div className="text-[10px] text-muster-muted uppercase tracking-wide mb-1">{label}</div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}

/// Token-usage entry card: chart icon tile + live aggregate (total tokens and
/// sessions from the cached usage summary) + chevron. Replaces the bare text
/// link; matches the card style used inside the usage panel itself.
function UsageEntry({ onOpen }: { onOpen: () => void }) {
  const { t } = useT();
  const [summary, setSummary] = useState<UsageSummary | null>(null);

  useEffect(() => {
    api.usage.summary().then(setSummary).catch(() => {});
  }, []);

  const totals = (summary?.tools ?? []).reduce(
    (acc, ts) => ({ tokens: acc.tokens + ts.total_tokens, sessions: acc.sessions + ts.session_count }),
    { tokens: 0, sessions: 0 }
  );

  return (
    <div className="mb-4">
      <button
        onClick={onOpen}
        className="w-full flex items-center gap-3 bg-white/[0.03] border border-white/[0.06] rounded-lg px-3 py-2.5 text-left hover:bg-white/[0.06] hover:border-white/[0.1] active:scale-[.99] transition-all duration-muster ease-muster"
      >
        <span className="w-8 h-8 shrink-0 rounded-md bg-muster-accent/15 text-muster-accent flex items-center justify-center">
          <IconChartBar size={16} />
        </span>
        <span className="flex-1 min-w-0">
          <span className="block text-xs text-muster-fg">{t("settings.openUsage")}</span>
          <span className="block text-[10px] text-muster-muted mt-0.5 tabular-nums">
            {totals.sessions > 0
              ? t("settings.usageTotals", { tokens: formatTokens(totals.tokens), sessions: totals.sessions })
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

/// Windows Explorer integration: install the "Open in Muster" context menu
/// entry. Written under HKCU (per-user), so no elevation is needed; any
/// failure message from reg.exe is shown inline.
function Integrations() {
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const { t } = useT();

  const install = () => {
    setResult(null);
    api
      .installExplorerContextMenu()
      .then(() => setResult({ ok: true, text: t("settings.integrationsInstalled") }))
      .catch((e) => setResult({ ok: false, text: String(e) }));
  };

  return (
    <Field label={t("settings.integrationsTitle")}>
      <button
        onClick={install}
        className="px-3 py-1.5 rounded-md bg-white/[0.05] text-xs hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
      >
        {t("settings.installExplorerMenu")}
      </button>
      {result && (
        <span className={`text-xs ${result.ok ? "text-green-400" : "text-red-400"}`}>
          {result.text}
        </span>
      )}
    </Field>
  );
}
