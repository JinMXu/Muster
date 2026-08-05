import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import Sidebar from "./components/Sidebar";
import Header from "./components/Header";
import PaneLayout from "./components/PaneLayout";
import RightSidebar from "./components/RightSidebar";
import ResizeHandle from "./components/ResizeHandle";
import CommandPalette from "./components/CommandPalette";
import TabSwitcher, { type SwitcherTab } from "./components/TabSwitcher";
import Settings, { type SettingsTab } from "./components/Settings";
import ContextMenu from "./components/ContextMenu";
import UsagePanel from "./components/UsagePanel";
import SearchPanel from "./components/SearchPanel";
import PasteWarning, { looksDangerousPaste } from "./components/PasteWarning";
import { IconTerminal } from "./components/icons";
import { api } from "./lib/invoke";
import { openMenu } from "./lib/menuStore";
import { pruneSessions, clear as clearSessionTerm, ensureListeners } from "./lib/terminalRegistry";
import { getLatestText, clearLatestText } from "./lib/fileEdits";
import { popClosedTab, pushClosedTab, filePathOf, diffMetaOf } from "./lib/recentFiles";
import { useTauriEvent } from "./hooks/useTauriEvent";
import { usePanelWidth } from "./hooks/usePanelWidth";
import { initSettings, reloadSettings, useSettings } from "./lib/settingsStore";
import { LanguageProvider, detectInitialLang, makeT, useT } from "./lib/i18n/context";
import { getFocusedSessionId } from "./lib/sessionUtils";
import { openTerminalSearch } from "./lib/terminalSearch";
import type { Lang } from "./lib/i18n/types";
import type { AppStateView, DirtyFile, PaneContentKind, ProjectView, TabView, Uuid } from "./lib/types";

/// Kind of a tab = its focused pane's content kind (first pane as fallback).
function tabKind(tab: TabView): PaneContentKind | null {
  return (
    tab.columns.flatMap((c) => c.panes).find((p) => p.id === tab.focused_pane_id)?.content.kind ??
    tab.columns[0]?.panes[0]?.content.kind ??
    null
  );
}

