import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useStartupInterruption } from "../useStartupInterruption";
import { commands } from "@/lib/bindings";
import type { HistoryEntry } from "@/lib/bindings";

// "Mocked bindings" pattern (test-strategy Frontend layer), matching
// useHistory's precedent: the hook is the only place that talks to the
// backend for this feature, so the seam under test is the two commands.
vi.mock("@/lib/bindings", () => ({
  commands: {
    startupInterruption: vi.fn(),
    historyList: vi.fn(),
  },
}));

const mockedStartup = vi.mocked(commands.startupInterruption);
const mockedHistory = vi.mocked(commands.historyList);

const RESULT = {
  job_id: 14,
  mode: "real" as const,
  interrupted: true,
  outcome: "completed" as const,
  in_doubt_op_id: 142,
  resume_offered: true,
  done_count: 142,
};

const ROW: HistoryEntry = {
  jobId: 14,
  mode: "real",
  state: "failed",
  startedAt: "2026-08-04T00:18:15Z",
  finishedAt: null,
  changesMade: 142,
  undo: { kind: "put-recent-changes-back", op_ids: [140, 141, 142] },
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("useStartupInterruption", () => {
  it("reports no interruption on a clean start, and never calls History", async () => {
    mockedStartup.mockResolvedValue(null);

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toBeNull();
    expect(result.current.entry).toBeNull();
    expect(mockedHistory).not.toHaveBeenCalled();
  });

  it("pairs the interruption with its History row", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockResolvedValue({ status: "ok", data: [ROW] });

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toEqual(RESULT);
    expect(result.current.entry).toEqual(ROW);
  });

  it("keeps the interruption but no entry when no History row matches", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockResolvedValue({ status: "ok", data: [{ ...ROW, jobId: 99 }] });

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toEqual(RESULT);
    expect(result.current.entry).toBeNull();
  });

  // A recovery offer that cannot be fully read still tells the user the run was
  // interrupted; it just cannot offer an undo. Failing to null here would hide
  // the interruption entirely, which is the worse outcome.
  it("still reports the interruption when History rejects", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockRejectedValue(new Error("db locked"));

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toEqual(RESULT);
    expect(result.current.entry).toBeNull();
  });

  it("still reports the interruption when History returns an error result", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockResolvedValue({
      status: "error",
      error: { kind: "history-unavailable", detail: "nope" },
    } as unknown as Awaited<ReturnType<typeof commands.historyList>>);

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toEqual(RESULT);
    expect(result.current.entry).toBeNull();
  });

  it("reports no interruption when the command itself fails, rather than blocking the app", async () => {
    mockedStartup.mockRejectedValue(new Error("ipc gone"));

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toBeNull();
  });

  it("dismiss clears the surface for the rest of the session", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockResolvedValue({ status: "ok", data: [ROW] });

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.interruption).not.toBeNull());

    act(() => result.current.dismiss());

    expect(result.current.interruption).toBeNull();
    expect(result.current.entry).toBeNull();
  });
});
