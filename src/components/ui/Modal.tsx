import { useEffect, type ReactNode } from "react";
import { cn } from "./cn";

/** Capture-phase Escape → callback. Shared by every overlay so Esc closes
 * the topmost layer consistently. Pass a stable callback (or null to
 * disable); `active` gates re-registration without changing the callback. */
export function useEscapeClose(onEscape: (() => void) | null, active = true) {
  useEffect(() => {
    if (!onEscape || !active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onEscape();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onEscape, active]);
}

export interface ModalProps {
  onClose: () => void;
  children: ReactNode;
  /** Card width in px. */
  width?: number;
  /** "center" = vertically centered dialog; "top" = card anchored near the
   * window top (command palette, tab switcher, search panel). */
  align?: "center" | "top";
  /** Distance from the top for align="top", in px. */
  topOffset?: number;
  /** fixed (viewport) by default; false renders absolute — for overlays
   * that live inside a positioned parent (Ctrl+Tab switcher etc.). */
  fixed?: boolean;
  z?: string;
  /** true = full scrim; "dim" = light dim; false = invisible click shield. */
  scrim?: boolean | "dim";
  /** Set false when the consumer owns Escape handling (e.g. dirty-confirm). */
  closeOnEscape?: boolean;
  labelledBy?: string;
  label?: string;
  className?: string;
}

/** App-standard modal: scrim + floating card with Escape/backdrop close.
 * The card itself is unstyled inside (`className` + children decide the
 * padding and content); closing semantics are centralized here. */
export function Modal({
  onClose,
  children,
  width,
  align = "center",
  topOffset = 64,
  fixed = true,
  z = "z-50",
  scrim = true,
  closeOnEscape = true,
  labelledBy,
  label,
  className,
}: ModalProps) {
  useEscapeClose(closeOnEscape ? onClose : null);
  return (
    <div
      className={cn(
        fixed ? "fixed" : "absolute",
        "inset-0 flex justify-center",
        align === "center" ? "items-center" : "items-start",
        scrim === true && "bg-muster-scrim",
        scrim === "dim" && "bg-muster-scrim-dim",
        z,
      )}
      style={align === "top" ? { paddingTop: topOffset } : undefined}
      onMouseDown={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-label={label}
        className={cn(
          "relative bg-muster-bg border border-muster-hairline rounded-[10px] shadow-pop muster-pop",
          className,
        )}
        style={width ? { width } : undefined}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}
