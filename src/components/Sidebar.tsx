import { memo, useCallback, useEffect, useRef, useState } from "react";
import type { AgentAttention, AgentState, ProjectView, Uuid } from "../lib/types";
import { api } from "../lib/invoke";
import { useGitStatus } from "../lib/gitStatusStore";
import { useAgentStatus, type LiveAgentStatus } from "../hooks/useAgentStatus";
import { IconFolder, IconPanelLeftClose, IconPlus, IconSettings, IconX } from "./icons";
import { useT } from "../lib/i18n/context";
import AgentMiniBar from "./AgentMiniBar";
import UsageMiniMeter from "./UsageMiniMeter";

type DotKind = AgentState | "approval";

/// Tailwind dot-class per kind — mirrors the colors used in `AgentMiniBar`.
const DOT_CLASS: Record<DotKind, string> = {
  approval: "bg-rose-400 animate-pulse",
  working: "bg-emerald-400",
  waiting: "bg-amber-400 animate-pulse",
  done: "bg-sky-400",
};

function dotOf(r: LiveAgentStatus): DotKind {
  return r.attention === "approval" ? "approval" : (r.state as AgentState);
}

/// Compact number: 1493 → "1.5k".
function fmtCount(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

/// Sidebar-only: resolve each open project's repo root (its first session's
/// cwd → `resolveProjectRoot`), so every project row can show its branch and
/// change stats. One shared 5s poll; root lookups are cached upstream.
function useProjectRepoRoots(projects: ProjectView[]): Record<string, string | null> {
  const [roots, setRoots] = useState<Record<string, string | null>>({});
  const keyRef = useRef("");
  const lastRunRef = useRef(0);
  useEffect(() => {
    let alive = true;
    const tick = async () => {
      // State events re-run this effect; skip when a poll just happened.
      const now = Date.now();
      if (now - lastRunRef.current < 2000) return;
      lastRunRef.current = now;
      const sessions = await api.listAllSessions().catch(() => []);
      const firstCwd = new Map<string, string>();
      for (const p of projects) {
        const s = sessions.find((x) => x.project_id === p.id);
        if (s) firstCwd.set(p.id, s.working_directory);
      }
      const key = [...firstCwd].map(([id, cwd]) => `${id}:${cwd}`).join("|");
      if (key === keyRef.current) return;
      keyRef.current = key;
      const next: Record<string, string | null> = {};
      for (const [id, cwd] of firstCwd) {
        next[id] = await api.resolveProjectRoot(cwd).catch(() => cwd);
      }
      if (alive) setRoots(next);
    };
    void tick();
    const timer = setInterval(tick, 5000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [projects]);
  return roots;
}

export default function Sidebar({
  projects,
  selected,
  onSelect,
  onClose,
  onNewProject,
  onMove,
  onRename,
  onProjectMenu,
  onOpenSettings,
  onOpenUsage,
  onToggle,
  width,
  agentBarOpen,
  onAgentBarToggle,
  onAgentBarClose,
}: {
  projects: ProjectView[];
  selected: Uuid | null;
  onSelect: (id: Uuid) => void;
  onClose: (id: Uuid) => void;
  onNewProject: () => void;
  onMove: (from: Uuid, to: Uuid) => void;
  onRename: (id: Uuid, name: string | null) => void;
  /// Right-click on a project row: App builds the menu; `requestRename`
  /// re-enters the row's inline rename field.
  onProjectMenu: (project: ProjectView, x: number, y: number, requestRename: () => void) => void;
  onOpenSettings: () => void;
  onOpenUsage: () => void;
  /// Collapse this sidebar (same backend command as Ctrl+B). Lives in the
  /// sidebar's top-left corner per the layout annotation; the Header hosts
  /// the expand twin while the sidebar is hidden.
  onToggle: () => void;
  /// Resizable panel width in px (drag handle lives in App).
  width: number;
  /// Cross-state agent mini-bar popover is owned by App so the global
  /// `Ctrl+Shift+A` shortcut can toggle it without a ref.
  agentBarOpen: boolean;
  onAgentBarToggle: () => void;
  onAgentBarClose: () => void;
}) {
  const { t } = useT();
  const [dragging, setDragging] = useState<Uuid | null>(null);
  // Stable id-based callbacks so the memoized ProjectRows skip re-rendering
  // on unrelated App/Header state changes.
  const handleDragStart = useCallback((id: Uuid) => setDragging(id), []);
  const handleDragEnd = useCallback(() => setDragging(null), []);
  // Live agent status (shared store) + per-project repo roots, feeding the
  // project rows' second line (dots · branch · change stats).
  const agentStates = useAgentStatus();
  const roots = useProjectRepoRoots(projects);

  return (
    <aside
      className="bg-muster-panel flex flex-col flex-shrink-0"
      style={{ width }}
      data-tauri-drag-region
    >
      <div className="h-9 flex items-center pl-2 flex-shrink-0" data-tauri-drag-region>
        <button
          onClick={onToggle}
          title={t("header.toggleLeftSidebarTooltip")}
          className="w-6 h-6 rounded text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn flex items-center justify-center ui-fs-base active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          <IconPanelLeftClose size={15} />
        </button>
        <div className="flex-1 self-stretch" data-tauri-drag-region />
      </div>
      <div className="flex-1 overflow-y-auto px-2">
        <div className="space-y-1">
          {projects.map((p, i) => {
            // Session ids across this project's panes → their agent dots.
            const sessionIds: string[] = [];
            for (const tab of p.tabs) {
              for (const col of tab.columns) {
                for (const pane of col.panes) {
                  if (pane.content.kind === "session") sessionIds.push(pane.content.id);
                }
              }
            }
            const dots: DotKind[] = [
              ...new Set(
                sessionIds
                  .map((id) => agentStates[id])
                  .filter((s): s is LiveAgentStatus => !!s)
                  .map(dotOf),
              ),
            ];
            return (
              <ProjectRow
                key={p.id}
                project={p}
                index={i}
                isSelected={p.id === selected}
                isDragging={dragging === p.id}
                dots={dots}
                root={roots[p.id] ?? null}
                onSelect={onSelect}
                onClose={onClose}
                onDragStart={handleDragStart}
                onDragEnd={handleDragEnd}
                onMove={onMove}
                onRename={onRename}
                onMenu={onProjectMenu}
              />
            );
          })}
        </div>
      </div>
      <div className="px-2 py-1.5 flex items-center gap-1">
<FooterButton title={t("sidebar.newProjectTooltip")} onClick={onNewProject}>
        <IconPlus size={14} />
      </FooterButton>
      <AgentMiniBar
        open={agentBarOpen}
        onToggle={onAgentBarToggle}
        onClose={onAgentBarClose}
      />
      <UsageMiniMeter onOpen={onOpenUsage} />
      <div className="flex-1" />
      <FooterButton title={t("sidebar.settingsTooltip")} onClick={onOpenSettings}>
        <IconSettings size={14} />
      </FooterButton>
      </div>
    </aside>
  );
}

function FooterButton({
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
      className="w-6 h-6 rounded-md text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn flex items-center justify-center ui-fs-base active:scale-[.97] transition-transform duration-muster ease-muster"
    >
      {children}
    </button>
  );
}

/// Memoized with a field-level comparator (project views are rebuilt with
/// fresh identities on every state-changed event). The dragged project id
/// rides the dataTransfer — same idiom as the Header tab strip — so `from`
/// and `to` can never collapse into the same id like the old per-row closure
/// did (which made drag-reordering a silent no-op).
const ProjectRow = memo(function ProjectRow({
  project,
  index,
  isSelected,
  isDragging,
  dots,
  root,
  onSelect,
  onClose,
  onDragStart,
  onDragEnd,
  onMove,
  onRename,
  onMenu,
}: {
  project: ProjectView;
  index: number;
  isSelected: boolean;
  isDragging: boolean;
  /** Agent status dots across this project's panes. */
  dots: DotKind[];
  /** Repo root (resolved from the project's first session) or null. */
  root: string | null;
  onSelect: (id: Uuid) => void;
  onClose: (id: Uuid) => void;
  onDragStart: (id: Uuid) => void;
  onDragEnd: () => void;
  onMove: (from: Uuid, to: Uuid) => void;
  onRename: (id: Uuid, name: string | null) => void;
  onMenu: (project: ProjectView, x: number, y: number, requestRename: () => void) => void;
}) {
  const [hovering, setHovering] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const { t } = useT();
  // Branch + change stats for the second line. Only the selected project
  // polls at the active cadence; the rest ride the idle one.
  const git = useGitStatus(root, isSelected);

  // Same inline-rename UX as the tab strip in Header.
  const startRename = () => {
    setDraft(project.custom_name ?? project.name);
    setRenaming(true);
    setTimeout(() => inputRef.current?.focus(), 0);
  };
  const finish = (apply: boolean) => {
    if (apply) {
      const trimmed = draft.trim();
      onRename(project.id, trimmed.length ? trimmed : null);
    }
    setRenaming(false);
  };

  return (
    <div
      onClick={() => onSelect(project.id)}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      onDoubleClick={startRename}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(project, e.clientX, e.clientY, startRename);
      }}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData("application/x-muster-project", project.id);
        e.dataTransfer.effectAllowed = "move";
        onDragStart(project.id);
      }}
      onDragEnd={onDragEnd}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        const from = e.dataTransfer.getData("application/x-muster-project");
        if (from && from !== project.id) onMove(from, project.id);
      }}
      className={`group rounded-md px-2 py-1.5 cursor-pointer flex items-center gap-2 ${
        isSelected ? "bg-muster-selected" : "hover:bg-muster-hover"
      } ${isDragging ? "opacity-65" : ""}`}
    >
      <span className={`flex items-center ${isSelected ? "text-muster-accent" : "text-muster-muted"}`}>
        <IconFolder size={13} />
      </span>
      <div className="flex-1 min-w-0 flex flex-col">
        {renaming ? (
          <input
            ref={inputRef}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={() => finish(true)}
            onKeyDown={(e) => {
              if (e.key === "Enter") finish(true);
              if (e.key === "Escape") finish(false);
            }}
            onClick={(e) => e.stopPropagation()}
            onDoubleClick={(e) => e.stopPropagation()}
            className="bg-transparent border border-muster-accent/60 rounded px-1 ui-fs-base outline-none"
          />
        ) : (
          <div className="ui-fs-base truncate text-muster-fg/90">{project.name}</div>
        )}
        {(project.session_count > 1 || dots.length > 0 || (git?.is_repo && git.branch)) && (
          <div className="ui-fs-xs text-muster-muted flex items-center gap-1.5 min-w-0">
            {project.session_count > 1 && (
              <span className="flex-shrink-0">
                {t("sidebar.sessions", { n: project.session_count })}
              </span>
            )}
            {dots.slice(0, 3).map((d, i) => (
              <span key={i} className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${DOT_CLASS[d]}`} />
            ))}
            {dots.length > 3 && (
              <span className="flex-shrink-0 tabular-nums">+{dots.length - 3}</span>
            )}
            {git?.is_repo && git.branch && (
              <span className="truncate min-w-0">{git.branch}</span>
            )}
            {git?.is_repo && git.line_additions > 0 && (
              <span className="text-emerald-400/90 tabular-nums flex-shrink-0">
                +{fmtCount(git.line_additions)}
              </span>
            )}
            {git?.is_repo && git.line_deletions > 0 && (
              <span className="text-red-400/90 tabular-nums flex-shrink-0">
                −{fmtCount(git.line_deletions)}
              </span>
            )}
          </div>
        )}
      </div>
      {hovering ? (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onClose(project.id);
          }}
          className="w-4 h-4 text-muster-muted hover:text-muster-fg flex items-center justify-center ui-fs-xs"
        >
          <IconX size={11} />
        </button>
      ) : (
        index < 9 && (
          <span className="ui-fs-xs text-muster-muted/70">{t("sidebar.shortcutCtrl", { index })}</span>
        )
      )}
    </div>
  );
}, (a, b) =>
  a.isSelected === b.isSelected &&
  a.isDragging === b.isDragging &&
  a.index === b.index &&
  a.project.id === b.project.id &&
  a.project.name === b.project.name &&
  a.project.custom_name === b.project.custom_name &&
  a.project.session_count === b.project.session_count &&
  a.root === b.root &&
  a.dots.length === b.dots.length &&
  a.dots.every((d, i) => d === b.dots[i])
);