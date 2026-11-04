import { cn } from "./cn";

/** Pill-track segmented control for picking one of 2–4 short options. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  className,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  className?: string;
}) {
  return (
    <div className={cn("flex bg-muster-hover rounded-md p-0.5", className)}>
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          onClick={() => onChange(opt.value)}
          className={cn(
            "px-2.5 py-1 rounded ui-fs-sm transition-colors duration-muster ease-muster outline-none",
            value === opt.value
              ? "bg-muster-fill-strong text-muster-fg shadow-sm"
              : "text-muster-muted hover:text-muster-fg",
          )}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}
