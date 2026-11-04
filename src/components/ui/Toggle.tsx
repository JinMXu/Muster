import { cn } from "./cn";

/** iOS-style switch. 36×20 track, 16px knob, accent when on. */
export function Toggle({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  /** Accessible name when the switch stands alone in a row. */
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative w-9 h-5 rounded-full transition-colors duration-muster ease-muster flex-shrink-0 outline-none",
        "disabled:opacity-40 disabled:pointer-events-none",
        checked ? "bg-muster-accent" : "bg-muster-fill-strong",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform duration-muster ease-muster",
          checked ? "translate-x-4" : "translate-x-0",
        )}
      />
    </button>
  );
}
