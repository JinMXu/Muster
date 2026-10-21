import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppStateView } from "./types";

const mocks = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  listAllSessions: vi.fn(),
  resolveProjectRoot: vi.fn(),
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
    listAllSessions: mocks.listAllSessions,
    resolveProjectRoot: mocks.resolveProjectRoot,
  },
}));

function stateFor(projectId: string, pinned: string | null): AppStateView {
  return {
    projects: [{ id: projectId, custom_directory: pinned }],
    selected_project_id: projectId,
  } as unknown as AppStateView;
}

async function loadHook() {
  vi.resetModules();
  return import("./useProjectCwd");
}

async function flush() {
  await act(async () => {
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
  });
}

describe("useProjectCwd", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.listeners.clear();
    mocks.listAllSessions.mockReset();
    mocks.resolveProjectRoot.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("shares one poll among all subscriber panels", async () => {
    mocks.listAllSessions.mockResolvedValue([
      { project_id: "p1", working_directory: "D:\\repo\\sub" },
    ]);
    mocks.resolveProjectRoot.mockResolvedValue("D:\\repo");
    const hook = await loadHook();
    const state = stateFor("p1", null);

    const a = renderHook(() => hook.useProjectCwd(state));
    const b = renderHook(() => hook.useProjectCwd(state));
    await flush();

    expect(mocks.listAllSessions).toHaveBeenCalledTimes(1);
    expect(a.result.current).toEqual({ cwd: "D:\\repo\\sub", root: "D:\\repo" });
    expect(b.result.current).toEqual({ cwd: "D:\\repo\\sub", root: "D:\\repo" });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(mocks.listAllSessions).toHaveBeenCalledTimes(2);
  });

  it("follows session-cwd-changed without waiting for the poll", async () => {
    mocks.listAllSessions.mockResolvedValue([
      { project_id: "p1", working_directory: "D:\\repo" },
    ]);
    mocks.resolveProjectRoot.mockImplementation(async (cwd: string) => cwd);
    const hook = await loadHook();

    const { result } = renderHook(() => hook.useProjectCwd(stateFor("p1", null)));
    await flush();
    expect(result.current).toEqual({ cwd: "D:\\repo", root: "D:\\repo" });

    mocks.listAllSessions.mockResolvedValue([
      { project_id: "p1", working_directory: "D:\\repo\\sub" },
    ]);
    await act(async () => {
      mocks.listeners.get("session-cwd-changed")?.({ payload: {} });
    });
    await flush();

    expect(result.current).toEqual({ cwd: "D:\\repo\\sub", root: "D:\\repo\\sub" });
    expect(mocks.resolveProjectRoot).toHaveBeenLastCalledWith("D:\\repo\\sub");
  });

  it("uses a pinned directory without polling", async () => {
    const hook = await loadHook();

    const { result } = renderHook(() =>
      hook.useProjectCwd(stateFor("p1", "D:\\pinned"))
    );
    await flush();

    expect(result.current).toEqual({ cwd: "D:\\pinned", root: "D:\\pinned" });
    expect(mocks.listAllSessions).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(mocks.listAllSessions).not.toHaveBeenCalled();
  });

  it("stops polling when the last subscriber unmounts", async () => {
    mocks.listAllSessions.mockResolvedValue([
      { project_id: "p1", working_directory: "D:\\repo" },
    ]);
    mocks.resolveProjectRoot.mockResolvedValue("D:\\repo");
    const hook = await loadHook();

    const { unmount } = renderHook(() => hook.useProjectCwd(stateFor("p1", null)));
    await flush();
    const calls = mocks.listAllSessions.mock.calls.length;

    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(20_000);
    });
    expect(mocks.listAllSessions).toHaveBeenCalledTimes(calls);
  });
});
