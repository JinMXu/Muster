/// Batches PTY flow-control acknowledgements from the terminal renderer so a
/// flood of `pty:data` events doesn't turn into one IPC call per chunk.
///
/// Backend credit is `MAX_PTY_IN_FLIGHT` (2 MiB). Flushing every 256 KiB
/// keeps the pump fed even when timers are throttled (hidden WebView), while
/// the 100ms timer bounds latency for low-volume streams.
export interface PtyAckBatcher {
  /** Add consumed bytes; flushes immediately once `batchBytes` is reached. */
  queue(sessionId: string, bytes: number): void;
  /** Send everything pending for a session now. */
  flush(sessionId: string): void;
  /** Drop pending bytes without acknowledging (session closed). */
  forget(sessionId: string): void;
}

export function createPtyAckBatcher(
  send: (sessionId: string, bytes: number) => void,
  opts: { batchBytes?: number; flushMs?: number } = {}
): PtyAckBatcher {
  const batchBytes = opts.batchBytes ?? 256 * 1024;
  const flushMs = opts.flushMs ?? 100;
  const pending = new Map<string, number>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  const flush = (sessionId: string) => {
    const timer = timers.get(sessionId);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.delete(sessionId);
    }
    const bytes = pending.get(sessionId) ?? 0;
    pending.delete(sessionId);
    if (bytes > 0) send(sessionId, bytes);
  };

  return {
    queue(sessionId, bytes) {
      if (bytes <= 0) return;
      const total = (pending.get(sessionId) ?? 0) + bytes;
      pending.set(sessionId, total);
      if (total >= batchBytes) {
        flush(sessionId);
        return;
      }
      if (!timers.has(sessionId)) {
        timers.set(sessionId, setTimeout(() => flush(sessionId), flushMs));
      }
    },
    flush,
    forget(sessionId) {
      const timer = timers.get(sessionId);
      if (timer !== undefined) {
        clearTimeout(timer);
        timers.delete(sessionId);
      }
      pending.delete(sessionId);
    },
  };
}
