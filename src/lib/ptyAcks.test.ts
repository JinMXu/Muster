import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPtyAckBatcher } from "./ptyAcks";

describe("createPtyAckBatcher", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("batches small acknowledgements on the timer", () => {
    const send = vi.fn();
    const acks = createPtyAckBatcher(send, { batchBytes: 1024, flushMs: 100 });

    acks.queue("s1", 100);
    acks.queue("s1", 200);
    expect(send).not.toHaveBeenCalled();

    vi.advanceTimersByTime(99);
    expect(send).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith("s1", 300);
  });

  it("flushes immediately when the batch threshold is reached", () => {
    const send = vi.fn();
    const acks = createPtyAckBatcher(send, { batchBytes: 1000, flushMs: 100 });

    acks.queue("s1", 600);
    acks.queue("s1", 500);
    expect(send).toHaveBeenCalledWith("s1", 1100);

    // The threshold flush cancels the pending timer.
    vi.advanceTimersByTime(100);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("keeps sessions independent and forget drops pending credit", () => {
    const send = vi.fn();
    const acks = createPtyAckBatcher(send, { batchBytes: 1000, flushMs: 100 });

    acks.queue("a", 400);
    acks.queue("b", 300);
    acks.forget("a");

    vi.advanceTimersByTime(100);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith("b", 300);
  });

  it("ignores non-positive byte counts", () => {
    const send = vi.fn();
    const acks = createPtyAckBatcher(send, { batchBytes: 1000, flushMs: 100 });

    acks.queue("s1", 0);
    acks.queue("s1", -5);
    vi.advanceTimersByTime(200);
    expect(send).not.toHaveBeenCalled();
  });
});
