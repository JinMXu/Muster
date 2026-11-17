import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AppStateView, ListenPort, ProcessInfo, SessionInfo, Uuid } from "../lib/types";
import { api } from "../lib/invoke";
import { refreshGitStatus, useGitStatus } from "../lib/gitStatusStore";
import { forceProjectPoll, useProjectSessions, useSessionAnchor } from "../lib/useProjectCwd";
import { reloadSettings, useSettings } from "../lib/settingsStore";
import { openMenu, type MenuEntry } from "../lib/menuStore";
import { IconArrowUpRight, IconFolder, IconGitBranch, IconGlobe, IconRefresh, IconTerminal, IconX } from "./icons";
import { BrandIcon, hasBrandIcon } from "./brandIcons";
import { useT } from "../lib/i18n/context";
import { agentLabel, useAgentStatus, type LiveAgentStatus } from "../hooks/useAgentStatus";
import { resolvePanelSessions } from "../lib/sessionUtils";
import { DOT_CLASS, STATE_LABEL_KEY, dotKind, type DotKind } from "../lib/agentColors";

/// Info panel: details about the focused session, selected project, and the
/// repository it lives in (shell/PID, cwd, project dir, git branch/remote),
/// plus the session's process tree (PROCESSES) and listening TCP ports
/// (PORTS — the session's own processes; with the "project ports" setting
/// on, also any process working in the project directory, so externally
/// started dev servers still show).
export default function InfoPanel({ state, active }: { state: AppStateView | null; active: boolean }) {
  // Sessions come from the shared per-project poller (one 5s
  // list_all_sessions round-trip feeds this panel and the cwd anchor),
  // not a private interval that ran in parallel with it.
  const sessions = useProjectSessions(state);
  const project = state?.projects.find((p) => p.id === state.selected_project_id) ?? null;
  // Every terminal pane in the selected tab gets its own card (a split tab
  // running Claude Code next to Codex shows both). The focused pane's
  // session is the selected one and drives every row below the cards —
  // directory, git, processes, ports.
  const { cards, selected: projSession, missing } = resolvePanelSessions(project, sessions);
  const multi = cards.length > 1;
  // A pane opened since the last 5s poll has no session row yet: poll now
  // instead of leaving its card out for up to 5s.
  const missingKey = missing.join(",");
  useEffect(() => {
    if (missingKey && project) forceProjectPoll(project.id);
  }, [missingKey, project?.id]);
  const { root, cwd } = useSessionAnchor(projSession?.working_directory ?? null, project?.custom_directory ?? null);
  const settings = useSettings();
  const projectPorts = settings?.project_ports ?? false;
  // Git branch/remote come from the shared store (one poller per repo root,
  // fanned out to GitPanel/FileTree/this panel) instead of a private interval.
  // `active` keeps this hidden panel from polling at full speed.
  const git = useGitStatus(root, active);
  // Poll results are tagged with the session they belong to: after a pane
  // switch the previous session's rows must not render (under the new
  // session's name) while the first poll for the new one is in flight.
  const [polled, setPolled] = useState<PolledRows>(NO_ROWS);
  const owned = polled.sessionId != null && polled.sessionId === projSession?.id;
  const procs = owned ? polled.procs : NO_ROWS.procs;
  const ports = owned ? polled.ports : NO_ROWS.ports;
  // Bumped by the header refresh button to re-run every poll below at once.
  const [nonce, setNonce] = useState(0);
  const [spinning, setSpinning] = useState(false);
  const refresh = () => {
    setSpinning(true);
    setNonce((n) => n + 1);
    forceProjectPoll(project?.id ?? null);
    if (root) refreshGitStatus(root);
    setTimeout(() => setSpinning(false), 600);
  };

  const { t } = useT();
  const liveAgents = useAgentStatus();
  const shellPid = projSession && !projSession.has_exited ? projSession.pid : null;

  // PROCESSES + PORTS: poll the session's tracked processes (Job Object
  // members, falling back to the shell's descendant tree), then the listening
  // ports of those pids. With the "project ports" setting on, ports of any
  // process working in the project directory are included too.
  // Stale guard: this effect is keyed on the session id and shell pid, so if
  // either changes while a request is in flight the effect is torn down
  // (alive=false) and the late result dropped.
  useEffect(() => {
    if (!active) return;
    if (shellPid == null || projSession == null) {
      setPolled(NO_ROWS);
      return;
    }
    const sessionId = projSession.id;
    let alive = true;
    const tick = async () => {
      if (document.hidden) return;
      const list = await api.sessionProcesses(sessionId, shellPid).catch(() => [] as ProcessInfo[]);
      if (!alive) return;
      // Ports of a different session are dropped, not carried over.
      setPolled((prev) => ({ sessionId, procs: list, ports: prev.sessionId === sessionId ? prev.ports : [] }));
      const pids = list.map((p) => p.pid);
      const ps = await api.sessionPorts(pids, projectPorts ? root : null).catch(() => [] as ListenPort[]);
      if (!alive) return;
      setPolled((prev) => (prev.sessionId === sessionId ? { ...prev, ports: ps } : prev));
    };
    tick();
    const i = setInterval(tick, 3000);
    return () => {
      alive = false;
      clearInterval(i);
    };
  }, [active, shellPid, projSession?.id, root, projectPorts, nonce]);

  /// Kill and re-poll after a beat so the OS has time to reap the process.
  const kill = (pid: number) => {
    if (shellPid != null && projSession) {
      api.killProcess(projSession.id, shellPid, pid).catch(() => {});
    }
    setTimeout(refresh, 300);
  };

  const procMenu = (e: React.MouseEvent, p: ProcessInfo) => {
    e.preventDefault();
    const items: MenuEntry[] = [
      { label: t("info.killProcess"), danger: true, action: () => kill(p.pid) },
      "sep",
      { label: t("info.copyPid"), action: () => navigator.clipboard.writeText(String(p.pid)) },
      { label: t("info.copyExePath"), action: () => navigator.clipboard.writeText(p.exe) },
    ];
    openMenu({ x: e.clientX, y: e.clientY, items });
  };

  const portMenu = (e: React.MouseEvent, pt: ListenPort) => {
    e.preventDefault();
    const url = `http://localhost:${pt.port}`;
    const items: MenuEntry[] = [
      { label: t("info.openInBrowser"), action: () => openUrl(url) },
      { label: t("info.copyUrl"), action: () => navigator.clipboard.writeText(url) },
      "sep",
      { label: t("info.killProcess"), danger: true, action: () => kill(pt.pid) },
    ];
    openMenu({ x: e.clientX, y: e.clientY, items });
  };

  /// Flip the "project ports" setting and persist it; reloadSettings notifies
  /// the store so this panel (and the Settings modal) pick up the new value.
  const toggleProjectPorts = () => {
    if (!settings) return;
    api
      .saveSettings({ ...settings, project_ports: !settings.project_ports })
      .then(reloadSettings)
      .catch(() => {});
  };

  return (
    <div className="p-3 ui-fs-base space-y-3">
      <div className="flex items-center justify-between">
        <div className="ui-fs-xs text-muster-muted uppercase tracking-wide">{t("info.title")}</div>
        <button
          onClick={refresh}
          title={t("info.refresh")}
          className="text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn rounded p-0.5 active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          <IconRefresh size={11} className={spinning ? "animate-spin" : ""} />
        </button>
      </div>

      {/* Hero: the agent (or bare shell) is each session's identity — brand
          mark, name, and the state pill get the only cards in the panel.
          With split panes there is one card per pane: the focused pane's is
          ringed, the others focus their pane on click. */}
      <div className="space-y-1.5">
        {cards.length > 0 ? (
          cards.map((s) => (
            <SessionCard
              key={s.id}
              session={s}
              live={liveAgents[s.id] ?? null}
              selected={multi && s.id === projSession?.id}
              onSelect={
                multi && s.id !== projSession?.id
                  ? () => void api.focusAgentSession(s.id).catch(() => {})
                  : undefined
              }
            />
          ))
        ) : (
          <SessionCard session={null} live={null} selected={false} />
        )}
      </div>

      {/* Git as one chip line; paths demote to muted mono-ish lines below —
          they are reference, not headline. */}
      {(git?.is_repo || root) && (
        <div className="space-y-1.5 min-w-0">
          {git?.is_repo && (
            <div className="flex items-center gap-1.5 min-w-0 flex-wrap">
              <span className="flex items-center gap-1 rounded-md bg-muster-hover/60 px-1.5 h-6 min-w-0">
                <IconGitBranch size={12} className="text-muster-accent flex-shrink-0" />
                <span className="ui-fs-xs font-medium text-muster-fg/90 truncate">
                  {git.branch ?? t("info.detached")}
                </span>
                {git.ahead > 0 && <span className="ui-fs-xs text-emerald-400/90 tabular-nums">↑{git.ahead}</span>}
                {git.behind > 0 && <span className="ui-fs-xs text-amber-400/90 tabular-nums">↓{git.behind}</span>}
                {git.is_worktree && (
                  <span className="ui-fs-2xs text-muster-muted/70">{t("info.worktree")}</span>
                )}
              </span>
              {git.upstream && (
                <span className="ui-fs-xs text-muster-muted/75 truncate">{git.upstream}</span>
              )}
            </div>
          )}
          {root && (
            <PathLine path={root} />
          )}
          {cwd && root && normalize(cwd) !== normalize(root) && (
            <PathLine path={cwd} label={t("info.currentDirectory")} />
          )}
        </div>
      )}

      <hr className="border-muster-hairline" />

      {/* Processes/ports render only while there is something to say — no
          "no listening ports" filler. */}
      {shellPid != null && procs.length > 0 && (
        <Section label={t("info.processes")} suffix={multi ? sessionName(projSession) : undefined}>
          {procs.map((p) => (
            <ProcRow key={p.pid} p={p} onKill={kill} onMenu={procMenu} />
          ))}
        </Section>
      )}
      {shellPid != null && ports.length > 0 && (
        <Section
          label={t("info.ports")}
          suffix={multi ? sessionName(projSession) : undefined}
          action={
            <button
              onClick={toggleProjectPorts}
              title={t("info.includeProjectPorts")}
              aria-pressed={projectPorts}
              className={`rounded p-0.5 active:scale-[.97] transition-colors duration-muster ease-muster ${
                projectPorts
                  ? "text-muster-accent"
                  : "text-muster-muted/60 hover:text-muster-fg hover:bg-muster-hover-btn"
              }`}
            >
              <IconGlobe size={11} />
            </button>
          }
        >
          {ports.map((pt) => (
            <PortRow key={pt.port} pt={pt} onMenu={portMenu} />
          ))}
        </Section>
      )}
    </div>
  );
}

