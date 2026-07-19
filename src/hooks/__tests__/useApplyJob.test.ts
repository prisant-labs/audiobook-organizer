// Genuine unit tests for the useApplyJob state machine (P8, IMPORTANT 6).
//
// The Apply ROUTE tests (routes/__tests__/Apply.test.tsx) mock this hook, so they
// never exercise its real logic. These tests drive the hook itself over a mocked
// bindings/event layer: phase derivation for every state, event handling (including
// the job:stopped terminal event, IMPORTANT 3), the mount backfill seeding
// (IMPORTANT 4), and the mode-aware feed sentences (Critical 1). No real IPC and no
// Tauri bridge - a fake event bus lets a test emit any event synchronously.
import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useApplyJob } from "../useApplyJob";
import { commands } from "@/lib/bindings";
import type { JobStatus } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";

// A hoisted fake event bus: `vi.mock` is hoisted above imports, so the registry it
// closes over must be hoisted too (the classic vi.mock/TDZ trap). Each named bus
// records the listeners the hook attaches; `emit` fans a payload out to them.
const { buses, makeBus } = vi.hoisted(() => {
  const buses: Record<string, Array<(e: { payload: unknown }) => void>> = {};
  const makeBus = (name: string) => ({
    listen: (cb: (e: { payload: unknown }) => void) => {
      (buses[name] ??= []).push(cb);
      return Promise.resolve(() => {
        buses[name] = (buses[name] ?? []).filter((c) => c !== cb);
      });
    },
  });
  return { buses, makeBus };
});

vi.mock("@/lib/bindings", () => ({
  commands: {
    jobStatus: vi.fn(),
    jobPause: vi.fn().mockResolvedValue({ status: "ok", data: null }),
    jobResume: vi.fn().mockResolvedValue({ status: "ok", data: null }),
    jobStop: vi.fn().mockResolvedValue({ status: "ok", data: true }),
    acknowledgeCheck: vi.fn().mockResolvedValue({ status: "ok", data: null }),
  },
  events: {
    applyOpExecuted: makeBus("apply:op-executed"),
    jobCompleted: makeBus("job:completed"),
    jobFailed: makeBus("job:failed"),
    jobStopped: makeBus("job:stopped"),
  },
}));

const mockedJobStatus = vi.mocked(commands.jobStatus);

function status(overrides: Partial<JobStatus> = {}): JobStatus {
  return {
    job_id: 1,
    state: "running",
    error_code: null,
    blocks_further_tidying: false,
    discrepancy_count: 0,
    paused: false,
    done_count: 0,
    total: 0,
    ...overrides,
  };
}

/** Make `commands.jobStatus` resolve to a given status for its next calls. */
function whenStatusIs(s: JobStatus) {
  mockedJobStatus.mockResolvedValue({ status: "ok", data: s });
}

function emit(name: string, payload: unknown) {
  act(() => {
    for (const cb of buses[name] ?? []) cb({ payload });
  });
}

beforeEach(() => {
  for (const key of Object.keys(buses)) buses[key] = [];
  mockedJobStatus.mockReset();
  whenStatusIs(status());
});

// ---------- phase derivation ----------

