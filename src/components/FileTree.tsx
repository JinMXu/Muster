import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { events } from "../lib/bindings";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { AppStateView, DirEntry, GitStatusEntry } from "../lib/types";
import { api } from "../lib/invoke";
import { useGitStatus } from "../lib/gitStatusStore";
import { useT } from "../lib/i18n/context";
import { useProjectCwd } from "../lib/useProjectCwd";
import { shellQuotePath } from "../lib/shellEscape";
import { getFocusedSessionId, getFocusedFileId } from "../lib/sessionUtils";
import { openMenu, type MenuEntry } from "../lib/menuStore";
import FileIcon from "./FileIcon";
import {
  IconChevronDown,
  IconChevronRight,
  IconFile,
  IconFolder,
  IconFolderPlus,
  IconPencil,
  IconPlus,
  IconTrash,
} from "./icons";

interface Node {
  name: string;
  path: string;
  is_directory: boolean;
  depth: number;
  /// Inline "new file/folder" draft row, rendered at the top of a dir.
  draft?: boolean;
}

/// Right-edge decoration for a tree row: a git status letter with its color.
interface GitBadge {
  letter: string;
  className: string;
}

/// Letter + color for one status entry, mirroring GitPanel's letter pick
/// (staged when present, else unstaged). M/R/... amber, A/? green,
/// D/conflict red.
function gitBadge(e: GitStatusEntry): GitBadge {
  const letter = e.staged !== "." && e.staged !== "?" ? e.staged : e.unstaged;
  const className =
    e.is_conflict || letter === "D"
      ? "text-red-400"
      : letter === "A" || letter === "?"
        ? "text-green-400"
        : "text-amber-400";
  return { letter, className };
}

