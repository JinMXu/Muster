import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { api } from "../lib/invoke";
import { fuzzyFilter } from "../lib/fuzzy";
import { getRecentFiles, subscribeRecentFiles } from "../lib/recentFiles";
import { useProjectCwd } from "../lib/useProjectCwd";
import type { AppStateView, SessionInfo } from "../lib/types";
import { useT } from "../lib/i18n/context";
import { IconFile, IconSearch } from "./icons";

interface CommandItem {
  id: string;
  title: string;
  icon: React.ReactNode;
  shortcut?: string;
  action: () => void;
}

/// File-row glyph reused for both project files and recent files.
const fileIcon = <IconFile size={12} />;

/// Centered Ctrl+P overlay: project-file quick open (VS Code style) + app
/// actions + open sessions, all fuzzy-filtered. Recent files surface first.
export default function CommandPalette({
  state,
  onClose,
  onAskNewProject,
  onClearTerminal,
  onCloseProject,
  onOpenSettings,
  onOpenShortcuts,
  onOpenUsage,
  onOpenSearch,
  onReopenClosed,
}: {
  state: AppStateView | null;
  onClose: () => void;
  onAskNewProject: () => void;
  onClearTerminal: () => void;
  onCloseProject: () => void;
  onOpenSettings: () => void;
  onOpenShortcuts: () => void;
  onOpenUsage: () => void;
  onOpenSearch: () => void;
  onReopenClosed: () => void;
}) {
  const { t } = useT();
  // The palette is mounted only while open, so this hook's polling only runs
  // then (same pattern as the SearchPanel).
  const { root } = useProjectCwd(state);
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState(0);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [projectFiles, setProjectFiles] = useState<string[] | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const recent = useSyncExternalStore(subscribeRecentFiles, getRecentFiles);

  useEffect(() => {
    api.listAllSessions().then(setSessions);
    setTimeout(() => inputRef.current?.focus(), 0);
  }, []);

  // Lazy-load the project's file list once (per open). Repos can be large,
  // so only the first 2000 paths come back.
  useEffect(() => {
    if (!root) {
      setProjectFiles(null);
      return;
    }
    api.listProjectFiles(root).then(setProjectFiles).catch(() => setProjectFiles([]));
  }, [root]);

  const openProjectFile = (rel: string) => {
    if (!root) return;
    const abs = `${root.replace(/[\\/]+$/, "")}\\${rel.replace(/\//g, "\\")}`;
    api.openFile(abs, false);
  };

  const commands: CommandItem[] = useMemo(
    () => [
      { id: "new-session", title: t("common.newSession"), icon: "+", shortcut: "Ctrl+T", action: () => api.spawnSession() },
      { id: "split-right", title: t("common.splitRight"), icon: "▸", shortcut: "Ctrl+D", action: () => api.split("right") },
      { id: "split-down", title: t("common.splitDown"), icon: "▾", shortcut: "Ctrl+Shift+D", action: () => api.split("bottom") },
      { id: "split-left", title: t("commandPalette.splitLeft"), icon: "◂", action: () => api.split("left") },
      { id: "split-up", title: t("commandPalette.splitUp"), icon: "▴", action: () => api.split("top") },
      { id: "focus-pane-left", title: t("commandPalette.focusPaneLeft"), icon: "←", shortcut: "Ctrl+Alt+←", action: () => api.focusPane("left") },
      { id: "focus-pane-right", title: t("commandPalette.focusPaneRight"), icon: "→", shortcut: "Ctrl+Alt+→", action: () => api.focusPane("right") },
      { id: "focus-pane-up", title: t("commandPalette.focusPaneUp"), icon: "↑", shortcut: "Ctrl+Alt+↑", action: () => api.focusPane("up") },
      { id: "focus-pane-down", title: t("commandPalette.focusPaneDown"), icon: "↓", shortcut: "Ctrl+Alt+↓", action: () => api.focusPane("down") },
      { id: "resize-pane-left", title: t("commandPalette.resizePaneLeft"), icon: "⇠", shortcut: "Ctrl+Shift+Alt+←", action: () => api.resizePane("left") },
      { id: "resize-pane-right", title: t("commandPalette.resizePaneRight"), icon: "⇢", shortcut: "Ctrl+Shift+Alt+→", action: () => api.resizePane("right") },
      { id: "resize-pane-up", title: t("commandPalette.resizePaneUp"), icon: "⇡", shortcut: "Ctrl+Shift+Alt+↑", action: () => api.resizePane("up") },
      { id: "resize-pane-down", title: t("commandPalette.resizePaneDown"), icon: "⇣", shortcut: "Ctrl+Shift+Alt+↓", action: () => api.resizePane("down") },
      { id: "toggle-pane-zoom", title: t("commandPalette.togglePaneZoom"), icon: "⤢", shortcut: "Ctrl+Shift+Enter", action: () => api.togglePaneZoom() },
      { id: "equalize-panes", title: t("commandPalette.equalizePanes"), icon: "▦", action: () => api.equalizePanes() },
      { id: "clear-terminal", title: t("commandPalette.clearTerminal"), icon: "⌫", shortcut: "Ctrl+K", action: onClearTerminal },
      { id: "save-file", title: t("commandPalette.saveFile"), icon: "⤓", shortcut: "Ctrl+S", action: () => api.saveSelectedFile() },
      { id: "new-project", title: t("commandPalette.newProject"), icon: "◈", shortcut: "Ctrl+N", action: onAskNewProject },
      { id: "close-project", title: t("commandPalette.closeProject"), icon: "⊗", action: onCloseProject },
      { id: "close-tab", title: t("commandPalette.closeTab"), icon: "✕", shortcut: "Ctrl+W", action: () => api.closeSelectedTab() },
      { id: "toggle-left-sidebar", title: t("commandPalette.toggleLeftSidebar"), icon: "◧", shortcut: "Ctrl+B", action: () => api.toggleLeftSidebar() },
      { id: "toggle-right-sidebar", title: t("commandPalette.toggleRightSidebar"), icon: "◨", shortcut: "Ctrl+Shift+B", action: () => api.toggleRightPanel() },
      { id: "toggle-files", title: t("commandPalette.toggleFilesPanel"), icon: "▤", shortcut: "Ctrl+Shift+E", action: () => api.togglePanel("files") },
      { id: "search-files", title: t("commandPalette.toggleSearchPanel"), icon: "⌕", shortcut: "Ctrl+Shift+F", action: onOpenSearch },
      { id: "toggle-git", title: t("commandPalette.toggleGitPanel"), icon: "⎇", shortcut: "Ctrl+Shift+G", action: () => api.togglePanel("git") },
      { id: "toggle-info", title: t("commandPalette.toggleInfoPanel"), icon: "i", shortcut: "Ctrl+Shift+I", action: () => api.togglePanel("info") },
      { id: "next-tab", title: t("commandPalette.nextTab"), icon: "⇥", shortcut: "Ctrl+Shift+]", action: () => api.selectNextTab() },
      { id: "prev-tab", title: t("commandPalette.previousTab"), icon: "⇤", shortcut: "Ctrl+Shift+[", action: () => api.selectPreviousTab() },
      { id: "next-project", title: t("commandPalette.nextProject"), icon: "⇉", shortcut: "Ctrl+Alt+]", action: () => api.selectNextProject() },
      { id: "prev-project", title: t("commandPalette.previousProject"), icon: "⇇", shortcut: "Ctrl+Alt+[", action: () => api.selectPreviousProject() },
      ...Array.from({ length: 9 }, (_, i) => ({
        id: `switch-project-${i + 1}`,
        title: t("commandPalette.switchToProject", { n: i + 1 }),
        icon: `${i + 1}`,
        shortcut: `Ctrl+${i + 1}`,
        action: () => api.selectProjectByIndex(i),
      })),
      { id: "reopen-tab", title: t("commandPalette.reopenClosedTab"), icon: "↺", shortcut: "Ctrl+Shift+T", action: onReopenClosed },
      { id: "open-settings", title: t("commandPalette.openSettings"), icon: "⚙", shortcut: "Ctrl+,", action: onOpenSettings },
      { id: "keyboard-shortcuts", title: t("shortcuts.title"), icon: "⌨", shortcut: "Ctrl+/", action: onOpenShortcuts },
      { id: "open-usage", title: t("commandPalette.openUsage"), icon: "▤", shortcut: "Ctrl+Shift+U", action: onOpenUsage },
    ],
    [onAskNewProject, onClearTerminal, onCloseProject, onOpenSettings, onOpenShortcuts, onOpenUsage, onOpenSearch, onReopenClosed, t]
  );

  const fileItems: CommandItem[] = useMemo(() => {
    const items = (projectFiles ?? []).slice(0, 1000).map((rel, i) => ({
      id: `file-${i}`,
      title: rel,
      icon: fileIcon,
      action: () => openProjectFile(rel),
    }));
    return items;
  }, [projectFiles, root]);

  const recentItems: CommandItem[] = useMemo(
    () =>
      recent.map((r, i) => ({
        id: `recent-${i}`,
        title: r.path,
        icon: fileIcon,
        action: () => api.openFile(r.path, false),
      })),
    [recent]
  );

  const sessionItems: CommandItem[] = sessions.map((s) => ({
    id: `session-${s.id}`,
    title: s.title,
    icon: ">",
    action: () => {
      api.selectProject(s.project_id);
    },
  }));

  const filtered = useMemo(() => {
    const all = [...recentItems, ...fileItems, ...commands, ...sessionItems];
    const matched = fuzzyFilter(all, query, (c) => c.title);
    return matched.slice(0, 60);
  }, [recentItems, fileItems, commands, sessionItems, query]);

  useEffect(() => setSelection(0), [query]);

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelection((s) => (s + 1) % Math.max(filtered.length, 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelection((s) => (s - 1 + Math.max(filtered.length, 1)) % Math.max(filtered.length, 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const picked = filtered[selection];
      if (picked) {
        picked.action();
        onClose();
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <div
      className="absolute inset-0 z-50 bg-black/15 flex justify-center items-start"
      onClick={onClose}
    >
      <div
        className="mt-20 w-[560px] max-w-[90vw] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] muster-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 h-11 flex-shrink-0 flex items-center gap-2">
          <span className="text-muster-muted flex items-center">
            <IconSearch size={15} />
          </span>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder={t("commandPalette.searchPlaceholder")}
            spellCheck={false}
            className="flex-1 bg-transparent outline-none text-[15px] text-muster-fg placeholder:text-muster-muted/60"
          />
        </div>
        <div className="border-t border-white/[0.08]" />
        <div className="max-h-80 overflow-y-auto p-1.5">
          {filtered.length === 0 && (
            <div className="text-muster-muted text-center py-6">{t("commandPalette.noMatches")}</div>
          )}
          {filtered.map((cmd, i) => (
            <button
              key={cmd.id}
              onClick={() => {
                cmd.action();
                onClose();
              }}
              onMouseEnter={() => setSelection(i)}
              className={`w-full flex items-center gap-2 px-2.5 h-7 rounded-md text-[12.5px] text-left ${
                i === selection ? "bg-white/[0.09] text-muster-fg" : "text-muster-muted"
              }`}
            >
              <span className={`ui-fs-sm flex-shrink-0 ${i === selection ? "text-muster-accent" : "text-muster-muted"}`}>
                {cmd.icon}
              </span>
              <span className="flex-1 truncate">{cmd.title}</span>
              {cmd.shortcut && <span className="ui-fs-sm text-muster-muted/80">{cmd.shortcut}</span>}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