describe("useApplyJob - phase derivation", () => {
  it("derives running (not paused) from state=running, paused=false", async () => {
    whenStatusIs(status({ state: "running", paused: false }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));
    expect(result.current.paused).toBe(false);
  });

  it("derives paused from state=running, paused=true (state stays running)", async () => {
    whenStatusIs(status({ state: "running", paused: true }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.paused).toBe(true));
    // Paused is still the running phase - the surface shows Resume, not a terminal.
    expect(result.current.phase).toBe("running");
  });

  it("derives stopped from state=stopped", async () => {
    whenStatusIs(status({ state: "stopped" }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("stopped"));
  });

  it("derives completed from state=completed with no block", async () => {
    whenStatusIs(status({ state: "completed" }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("completed"));
    expect(result.current.blocked).toBe(false);
  });

  it("derives blocked from state=completed WITH an unacknowledged block", async () => {
    whenStatusIs(status({ state: "completed", blocks_further_tidying: true, discrepancy_count: 3 }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("blocked"));
    expect(result.current.blocked).toBe(true);
    expect(result.current.discrepancyCount).toBe(3);
  });

  it("derives failed from state=failed and exposes the error code as the detail", async () => {
    whenStatusIs(status({ state: "failed", error_code: "access-denied" }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("failed"));
    expect(result.current.errorCode).toBe("access-denied");
    // `error` is the FD-13 disclosure detail; with no raw message it is the code.
    expect(result.current.error).toBe("access-denied");
  });

  it("plumbs the mode through unchanged (dry-run vs real both reach 'completed')", async () => {
    whenStatusIs(status({ state: "completed" }));
    const dry = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(dry.result.current.phase).toBe("completed"));
    expect(dry.result.current.mode).toBe("dry-run");

    const real = renderHook(() => useApplyJob(2, "real"));
    await waitFor(() => expect(real.result.current.phase).toBe("completed"));
    expect(real.result.current.mode).toBe("real");
  });
});

// ---------- backfill (IMPORTANT 4) ----------

describe("useApplyJob - mount backfill", () => {
  it("seeds the progress counters from job_status even with no events (fast dry-run)", async () => {
    // The exact "0 of 0 books on a fast dry-run" bug: the job finished before the
    // listeners attached, so no apply:op-executed event ever arrives. The mount
    // status read must still surface the true count.
    whenStatusIs(status({ state: "completed", done_count: 5, total: 5 }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("completed"));
    expect(result.current.doneCount).toBe(5);
    expect(result.current.total).toBe(5);
  });

  it("never drags the counters backward (monotonic max against a stale status read)", async () => {
    whenStatusIs(status({ state: "running", done_count: 0, total: 3 }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.total).toBe(3));

    // A live event advances to 3 of 3...
    emit("apply:op-executed", {
      job_id: 1,
      op_id: 9,
      kind: "move",
      label: "X",
      done_count: 3,
      total: 3,
    });
    await waitFor(() => expect(result.current.doneCount).toBe(3));

    // ...and a subsequent (slightly stale) status read reporting 2 must not regress.
    whenStatusIs(status({ state: "running", done_count: 2, total: 3 }));
    emit("job:completed", { job_id: 1 }); // any event triggers a refresh
    await waitFor(() => expect(mockedJobStatus).toHaveBeenCalled());
    expect(result.current.doneCount).toBe(3);
  });
});

// ---------- feed (Critical 1: mode-aware) ----------

describe("useApplyJob - feed", () => {
  it("composes a REHEARSAL 'Checked' sentence for a dry-run move (never 'Moved')", async () => {
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    emit("apply:op-executed", {
      job_id: 1,
      op_id: 1,
      kind: "move",
      label: "The Eye of the World",
      done_count: 1,
      total: 3,
    });

    await waitFor(() => expect(result.current.feed).toHaveLength(1));
    expect(result.current.feed[0].sentence).toBe(
      STRINGS.apply.rehearsalOpMovedSentence("The Eye of the World"),
    );
    expect(result.current.feed[0].sentence).not.toMatch(/^Moved /);
    expect(result.current.doneCount).toBe(1);
    expect(result.current.total).toBe(3);
  });

  it("composes a REAL 'Moved' sentence for a real-mode move", async () => {
    const { result } = renderHook(() => useApplyJob(1, "real"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    emit("apply:op-executed", {
      job_id: 1,
      op_id: 1,
      kind: "move",
      label: "Dune",
      done_count: 1,
      total: 1,
    });

    await waitFor(() => expect(result.current.feed).toHaveLength(1));
    expect(result.current.feed[0].sentence).toBe(STRINGS.apply.opMovedSentence("Dune"));
  });

  it("omits no-op rows from the feed but still advances the counters", async () => {
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    emit("apply:op-executed", {
      job_id: 1,
      op_id: 2,
      kind: "no-op",
      label: "ignored",
      done_count: 2,
      total: 4,
    });

    await waitFor(() => expect(result.current.doneCount).toBe(2));
    expect(result.current.feed).toHaveLength(0);
  });

  it("ignores events addressed to a DIFFERENT job id", async () => {
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    emit("apply:op-executed", {
      job_id: 999,
      op_id: 7,
      kind: "move",
      label: "Not mine",
      done_count: 1,
      total: 1,
    });

    // Give any spurious update a chance to land, then assert nothing changed.
    await new Promise((r) => setTimeout(r, 10));
    expect(result.current.feed).toHaveLength(0);
  });
});

// ---------- terminal events (IMPORTANT 3) ----------

describe("useApplyJob - terminal events", () => {
  it("transitions to stopped on the job:stopped event (no op event follows a Stop)", async () => {
    whenStatusIs(status({ state: "running" }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    // The walk marks the row stopped, then fires job:stopped. Without this event the
    // surface would be stranded on 'running' (the last op event fired while the row
    // was still 'running').
    whenStatusIs(status({ state: "stopped" }));
    emit("job:stopped", { job_id: 1 });

    await waitFor(() => expect(result.current.phase).toBe("stopped"));
  });

  it("transitions to completed on the job:completed event", async () => {
    whenStatusIs(status({ state: "running" }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    whenStatusIs(status({ state: "completed", done_count: 2, total: 2 }));
    emit("job:completed", { job_id: 1 });

    await waitFor(() => expect(result.current.phase).toBe("completed"));
    expect(result.current.doneCount).toBe(2);
  });

  it("transitions to failed on the job:failed event", async () => {
    whenStatusIs(status({ state: "running" }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    whenStatusIs(status({ state: "failed", error_code: "source-vanished" }));
    emit("job:failed", { job_id: 1 });

    await waitFor(() => expect(result.current.phase).toBe("failed"));
    expect(result.current.errorCode).toBe("source-vanished");
  });

  it("ignores a terminal event for a different job id", async () => {
    whenStatusIs(status({ state: "running" }));
    const { result } = renderHook(() => useApplyJob(1, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    whenStatusIs(status({ state: "stopped" }));
    emit("job:stopped", { job_id: 42 });

    await new Promise((r) => setTimeout(r, 10));
    expect(result.current.phase).toBe("running");
  });
});

// ---------- actions ----------

describe("useApplyJob - actions", () => {
  it("wires pause/resume/stop/acknowledge to the typed commands", async () => {
    const { result } = renderHook(() => useApplyJob(7, "dry-run"));
    await waitFor(() => expect(result.current.phase).toBe("running"));

    act(() => result.current.actions.pause());
    expect(commands.jobPause).toHaveBeenCalledWith(7);

    act(() => result.current.actions.resume());
    expect(commands.jobResume).toHaveBeenCalledWith(7);

    act(() => result.current.actions.stop());
    expect(commands.jobStop).toHaveBeenCalledWith(7);

    act(() => result.current.actions.acknowledge());
    expect(commands.acknowledgeCheck).toHaveBeenCalledWith(7);
  });
});