/// File tree with lazy directory expansion, inline rename, new file/folder,
/// move-to-trash, a right-click context menu, and auto-refresh on external
/// changes (backend watches every loaded directory and emits `fs-changed`).
/// Loaded children are cached per directory path; the root listing lives
/// under the project directory key.
export default function FileTree({ state, active }: { state: AppStateView | null; active: boolean }) {
  const project = state?.projects.find((p) => p.id === state.selected_project_id) ?? null;
  // Panels anchor to the pinned directory or the containing repo's toplevel,
  // so `cd` inside a repo does not reset the tree.
  const { root } = useProjectCwd(state);
  const cwd = root ?? "";
  const [children, setChildren] = useState<Record<string, DirEntry[]>>({});
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState<{ parentDir: string; isDirectory: boolean } | null>(null);
  const { t } = useT();
  // Long-lived handlers below are passed to memoized rows; refs keep the
  // closures current without re-creating (and re-rendering) every row.
  const tRef = useRef(t);
  tRef.current = t;
  const childrenRef = useRef(children);
  childrenRef.current = children;
  const expandedRef = useRef(expanded);
  expandedRef.current = expanded;

  // Git status decorations from the shared poller (gitStatusStore); null
  // until the first fetch resolves or when the project is not a repo. The
  // hidden Files tab drops to the store's slow cadence.
  const gitInfo = useGitStatus(root, active);

  // Decorations keyed by lowercased absolute path (Windows is case-
  // insensitive): files get a status letter, directories containing any
  // changed path get a dot marker. Paths under the repo root only.
  const { badges, dirtyDirs } = useMemo(() => {
    const badges = new Map<string, GitBadge>();
    const dirtyDirs = new Set<string>();
    if (gitInfo?.is_repo) {
      const repoRoot = gitInfo.repo_root.replace(/[\\/]+$/, "");
      for (const list of [gitInfo.merge_entries, gitInfo.staged_entries, gitInfo.changed_entries]) {
        for (const e of list) {
          const abs = `${repoRoot}\\${e.path.replace(/\//g, "\\")}`;
          const key = abs.toLowerCase();
          if (!badges.has(key)) badges.set(key, gitBadge(e));
          let dir = abs;
          while (dir.length > repoRoot.length) {
            const idx = dir.lastIndexOf("\\");
            if (idx <= repoRoot.length) break;
            dir = dir.slice(0, idx);
            dirtyDirs.add(dir.toLowerCase());
          }
        }
      }
    }
    return { badges, dirtyDirs };
  }, [gitInfo]);

  const loadDir = useCallback(async (dir: string) => {
    try {
      const entries = await api.listDirectory(dir);
      setChildren((prev) => ({ ...prev, [dir]: entries }));
    } catch {
      // Directory may have been deleted; drop its cache entry.
      setChildren((prev) => {
        const next = { ...prev };
        delete next[dir];
        return next;
      });
    }
  }, []);

  // Reset everything when the project directory changes.
  useEffect(() => {
    setChildren({});
    setExpanded({});
    setSelected(null);
    setRenaming(null);
    setDraft(null);
    if (cwd) loadDir(cwd);
  }, [cwd, loadDir]);

  // Keep the backend's watch set in sync with the directories we display
  // (root + everything loaded/expanded). Watching is non-recursive, so only
  // loaded dirs are sent; the key dedupe avoids re-sending after reloads.
  const watchedKeyRef = useRef<string>("");
  useEffect(() => {
    const dirs = Object.keys(children).sort();
    const key = dirs.join("\n");
    if (key === watchedKeyRef.current) return;
    watchedKeyRef.current = key;
    api.watchDirectories(dirs);
  }, [children]);

  // Mirror of the loaded-dir set for the fs-changed listener (which would
  // otherwise capture stale state).
  const loadedRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    loadedRef.current = new Set(Object.keys(children));
  }, [children]);

  // Auto-refresh a directory when it changes on disk outside the app.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    // Async listen vs. unmount race: if cleanup ran before the promise
    // resolved, unlisten the moment the handle arrives.
    let cancelled = false;
    const lastReload = new Map<string, number>();
    events.fsChanged.listen((e) => {
      const dir = e.payload.dir;
      if (!loadedRef.current.has(dir)) return;
      // The backend already debounces per dir; this guards against bursts.
      const now = Date.now();
      if (now - (lastReload.get(dir) ?? 0) < 300) return;
      lastReload.set(dir, now);
      loadDir(dir);
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadDir]);

  // Re-list every directory that is currently loaded (after mutations).
  const refresh = useCallback(() => {
    for (const dir of Object.keys(childrenRef.current)) void loadDir(dir);
  }, [loadDir]);

  const toggle = async (node: Node) => {
    if (!node.is_directory) return;
    if (expandedRef.current[node.path]) {
      setExpanded((prev) => ({ ...prev, [node.path]: false }));
    } else {
      if (!childrenRef.current[node.path]) await loadDir(node.path);
      setExpanded((prev) => (prev[node.path] ? prev : { ...prev, [node.path]: true }));
    }
  };

  const onOpenFile = (path: string) => {
    setSelected(path);
    api.openFile(path, false);
  };

  // Insert an inline draft row at the top of the target directory (VS Code
  // style): the directory is expanded and its children loaded first so the
  // draft row is visible. Enter/blur commits, Escape cancels.
  const startCreate = async (parentDir: string, isDirectory: boolean) => {
    if (!parentDir) return;
    if (!childrenRef.current[parentDir]) await loadDir(parentDir);
    setExpanded((prev) => ({ ...prev, [parentDir]: true }));
    setRenaming(null);
    setDraft({ parentDir, isDirectory });
  };

  const commitDraft = async (name: string) => {
    const d = draft;
    setDraft(null);
    if (!d) return;
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      const created = await api.createFile(d.parentDir, trimmed, d.isDirectory);
      await loadDir(d.parentDir);
      if (!d.isDirectory) api.openFile(created, false);
    } catch (err) {
      window.alert(tRef.current("fileTree.createFailed", { msg: String(err) }));
    }
  };

  // Rewrite `path` after `from` was renamed to `to` (exact match, or a path
  // inside the renamed directory). Mirrors the backend's open-tab remap.
  const remapRenamedPath = (p: string, from: string, to: string) => {
    if (p === from) return to;
    if (p.startsWith(from) && (p[from.length] === "\\" || p[from.length] === "/")) {
      return to + p.slice(from.length);
    }
    return p;
  };

  // After renaming a directory, remap the tree's local state so the whole
  // subtree stays expanded and loaded under the new prefix.
  const remapTreeState = (from: string, to: string) => {
    const remapRecord = <T,>(rec: Record<string, T>) =>
      Object.fromEntries(Object.entries(rec).map(([k, v]) => [remapRenamedPath(k, from, to), v]));
    setChildren((prev) => remapRecord(prev));
    setExpanded((prev) => remapRecord(prev));
    setSelected((prev) => (prev ? remapRenamedPath(prev, from, to) : prev));
  };

  const onRename = async (node: Node, newName: string) => {
    setRenaming(null);
    const trimmed = newName.trim();
    if (!trimmed || trimmed === node.name) return;
    // Client-side mirror of the backend validation; invalid names cancel.
    if (/[\\/]/.test(trimmed) || trimmed === "." || trimmed === "..") return;
    if (!node.path.endsWith(node.name)) return;
    const to = node.path.slice(0, node.path.length - node.name.length) + trimmed;
    try {
      await api.renamePath(node.path, to);
      remapTreeState(node.path, to);
      refresh();
    } catch (err) {
      window.alert(tRef.current("fileTree.renameFailed", { msg: String(err) }));
    }
  };

  const onTrash = async (node: Node) => {
    if (!window.confirm(tRef.current("fileTree.trashConfirm", { name: node.name }))) return;
    try {
      await api.trashFile(node.path);
      // Drop any cached state for the removed subtree.
      setChildren((prev) => {
        const next = { ...prev };
        delete next[node.path];
        return next;
      });
      setExpanded((prev) => {
        const next = { ...prev };
        delete next[node.path];
        return next;
      });
      refresh();
    } catch (err) {
      window.alert(tRef.current("fileTree.trashFailed", { msg: String(err) }));
    }
  };

  // Focused terminal session (selected tab -> focused pane), used by the
  // "cd Here" menu item. Omitted when no terminal is focused. Kept in a ref
  // so memoized rows' context-menu closures see the current session.
  const focusedSessionId = getFocusedSessionId(state);
  const focusedSessionIdRef = useRef(focusedSessionId);
  focusedSessionIdRef.current = focusedSessionId;

  // Focused file tab (selected tab -> focused pane), used to keep the
  // tree's selection in sync with the file open in the focused pane.
  const focusedFileId = getFocusedFileId(state);

  // Resolve the focused file tab's path (cached per tab id).
  const filePathCache = useRef<Map<string, string>>(new Map());
  const [focusedFilePath, setFocusedFilePath] = useState<string | null>(null);
  useEffect(() => {
    if (!focusedFileId) {
      setFocusedFilePath(null);
      return;
    }
    const cached = filePathCache.current.get(focusedFileId);
    if (cached) {
      setFocusedFilePath(cached);
      return;
    }
    let cancelled = false;
    api.fileInfo(focusedFileId).then((info) => {
      if (!info) return;
      filePathCache.current.set(focusedFileId, info.path);
      if (!cancelled) setFocusedFilePath(info.path);
    });
    return () => {
      cancelled = true;
    };
  }, [focusedFileId]);

  // The selected row follows the file open in the focused pane (when the
  // path points inside the tree root). Manual clicks still work: they set
  // the same local selection.
  useEffect(() => {
    if (!focusedFilePath || !cwd) return;
    const sep = focusedFilePath[cwd.length];
    if (!(focusedFilePath.startsWith(cwd) && (sep === "\\" || sep === "/"))) return;
    setSelected(focusedFilePath);
    // Auto-expand the ancestors so the selected row is visible.
    const ancestors: string[] = [];
    let dir = focusedFilePath;
    while (dir.length > cwd.length) {
      const idx = Math.max(dir.lastIndexOf("\\"), dir.lastIndexOf("/"));
      if (idx < cwd.length) break;
      dir = dir.slice(0, idx);
      if (dir !== cwd) ancestors.push(dir);
    }
    for (const dir of ancestors) {
      if (!children[dir]) loadDir(dir);
      setExpanded((prev) => (prev[dir] ? prev : { ...prev, [dir]: true }));
    }
  }, [focusedFilePath, cwd, children, loadDir]);

  const rowMenu = (node: Node, x: number, y: number) => {
    // New File/Folder/cd Here target the row's dir, or its parent for files.
    const targetDir = node.is_directory
      ? node.path
      : node.path.slice(0, node.path.length - node.name.length).replace(/[\\/]+$/, "");
    const items: MenuEntry[] = [];
    const tk = tRef.current;
    if (!node.is_directory) {
      items.push({ label: tk("fileTree.open"), action: () => onOpenFile(node.path) });
      items.push({ label: tk("fileTree.openToSide"), action: () => api.openFile(node.path, true) });
    }
    items.push({ label: tk("fileTree.openInDefaultApp"), action: () => openPath(node.path) });
    items.push({ label: tk("common.revealInExplorer"), action: () => revealItemInDir(node.path) });
    items.push({ label: tk("common.copyPath"), action: () => navigator.clipboard.writeText(node.path) });
    const sessionId = focusedSessionIdRef.current;
    if (sessionId) {
      items.push({ label: tk("fileTree.cdHere"), action: () => api.sendText(sessionId, `cd ${shellQuotePath(targetDir)}\r`) });
    }
    items.push("sep");
    items.push({ label: tk("fileTree.newFile"), action: () => startCreate(targetDir, false) });
    items.push({ label: tk("fileTree.newFolder"), action: () => startCreate(targetDir, true) });
    items.push({ label: tk("common.rename"), action: () => setRenaming(node.path) });
    items.push("sep");
    items.push({ label: tk("fileTree.moveToTrash"), danger: true, action: () => onTrash(node) });
    openMenu({ x, y, items });
  };

  // Flatten the visible tree depth-first; a draft row sits at the top of
  // its parent directory's child list. Memoized: this is O(visible rows) on
  // every render (state updates from tab switches, git polls, ...).
  const flattened = useMemo(() => {
    const out: Node[] = [];
    const walk = (dir: string, depth: number) => {
      if (draft && draft.parentDir === dir) {
        out.push({ name: "", path: `${dir}\\__draft__`, is_directory: draft.isDirectory, depth, draft: true });
      }
      for (const e of children[dir] ?? []) {
        out.push({ ...e, depth });
        if (e.is_directory && expanded[e.path]) walk(e.path, depth + 1);
      }
    };
    if (cwd) walk(cwd, 0);
    return out;
  }, [children, expanded, draft, cwd]);

  // ── Windowing ─────────────────────────────────────────────────────────
  // Fully expanded trees hold thousands of rows; only the slice around the
  // scroll position is rendered, with spacer divs standing in for the rest.
  // Row height is uniform but font-size dependent (ui-fs-* follows the
  // user's setting), so an invisible probe row with a Row's exact classes
  // is measured via ResizeObserver.
  const listRef = useRef<HTMLDivElement>(null);
  const probeRef = useRef<HTMLDivElement>(null);
  const [rowH, setRowH] = useState(26);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportH, setViewportH] = useState(600);

  useEffect(() => {
    const probe = probeRef.current;
    const list = listRef.current;
    if (!probe || !list) return;
    const ro = new ResizeObserver(() => {
      const h = probe.offsetHeight;
      if (h > 0) setRowH((prev) => (prev === h ? prev : h));
      setViewportH(list.clientHeight);
    });
    ro.observe(probe);
    ro.observe(list);
    return () => ro.disconnect();
  }, []);

  const OVERSCAN = 8;
  const sliceStart = Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN);
  const sliceEnd = Math.min(flattened.length, Math.ceil((scrollTop + viewportH) / rowH) + OVERSCAN);
  const visibleNodes = flattened.slice(sliceStart, sliceEnd);

  return (
    <div
      ref={listRef}
      className="h-full overflow-y-auto px-2 py-2"
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
    >
      {/* Row-height probe: same classes as a Row, kept out of view. */}
      <div ref={probeRef} aria-hidden className="invisible absolute flex items-center gap-1 px-1.5 py-1 ui-fs-base">
        <IconFile size={12} />
        <span className="truncate">probe</span>
      </div>
      <div className="flex items-center gap-1 mb-2 px-1">
        <div className="flex-1 min-w-0 flex items-center gap-1 ui-fs-base text-muster-muted truncate" title={cwd}>
          <IconFolder size={12} className="flex-shrink-0" />
          <span className="truncate">{project?.name ?? cwd}</span>
        </div>
        <HeaderButton title={t("fileTree.newFileTooltip")} onClick={() => startCreate(cwd, false)}>
          <IconPlus size={13} />
        </HeaderButton>
        <HeaderButton title={t("fileTree.newFolderTooltip")} onClick={() => startCreate(cwd, true)}>
          <IconFolderPlus size={13} />
        </HeaderButton>
      </div>
      <div aria-hidden style={{ height: sliceStart * rowH }} />
      {visibleNodes.map((node) =>
        node.draft ? (
          <DraftRow
            key={node.path}
            depth={node.depth}
            isDirectory={node.is_directory}
            onCommit={commitDraft}
            onCancel={() => setDraft(null)}
          />
        ) : (
          <Row
            key={node.path}
            node={node}
            isExpanded={!!expanded[node.path]}
            isSelected={selected === node.path}
            isRenaming={renaming === node.path}
            badge={node.is_directory ? undefined : badges.get(node.path.toLowerCase())}
            dirty={node.is_directory && dirtyDirs.has(node.path.toLowerCase())}
            onToggle={() => toggle(node)}
            onOpen={() => onOpenFile(node.path)}
            onStartRename={() => setRenaming(node.path)}
            onRename={(name) => onRename(node, name)}
            onCancelRename={() => setRenaming(null)}
            onTrash={() => onTrash(node)}
            onNewFile={() => startCreate(node.is_directory ? node.path : cwd, false)}
            onNewFolder={() => startCreate(node.is_directory ? node.path : cwd, true)}
            onContextMenu={(x, y) => rowMenu(node, x, y)}
          />
        )
      )}
      <div aria-hidden style={{ height: (flattened.length - sliceEnd) * rowH }} />
      {flattened.length === 0 && (
        <div className="ui-fs-sm text-muster-muted/70 px-2 py-3">{t("fileTree.empty")}</div>
      )}
    </div>
  );
}