/// Card title for a session: the agent's brand name, else the shell name.
function sessionName(s: SessionInfo | null): string {
  if (!s) return "—";
  return s.agent ? agentLabel(s.agent) : s.shell_name;
}

/// One session's identity card: brand mark, name, live state pill, and a
/// muted shell/PID line. `selected` rings the focused pane's card when the
/// tab has several; `onSelect` (set on the other cards) focuses that pane.
function SessionCard({
  session,
  live,
  selected,
  onSelect,
}: {
  session: SessionInfo | null;
  live: LiveAgentStatus | null;
  selected: boolean;
  onSelect?: () => void;
}) {
  const { t } = useT();
  // The agent's live state drives the pill; a session without a live agent
  // falls back to plain running/exited.
  const pillKind: PillKind = live ? dotKind(live) : session && !session.has_exited ? "running" : "exited";
  const pillLabel = t(live ? STATE_LABEL_KEY[dotKind(live)] : pillKind === "running" ? "info.running" : "info.exited");

  // Not a <button>: the card can hold the Restart button, and buttons don't
  // nest. A clickable card gets the button role and keyboard activation.
  const interactive = onSelect
    ? {
        role: "button",
        tabIndex: 0,
        onClick: onSelect,
        onKeyDown: (e: React.KeyboardEvent) => {
          if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
          e.preventDefault();
          onSelect();
        },
      }
    : {};

  return (
    <div
      {...interactive}
      aria-current={selected || undefined}
      className={`rounded-lg border px-2.5 py-2 transition-colors duration-muster ease-muster ${
        selected ? "bg-muster-hover/60 border-muster-accent/60" : "bg-muster-hover/40 border-muster-hairline"
      } ${
        onSelect
          ? "cursor-pointer hover:bg-muster-hover/60 focus-visible:outline-none focus-visible:border-muster-accent/60"
          : ""
      }`}
    >
      <div className="flex items-center gap-2 min-w-0">
        {session?.agent && hasBrandIcon(session.agent) ? (
          <BrandIcon kind={session.agent} size={16} className="text-muster-fg/90 flex-shrink-0" />
        ) : (
          <span className="text-muster-fg/90 flex-shrink-0 flex items-center">
            <IconTerminal size={15} />
          </span>
        )}
        <span className="ui-fs-sm font-medium text-muster-fg/90 truncate min-w-0">{sessionName(session)}</span>
        <span
          className={`ml-auto flex-shrink-0 flex items-center gap-1 px-1.5 h-5 rounded-full bg-muster-hover/60 ui-fs-xs leading-none ${PILL_TEXT[pillKind]}`}
          title={pillLabel}
        >
          <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${PILL_DOT[pillKind]}`} />
          {pillLabel}
        </span>
      </div>
      {session && (
        <div className="mt-1.5 flex items-center gap-1.5 ui-fs-xs text-muster-muted/80 min-w-0">
          <span className="truncate">
            {session.shell_name}
            {session.pid ? ` · PID ${session.pid}` : ""}
          </span>
          {session.agent && session.agent_command && !live && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                api.restartSessionAgent(session.id).catch(() => {});
              }}
              className="ml-auto flex-shrink-0 text-muster-accent hover:underline"
            >
              {t("info.restartAgent")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/// Agent/shell state pill palette: the shared agent-dot vocabulary plus the
/// two plain-shell kinds, so a state reads the same color everywhere.
type PillKind = DotKind | "running" | "exited";

const PILL_DOT: Record<PillKind, string> = {
  ...DOT_CLASS,
  running: "bg-emerald-400",
  exited: "bg-muster-muted/50",
};

const PILL_TEXT: Record<PillKind, string> = {
  approval: "text-rose-300",
  working: "text-muster-fg/85",
  waiting: "text-amber-200/90",
  done: "text-sky-200/90",
  running: "text-muster-fg/85",
  exited: "text-muster-muted/70",
};

/// The process/port rows of the last poll, with the session they belong to.
interface PolledRows {
  sessionId: Uuid | null;
  procs: ProcessInfo[];
  ports: ListenPort[];
}

const NO_ROWS: PolledRows = { sessionId: null, procs: [], ports: [] };

/// Path comparison ignoring trailing separators and slash direction, so the
/// Current Directory row doesn't flicker over cosmetic differences.
function normalize(p: string): string {
  return p.replace(/[\\/]+$/, "").replace(/\\/g, "/").toLowerCase();
}

/// 1024-based KB/MB for the PROCESSES memory column.
function formatMem(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${Math.round(bytes / (1024 * 1024))} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

/// A muted reference line for a filesystem path (project root, current cwd).
/// Paths are reference, not headline: truncate with the full path on hover.
function PathLine({ path, label }: { path: string; label?: string }) {
  return (
    <div className="flex items-center gap-1.5 min-w-0" title={path}>
      <IconFolder size={12} className="text-muster-muted/70 flex-shrink-0" />
      <span className="ui-fs-xs text-muster-muted/85 truncate min-w-0">
        {label && <span className="text-muster-muted/60 mr-1">{label}:</span>}
        {path}
      </span>
    </div>
  );
}

function Section({
  label,
  suffix,
  action,
  children,
}: {
  label: string;
  /// Whose rows these are, shown when the tab holds several sessions.
  suffix?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="flex items-center justify-between gap-2 min-w-0">
        <div className="ui-fs-xs text-muster-muted uppercase tracking-wide truncate min-w-0">
          {label}
          {suffix && <span className="normal-case tracking-normal text-muster-muted/70"> · {suffix}</span>}
        </div>
        {action}
      </div>
      <div className="mt-1 space-y-0.5">{children}</div>
    </div>
  );
}

function ProcRow({
  p,
  onKill,
  onMenu,
}: {
  p: ProcessInfo;
  onKill: (pid: number) => void;
  onMenu: (e: React.MouseEvent, p: ProcessInfo) => void;
}) {
  const { t } = useT();
  return (
    <div
      className="group flex items-center gap-1.5 px-1 py-0.5 -mx-1 rounded hover:bg-muster-hover ui-fs-sm text-muster-fg/80"
      onContextMenu={(e) => onMenu(e, p)}
      // PID and CPU are power-user details: live on hover, not in the row.
      title={`${p.name} · PID ${p.pid} · CPU ${p.cpu.toFixed(0)}%`}
    >
      <span className="w-1.5 h-1.5 rounded-full bg-green-500 flex-shrink-0" />
      <span className="truncate min-w-0 flex-1" title={p.exe || p.name}>
        {p.name}
      </span>
      <span className="text-muster-muted flex-shrink-0 text-right tabular-nums">
        {formatMem(p.mem_bytes)}
      </span>
      <button
        onClick={() => onKill(p.pid)}
        title={t("info.killProcessTitle")}
        className="opacity-0 group-hover:opacity-100 flex-shrink-0 text-muster-muted hover:text-red-400 hover:bg-muster-hover-btn rounded p-0.5 active:scale-[.97] transition-transform duration-muster ease-muster"
      >
        <IconX size={10} />
      </button>
    </div>
  );
}

function PortRow({
  pt,
  onMenu,
}: {
  pt: ListenPort;
  onMenu: (e: React.MouseEvent, pt: ListenPort) => void;
}) {
  const { t } = useT();
  const url = `http://localhost:${pt.port}`;
  return (
    <div
      className="group flex items-center gap-1.5 px-1 py-0.5 -mx-1 rounded hover:bg-muster-hover ui-fs-sm text-muster-fg/80 cursor-pointer"
      title={t("info.openPortUrl", { port: pt.port })}
      onClick={() => openUrl(url)}
      onContextMenu={(e) => onMenu(e, pt)}
    >
      <span className="truncate min-w-0 flex-1">
        {t("info.localhostPort", { port: pt.port })}
        {pt.process_name && <span className="text-muster-muted ml-1.5">{pt.process_name}</span>}
      </span>
      <span className="opacity-0 group-hover:opacity-100 text-muster-muted flex-shrink-0">
        <IconArrowUpRight size={10} />
      </span>
    </div>
  );
}
