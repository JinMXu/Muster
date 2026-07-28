import { useSyncExternalStore } from "react";

export interface MenuItem {
  label: string;
  danger?: boolean;
  disabled?: boolean;
  action: () => void;
}

export type MenuEntry = MenuItem | "sep";

export interface MenuState {
  x: number;
  y: number;
  items: MenuEntry[];
}

/// Tiny module-level store for the one global context menu. Any component can
/// open a menu without prop-drilling; <ContextMenu /> (mounted once in App)
/// renders whatever is open.
let current: MenuState | null = null;
const listeners = new Set<() => void>();

const notify = () => listeners.forEach((l) => l());

export function openMenu(state: MenuState) {
  current = state;
  notify();
}

export function closeMenu() {
  if (current === null) return;
  current = null;
  notify();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useMenuState(): MenuState | null {
  return useSyncExternalStore(subscribe, () => current);
}