function HeaderButton({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className="px-1 rounded ui-fs-xs text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg active:scale-[.97] transition-transform duration-muster ease-muster flex items-center"
    >
      {children}
    </button>
  );
}

/// Inline input row for a new file/folder draft, styled like a rename row.
/// Enter commits, Escape cancels, blur commits a non-empty name (an empty
/// one cancels silently — commitDraft drops it).
function DraftRow({
  depth,
  isDirectory,
  onCommit,
  onCancel,
}: {
  depth: number;
  isDirectory: boolean;
  onCommit: (name: string) => void;
  onCancel: () => void;
}) {
  const { t } = useT();
  return (
    <div
      className="flex items-center gap-1 px-1.5 py-1 ui-fs-base rounded"
      style={{ paddingLeft: `${depth * 12 + 6}px` }}
    >
      {isDirectory ? (
        <span className="flex items-center text-muster-muted/70">
          <IconChevronRight size={10} />
        </span>
      ) : (
        <span className="w-2" />
      )}
      <span className={`flex items-center ${isDirectory ? "text-muster-accent/80" : "text-muster-muted"}`}>
        {isDirectory ? <IconFolder size={12} /> : <IconFile size={12} />}
      </span>
      <input
        autoFocus
        placeholder={isDirectory ? t("fileTree.newFolderPlaceholder") : t("fileTree.newFilePlaceholder")}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Enter") onCommit((e.target as HTMLInputElement).value);
          else if (e.key === "Escape") onCancel();
        }}
        onBlur={(e) => onCommit(e.target.value)}
        className="flex-1 min-w-0 bg-white/[0.08] rounded px-1 ui-fs-sm outline-none text-muster-fg"
      />
    </div>
  );
}

