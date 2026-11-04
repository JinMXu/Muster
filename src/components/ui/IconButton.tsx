import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

export type IconButtonSize = "xs" | "sm" | "md";

const SIZES: Record<IconButtonSize, string> = {
  xs: "w-4 h-4 rounded",
  sm: "w-5 h-5 rounded-md",
  md: "w-6 h-6 rounded-md",
};

/** Square ghost button for a lone icon: muted until hovered. */
export const IconButton = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { size?: IconButtonSize }>(
  function IconButton({ size = "sm", className, type, ...rest }, ref) {
    return (
      <button
        ref={ref}
        type={type ?? "button"}
        className={cn(
          "flex items-center justify-center flex-shrink-0 outline-none",
          "text-muster-muted hover:text-muster-fg hover:bg-muster-hover-btn",
          "transition-colors duration-muster ease-muster",
          "disabled:opacity-40 disabled:pointer-events-none",
          SIZES[size],
          className,
        )}
        {...rest}
      />
    );
  },
);
