import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AppStateView, GitStatusInfo, ListenPort, ProcessInfo, SessionInfo } from "../lib/types";
import { api } from "../lib/invoke";
import { useProjectCwd } from "../lib/useProjectCwd";
import { openMenu, type MenuEntry } from "../lib/menuStore";
import { IconArrowUpRight, IconRefresh, IconX } from "./icons";
import { useT } from "../lib/i18n/context";

/// Info panel: details about the focused session, selected project, and the
/// repository it lives in (shell/PID, cwd, project dir, git branch/remote),
/// plus the session's process tree (PROCESSES) and listening TCP ports
/// (PORTS — session processes plus any process working in the project
/// directory, so externally started dev servers still show).
export default function InfoPanel({ state }: { state: AppStateView | null }) {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const project = state?.projects.find((p) => p.id === state.selected_project_id) ?? null;
  const projSession = sessions.find((s) => s.project_id === project?.id) ?? null;
  const { root, cwd } = useProjectCwd(state);
  const [git, setGit] = useState<GitStatusInfo | null>(null);
  const [procs, setProcs] = useState<ProcessInfo[]>([]);
  const [ports, setPorts] = useState<ListenPort[]>([]);
  // Bumped by the header refresh button to re-run every poll below at once.
  const [nonce, setNonce] = useState(0);
  const refresh = () => setNonce((n) => n + 1);

  const { t } = useT();
  const shellPid = projSession && !projSession.has_exited ? projSession.pid : null;

  useEffect(() => {
    let alive = true;
    const tick = () => api.listAllSessions().then((s) => alive && setSessions(s));
    tick();
    const i = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(i);
    };
  }, [nonce]);

  useEffect(() => {
    if (!root) {
      setGit(null);
      return;
    }
    let alive = true;
    const tick = () => api.gitStatus(root).then((g) => alive && setGit(g));
    tick();
    const i = setInterval(tick, 4000);
    return () => {
      alive = false;
      clearInterval(i);
    };
  }, [root, nonce]);

  // PROCESSES + PORTS: poll the session's tracked processes (Job Object
  // members, falling back to the shell's descendant tree), then the listening
  // ports of those pids plus any process belonging to the project directory.
  // Stale guard: this effect is keyed on the session id and shell pid, so if
  // either changes while a request is in flight the effect is torn down
  // (alive=false) and the late result dropped.
  useEffect(() => {
    if (shellPid == null || projSession == null) {
      setProcs([]);
      setPorts([]);
      return;
    }
    const sessionId = projSession.id;
    let alive = true;
    const tick = async () => {
      const list = await api.sessionProcesses(sessionId, shellPid).catch(() => [] as ProcessInfo[]);
      if (!alive) return;
      setProcs(list);
      const pids = list.map((p) => p.pid);
      const ps = await api.sessionPorts(pids, root).catch(() => [] as ListenPort[]);
      if (!alive) return;
      setPorts(ps);
    };
    tick();
    const i = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(i);
    };
  }, [shellPid, projSession?.id, root, nonce]);

  /// Kill and re-poll after a beat so the OS has time to reap the process.
  const kill = (pid: number) => {
    api.killProcess(pid).catch(() => {});
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

  return (
    <div className="p-3 text-xs space-y-3">
      <div className="flex items-center justify-between">
        <div className="text-[10px] text-muster-muted uppercase tracking-wide">{t("info.title")}</div>
        <button
          onClick={refresh}
          title={t("info.refresh")}
          className="text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn rounded p-0.5 active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          <IconRefresh size={11} />
        </button>
      </div>
      <Row label={t("info.project")} value={project?.name ?? "—"} />
      <Row
        label={t("info.projectDirectory")}
        value={root ?? "—"}
        hint={project?.custom_directory ? undefined : t("info.auto")}
      />
      {cwd && root && normalize(cwd) !== normalize(root) && (
        <Row label={t("info.currentDirectory")} value={cwd} />
      )}
      <hr className="border-white/[0.08]" />
      <Row
        label={t("info.shell")}
        value={
          projSession
            ? `${projSession.shell_name}${projSession.pid ? `${t("info.pidSep")}${projSession.pid}` : ""}`
            : "—"
        }
      />
      <Row label={t("info.status")} value={projSession ? (projSession.has_exited ? t("info.exited") : t("info.running")) : "—"} />
      {git?.is_repo && (
        <>
          <hr className="border-white/[0.08]" />
          <Row label={t("info.branch")} value={git.branch ?? t("info.detached")} />
          <Row
            label={t("info.remote")}
            value={
              git.upstream
                ? `${git.upstream}${git.ahead > 0 ? ` ↑${git.ahead}` : ""}${git.behind > 0 ? ` ↓${git.behind}` : ""}`
                : t("info.unpublished")
            }
          />
        </>
      )}
      {shellPid != null && (
        <>
          <hr className="border-white/[0.08]" />
          <Section label={t("info.processes")}>
            {procs.length === 0 ? (
              <div className="text-[11px] text-muster-muted/70 px-1">{t("info.noRunningProcesses")}</div>
            ) : (
              procs.map((p) => (
                <ProcRow key={p.pid} p={p} onKill={kill} onMenu={procMenu} />
              ))
            )}
          </Section>
          <Section label={t("info.ports")}>
            {ports.length === 0 ? (
              <div className="text-[11px] text-muster-muted/70 px-1">{t("info.noListeningPorts")}</div>
            ) : (
              ports.map((pt) => (
                <PortRow key={pt.port} pt={pt} onMenu={portMenu} />
              ))
            )}
          </Section>
        </>
      )}
    </div>
  );
}

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

function Row({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div>
      <div className="text-[10px] text-muster-muted uppercase tracking-wide">{label}</div>
      <div className="text-[11px] text-muster-fg/80 break-all mt-0.5">
        {value}
        {hint && <span className="text-muster-muted/70 ml-1">{hint}</span>}
      </div>
    </div>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="text-[10px] text-muster-muted uppercase tracking-wide">{label}</div>
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
      className="group flex items-center gap-1.5 px-1 py-0.5 -mx-1 rounded hover:bg-muster-hover text-[11px] text-muster-fg/80"
      onContextMenu={(e) => onMenu(e, p)}
    >
      <span className="w-1.5 h-1.5 rounded-full bg-green-500 flex-shrink-0" />
      <span className="truncate min-w-0 flex-1" title={p.exe || p.name}>
        {p.name}
      </span>
      <span className="text-muster-muted flex-shrink-0">{p.pid}</span>
      <span className="text-muster-muted flex-shrink-0 text-right">
        {p.cpu.toFixed(0)}% · {formatMem(p.mem_bytes)}
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
      className="group flex items-center gap-1.5 px-1 py-0.5 -mx-1 rounded hover:bg-muster-hover text-[11px] text-muster-fg/80 cursor-pointer"
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
