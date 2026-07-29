import { useEffect } from "react";
import { useT, type TKey } from "../lib/i18n/context";

interface ShortcutItem {
  keys: string;
  labelKey: TKey;
}

/// Static shortcut reference, grouped to match the README's shortcuts
/// section. Keep in sync with the NAV_MAP in App.tsx.
const GROUPS: { titleKey: TKey; items: ShortcutItem[] }[] = [
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
      { keys: "Ctrl+Tab", labelKey: "shortcuts.tabSwitcher" },
      { keys: "Ctrl+Shift+[ / ]", labelKey: "shortcuts.prevNextTab" },
      { keys: "Ctrl+D", labelKey: "shortcuts.splitRight" },
      { keys: "Ctrl+Shift+D", labelKey: "shortcuts.splitDown" },
      { keys: "Ctrl+[ / ]", labelKey: "shortcuts.prevNextPane" },
      { keys: "Ctrl+Alt+←→↑↓", labelKey: "shortcuts.focusPane" },
      { keys: "Ctrl+Alt+Shift+←→↑↓", labelKey: "shortcuts.resizePane" },
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
      { keys: "Ctrl+Shift+G", labelKey: "shortcuts.gitPanel" },
      { keys: "Ctrl+Shift+I", labelKey: "shortcuts.infoPanel" },
      { keys: "Ctrl+S", labelKey: "shortcuts.saveFile" },
      { keys: "Ctrl+K", labelKey: "shortcuts.clearTerminal" },
      { keys: "Ctrl+Shift+U", labelKey: "shortcuts.usagePanel" },
    ],
  },
];

/// Keyboard-shortcuts reference overlay (Ctrl+/ or the command palette).
/// Same modal idiom as Settings: dimmed backdrop closes on click, Esc closes.
export default function ShortcutsHelp({ onClose }: { onClose: () => void }) {
  const { t } = useT();

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

  return (
    <div className="absolute inset-0 z-40 bg-black/35 flex justify-center items-start" onClick={onClose}>
      <div
        className="mt-16 w-[480px] max-w-[90vw] max-h-[75vh] overflow-y-auto bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] px-5 py-4 muster-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="ui-fs-base font-semibold mb-3">{t("shortcuts.title")}</h2>
        {GROUPS.map((group) => (
          <div key={group.titleKey} className="mb-3">
            <div className="ui-fs-xs text-muster-muted uppercase tracking-wide mb-1">
              {t(group.titleKey)}
            </div>
            {group.items.map((item) => (
              <div key={item.keys} className="flex items-center justify-between py-[3px]">
                <span className="ui-fs-base text-muster-fg/80">{t(item.labelKey)}</span>
                <kbd className="bg-white/[0.06] rounded px-1.5 py-0.5 ui-fs-sm font-mono text-muster-muted whitespace-nowrap">
                  {item.keys}
                </kbd>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