interface RowProps {
  node: Node;
  isExpanded: boolean;
  isSelected: boolean;
  isRenaming: boolean;
  /// Git status letter for files; undefined when clean or unknown.
  badge?: GitBadge;
  /// Directory contains at least one changed path (aggregated dot).
  dirty?: boolean;
  onToggle: () => void;
  onOpen: () => void;
  onStartRename: () => void;
  onRename: (name: string) => void;
  onCancelRename: () => void;
  onTrash: () => void;
  onNewFile: () => void;
  onNewFolder: () => void;
  onContextMenu: (x: number, y: number) => void;
}

/// Rows are memoized: the tree can hold thousands of nodes and the parent
/// re-renders on every app-state / git-status update. Callbacks are excluded
/// from the comparison on purpose — they are re-created per render but read
/// current state through refs (see `childrenRef`/`expandedRef`/`tRef` above).
function rowPropsEqual(a: RowProps, b: RowProps): boolean {
  return (
    a.node.path === b.node.path &&
    a.node.name === b.node.name &&
    a.node.depth === b.node.depth &&
    a.node.is_directory === b.node.is_directory &&
    a.isExpanded === b.isExpanded &&
    a.isSelected === b.isSelected &&
    a.isRenaming === b.isRenaming &&
    a.badge?.letter === b.badge?.letter &&
    a.badge?.className === b.badge?.className &&
    a.dirty === b.dirty
  );
}

