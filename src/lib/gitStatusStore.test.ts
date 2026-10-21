import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GitStatusInfo } from "./types";

const mocks = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  gitStatus: vi.fn(),
  gitStatusRevision: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(name, handler);
    return () => {
      mocks.listeners.delete(name);
    };
  }),
}));

vi.mock("./invoke", () => ({
  api: {
    gitStatus: mocks.gitStatus,
    gitStatusRevision: mocks.gitStatusRevision,
  },
}));

function makeInfo(revision: string): GitStatusInfo {
  return {
    is_repo: true,
    repo_root: "D:\\repo",
    root_path: "D:\\repo",
    branch: "main",
    upstream: null,
    ahead: 0,
    behind: 0,
    has_upstream: false,
    merge_entries: [],
    staged_entries: [],
    changed_entries: [],
    line_additions: 0,
    line_deletions: 0,
    branches: [],
    remotes: [],
    recent_commits: [],
    stash_count: 0,
    error: null,
    is_worktree: false,
    revision,
  };
}

async function loadStore() {
  vi.resetModules();
  return import("./gitStatusStore");
}

async function flush() {
  await act(async () => {
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
  });
}

describe("gitStatusStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.listeners.clear();
    mocks.gitStatus.mockReset();
    mocks.gitStatusRevision.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("loads the full status on the first subscription", async () => {
    mocks.gitStatus.mockResolvedValue(makeInfo("r1"));
    mocks.gitStatusRevision.mockResolvedValue("r1");
    const store = await loadStore();

    const { result } = renderHook(() => store.useGitStatus("D:\\repo", true));
    await flush();

    expect(mocks.gitStatus).toHaveBeenCalledTimes(1);
    expect(mocks.gitStatus).toHaveBeenCalledWith("D:\\repo", true);
    expect(result.current?.revision).toBe("r1");
  });

  it("only requests a full status when the revision changes", async () => {
    mocks.gitStatus.mockResolvedValue(makeInfo("r1"));
    mocks.gitStatusRevision.mockResolvedValue("r1");
    const store = await loadStore();

    const { unmount } = renderHook(() => store.useGitStatus("D:\\repo", true));
    await flush();
    expect(mocks.gitStatus).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(8_000);
    });
    expect(mocks.gitStatusRevision).toHaveBeenCalled();
    expect(mocks.gitStatus).toHaveBeenCalledTimes(1);

    mocks.gitStatusRevision.mockResolvedValue("r2");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(8_000);
    });
    expect(mocks.gitStatus).toHaveBeenCalledTimes(2);
    expect(mocks.gitStatus).toHaveBeenLastCalledWith("D:\\repo", false);
    unmount();
  });

  it("forces a refresh on an fs-changed event under the repo root", async () => {
    mocks.gitStatus.mockResolvedValue(makeInfo("r1"));
    mocks.gitStatusRevision.mockResolvedValue("r1");
    const store = await loadStore();

    const { unmount } = renderHook(() => store.useGitStatus("D:\\repo", true));
    await flush();
    expect(mocks.gitStatus).toHaveBeenCalledTimes(1);

    const onFsChanged = mocks.listeners.get("fs-changed");
    expect(onFsChanged).toBeDefined();
    await act(async () => {
      onFsChanged?.({ payload: { dir: "D:\\repo\\src" } });
      await vi.advanceTimersByTimeAsync(2_000);
    });

    expect(mocks.gitStatus).toHaveBeenCalledTimes(2);
    expect(mocks.gitStatus).toHaveBeenLastCalledWith("D:\\repo", true);
    unmount();
  });

  it("stops polling once the last subscriber unmounts", async () => {
    mocks.gitStatus.mockResolvedValue(makeInfo("r1"));
    mocks.gitStatusRevision.mockResolvedValue("r1");
    const store = await loadStore();

    const { unmount } = renderHook(() => store.useGitStatus("D:\\repo", true));
    await flush();
    const probes = mocks.gitStatusRevision.mock.calls.length;

    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(mocks.gitStatusRevision).toHaveBeenCalledTimes(probes);
  });
});
