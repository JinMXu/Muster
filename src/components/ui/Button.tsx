import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

export type ButtonVariant =
  | "primary" // accent fill — the one main action of a view
  | "default" // quiet solid fill — secondary actions
  | "danger" // red fill — destructive main action
  | "dangerGhost" // red text, no fill — destructive secondary
  | "warning" // yellow fill — cautionary action
  | "ghost"; // text only — toolbar/inline actions

export type ButtonSize = "xs" | "sm" | "md" | "lg";

const VARIANTS: Record<ButtonVariant, string> = {
  primary:
    "bg-muster-accent text-white hover:bg-muster-accent/85 transition-transform active:scale-[.97]",
  default:
    "bg-muster-fill text-muster-fg hover:bg-muster-hover-btn transition-transform active:scale-[.97]",
  danger:
    "bg-red-500/90 text-white hover:bg-red-500 transition-transform active:scale-[.97]",
  dangerGhost:
    "bg-transparent text-red-400 hover:bg-muster-hover-btn transition-colors",
  warning:
    "bg-yellow-600/80 text-white hover:bg-yellow-600 transition-transform active:scale-[.97]",
  ghost:
    "bg-transparent text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn transition-colors",
};

const SIZES: Record<ButtonSize, string> = {
  xs: "px-1.5 py-0.5 rounded ui-fs-xs",
  sm: "px-2.5 py-1 rounded-md ui-fs-sm",
  md: "px-3 py-1.5 rounded-md ui-fs-sm",
  lg: "px-4 py-1.5 rounded-md ui-fs-base",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Shows a spinner and blocks interaction while true. */
  loading?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "default", size = "sm", loading, className, children, disabled, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      disabled={disabled || loading}
      className={cn(
        "inline-flex items-center justify-center gap-1.5 outline-none select-none",
        "disabled:opacity-40 disabled:pointer-events-none",
        "duration-muster ease-muster",
        VARIANTS[variant],
        SIZES[size],
        className,
      )}
      {...rest}
    >
      {loading && (
        <span
          aria-hidden
          className="w-3 h-3 rounded-full border border-current border-t-transparent animate-spin flex-shrink-0"
        />
      )}
      {children}
    </button>
  );
});
