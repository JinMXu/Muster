import { useCallback, useRef, useState } from "react";

/// Resizable sidebar width persisted to localStorage. `side` is the window
/// edge the panel hugs: dragging a left panel's handle right grows it,
/// dragging a right panel's handle left grows it. Returns the width plus the
/// mousedown handler for the resize handle.
export function usePanelWidth(key: string, def: number, side: "left" | "right", min = 180, max = 520) {
  const [width, setWidth] = useState(() => {
    const v = Number(localStorage.getItem(key));
    return Number.isFinite(v) && v > 0 ? Math.min(max, Math.max(min, v)) : def;
  });
  const widthRef = useRef(width);
  widthRef.current = width;

  const onHandleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startW = widthRef.current;
      const onMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        const w = Math.min(max, Math.max(min, side === "left" ? startW + dx : startW - dx));
        widthRef.current = w;
        setWidth(w);
      };
      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        localStorage.setItem(key, String(widthRef.current));
      };
      // Keep the resize cursor and block text selection for the whole drag,
      // even when the pointer leaves the handle.
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [key, side, min, max]
  );

  return { width, onHandleMouseDown };
}
