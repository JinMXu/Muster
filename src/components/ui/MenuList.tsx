import type { ReactNode } from "react";
import { cn } from "./cn";

/** Shared styling for the app's selectable list/menu rows (context menu,
 * command palette, tab switcher, dropdown lists). */
export function MenuList({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={cn("py-1", className)}>{children}</div>;
}

export interface MenuItemProps {
  selected?: boolean;
  danger?: boolean;
  disabled?: boolean;
  /** "sm" = h-7 (context menu, palette), "md" = h-8 (tab switcher). */
  height?: "sm" | "md";
  onClick?: () => void;
  onMouseEnter?: () => void;
  children: ReactNode;
  className?: string;
  title?: string;
  /** data-idx hook for scroll-into-view callers (tab switcher). */
  dataIdx?: number;
}

export function MenuItem({
  selected,
  danger,
  disabled,
  height = "sm",
  onClick,
  onMouseEnter,
  children,
  className,
  title,
  dataIdx,
}: MenuItemProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={onMouseEnter}
      disabled={disabled}
      title={title}
      data-idx={dataIdx}
      className={cn(
        "w-full flex items-center gap-2 px-2.5 rounded-md text-[12.5px] text-left outline-none",
        "transition-colors duration-muster ease-muster",
        height === "sm" ? "h-7" : "h-8",
        disabled
          ? "opacity-40 pointer-events-none"
          : danger
            ? "text-red-400 hover:bg-muster-selected"
            : selected
              ? "bg-muster-selected text-muster-fg"
              : "text-muster-muted hover:bg-muster-selected hover:text-muster-fg",
        className,
      )}
    >
      {children}
    </button>
  );
}
