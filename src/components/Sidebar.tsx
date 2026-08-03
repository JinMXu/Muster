import type { ProjectView, Uuid } from "../lib/types";
import { IconFolder, IconPlus, IconSettings, IconX } from "./icons";
import { useT } from "../lib/i18n/context";
import AgentMiniBar from "./AgentMiniBar";

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

  return (
    <aside
      className="bg-muster-panel flex flex-col flex-shrink-0"
      style={{ width }}
      data-tauri-drag-region
    >
      <div className="h-9" data-tauri-drag-region />
      <div className="flex-1 overflow-y-auto px-2">
        <div className="space-y-1">
          {projects.map((p, i) => (
            <ProjectRow
              key={p.id}
              project={p}
              index={i}
              isSelected={p.id === selected}
              isDragging={dragging === p.id}
              onSelect={() => onSelect(p.id)}
              onClose={() => onClose(p.id)}
              onDragStart={() => setDragging(p.id)}
              onDragEnd={() => setDragging(null)}
              onMove={(to) => onMove(p.id, to)}
              onRename={(name) => onRename(p.id, name)}
              onMenu={(x, y, requestRename) => onProjectMenu(p, x, y, requestRename)}
            />
          ))}
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

import { useRef, useState } from "react";

function ProjectRow({
  project,
  index,
  isSelected,
  isDragging,
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
  onSelect: () => void;
  onClose: () => void;
  onDragStart: () => void;
  onDragEnd: () => void;
  onMove: (to: Uuid) => void;
  onRename: (name: string | null) => void;
  onMenu: (x: number, y: number, requestRename: () => void) => void;
}) {
  const [hovering, setHovering] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const { t } = useT();

  // Same inline-rename UX as the tab strip in Header.
  const startRename = () => {
    setDraft(project.custom_name ?? project.name);
    setRenaming(true);
    setTimeout(() => inputRef.current?.focus(), 0);
  };
  const finish = (apply: boolean) => {
    if (apply) {
      const trimmed = draft.trim();
      onRename(trimmed.length ? trimmed : null);
    }
    setRenaming(false);
  };

  return (
    <div
      onClick={onSelect}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      onDoubleClick={startRename}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(e.clientX, e.clientY, startRename);
      }}
      draggable
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        onMove(project.id);
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
        {project.session_count > 1 && (
          <div className="ui-fs-xs text-muster-muted">
            {t("sidebar.sessions", { n: project.session_count })}
          </div>
        )}
      </div>
      {hovering ? (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onClose();
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
}