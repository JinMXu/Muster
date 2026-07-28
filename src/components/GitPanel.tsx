import { useEffect, useRef, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { AppStateView, GitGuard, GitStatusEntry, GitStatusInfo } from "../lib/types";
import { api } from "../lib/invoke";
import { useProjectCwd } from "../lib/useProjectCwd";
import { openMenu, type MenuEntry } from "../lib/menuStore";
import { IconChevronDown, IconGitBranch, IconSearch } from "./icons";
import { useT } from "../lib/i18n/context";

/// One entry in the operation banner: a git mutation with running/ok/failed
/// state and an expandable transcript (`$ command` + result/error).
type GitOp = {
  id: number;
  label: string;
  status: "running" | "ok" | "failed";
  output: string;
  expanded: boolean;
};

/// Pending discard confirmation: the guard snapshots the file(s) + HEAD +
/// branch when the dialog opens and is re-validated on confirm. "single" is
/// one entry's hover/menu discard; "all" is the Changes section batch
/// (entries listed from the guard itself).
type DiscardTarget =
  | { kind: "single"; path: string; is_untracked: boolean; guard: GitGuard }
  | { kind: "all"; has_untracked: boolean; guard: GitGuard };

/// Which status section a row belongs to (drives the context menu's
/// stage/unstage/resolve item and whether Open Changes shows the staged diff).
type SectionKind = "merge" | "staged" | "changes";

/// Git panel: status, stage/unstage, commit, branch operations.
/// Polls every 2 seconds. Mutations report through the ops banner; discard
/// goes through a guarded inline confirmation.
export default function GitPanel({ state }: { state: AppStateView | null }) {
  const { root: cwd } = useProjectCwd(state);
  const [info, setInfo] = useState<GitStatusInfo | null>(null);
  const [message, setMessage] = useState("");
  const [ops, setOps] = useState<GitOp[]>([]);
  const [discardTarget, setDiscardTarget] = useState<DiscardTarget | null>(null);
  const [filterOpen, setFilterOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const opSeq = useRef(0);
  const { t } = useT();

  useEffect(() => {
    if (!cwd) return;
    let alive = true;
    const tick = () => api.gitStatus(cwd!).then((i) => alive && setInfo(i));
    tick();
    const interval = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(interval);
    };
  }, [cwd]);

  /// Run one git mutation through the banner: push a running entry with the
  /// equivalent CLI line, then record the summary or the error (failed
  /// entries auto-expand their transcript). Keeps the last 3 entries.
  const runOp = (label: string, cliHint: string, promise: Promise<string>) => {
    const id = ++opSeq.current;
    setOps((prev) =>
      [{ id, label, status: "running" as const, output: `$ ${cliHint}\n`, expanded: false }, ...prev].slice(0, 3)
    );
    promise.then(
      (summary) =>
        setOps((prev) =>
          prev.map((o) => (o.id === id ? { ...o, status: "ok" as const, output: o.output + `✓ ${summary}` } : o))
        ),
      (err) =>
        setOps((prev) =>
          prev.map((o) =>
            o.id === id ? { ...o, status: "failed" as const, output: o.output + String(err), expanded: true } : o
          )
        )
    );
  };

  const banner = (
    <OpsBanner
      ops={ops}
      onToggle={(id) => setOps((prev) => prev.map((o) => (o.id === id ? { ...o, expanded: !o.expanded } : o)))}
      onDismiss={(id) => setOps((prev) => prev.filter((o) => o.id !== id))}
    />
  );

  if (!cwd || !info) {
    return <div className="text-muster-muted text-xs p-3">{t("git.locatingRepo")}</div>;
  }
  if (!info.is_repo) {
    return (
      <div className="p-3 text-xs text-muster-muted">
        {banner}
        <div className="mb-2">{t("git.notARepo")}</div>
        <button
          onClick={() =>
            runOp(
              t("git.initializeRepo"),
              t("git.initCli"),
              api.git.init(cwd).then(() => {
                setInfo(null);
                return t("git.initialized");
              })
            )
          }
          className="px-2 py-1 rounded bg-muster-accent/80 text-white text-[11px] active:scale-[.97] transition-transform duration-muster ease-muster"
        >
          {t("git.initializeRepo")}
        </button>
      </div>
    );
  }

  // Focused terminal session (selected tab -> focused pane), used by the row
  // menu's "Insert Absolute Path in Terminal" item. Omitted when no terminal
  // is focused.
  const project = state?.projects.find((p) => p.id === state.selected_project_id) ?? null;
  const selectedTab = project?.tabs.find((t) => t.id === project.selected_tab_id) ?? null;
  const focusedSessionId = (() => {
    if (!selectedTab) return null;
    for (const col of selectedTab.columns) {
      for (const pane of col.panes) {
        if (pane.id === selectedTab.focused_pane_id && pane.content.kind === "session") {
          return pane.content.id;
        }
      }
    }
    return null;
  })();

  // Commit validation: a message is always required, conflicts block any
  // commit, and the staged-only variants need something staged. Amend needs
  // an existing HEAD.
  const hasHead = info.recent_commits.length > 0;
  const hasConflicts = info.merge_entries.length > 0;
  const msgEmpty = message.trim().length === 0;
  const stagedCount = info.staged_entries.length;
  const anythingToCommit = stagedCount + info.changed_entries.length > 0;
  const canCommitStaged = !msgEmpty && !hasConflicts && stagedCount > 0;
  const canCommitAll = !msgEmpty && !hasConflicts && anythingToCommit;
  const canAmend = !msgEmpty && !hasConflicts && hasHead && stagedCount > 0;
  const canAmendAll = !msgEmpty && !hasConflicts && hasHead && anythingToCommit;

  const doCommit = (includeAll: boolean, amend: boolean) => {
    const msg = message;
    const flag = amend ? (includeAll ? "--amend -am" : "--amend -m") : includeAll ? "-am" : "-m";
    runOp(
      amend ? t("git.amendCommit") : t("git.commit"),
      t("git.commitCli", { flag, msg }),
      api.git.commit(info.repo_root, msg, includeAll, amend).then((oid) => {
        setMessage("");
        return amend ? t("git.amended", { oid: oid.slice(0, 7) }) : t("git.committed", { oid: oid.slice(0, 7) });
      })
    );
  };

  /// Main split-button action: Commit Staged when something is staged,
  /// otherwise Stage All & Commit.
  const defaultCommit = () => doCommit(stagedCount === 0, false);
  const defaultCommitEnabled = stagedCount > 0 ? canCommitStaged : canCommitAll;

  const openCommitMenu = (ev: React.MouseEvent<HTMLButtonElement>) => {
    const rect = ev.currentTarget.getBoundingClientRect();
    openMenu({
      x: rect.right - 180,
      y: rect.bottom + 4,
      items: [
        { label: t("git.commitStaged"), disabled: !canCommitStaged, action: () => doCommit(false, false) },
        { label: t("git.stageAllAndCommit"), disabled: !canCommitAll, action: () => doCommit(true, false) },
        "sep",
        { label: t("git.amendLastCommit"), disabled: !canAmend, action: () => doCommit(false, true) },
        { label: t("git.stageAllAndAmend"), disabled: !canAmendAll, action: () => doCommit(true, true) },
      ],
    });
  };

  const stageEntry = (e: GitStatusEntry, verb?: string) =>
    runOp(
      verb ? `${verb} ${e.path}` : t("git.stagePath", { path: e.path }),
      t("git.stageCli", { path: e.path }),
      api.git.stage(info.repo_root, e.path).then(() => t("git.staged"))
    );

  const unstageEntry = (e: GitStatusEntry) =>
    runOp(
      t("git.unstagePath", { path: e.path }),
      t("git.unstageCli", { path: e.path }),
      api.git.unstage(info.repo_root, e.path).then(() => t("git.unstaged"))
    );

  const stageAll = () =>
    runOp(t("git.stageAll"), t("git.stageAllCli"), api.git.stageAll(info.repo_root).then(() => t("git.stagedAll")));

  const unstageAll = () =>
    runOp(t("git.unstageAll"), t("git.unstageAllCli"), api.git.unstageAll(info.repo_root).then(() => t("git.unstagedAll")));

  const requestDiscard = (path: string, is_untracked: boolean) => {
    const hint = is_untracked ? `git clean -f -- ${path}` : `git checkout -- ${path}`;
    api.gitGuard(info.repo_root, [path]).then(
      (guard) => setDiscardTarget({ kind: "single", path, is_untracked, guard }),
      (err) => runOp(t("git.discardPath", { path }), hint, Promise.reject(err))
    );
  };

  /// Batch discard of every changed + untracked path, through the same
  /// guarded modal as single-file discard.
  const requestDiscardAll = () => {
    const paths = info.changed_entries.map((e) => e.path);
    const has_untracked = info.changed_entries.some((e) => e.is_untracked);
    api.gitGuard(info.repo_root, paths).then(
      (guard) => setDiscardTarget({ kind: "all", has_untracked, guard }),
      (err) => runOp(t("git.discardAllChanges"), "git checkout -- .", Promise.reject(err))
    );
  };

  const confirmDiscard = () => {
    const target = discardTarget;
    setDiscardTarget(null);
    if (!target) return;
    if (target.kind === "single") {
      const hint = target.is_untracked ? `git clean -f -- ${target.path}` : `git checkout -- ${target.path}`;
      runOp(
        t("git.discardPath", { path: target.path }),
        hint,
        api.git.discardGuarded(info.repo_root, target.path, target.guard)
      );
    } else {
      runOp(
        t("git.discardAllChanges"),
        "git checkout -- . && git clean -fd",
        api.git.discardAllGuarded(info.repo_root, target.guard)
      );
    }
  };

  /// Right-click menu for a status row. `kind` picks the diff side and the
  /// stage/unstage/resolve action.
  const rowMenu = (kind: SectionKind, e: GitStatusEntry, x: number, y: number) => {
    const abs = `${info.repo_root.replace(/[\\/]+$/, "")}\\${e.path.replace(/\//g, "\\")}`;
    const items: MenuEntry[] = [
      { label: t("git.openChanges"), action: () => api.openDiff(info.repo_root, e.path, kind === "staged") },
      { label: t("git.openFile"), action: () => api.openFile(abs, false) },
      { label: t("git.openFileToSide"), action: () => api.openFile(abs, true) },
      "sep",
    ];
    if (kind === "staged") {
      items.push({ label: t("git.unstage"), action: () => unstageEntry(e) });
    } else if (kind === "merge") {
      // `git add` marks a conflicted path resolved.
      items.push({ label: t("git.markResolved"), action: () => stageEntry(e, t("git.markResolvedVerb")) });
    } else {
      items.push({ label: t("git.stage"), action: () => stageEntry(e) });
      items.push({
        label: e.is_untracked ? t("git.moveToTrash") : t("git.discardChanges"),
        danger: true,
        action: () => requestDiscard(e.path, e.is_untracked),
      });
    }
    items.push("sep");
    items.push({ label: t("common.revealInExplorer"), action: () => revealItemInDir(abs) });
    items.push({ label: t("common.copyPath"), action: () => navigator.clipboard.writeText(abs) });
    items.push({ label: t("git.copyRelativePath"), action: () => navigator.clipboard.writeText(e.path) });
    if (focusedSessionId) {
      const sessionId = focusedSessionId;
      items.push({
        label: t("git.insertPathInTerminal"),
        action: () => api.sendText(sessionId, `"${abs}" `),
      });
    }
    openMenu({ x, y, items });
  };

  // Path filter (case-insensitive substring) applied to all three sections.
  const q = filter.trim().toLowerCase();
  const match = (e: GitStatusEntry) => !q || e.path.toLowerCase().includes(q);
  const mergeEntries = info.merge_entries.filter(match);
  const stagedEntries = info.staged_entries.filter(match);
  const changedEntries = info.changed_entries.filter(match);
  const noMatches = q.length > 0 && mergeEntries.length + stagedEntries.length + changedEntries.length === 0;

  return (
    <div className="h-full flex flex-col text-xs">
      <div className="px-3 pt-2 pb-1 flex items-center gap-2">
        <span className="text-muster-accent flex items-center">
          <IconGitBranch size={13} />
        </span>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1">
            <select
              value={info.branch ?? ""}
              onChange={(e) => {
                const name = e.target.value;
                if (name && name !== info.branch) {
                  runOp(
                    t("git.switchTo", { name }),
                    t("git.switchCli", { name }),
                    api.git.switchBranch(info.repo_root, name).then(() => t("git.nowOn", { name }))
                  );
                }
              }}
              className="bg-transparent text-[12px] font-medium outline-none max-w-[150px] cursor-pointer"
              title={t("git.switchBranch")}
            >
              {!info.branch && <option value="">{t("git.detached")}</option>}
              {info.branch && !info.branches.includes(info.branch) && (
                <option value={info.branch}>{info.branch}</option>
              )}
              {info.branches.map((b) => (
                <option key={b} value={b} className="bg-muster-bg text-muster-fg">
                  {b}
                </option>
              ))}
            </select>
            <button
              title={t("git.newBranch")}
              onClick={() => {
                const name = window.prompt(t("git.newBranchPrompt"));
                if (!name) return;
                runOp(
                  t("git.createBranch", { name }),
                  t("git.createBranchCli", { name }),
                  api.git.createBranch(info.repo_root, name).then(() => t("git.createdBranch", { name }))
                );
              }}
              className="px-1 rounded text-[10px] text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg active:scale-[.97] transition-transform duration-muster ease-muster"
            >
              +
            </button>
          </div>
          <div className="text-[10px] text-muster-muted truncate">
            {info.upstream ? `${info.upstream}` : "unpublished"}
            {info.ahead > 0 && ` ↑${info.ahead}`}
            {info.behind > 0 && ` ↓${info.behind}`}
          </div>
        </div>
        <button
          title={t("git.filterFiles")}
          onClick={() => {
            if (filterOpen) setFilter("");
            setFilterOpen(!filterOpen);
          }}
          className={`px-1 rounded hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster ${
            filterOpen ? "text-muster-fg" : "text-muster-muted hover:text-muster-fg"
          }`}
        >
          <IconSearch size={12} />
        </button>
      </div>

      {banner}

      {filterOpen && (
        <div className="px-3 pb-1 flex items-center gap-1.5">
          <span className="text-muster-muted flex items-center">
            <IconSearch size={11} />
          </span>
          <input
            autoFocus
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder={t("git.filterPlaceholder")}
            className="flex-1 min-w-0 bg-white/[0.05] rounded-md px-2 py-1 text-[11px] outline-none"
          />
          {filter && (
            <button
              title={t("git.clearFilter")}
              onClick={() => setFilter("")}
              className="px-0.5 rounded text-[9px] text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
            >
              ✕
            </button>
          )}
        </div>
      )}

      <div className="px-3 py-2 space-y-1.5">
        <textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder={t("git.commitPlaceholder", { branch: info.branch ?? "HEAD" })}
          rows={3}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && defaultCommitEnabled) {
              defaultCommit();
            }
          }}
          className="w-full bg-white/[0.05] rounded-md px-2 py-1.5 text-[11px] outline-none resize-none"
        />
        <div className="flex">
          <button
            onClick={defaultCommit}
            disabled={!defaultCommitEnabled}
            className="flex-1 px-3 py-1.5 rounded-l-md bg-muster-accent/85 text-white text-[11px] disabled:opacity-40 enabled:active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            {stagedCount > 0 ? t("git.commitNStaged", { n: stagedCount }) : t("git.stageAllAndCommitBtn")}
          </button>
          <button
            onClick={openCommitMenu}
            title={t("git.commitOptions")}
            className="px-1.5 rounded-r-md bg-muster-accent/85 text-white border-l border-white/20 flex items-center active:scale-[.97] transition-transform duration-muster ease-muster"
          >
            <IconChevronDown size={11} />
          </button>
        </div>
        <div className="flex gap-1">
          <SmallButton
            onClick={() =>
              runOp(t("git.fetch"), t("git.fetchCli"), api.git.fetch(info.repo_root).then(() => t("git.fetched")))
            }
            disabled={info.remotes.length === 0}
          >
            {t("git.fetch")}
          </SmallButton>
          <SmallButton
            onClick={() =>
              runOp(t("git.pull"), t("git.pullCli"), api.git.pull(info.repo_root).then(() => t("git.pulled")))
            }
            disabled={!info.has_upstream}
          >
            {t("git.pull")}
          </SmallButton>
          <SmallButton
            onClick={() => {
              const remote = info.remotes[0] ?? "origin";
              runOp(
                t("git.push"),
                t("git.pushCli", { remote, branch: info.branch ?? "HEAD" }),
                api.git.push(info.repo_root, remote).then(() => t("git.pushed"))
              );
            }}
          >
            {t("git.push")}
          </SmallButton>
          <SmallButton
            onClick={() =>
              runOp(t("git.stash"), t("git.stashCli"), api.git.stashAll(info.repo_root).then(() => t("git.stashed")))
            }
          >
            {t("git.stash")}
          </SmallButton>
          <SmallButton
            onClick={() =>
              runOp(t("git.pop"), t("git.popCli"), api.git.stashPop(info.repo_root).then(() => t("git.popped")))
            }
            disabled={info.stash_count === 0}
          >
            {t("git.pop")}
          </SmallButton>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {mergeEntries.length > 0 && (
          <Section
            title={t("git.mergeChanges")}
            items={mergeEntries}
            onOpenDiff={(e) => api.openDiff(info.repo_root, e.path, false)}
            onRowMenu={(e, x, y) => rowMenu("merge", e, x, y)}
          />
        )}
        {stagedEntries.length > 0 && (
          <Section
            title={t("git.stagedChanges")}
            items={stagedEntries}
            onStage={unstageEntry}
            onOpenDiff={(e) => api.openDiff(info.repo_root, e.path, true)}
            onRowMenu={(e, x, y) => rowMenu("staged", e, x, y)}
            stageLabel="−"
            actions={
              !filterOpen ? (
                <HeaderButton onClick={unstageAll}>{t("git.unstageAllBtn")}</HeaderButton>
              ) : undefined
            }
          />
        )}
        <Section
          title={t("git.changes")}
          items={changedEntries}
          onStage={(e) => stageEntry(e)}
          onDiscard={(e) => requestDiscard(e.path, e.is_untracked)}
          onOpenDiff={(e) => api.openDiff(info.repo_root, e.path, false)}
          onRowMenu={(e, x, y) => rowMenu("changes", e, x, y)}
          stageLabel="+"
          actions={
            !filterOpen ? (
              <>
                <HeaderButton onClick={stageAll}>{t("git.stageAllBtn")}</HeaderButton>
                <HeaderButton danger onClick={requestDiscardAll}>
                  {t("git.discardAllBtn")}
                </HeaderButton>
              </>
            ) : undefined
          }
        />
        {noMatches && <div className="px-2 py-2 text-[10px] text-muster-muted">{t("git.noMatchingFiles")}</div>}
        {!filterOpen && info.recent_commits.length > 0 && (
          <div>
            <div className="text-[10px] text-muster-muted/80 px-2 py-1 mt-2">{t("git.recentCommits")}</div>
            {info.recent_commits.map((c) => (
              <div key={c.hash} className="px-2 py-1 hover:bg-muster-hover rounded">
                <div className="flex items-center gap-2">
                  <span className="text-muster-accent/80 text-[10px] font-mono">{c.short_hash}</span>
                  <span className="text-muster-fg/80 text-[11px] flex-1 truncate">{c.subject}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {discardTarget && (
        <div
          className="fixed inset-0 z-50 bg-black/30 flex items-center justify-center"
          onClick={() => setDiscardTarget(null)}
        >
          <div
            className="w-[380px] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] p-4 muster-pop"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="text-[13px] font-medium mb-1">{t("git.discardConfirmTitle")}</div>
            <div className="text-[11px] text-muster-muted mb-2">
              {discardTarget.kind === "single"
                ? discardTarget.is_untracked
                  ? t("git.discardConfirmSingularUntracked")
                  : t("git.discardConfirmSingularTracked")
                : discardTarget.has_untracked
                  ? t("git.discardConfirmPluralMixed", { n: discardTarget.guard.entries.length })
                  : t("git.discardConfirmPluralTracked", { n: discardTarget.guard.entries.length })}
            </div>
            <div className="max-h-32 overflow-y-auto mb-4 space-y-0.5">
              {discardTarget.kind === "single" ? (
                <div className="text-[12px] text-muster-fg/80 truncate">• {discardTarget.path}</div>
              ) : (
                discardTarget.guard.entries.map((entry) => (
                  <div key={entry.path} className="text-[12px] text-muster-fg/80 truncate">
                    • {entry.path}
                  </div>
                ))
              )}
            </div>
            <div className="flex justify-end gap-2 text-[11px]">
              <button
                className="px-2.5 py-1 rounded-md text-muster-muted hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
                onClick={() => setDiscardTarget(null)}
              >
                {t("common.cancel")}
              </button>
              <button
                className="px-2.5 py-1 rounded-md text-red-400 hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster"
                onClick={confirmDiscard}
              >
                {t("git.discard")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/// Operation banner strip: one row per recent git mutation, expandable
/// transcript (auto-expanded on failure) and dismissible.
function OpsBanner({
  ops,
  onToggle,
  onDismiss,
}: {
  ops: GitOp[];
  onToggle: (id: number) => void;
  onDismiss: (id: number) => void;
}) {
  const { t } = useT();
  if (ops.length === 0) return null;
  return (
    <div className="px-2 pb-1 space-y-1">
      {ops.map((op) => (
        <div key={op.id} className="bg-muster-panel rounded px-2 py-1">
          <div className="flex items-center gap-1.5">
            {op.status === "running" && (
              <span className="w-2.5 h-2.5 shrink-0 rounded-full border border-muster-muted/40 border-t-muster-accent animate-spin" />
            )}
            {op.status === "ok" && <span className="text-green-400 text-[10px]">✓</span>}
            {op.status === "failed" && <span className="text-red-400 text-[10px]">✕</span>}
            <span className="flex-1 truncate text-[11px] text-muster-fg/80">{op.label}</span>
            <button
              onClick={() => onToggle(op.id)}
              title={t("git.toggleOutput")}
              className="text-[9px] text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn rounded px-0.5 active:scale-[.97] transition-transform duration-muster ease-muster"
            >
              {op.expanded ? "▾" : "▸"}
            </button>
            <button
              onClick={() => onDismiss(op.id)}
              title={t("git.dismiss")}
              className="text-[9px] text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn rounded px-0.5 active:scale-[.97] transition-transform duration-muster ease-muster"
            >
              ✕
            </button>
          </div>
          {op.expanded && (
            <pre className="mt-1 max-h-28 overflow-auto select-text whitespace-pre-wrap break-all font-mono text-[10px] text-muster-muted">
              {op.output}
            </pre>
          )}
        </div>
      ))}
    </div>
  );
}

function Section({
  title,
  items,
  onStage,
  onDiscard,
  onOpenDiff,
  onRowMenu,
  stageLabel,
  actions,
}: {
  title: string;
  items: GitStatusEntry[];
  onStage?: (entry: GitStatusEntry) => void;
  onDiscard?: (entry: GitStatusEntry) => void;
  onOpenDiff?: (entry: GitStatusEntry) => void;
  onRowMenu?: (entry: GitStatusEntry, x: number, y: number) => void;
  stageLabel?: string;
  /// Batch operations rendered on the right side of the title row.
  actions?: React.ReactNode;
}) {
  const { t } = useT();
  if (items.length === 0) return null;
  return (
    <div className="mb-1">
      <div className="flex items-center justify-between px-2 py-1">
        <span className="text-[10px] text-muster-muted/80">
          {t("git.sectionCount", { title, count: items.length })}
        </span>
        {actions && <span className="flex items-center gap-0.5">{actions}</span>}
      </div>
      {items.map((e) => (
        <div
          key={e.path}
          className="group flex items-center gap-1 px-2 py-1 hover:bg-muster-hover rounded text-[11px]"
          onContextMenu={(ev) => {
            if (!onRowMenu) return;
            ev.preventDefault();
            onRowMenu(e, ev.clientX, ev.clientY);
          }}
        >
          <span className="text-muster-accent w-3 inline-block">{e.staged !== "." && e.staged !== "?" ? e.staged : e.unstaged}</span>
          <span className="flex-1 truncate text-muster-fg/80" onClick={() => onOpenDiff?.(e)}>
            {e.orig_path && <span className="text-muster-muted">{e.orig_path} → </span>}
            {e.path}
          </span>
          {onDiscard && (
            <button
              onClick={() => onDiscard(e)}
              title={t("git.discardRowTitle")}
              className="opacity-0 group-hover:opacity-100 text-[9px] text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn rounded px-0.5 active:scale-[.97] transition-transform duration-muster ease-muster"
            >
              ✕
            </button>
          )}
          {stageLabel && onStage && (
            <button
              onClick={() => onStage(e)}
              className="opacity-0 group-hover:opacity-100 text-[9px] text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn rounded px-0.5 active:scale-[.97] transition-transform duration-muster ease-muster"
            >
              {stageLabel}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

/// Small text button on a section title row (batch operations).
function HeaderButton({
  children,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`px-1 rounded text-[10px] hover:bg-muster-hover-btn active:scale-[.97] transition-transform duration-muster ease-muster ${
        danger ? "text-muster-muted hover:text-red-400" : "text-muster-muted hover:text-muster-fg"
      }`}
    >
      {children}
    </button>
  );
}

function SmallButton({
  children,
  onClick,
  disabled,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="flex-1 px-1.5 py-1 rounded bg-white/[0.05] text-[10px] enabled:hover:bg-muster-hover-btn disabled:opacity-40 enabled:active:scale-[.97] transition-transform duration-muster ease-muster"
    >
      {children}
    </button>
  );
}
