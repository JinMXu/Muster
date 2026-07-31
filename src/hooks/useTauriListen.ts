import { useEffect, type DependencyList } from "react";
import { listen, type UnlistenFn, type Event } from "@tauri-apps/api/event";

/// Subscribe to a Tauri event and run a side-effect handler on each payload.
/// Handles the async-listen-vs-unmount race: if the component unmounts before
/// the listen() promise resolves, the listener is immediately removed.
///
/// `deps` should include any values the handler closes over (same as
/// useEffect deps). The listener is re-registered when deps change.
export function useTauriListen<T>(
  name: string,
  handler: (event: Event<T>) => void,
  deps?: DependencyList,
): void {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    listen<T>(name, handler).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps ?? [name, handler]);
}