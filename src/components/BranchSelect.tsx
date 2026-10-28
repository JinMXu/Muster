import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { fuzzyScore, prepareFuzzyText } from "../lib/fuzzy";
import { useT } from "../lib/i18n/context";
import { IconCheck, IconChevronDown, IconGitBranch, IconPlus, IconSearch } from "./icons";

/// Branch picker for the Git panel header: a styled trigger (icon + name +
/// chevron) opening an anchored popover with fuzzy filtering, a check mark
/// on the current branch, full keyboard navigation and a "new branch" action.
/// Replaces the native <select>, which WebView2 renders with the plain OS
/// styling that clashes with the app theme.
export default function BranchSelect({
  branch,
  branches,
  onSelect,
  onCreateBranch,
}: {
  /// Current branch name; null when HEAD is detached.
  branch: string | null;
  /// Local branch names (sorted by the backend).
  branches: string[];
  /// Switch to a branch (already guarded against no-op re-selects).
  onSelect: (name: string) => void;
  /// Open the create-branch flow.
  onCreateBranch: () => void;
}) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<Map<number, HTMLButtonElement>>(new Map());

  // Filtered + fuzzy-ranked branch list; the current branch stays listed
  // even when the query doesn't match it, so the check mark is always
  // visible while filtering.
  const filtered = useMemo(() => {
    const q = query.trim();
    const all = branch && !branches.includes(branch) ? [branch, ...branches] : branches;
    if (!q) return all;
    const pat = Array.from(q.toLowerCase());
    return all
      .map((b) => ({ b, s: fuzzyScore(prepareFuzzyText(b), pat) }))
      .filter((x) => x.s != null)
      .sort((a, b) => b.s! - a.s!)
      .map((x) => x.b);
  }, [branches, branch, query]);

  // Reset keyboard selection whenever the list or query changes.
  useEffect(() => setActive(0), [query, open]);
  // Keep the keyboard-selected row in view.
  useEffect(() => {
    rowRefs.current.get(active)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  // Escape closes even when focus has drifted off the filter input (e.g.
  // onto the clear button).
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Measure the popover after render and clamp it into the viewport
  // (flipping above the trigger when there is no room below). Hidden until
  // positioned so it doesn't flash at the unclamped spot.
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    const trigger = triggerRef.current;
    const el = popRef.current;
    if (!trigger || !el) return;
    const r = trigger.getBoundingClientRect();
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const left = Math.max(8, Math.min(r.left, window.innerWidth - w - 8));
    let top = r.bottom + 6;
    if (top + h > window.innerHeight - 8) {
      const above = r.top - 6 - h;
      top = above >= 8 ? above : Math.max(8, window.innerHeight - h - 8);
    }
    setPos({ left, top });
  }, [open, filtered.length]);

  const close = () => {
    setOpen(false);
    setQuery("");
  };

  const pick = (name: string) => {
    close();
    if (name !== branch) onSelect(name);
  };

  if (!branches.length && !branch) {
    // Not a repo worth showing a picker for; keep the header quiet.
    return null;
  }

  return (
    <>
      <button
        ref={triggerRef}
        title={t("git.switchBranch")}
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-1 rounded-md pl-1.5 pr-1 h-6 max-w-[190px] transition-colors duration-muster ease-muster ${
          open ? "bg-muster-hover text-muster-fg" : "text-muster-fg hover:bg-muster-hover"
        }`}
      >
        <span className="text-muster-accent flex items-center flex-shrink-0">
          <IconGitBranch size={12} />
        </span>
        <span className="truncate ui-fs-base font-medium">
          {branch ?? t("git.detached")}
        </span>
        <IconChevronDown
          size={11}
          className={`flex-shrink-0 text-muster-muted/70 transition-transform duration-muster ease-muster ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {open && (
        <div className="fixed inset-0 z-50" onClick={close}>
          <div
            ref={popRef}
            className="fixed z-50 w-[248px] max-w-[calc(100vw-16px)] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] muster-pop flex flex-col overflow-hidden"
            style={{
              left: pos?.left ?? triggerRef.current?.getBoundingClientRect().left ?? 0,
              top: pos?.top ?? (triggerRef.current?.getBoundingClientRect().bottom ?? 0) + 6,
              visibility: pos ? "visible" : "hidden",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            {/* Filter input */}
            <div className="flex items-center gap-1.5 px-2.5 h-8 flex-shrink-0 border-b border-white/[0.06]">
              <span className="text-muster-muted/70 flex items-center flex-shrink-0">
                <IconSearch size={12} />
              </span>
              <input
                autoFocus
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("git.filterBranches")}
                spellCheck={false}
                className="flex-1 min-w-0 bg-transparent outline-none ui-fs-sm text-muster-fg placeholder:text-muster-muted/60"
                onKeyDown={(e) => {
                  if (e.key === "ArrowDown") {
                    e.preventDefault();
                    if (filtered.length) setActive((i) => (i + 1) % filtered.length);
                  } else if (e.key === "ArrowUp") {
                    e.preventDefault();
                    if (filtered.length) setActive((i) => (i - 1 + filtered.length) % filtered.length);
                  } else if (e.key === "Enter") {
                    e.preventDefault();
                    const name = filtered[active];
                    if (name) pick(name);
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    close();
                  }
                }}
              />
              {query && (
                <button
                  onClick={() => setQuery("")}
                  title={t("git.clearFilter")}
                  className="text-muster-muted hover:text-muster-fg flex items-center flex-shrink-0"
                >
                  ✕
                </button>
              )}
            </div>

            {/* Branch list */}
            <div className="flex-1 min-h-0 overflow-y-auto p-1 max-h-[264px]">
              {filtered.length === 0 ? (
                <div className="ui-fs-sm text-muster-muted/70 px-2 py-2.5">
                  {t("git.noBranchMatch")}
                </div>
              ) : (
                filtered.map((b, i) => {
                  const isCurrent = b === branch;
                  return (
                    <button
                      key={b}
                      ref={(el) => {
                        if (el) rowRefs.current.set(i, el);
                        else rowRefs.current.delete(i);
                      }}
                      onClick={() => pick(b)}
                      onMouseEnter={() => setActive(i)}
                      title={b}
                      className={`w-full px-2 h-7 rounded-md flex items-center gap-1.5 ui-fs-sm text-left ${
                        i === active
                          ? "bg-white/[0.09] text-muster-fg"
                          : isCurrent
                            ? "text-muster-fg/90"
                            : "text-muster-muted hover:text-muster-fg"
                      }`}
                    >
                      <span className="w-3 flex-shrink-0 flex items-center text-muster-accent">
                        {isCurrent && <IconCheck size={11} />}
                      </span>
                      <span className="truncate">{b}</span>
                    </button>
                  );
                })
              )}
            </div>

            {/* Footer actions */}
            <div className="flex-shrink-0 border-t border-white/[0.06] p-1">
              <button
                onClick={() => {
                  close();
                  onCreateBranch();
                }}
                className="w-full px-2 h-7 rounded-md flex items-center gap-1.5 ui-fs-sm text-muster-muted hover:bg-white/[0.09] hover:text-muster-fg text-left"
              >
                <span className="w-3 flex-shrink-0 flex items-center">
                  <IconPlus size={11} />
                </span>
                {t("git.newBranch")}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