export default function App() {
  const [state, setStateRaw] = useTauriEvent<AppStateView | null>("state-changed", null);
  const [showPalette, setShowPalette] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  const [showUsage, setShowUsage] = useState(false);
  const [showSearch, setShowSearch] = useState(false);
  const [agentBarOpen, setAgentBarOpen] = useState(false);
  const [pasteWarning, setPasteWarning] = useState<{ text: string; sessionId: string } | null>(null);
  // Resizable sidebar widths (drag handles between the panels and main).
  const leftPanel = usePanelWidth("muster:leftSidebarWidth", 224, "left");
  const rightPanel = usePanelWidth("muster:rightSidebarWidth", 256, "right");

  // Settings (theme, fonts, editor options) live in settingsStore: loaded
  // once here, applied there, reloaded when the Settings modal closes.
  useEffect(() => {
    api.state().then((view) => setStateRaw(view)).catch(() => {});
    initSettings();
    // Register the pty:data / pty:exit listeners, then start restored
    // sessions' read pumps only AFTER the listeners are actually registered
    // (the IPC round-trip completes). Starting them earlier drops their
    // initial output — restored sessions would show a blank terminal.
    ensureListeners().then(() => invoke('init_read_loops'));
  }, [setStateRaw]);

  const savedSettings = useSettings();
  const sysLang = useMemo(() => detectInitialLang(), []);
  const [lang, setLang] = useState<Lang>(sysLang);
  // When settings load, override system detection with a saved preference.
  useEffect(() => {
    if (!savedSettings) return;
    const sl = savedSettings.language as string;
    if (sl === "en" || sl === "zh") setLang(sl);
  }, [savedSettings]);

  // App renders the LanguageProvider below, so it cannot consume it with
  // useT() (that would return the default identity t, printing raw keys).
  // Build t directly from the same lang state the provider receives.
  const t = useMemo(() => makeT(lang), [lang]);

  const stateView = state ?? null;
  const selectedProject = useMemo(
    () => stateView?.projects.find((p) => p.id === stateView.selected_project_id) ?? null,
    [stateView]
  );

  // Ctrl+Tab switcher subtitles, cached per tab id (see the switcher below).
  const subtitleCache = useRef(new Map<Uuid, string>());
  // Most-recently-used tab order (front = most recent) for the Ctrl+Tab
  // switcher. Updated on every tab activation; closed tabs are pruned below.
  const mruRef = useRef<Uuid[]>([]);

  // Dispose parked terminal instances whose session no longer exists
  // (tab/pane/project closed). Terminals for live sessions stay parked in
  // the registry so their buffers survive tab/zoom/project switches.
  // Also records tabs that just disappeared (Ctrl+Shift+T reopen stack) and
  // prunes the switcher subtitle cache. `stateRef.current` still holds the
  // previous state here because the ref-update effect below runs later.
  useEffect(() => {
    if (!stateView) return;
    if (stateRef.current) {
      const nextTabs = new Map<string, TabView>();
      for (const p of stateView.projects) {
        for (const t of p.tabs) nextTabs.set(t.id, t);
      }
      for (const p of stateRef.current.projects) {
        // Tabs that vanished because their whole project closed are not
        // reopenable in a meaningful place — skip them.
        const projectGone = !stateView.projects.some((np) => np.id === p.id);
        for (const t of p.tabs) {
          if (nextTabs.has(t.id)) continue;
          if (projectGone) continue;
          const content =
            t.columns.flatMap((c) => c.panes).find((pn) => pn.id === t.focused_pane_id)?.content ??
            t.columns[0]?.panes[0]?.content ??
            null;
          if (!content) continue;
          if (content.kind === "file") {
            const path = filePathOf(content.id);
            if (path) pushClosedTab({ projectId: p.id, content: { kind: "file", path } });
          } else if (content.kind === "diff") {
            const meta = diffMetaOf(content.id);
            if (meta) {
              pushClosedTab({
                projectId: p.id,
                content: {
                  kind: "diff",
                  repoRoot: meta.repoRoot,
                  path: meta.path,
                  staged: meta.staged,
                  oldRev: meta.oldRev ?? undefined,
                  newRev: meta.newRev ?? undefined,
                  workdir: meta.workdir,
                },
              });
            }
          } else if (content.kind === "session") {
            pushClosedTab({ projectId: p.id, content: { kind: "session" } });
          }
        }
      }
    }
    const ids = new Set<string>();
    const tabIds = new Set<string>();
    for (const p of stateView.projects) {
      for (const t of p.tabs) {
        tabIds.add(t.id);
        for (const c of t.columns) {
          for (const pane of c.panes) {
            if (pane.content.kind === "session") ids.add(pane.content.id);
          }
        }
      }
    }
    pruneSessions(ids);
    // Drop switcher-subtitle cache entries for tabs that no longer exist.
    for (const id of [...subtitleCache.current.keys()]) {
      if (!tabIds.has(id)) subtitleCache.current.delete(id);
    }
    // Same for the MRU list.
    mruRef.current = mruRef.current.filter((id) => tabIds.has(id));
  }, [stateView]);

  // Track tab recency: every activation moves the tab to the front of the
  // MRU list. `tab-focused` patches selected_tab_id into state, so pane-only
  // state updates don't retrigger this.
  const activeTabId = selectedProject?.selected_tab_id ?? null;
  useEffect(() => {
    if (!activeTabId) return;
    const mru = mruRef.current;
    const i = mru.indexOf(activeTabId);
    if (i !== -1) mru.splice(i, 1);
    mru.unshift(activeTabId);
  }, [activeTabId]);

  const refresh = useCallback(() => api.state().then(setStateRaw), [setStateRaw]);

  // Ref holding the latest state so action callbacks can be stable (not
  // re-created on every state-changed event) and the global keyboard listener
  // doesn't tear down + re-add on every keystroke-driven state update.
  const stateRef = useRef<AppStateView | null>(null);
  useEffect(() => { stateRef.current = stateView; }, [stateView]);

  // Narrow `tab-focused` event: the backend sends just the tab UUID on tab
  // switches (the most frequent state mutation) instead of a full
  // `state-changed` broadcast. Patch `selected_tab_id` locally - the frontend
  // already has all tab data in its state; it only needs to know which tab is
  // now active. A full `state-changed` from other operations (open/close/split)
  // still arrives and authoritatively replaces this patched state.
  useEffect(() => {
    const unlisten = listen<Uuid>("tab-focused", (event) => {
      const prev = stateRef.current;
      if (!prev || !prev.selected_project_id) return;
      setStateRaw({
        ...prev,
        projects: prev.projects.map((p) =>
          p.id === prev.selected_project_id
            ? { ...p, selected_tab_id: event.payload }
            : p,
        ),
      });
    });
    return () => { unlisten.then((u) => u()); };
  }, [setStateRaw]);

  // ---- action helpers ---------------------------------------------------
  const newProjectWithDialog = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir === "string") await api.newProject(dir);
    else await api.newProject();
  }, []);

  const newProjectBlank = useCallback(() => api.newProject(), []);

  const newSession = useCallback(async () => {
    await api.spawnSession();
  }, []);

  const [closePrompt, setClosePrompt] = useState<{ files: DirtyFile[]; proceed: () => void } | null>(null);

  // Close one specific tab, with the unsaved-files confirmation. The backend
  // `close_tab` command closes the given tab atomically (no select-then-close).
  const closeTabById = useCallback(async (tabId: Uuid) => {
    const proceed = () => {
      api.closeTab(tabId);
    };
    const dirty = await api.tabDirtyFiles(tabId);
    if (dirty.length > 0) {
      setClosePrompt({ files: dirty, proceed });
      return;
    }
    proceed();
  }, []);

  const closeTab = useCallback(async () => {
    const s = stateRef.current;
    const tabId =
      s?.projects.find((p) => p.id === s.selected_project_id)?.selected_tab_id ?? null;
    if (tabId) closeTabById(tabId);
  }, [closeTabById]);

  // Dirty files across several tabs, deduped by file id — a file open in two
  // of the closing tabs should only appear once in the confirmation.
  const dirtyFilesForTabs = useCallback(async (tabIds: Uuid[]) => {
    const seen = new Set<Uuid>();
    const files: DirtyFile[] = [];
    for (const tabId of tabIds) {
      for (const f of await api.tabDirtyFiles(tabId)) {
        if (!seen.has(f.id)) {
          seen.add(f.id);
          files.push(f);
        }
      }
    }
    return files;
  }, []);

  const closeTabsWithPrompt = useCallback(
    async (affected: Uuid[], proceed: () => void) => {
      const dirty = await dirtyFilesForTabs(affected);
      if (dirty.length > 0) {
        setClosePrompt({ files: dirty, proceed });
        return;
      }
      proceed();
    },
    [dirtyFilesForTabs]
  );

  const tabsOfProjectWith = useCallback(
    (tabId: Uuid) => stateRef.current?.projects.find((p) => p.tabs.some((t) => t.id === tabId))?.tabs ?? [],
    []
  );

  const closeOtherTabs = useCallback(
    (tabId: Uuid) => {
      const others = tabsOfProjectWith(tabId).filter((t) => t.id !== tabId).map((t) => t.id);
      closeTabsWithPrompt(others, () => api.closeOtherTabs(tabId));
    },
    [tabsOfProjectWith, closeTabsWithPrompt]
  );

  const closeTabsToRight = useCallback(
    (tabId: Uuid) => {
      const tabs = tabsOfProjectWith(tabId);
      const pos = tabs.findIndex((t) => t.id === tabId);
      const right = pos >= 0 ? tabs.slice(pos + 1).map((t) => t.id) : [];
      closeTabsWithPrompt(right, () => api.closeTabsToRight(tabId));
    },
    [tabsOfProjectWith, closeTabsWithPrompt]
  );

  const closeAllTabs = useCallback(() => {
    const tabs = selectedProject?.tabs.map((t) => t.id) ?? [];
    closeTabsWithPrompt(tabs, () => api.closeAllTabs());
  }, [selectedProject, closeTabsWithPrompt]);

  // Close every tab of one kind ("file" / "diff") in the clicked tab's
  // project, with the same unsaved-changes confirmation as the other batch
  // close actions.
  const closeTabsByKind = useCallback(
    (anchorTabId: Uuid, kind: PaneContentKind) => {
      const ids = tabsOfProjectWith(anchorTabId)
        .filter((t) => tabKind(t) === kind)
        .map((t) => t.id);
      if (ids.length === 0) return;
      closeTabsWithPrompt(ids, () => ids.forEach((id) => api.closeTab(id)));
    },
    [tabsOfProjectWith, closeTabsWithPrompt]
  );

  const closeProject = useCallback(async (id: Uuid) => {
    const dirty = await api.projectDirtyFiles(id);
    if (dirty.length > 0) {
      setClosePrompt({ files: dirty, proceed: () => api.closeProject(id) });
      return;
    }
    api.closeProject(id);
  }, []);

  const closeSelectedProject = useCallback(() => {
    const sid = stateRef.current?.selected_project_id;
    if (sid) closeProject(sid);
  }, [closeProject]);

  const split = useCallback((edge: "left" | "right" | "top" | "bottom") => api.split(edge), []);

  const saveFile = useCallback(() => api.saveSelectedFile(), []);

  const clearTerminal = useCallback(() => {
    const s = stateRef.current;
    const project = s?.projects.find((p) => p.id === s.selected_project_id);
    const tab = project?.tabs.find((t) => t.id === project.selected_tab_id);
    const pane = tab?.columns.flatMap((c) => c.panes).find((p) => p.id === tab.focused_pane_id);
    if (pane?.content.kind !== "session") return;
    const sessionId = pane.content.id;
    // Wipe the local xterm buffer, then have the shell clear its own screen.
    clearSessionTerm(sessionId);
    api.clearTerminal(sessionId);
  }, []);

  // Ctrl+Shift+T: reopen the last closed tab. Files/diffs reopen their path;
  // a closed terminal session spawns a fresh one in the project (the old
  // PTY is gone and can't be revived).
  const reopenClosedTab = useCallback(() => {
    const tab = popClosedTab();
    if (!tab) return;
    const c = tab.content;
    if (c.kind === "file") {
      api.openFile(c.path, false);
    } else if (c.kind === "diff") {
      if (c.workdir) {
        if (c.oldRev) api.openCheckpointDiff(c.repoRoot, c.path, c.oldRev);
        else api.openWorkdirDiff(c.repoRoot, c.path);
      } else if (c.oldRev && c.newRev) {
        api.openCommitDiff(c.repoRoot, c.path, c.oldRev, c.newRev);
      } else {
        api.openDiff(c.repoRoot, c.path, c.staged ?? false);
      }
    } else {
      api.spawnSession();
    }
  }, []);

  // ---- keyboard shortcuts ----------------------------------------------
  useEffect(() => {
    const NAV_MAP: Record<string, () => void> = {
      "ctrl+n": newProjectWithDialog,
      "ctrl+shift+n": () => api.newWindow(),
      "ctrl+t": newSession,
      "ctrl+w": closeTab,
      "ctrl+p": () => setShowPalette(true),
      "ctrl+/": () => { setSettingsTab("shortcuts"); setShowSettings(true); },
      "ctrl+b": () => api.toggleLeftSidebar(),
      "ctrl+shift+b": () => api.toggleRightPanel(),
      "ctrl+shift+e": () => api.togglePanel("files"),
      "ctrl+shift+g": () => api.togglePanel("git"),
      "ctrl+shift+i": () => api.togglePanel("info"),
      "ctrl+d": () => split("right"),
      "ctrl+shift+d": () => split("bottom"),
      "ctrl+[": () => api.focusPane("previous"),
      "ctrl+]": () => api.focusPane("next"),
      "ctrl+shift+enter": () => api.togglePaneZoom(),
      "ctrl+shift+[": () => api.selectPreviousTab(),
      "ctrl+shift+]": () => api.selectNextTab(),
      "ctrl+alt+]": () => api.selectNextProject(),
      "ctrl+alt+[": () => api.selectPreviousProject(),
      "ctrl+1": () => api.selectProjectByIndex(0),
      "ctrl+2": () => api.selectProjectByIndex(1),
      "ctrl+3": () => api.selectProjectByIndex(2),
      "ctrl+4": () => api.selectProjectByIndex(3),
      "ctrl+5": () => api.selectProjectByIndex(4),
      "ctrl+6": () => api.selectProjectByIndex(5),
      "ctrl+7": () => api.selectProjectByIndex(6),
      "ctrl+8": () => api.selectProjectByIndex(7),
      "ctrl+9": () => api.selectProjectByIndex(8),
      "ctrl+alt+arrowleft": () => api.focusPane("left"),
      "ctrl+alt+arrowright": () => api.focusPane("right"),
      "ctrl+alt+arrowup": () => api.focusPane("up"),
      "ctrl+alt+arrowdown": () => api.focusPane("down"),
      "ctrl+alt+shift+arrowleft": () => api.resizePane("left"),
      "ctrl+alt+shift+arrowright": () => api.resizePane("right"),
      "ctrl+alt+shift+arrowup": () => api.resizePane("up"),
      "ctrl+alt+shift+arrowdown": () => api.resizePane("down"),
      "ctrl+s": saveFile,
      "ctrl+k": clearTerminal,
      "ctrl+shift+t": reopenClosedTab,
      "ctrl+,": () => { setSettingsTab("general"); setShowSettings(true); },
      "ctrl+shift+u": () => setShowUsage(true),
      "ctrl+shift+a": () => setAgentBarOpen((v) => !v),
    };
    const onKey = (e: KeyboardEvent) => {
      const parts: string[] = [];
      if (e.ctrlKey || e.metaKey) parts.push("ctrl"); // treat Cmd like Ctrl on macOS
      if (e.altKey) parts.push("alt");
      if (e.shiftKey) parts.push("shift");
      parts.push(e.key.toLowerCase());
      const key = parts.join("+");
      const target = e.target as HTMLElement | null;
      // Don't intercept text inputs (rename fields, find bar, message boxes) —
      // except combos that never produce text and must also work from a
      // terminal (project switching, pane focus/resize arrows).
      if (
        target &&
        (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) &&
        !/^ctrl\+([0-9]|alt\+)/.test(key)
      ) {
        return;
      }
      const handler = NAV_MAP[key];
      if (handler) {
        e.preventDefault();
        e.stopPropagation();
        handler();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [newProjectWithDialog, newSession, closeTab, split, saveFile, clearTerminal, reopenClosedTab]);

  // ---- search shortcuts -----------------------------------------------
  // Ctrl+Shift+F opens the project search overlay; Ctrl+F opens scrollback
  // search when a terminal pane is focused (otherwise Monaco keeps its own
  // find). Both use a dedicated capture listener because the NAV_MAP handler
  // skips INPUT/TEXTAREA targets — xterm keeps a focused textarea and these
  // must work from the terminal too.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      if (!ctrl || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key !== "f") return;
      const target = e.target as HTMLElement | null;
      // The terminal's hidden textarea is inside [data-terminal-pane]; every
      // other input (Monaco's textarea, search/rename fields) keeps its own
      // Ctrl+F. Intercept only inside the terminal.
      const inTerminal = !!target?.closest("[data-terminal-pane]");
      if (!e.shiftKey) {
        if (!inTerminal) return;
        const sid = getFocusedSessionId(stateRef.current);
        if (!sid) return;
        e.preventDefault();
        e.stopPropagation();
        openTerminalSearch(sid);
        return;
      }
      // Ctrl+Shift+F works from anywhere (including Monaco/inputs).
      e.preventDefault();
      e.stopPropagation();
      setShowSearch(true);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  // ---- global paste interception (paste protection) ---------------------
  // Intercept paste events in terminal panes and warn before sending
  // text that looks like executable commands to the PTY.
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (!target) return;
      // Check if we're inside a terminal container
      const termContainer = target.closest("[data-terminal-pane]") as HTMLElement | null;
      if (!termContainer) return;
      const text = e.clipboardData?.getData("text/plain") ?? "";
      if (!text || !looksDangerousPaste(text)) return;
      e.preventDefault();
      e.stopPropagation();
      const sid = termContainer.dataset.terminalPane ?? "";
      setPasteWarning({ text, sessionId: sid });
    };
    document.addEventListener("paste", onPaste, true);
    return () => document.removeEventListener("paste", onPaste, true);
  }, []);

  // ---- Ctrl+Tab switcher -------------------------------------------------
  // Separate capture-phase listeners: the NAV_MAP handler skips INPUT/TEXTAREA
  // targets (xterm keeps a focused textarea), but Ctrl+Tab must work from a
  // terminal too. The live list/index live in a ref so rapid cycling doesn't
  // depend on re-renders; switcherView mirrors it for rendering.
  const [switcherView, setSwitcherView] = useState<{ tabs: SwitcherTab[]; index: number } | null>(null);
  const switcherRef = useRef<{ tabs: SwitcherTab[]; index: number } | null>(null);

  useEffect(() => {
    const setSwitcher = (v: { tabs: SwitcherTab[]; index: number } | null) => {
      switcherRef.current = v;
      setSwitcherView(v);
    };
    const focusedContent = (t: TabView) =>
      t.columns.flatMap((c) => c.panes).find((p) => p.id === t.focused_pane_id)?.content ??
      t.columns[0]?.panes[0]?.content ??
      null;
    const toSwitcherTab = (tab: TabView): SwitcherTab => ({
      id: tab.id,
      title: tab.custom_name ?? tab.display_title ?? t("app.untitled"),
      subtitle: subtitleCache.current.get(tab.id) ?? null,
      kind: focusedContent(tab)?.kind ?? null,
    });
    // Resolve subtitles (session cwd / file path / diff path) lazily, cached
    // per tab id. The overlay shows titles until the promises settle.
    const resolveSubtitles = (tabs: TabView[]) => {
      Promise.all(
        tabs.map(async (t) => {
          if (subtitleCache.current.has(t.id)) return;
          const content = focusedContent(t);
          if (!content) return;
          try {
            let sub: string | null = null;
            if (content.kind === "session")
              sub = (await api.sessionInfo(content.id))?.working_directory ?? null;
            else if (content.kind === "file")
              sub = (await api.fileInfo(content.id))?.path ?? null;
            else sub = (await api.diffInfo(content.id))?.path ?? null;
            if (sub) subtitleCache.current.set(t.id, sub);
          } catch {
            // content vanished meanwhile — leave the subtitle unresolved
          }
        })
      ).then(() => {
        const cur = switcherRef.current;
        if (!cur) return;
        setSwitcher({
          ...cur,
          tabs: cur.tabs.map((t) => ({ ...t, subtitle: subtitleCache.current.get(t.id) ?? t.subtitle })),
        });
      });
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Tab" && e.ctrlKey) {
        // Always swallow — even inside xterm's textarea — so the WebView
        // doesn't move focus instead of switching tabs.
        e.preventDefault();
        e.stopPropagation();
        const step = e.shiftKey ? -1 : 1;
        const cur = switcherRef.current;
        if (cur) {
          setSwitcher({ ...cur, index: (cur.index + step + cur.tabs.length) % cur.tabs.length });
          return;
        }
        const s = stateRef.current;
        const project = s?.projects.find((p) => p.id === s.selected_project_id);
        const tabs = project?.tabs ?? [];
        if (!project || tabs.length < 2) return; // nothing to switch between
        // Order by recency (MRU): the active tab comes first, never-activated
        // tabs (missing from the list) fall back to tab order at the end.
        const mru = mruRef.current;
        const ordered = [...tabs].sort(
          (a, b) =>
            (mru.indexOf(a.id) < 0 ? mru.length : mru.indexOf(a.id)) -
            (mru.indexOf(b.id) < 0 ? mru.length : mru.indexOf(b.id))
        );
        // Forward starts at the previously used tab, backward at the oldest.
        const index = step === 1 ? 1 : ordered.length - 1;
        setSwitcher({ tabs: ordered.map(toSwitcherTab), index });
        resolveSubtitles(ordered);
        return;
      }
      if (e.key === "Escape" && switcherRef.current) {
        e.preventDefault();
        e.stopPropagation();
        setSwitcher(null); // close without switching
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      // Releasing Ctrl commits the highlighted tab and closes the overlay.
      if (e.key === "Control" && switcherRef.current) {
        const cur = switcherRef.current;
        setSwitcher(null);
        const target = cur.tabs[cur.index];
        if (target) api.selectTab(target.id);
      }
    };
    const onBlur = () => {
      // Focus left the window (e.g. Alt+Tab) — the Ctrl keyup may never
      // arrive; close without committing rather than leaving a stale overlay.
      if (switcherRef.current) setSwitcher(null);
    };
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  // Close palette / settings on Escape handled by their own components.

  // Keep the window's zoom on double-click (handled natively), and make the
  // title bar draggable via the class `.drag-region` — Tauri picks up the
  // `data-tauri-drag-region` attribute automatically.

  const selectProject = (id: Uuid) => api.selectProject(id);
  const moveProject = (from: Uuid, to: Uuid) => api.moveProject(from, to);

  // ---- context menus -------------------------------------------------------
  const tabMenu = useCallback(
    (tab: TabView, x: number, y: number, requestRename: () => void) => {
      // Reveal/Copy Path resolve against the tab's focused pane at click time.
      const contextPath = () => api.paneContextPath(tab.id, tab.focused_pane_id);
      // Batch-close entries only appear when the project has a tab of that kind.
      const kinds = new Set(tabsOfProjectWith(tab.id).map(tabKind));
      openMenu({
        x,
        y,
        items: [
          { label: t("common.rename"), action: requestRename },
          { label: t("app.useAutomaticTitle"), action: () => api.renameTab(tab.id, null) },
          "sep",
          {
            label: t("common.revealInExplorer"),
            action: async () => {
              const p = await contextPath();
              if (p) revealItemInDir(p);
            },
          },
          {
            label: t("common.copyPath"),
            action: async () => {
              const p = await contextPath();
              if (p) navigator.clipboard.writeText(p);
            },
          },
          "sep",
          { label: t("common.close"), action: () => closeTabById(tab.id) },
          { label: t("app.closeOthers"), action: () => closeOtherTabs(tab.id) },
          { label: t("app.closeToRight"), action: () => closeTabsToRight(tab.id) },
          ...(kinds.has("file")
            ? [{ label: t("app.closeAllFiles"), action: () => closeTabsByKind(tab.id, "file") }]
            : []),
          ...(kinds.has("diff")
            ? [{ label: t("app.closeAllDiffs"), action: () => closeTabsByKind(tab.id, "diff") }]
            : []),
          { label: t("app.closeAll"), action: () => closeAllTabs() },
        ],
      });
    },
    [closeTabById, closeOtherTabs, closeTabsToRight, closeTabsByKind, closeAllTabs, tabsOfProjectWith, t]
  );

  const projectMenu = useCallback(
    (project: ProjectView, x: number, y: number, requestRename: () => void) => {
      openMenu({
        x,
        y,
        items: [
          { label: t("common.rename"), action: requestRename },
          { label: t("app.useAutomaticTitle"), action: () => api.renameProject(project.id, null) },
          "sep",
          {
            label: t("app.setProjectDirectory"),
            action: async () => {
              const dir = await openDialog({ directory: true, multiple: false });
              if (typeof dir === "string") api.setProjectDirectory(project.id, dir);
            },
          },
          {
            label: t("app.useAutomaticDirectory"),
            action: () => api.setProjectDirectory(project.id, null),
          },
          "sep",
          { label: t("app.closeProject"), danger: true, action: () => closeProject(project.id) },
        ],
      });
    },
    [closeProject, t]
  );

  return (
    <LanguageProvider lang={lang}>
    <div className="h-screen w-screen flex flex-col bg-muster-float text-muster-fg overflow-hidden">
      <div className="flex-1 flex min-h-0">
        {stateView?.is_left_sidebar_visible && (
          <>
            <Sidebar
              projects={stateView?.projects ?? []}
              selected={stateView?.selected_project_id ?? null}
              onSelect={selectProject}
              onClose={(id) => closeProject(id)}
              onNewProject={newProjectBlank}
              onMove={moveProject}
              onRename={(id, name) => api.renameProject(id, name)}
              onProjectMenu={projectMenu}
              onOpenSettings={() => { setSettingsTab("general"); setShowSettings(true); }}
              width={leftPanel.width}
              agentBarOpen={agentBarOpen}
              onAgentBarToggle={() => setAgentBarOpen((v) => !v)}
              onAgentBarClose={() => setAgentBarOpen(false)}
            />
            <ResizeHandle onMouseDown={leftPanel.onHandleMouseDown} />
          </>
        )}
        <main className="flex-1 flex flex-col min-w-0">
          <Header
            project={selectedProject}
            onNewSession={newSession}
            onCloseTab={closeTabById}
            onSelectTab={(id) => api.selectTab(id)}
            onMoveTab={(from, to) => api.moveTab(from, to)}
            onRenameTab={(id, name) => api.renameTab(id, name)}
            onTabMenu={tabMenu}
            togglePanel={() => api.toggleRightPanel()}
            panelVisible={stateView?.is_panel_visible ?? false}
            isPaneZoomed={stateView?.is_pane_zoomed ?? false}
            onExitZoom={() => api.togglePaneZoom()}
          />
          <div className="flex-1 min-h-0 relative">
            {selectedProject && selectedProject.tabs.length > 0 ? (
              <PaneLayout project={selectedProject} />
            ) : (
              <EmptyState onNewProject={newProjectWithDialog} onNewSession={newSession} hasProject={!!selectedProject} />
            )}
          </div>
        </main>
        {(stateView?.is_panel_visible ?? false) && (
          <>
            <ResizeHandle onMouseDown={rightPanel.onHandleMouseDown} />
            <RightSidebar state={stateView} width={rightPanel.width} />
          </>
        )}
      </div>

      {showPalette && (
        <CommandPalette
          state={stateView}
          onClose={() => setShowPalette(false)}
          onAskNewProject={newProjectWithDialog}
          onClearTerminal={clearTerminal}
          onCloseProject={closeSelectedProject}
          onOpenSettings={() => { setSettingsTab("general"); setShowSettings(true); }}
          onOpenShortcuts={() => { setSettingsTab("shortcuts"); setShowSettings(true); }}
          onOpenUsage={() => setShowUsage(true)}
          onOpenSearch={() => setShowSearch(true)}
          onReopenClosed={reopenClosedTab}
        />
      )}
      {switcherView && (
        <TabSwitcher
          tabs={switcherView.tabs}
          index={switcherView.index}
          onSelect={(id) => {
            switcherRef.current = null;
            setSwitcherView(null);
            api.selectTab(id);
          }}
          onClose={() => {
            switcherRef.current = null;
            setSwitcherView(null);
          }}
        />
      )}
      {showSettings && (
        <Settings
          initialTab={settingsTab}
          onClose={() => { setShowSettings(false); refresh(); reloadSettings(); }}
          onOpenUsage={() => { setShowSettings(false); setShowUsage(true); }}
        />
      )}
      {showUsage && <UsagePanel onClose={() => setShowUsage(false)} />}
      {showSearch && <SearchPanel state={stateView} onClose={() => setShowSearch(false)} />}
      <ContextMenu />
      {closePrompt && (
        <div
          className="absolute inset-0 z-50 bg-black/30 flex items-center justify-center"
          onClick={() => setClosePrompt(null)}
        >
          <div
            className="w-[380px] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] p-4 muster-pop"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="ui-fs-base font-medium mb-1">{t("app.unsavedChangesTitle")}</div>
            <div className="ui-fs-sm text-muster-muted mb-2">
              {closePrompt.files.length === 1
                ? t("app.unsavedChangesSingular")
                : t("app.unsavedChangesPlural", { n: closePrompt.files.length })}
            </div>
            <div className="max-h-32 overflow-y-auto mb-4 space-y-0.5">
              {closePrompt.files.map((f) => (
                <div key={f.id} className="ui-fs-base text-muster-fg/80 truncate">
                  • {f.name}
                </div>
              ))}
            </div>
            <div className="flex justify-end gap-2 ui-fs-sm">
              <button
                className="px-2.5 py-1 rounded-md text-muster-muted hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
                onClick={() => setClosePrompt(null)}
              >
                {t("common.cancel")}
              </button>
              <button
                className="px-2.5 py-1 rounded-md text-red-400 hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
                onClick={() => {
                  const p = closePrompt;
                  setClosePrompt(null);
                  p.proceed();
                }}
              >
                {t("app.dontSave")}
              </button>
              <button
                className="px-2.5 py-1 rounded-md bg-muster-accent text-white active:scale-[.97] transition-transform duration-muster ease-muster"
                onClick={async () => {
                  const p = closePrompt;
                  setClosePrompt(null);
                  await Promise.all(p.files.map((f) => {
                    const text = getLatestText(f.id);
                    if (text !== undefined) clearLatestText(f.id);
                    return api.saveFile(f.id, text);
                  }));
                  p.proceed();
                }}
              >
                {t("app.saveAndClose")}
              </button>
            </div>
          </div>
        </div>
      )}
      {pasteWarning && (
        <PasteWarning
          text={pasteWarning.text}
          onConfirm={() => {
            api.sendText(pasteWarning.sessionId, pasteWarning.text);
            setPasteWarning(null);
          }}
          onCancel={() => setPasteWarning(null)}
        />
      )}
    </div>
      </LanguageProvider>
  );
}

function EmptyState({
  hasProject,
  onNewProject,
  onNewSession,
}: {
  hasProject: boolean;
  onNewProject: () => void;
  onNewSession: () => void;
}) {
  const { t } = useT();
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 text-muster-muted">
      <IconTerminal size={32} />
      <div className="ui-fs-base">{hasProject ? t("app.noOpenSessions") : t("app.noOpenProjects")}</div>
      <button
        className="px-3 py-1.5 rounded-md bg-muster-accent text-white ui-fs-base"
        onClick={hasProject ? onNewSession : onNewProject}
      >
        {hasProject ? t("app.newSessionHint") : t("app.newProjectHint")}
      </button>
    </div>
  );
}
