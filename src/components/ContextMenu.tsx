import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { closeMenu, useMenuState } from "../lib/menuStore";

/// The single right-click menu for the whole app, driven by `menuStore`.
/// Closes on backdrop click, Escape, or when an item is chosen; the position
/// is clamped so the menu never overflows the viewport.
export default function ContextMenu() {
  const menu = useMenuState();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);

  // Measure after render, then clamp into the viewport (hidden until then so
  // the menu doesn't flash at the unclamped spot).
  useLayoutEffect(() => {
    if (!menu) {
      setPos(null);
      return;
    }
    const el = ref.current;
    const w = el?.offsetWidth ?? 180;
    const h = el?.offsetHeight ?? menu.items.length * 28;
    setPos({
      x: Math.max(4, Math.min(menu.x, window.innerWidth - w - 4)),
      y: Math.max(4, Math.min(menu.y, window.innerHeight - h - 4)),
    });
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        closeMenu();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [menu]);

  if (!menu) return null;

  return (
    <div
      className="fixed inset-0 z-50"
      onClick={closeMenu}
      onContextMenu={(e) => {
        e.preventDefault();
        closeMenu();
      }}
    >
      <div
        ref={ref}
        className="absolute min-w-[180px] bg-muster-bg rounded-[10px] border border-white/[0.08] shadow-[0_12px_32px_rgba(0,0,0,0.5)] p-1.5 muster-pop"
        style={{
          left: pos?.x ?? menu.x,
          top: pos?.y ?? menu.y,
          visibility: pos ? "visible" : "hidden",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {menu.items.map((item, i) =>
          item === "sep" ? (
            <div key={i} className="my-1 border-t border-white/[0.08]" />
          ) : (
            <button
              key={i}
              disabled={item.disabled}
              onClick={() => {
                closeMenu();
                item.action();
              }}
              className={`w-full px-2.5 h-7 rounded-md text-[12.5px] text-left flex items-center disabled:opacity-40 enabled:hover:bg-white/[0.09] ${
                item.danger ? "text-red-400" : "text-muster-muted enabled:hover:text-muster-fg"
              }`}
            >
              {item.label}
            </button>
          )
        )}
      </div>
    </div>
  );
}
