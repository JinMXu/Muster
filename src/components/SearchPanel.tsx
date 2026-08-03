import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppStateView, SearchMatch } from "../lib/types";
import { api } from "../lib/invoke";
import { useT } from "../lib/i18n/context";
import { useProjectCwd } from "../lib/useProjectCwd";
import { IconSearch, IconFile } from "./icons";

/// Map a char index into a JS string to its UTF-16 offset (Rust sends char
/// indices; emoji / astral chars need more than one UTF-16 unit).
function utf16Index(s: string, charIndex: number): number {
  let off = 0;
  for (let i = 0; i < charIndex && off < s.length; i++) {
    const cp = s.codePointAt(off) ?? 0;
    off += cp > 0xffff ? 2 : 1;
  }
  return off;
}

interface MatchRowProps {
  m: SearchMatch;
  id: string;
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
}

/// Project-wide full-text search, opened as a standalone overlay by
/// Ctrl+Shift+F (kept out of the right sidebar so it doesn't crowd the
/// other panels). Debounced query against the backend, results grouped by
/// file; ArrowUp/Down move through matches, Enter / click open the file.
export default function SearchPanel({
  state,
  onClose,
}: {
  state: AppStateView | null;
  onClose: () => void;
}) {
  const { t } = useT();
  const { root } = useProjectCwd(state);
  const [query, setQuery] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [results, setResults] = useState<SearchMatch[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(-1);
  const inputRef = useRef<HTMLInputElement>(null);
  // Guards against out-of-order responses when typing quickly.
  const seqRef = useRef(0);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const run = useCallback(
    async (q: string, cs: boolean) => {
      const trimmed = q.trim();
      if (!root || !trimmed) {
        setResults(null);
        setSearching(false);
        setError(null);
        setSelected(-1);
        return;
      }
      const seq = ++seqRef.current;
      setSearching(true);
      try {
        const res = await api.searchFiles(root, trimmed, cs);
        if (seq !== seqRef.current) return;
        setResults(res);
        setError(null);
        setSelected(-1);
      } catch (e) {
        if (seq !== seqRef.current) return;
        setError(String(e));
        setResults([]);
      } finally {
        if (seq === seqRef.current) setSearching(false);
      }
    },
    [root]
  );

  // Debounced search while typing (or toggling case sensitivity).
  useEffect(() => {
    const id = setTimeout(() => run(query, caseSensitive), 250);
    return () => clearTimeout(id);
  }, [query, caseSensitive, run]);

  const groups = useMemo(() => {
    const map = new Map<string, SearchMatch[]>();
    for (const m of results ?? []) {
      const arr = map.get(m.rel_path);
      if (arr) arr.push(m);
      else map.set(m.rel_path, [m]);
    }
    return [...map.entries()];
  }, [results]);

  // Flattened match list for selection/opening.
  const flat = useMemo(() => results ?? [], [results]);

  // Move the selection and keep it in view. Shift+Enter / ArrowUp go up.
  const move = useCallback(
    (delta: number) => {
      if (!flat.length) return;
      const next = (selected + delta + flat.length) % flat.length;
      setSelected(next);
      document.getElementById(`search-result-${next}`)?.scrollIntoView({ block: "nearest" });
    },
    [flat, selected]
  );

  const openSelected = useCallback(() => {
    if (selected >= 0 && selected < flat.length) {
      api.openFile(flat[selected].path, false);
      onClose();
    }
  }, [flat, selected, onClose]);

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      openSelected();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  const totalMatches = results?.length ?? 0;

  return (
    <div
      className="absolute inset-0 z-50 bg-black/35 flex justify-center items-start"
      onClick={onClose}
    >
      <div
        className="mt-16 w-[760px] max-w-[92vw] max-h-[72vh] flex flex-col bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] muster-pop"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Input row — flex-shrink-0 so the results list can't squash it. */}
        <div className="px-4 h-11 flex-shrink-0 flex items-center gap-2 border-b border-white/[0.08]">
          <span className="text-muster-muted flex items-center">
            <IconSearch size={15} />
          </span>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder={t("search.placeholder")}
            spellCheck={false}
            className="flex-1 bg-transparent outline-none text-[15px] text-muster-fg placeholder:text-muster-muted/60"
          />
          <label className="flex items-center gap-1 ui-fs-sm text-muster-muted cursor-pointer select-none whitespace-nowrap">
            <input
              type="checkbox"
              checked={caseSensitive}
              onChange={(e) => setCaseSensitive(e.target.checked)}
              className="accent-muster-accent"
            />
            {t("search.caseSensitive")}
          </label>
          {searching ? (
            <span className="ui-fs-sm text-muster-muted whitespace-nowrap">{t("search.searching")}</span>
          ) : totalMatches > 0 ? (
            <span className="ui-fs-sm text-muster-muted tabular-nums whitespace-nowrap">
              {t("search.results", { n: totalMatches })}
            </span>
          ) : null}
        </div>

        {/* Results */}
        <div className="flex-1 min-h-0 overflow-y-auto p-2">
          {error ? (
            <div className="ui-fs-sm text-red-400/90 px-1 py-3">{error}</div>
          ) : !root ? (
            <div className="ui-fs-sm text-muster-muted/70 px-1 py-3">{t("search.noRoot")}</div>
          ) : query.trim() === "" ? (
            <div className="ui-fs-sm text-muster-muted/70 px-1 py-3">{t("search.typeToSearch")}</div>
          ) : totalMatches === 0 && !searching ? (
            <div className="ui-fs-sm text-muster-muted/70 px-1 py-3">{t("search.noResults")}</div>
          ) : (
            groups.map(([rel, matches]) => (
              <div key={rel} className="mb-1">
                <div className="flex items-center gap-1 px-1 py-0.5 ui-fs-xs text-muster-muted/80 truncate">
                  <IconFile size={11} className="flex-shrink-0" />
                  <span className="truncate">{rel}</span>
                  <span className="text-muster-muted/50 tabular-nums flex-shrink-0">{matches.length}</span>
                </div>
                {matches.map((m) => {
                  const i = flat.findIndex((f) => f === m);
                  return (
                    <MatchRow
                      key={`${m.path}:${m.line}:${m.column}`}
                      id={`search-result-${i}`}
                      m={m}
                      selected={i === selected}
                      onSelect={() => setSelected(i)}
                      onOpen={openSelected}
                    />
                  );
                })}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function MatchRow({ m, id, selected, onSelect, onOpen }: MatchRowProps) {
  const start = utf16Index(m.line_text, m.match_start);
  const end = utf16Index(m.line_text, m.match_start + m.match_len);
  const before = m.line_text.slice(0, start);
  const hit = m.line_text.slice(start, end);
  const after = m.line_text.slice(end);

  return (
    <div
      id={id}
      onClick={onOpen}
      onMouseEnter={onSelect}
      className={`cursor-pointer rounded px-1.5 py-0.5 ui-fs-sm ${
        selected ? "bg-muster-selected text-muster-fg" : "hover:bg-muster-hover text-muster-muted"
      }`}
    >
      <span className="ui-fs-xs text-muster-muted/60 tabular-nums mr-1.5">
        {m.line}:{m.column}
      </span>
      <code className="font-mono break-all">
        {before}
        <mark className="bg-muster-accent/30 text-inherit rounded-[2px]">{hit}</mark>
        {after}
      </code>
    </div>
  );
}
