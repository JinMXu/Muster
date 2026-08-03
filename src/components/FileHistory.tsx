import { useEffect, useMemo, useRef, useState } from "react";
import type { FileCommit } from "../lib/types";
import { api } from "../lib/invoke";
import { useT } from "../lib/i18n/context";
import { IconGitBranch, IconX } from "./icons";

/// File history overlay (opened from a Git panel row's context menu): lists
/// every commit that touched the file, newest first. Click a commit to diff
/// it against its parent; or set a "compare base" and click another commit to
/// diff any two versions of the file.
export default function FileHistory({
  repoRoot,
  path,
  onClose,
}: {
  repoRoot: string;
  path: string;
  onClose: () => void;
}) {
  const { t } = useT();
  const [commits, setCommits] = useState<FileCommit[] | null>(null);
  const [base, setBase] = useState<FileCommit | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api.git.fileHistory(repoRoot, path).then(
      (c) => alive && setCommits(c),
      (e) => alive && setError(String(e))
    );
    return () => {
      alive = false;
    };
  }, [repoRoot, path]);

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

  /// Open a diff of the file between two commits, ordered chronologically.
  const compare = (a: FileCommit, b: FileCommit) => {
    const [oldC, newC] = a.date_ms <= b.date_ms ? [a, b] : [b, a];
    api.openCommitDiff(repoRoot, path, oldC.hash, newC.hash);
    onClose();
  };

  const onClick = (c: FileCommit) => {
    if (base) {
      if (base.hash === c.hash) {
        setBase(null); // click the base row again to clear it
        return;
      }
      compare(base, c);
      return;
    }
    // No base: diff this commit against its parent (empty parent = the file
    // was added in this commit, so the old side is blank).
    api.openCommitDiff(repoRoot, path, c.parent ?? "", c.hash);
    onClose();
  };

  const fileName = useMemo(() => {
    const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return idx >= 0 ? path.slice(idx + 1) : path;
  }, [path]);

  return (
    <div className="absolute inset-0 z-50 bg-black/35 flex justify-center items-start" onClick={onClose}>
      <div
        className="mt-16 w-[440px] max-w-[92vw] max-h-[70vh] flex flex-col bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] muster-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 h-11 flex-shrink-0 flex items-center gap-2 border-b border-white/[0.08]">
          <span className="text-muster-accent flex items-center">
            <IconGitBranch size={14} />
          </span>
          <div className="flex-1 min-w-0">
            <div className="ui-fs-base font-medium truncate">{fileName}</div>
            <div className="ui-fs-xs text-muster-muted truncate" title={path}>
              {path}
            </div>
          </div>
          <button
            onClick={onClose}
            title={t("common.close")}
            className="px-1 rounded text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg flex items-center"
          >
            <IconX size={13} />
          </button>
        </div>

        {base && (
          <div className="flex-shrink-0 px-3 py-1.5 flex items-center gap-2 bg-white/[0.03] border-b border-white/[0.08]">
            <span className="ui-fs-sm text-muster-fg/80 flex-1 truncate">
              {t("fileHistory.comparing", { hash: base.short_hash })}
            </span>
            <button
              onClick={() => setBase(null)}
              className="ui-fs-xs text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn rounded px-1.5 py-0.5"
            >
              {t("fileHistory.clearBase")}
            </button>
          </div>
        )}

        <div className="flex-1 min-h-0 overflow-y-auto p-1.5">
          {error ? (
            <div className="ui-fs-sm text-red-400/90 px-2 py-3">{error}</div>
          ) : commits === null ? (
            <div className="ui-fs-sm text-muster-muted/70 px-2 py-3">{t("fileHistory.loading")}</div>
          ) : commits.length === 0 ? (
            <div className="ui-fs-sm text-muster-muted/70 px-2 py-3">{t("fileHistory.noCommits")}</div>
          ) : (
            commits.map((c) => {
              const isBase = base?.hash === c.hash;
              return (
                <div
                  key={c.hash}
                  onClick={() => onClick(c)}
                  title={base ? (isBase ? t("fileHistory.clearBase") : t("fileHistory.compareWith", { hash: base.short_hash })) : t("fileHistory.vsParent")}
                  className={`group flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer ui-fs-sm ${
                    isBase ? "bg-muster-accent/15" : "hover:bg-muster-hover"
                  }`}
                >
                  <span className={`font-mono text-muster-accent/80 ${isBase ? "font-medium" : ""}`}>{c.short_hash}</span>
                  <span className="flex-1 min-w-0">
                    <span className="block truncate text-muster-fg/85">{c.subject}</span>
                    <span className="block ui-fs-xs text-muster-muted/80 truncate">
                      {c.author} · {c.relative_date}
                    </span>
                  </span>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setBase(isBase ? null : c);
                    }}
                    title={isBase ? t("fileHistory.clearBase") : t("fileHistory.setBase")}
                    className={`ui-fs-2xs rounded px-1 py-0.5 flex-shrink-0 ${
                      isBase
                        ? "bg-muster-accent text-white"
                        : "opacity-0 group-hover:opacity-100 text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn"
                    }`}
                  >
                    {base ? "↔" : "⊞"}
                  </button>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
