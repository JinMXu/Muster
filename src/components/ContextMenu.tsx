import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { closeMenu, useMenuState } from "../lib/menuStore";
import { useEscapeClose } from "./ui/Modal";
import { MenuItem } from "./ui/MenuList";

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

  useEscapeClose(menu ? closeMenu : null);

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
        className="absolute min-w-[180px] bg-muster-bg rounded-[10px] border border-muster-hairline shadow-pop p-1.5 muster-pop"
        style={{
          left: pos?.x ?? menu.x,
          top: pos?.y ?? menu.y,
          visibility: pos ? "visible" : "hidden",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {menu.items.map((item, i) =>
          item === "sep" ? (
            <div key={i} className="my-1 border-t border-muster-hairline" />
          ) : (
            <MenuItem
              key={i}
              danger={item.danger}
              disabled={item.disabled}
              onClick={() => {
                closeMenu();
                item.action();
              }}
            >
              {item.label}
            </MenuItem>
          )
        )}
      </div>
    </div>
  );
}