const Row = memo(function Row({
  node,
  isExpanded,
  isSelected,
  isRenaming,
  badge,
  dirty,
  onToggle,
  onOpen,
  onStartRename,
  onRename,
  onCancelRename,
  onTrash,
  onNewFile,
  onNewFolder,
  onContextMenu,
}: RowProps) {
  const { t } = useT();
  return (
    <div
      draggable
      onDragStart={(e) => {
        // Dragging a row onto a terminal pastes its absolute path; text/plain
        // is the fallback for drop targets that don't know the app's MIME type.
        e.dataTransfer.setData("application/x-muster-path", node.path);
        e.dataTransfer.setData("text/plain", node.path);
        e.dataTransfer.effectAllowed = "copy";
      }}
      onClick={() => (node.is_directory ? onToggle() : onOpen())}
      onDoubleClick={(e) => {
        e.stopPropagation();
        onStartRename();
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(e.clientX, e.clientY);
      }}
      className={`group flex items-center gap-1 px-1.5 py-1 ui-fs-base cursor-pointer rounded ${
        isSelected ? "bg-muster-selected text-muster-fg" : "hover:bg-muster-hover text-muster-muted"
      }`}
      style={{ paddingLeft: `${node.depth * 12 + 6}px` }}
    >
      {node.is_directory ? (
        <span className="flex items-center text-muster-muted/70">
          {isExpanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        </span>
      ) : (
        <span className="w-2" />
      )}
      <span className={`flex items-center ${node.is_directory ? "text-muster-accent/80" : "text-muster-muted"}`}>
        {node.is_directory ? <IconFolder size={12} /> : <FileIcon name={node.name} size={12} />}
      </span>
      {isRenaming ? (
        <input
          autoFocus
          defaultValue={node.name}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === "Enter") onRename((e.target as HTMLInputElement).value);
            else if (e.key === "Escape") onCancelRename();
          }}
          onBlur={(e) => onRename(e.target.value)}
          className="flex-1 min-w-0 bg-white/[0.08] rounded px-1 ui-fs-sm outline-none text-muster-fg"
        />
      ) : (
        <span className={`flex-1 truncate ${!node.is_directory && badge ? badge.className : ""}`}>
          {node.name}
        </span>
      )}
      {!isRenaming && badge && (
        <span className={`flex-shrink-0 ui-fs-xs ${badge.className}`}>{badge.letter}</span>
      )}
      {!isRenaming && dirty && (
        <span className="flex-shrink-0 ui-fs-xs text-amber-400" title={t("fileTree.containsChanges")}>
          ●
        </span>
      )}
      {!isRenaming && (
        <span className="hidden group-hover:flex items-center gap-0.5">
          {node.is_directory && (
            <>
              <RowButton title={t("fileTree.newFileHereTooltip")} onClick={onNewFile}>
                <IconPlus size={11} />
              </RowButton>
              <RowButton title={t("fileTree.newFolderHereTooltip")} onClick={onNewFolder}>
                <IconFolderPlus size={11} />
              </RowButton>
            </>
          )}
          <RowButton title={t("fileTree.renameTooltip")} onClick={onStartRename}>
            <IconPencil size={11} />
          </RowButton>
          <RowButton title={t("fileTree.trashTooltip")} onClick={onTrash}>
            <IconTrash size={11} />
          </RowButton>
        </span>
      )}
    </div>
  );
}, rowPropsEqual);

function RowButton({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="px-0.5 rounded ui-fs-2xs text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster flex items-center"
    >
      {children}
    </button>
  );
}
