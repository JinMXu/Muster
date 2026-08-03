import { useEffect, useRef, useState } from "react";
import { getSearchAddon } from "../lib/terminalRegistry";
import { closeTerminalSearch } from "../lib/terminalSearch";
import { useT } from "../lib/i18n/context";
import { IconChevronDown, IconChevronUp, IconX } from "./icons";

interface Props {
  sessionId: string;
  /// Bumped by the store on every open; used as the React key so the input
  /// re-mounts (and refocuses) even when re-opening on the same session.
  nonce: number;
}

/// Floating scrollback search bar for one terminal pane (Ctrl+F). Incremental
/// highlighting while typing, Enter / Shift+Enter to move between matches,
/// Esc or the X button to close.
export default function TerminalSearchBar({ sessionId, nonce }: Props) {
  const { t } = useT();
  const [query, setQuery] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [count, setCount] = useState<{ index: number; total: number } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const addon = getSearchAddon(sessionId);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [nonce]);

  // Live result counter (updated by the addon after each find).
  useEffect(() => {
    if (!addon) return;
    const d = addon.onDidChangeResults((r) =>
      setCount({ index: r.resultIndex, total: r.resultCount })
    );
    return () => d.dispose();
  }, [addon]);

  const find = (dir: "next" | "prev") => {
    if (!addon || !query) return;
    if (dir === "next") addon.findNext(query, { caseSensitive, incremental: false });
    else addon.findPrevious(query, { caseSensitive });
  };

  // Incremental search while typing; clear decorations when emptied.
  useEffect(() => {
    if (!addon) return;
    if (!query) {
      addon.clearDecorations();
      setCount(null);
      return;
    }
    const id = setTimeout(() => {
      addon.findNext(query, { caseSensitive, incremental: true });
    }, 120);
    return () => clearTimeout(id);
  }, [query, caseSensitive, addon]);

  // Clean up decorations when the bar closes (unmount).
  useEffect(() => {
    return () => {
      addon?.clearDecorations();
    };
  }, [addon]);

  return (
    <div className="absolute top-2 right-2 z-20 flex items-center gap-1 bg-muster-panel border border-white/[0.08] rounded-md shadow-[0_4px_12px_rgba(0,0,0,0.4)] px-2 py-1">
      <input
        ref={inputRef}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            find(e.shiftKey ? "prev" : "next");
          } else if (e.key === "Escape") {
            e.preventDefault();
            closeTerminalSearch();
          }
        }}
        placeholder={t("terminal.searchPlaceholder")}
        spellCheck={false}
        className="w-44 bg-transparent outline-none ui-fs-sm text-muster-fg placeholder:text-muster-muted/60"
      />
      {count && count.total > 0 && (
        <span className="ui-fs-xs text-muster-muted tabular-nums whitespace-nowrap">
          {count.index + 1}/{count.total}
        </span>
      )}
      <button
        title={t("terminal.searchCaseSensitive")}
        onClick={() => setCaseSensitive((v) => !v)}
        className={`px-1 rounded ui-fs-xs font-mono leading-none ${
          caseSensitive ? "bg-muster-accent text-white" : "text-muster-muted hover:bg-muster-hover-btn"
        }`}
      >
        Aa
      </button>
      <button
        title={t("terminal.searchPrevious")}
        onClick={() => find("prev")}
        className="px-1 rounded text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg flex items-center"
      >
        <IconChevronUp size={12} />
      </button>
      <button
        title={t("terminal.searchNext")}
        onClick={() => find("next")}
        className="px-1 rounded text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg flex items-center"
      >
        <IconChevronDown size={12} />
      </button>
      <button
        title={t("common.close")}
        onClick={closeTerminalSearch}
        className="px-1 rounded text-muster-muted hover:bg-muster-hover-btn hover:text-muster-fg flex items-center"
      >
        <IconX size={12} />
      </button>
    </div>
  );
}
