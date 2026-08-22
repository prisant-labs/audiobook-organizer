// Unit tests for the useDuplicates state machine (F-905, v0.6.0 P5).
//
// The hook shipped with no tests of its own: the P5 surface added component
// tests for DuplicateCard and PolicySelector and two accessibility smokes, and
// nothing drove the hook. These tests drive it over a mocked bindings/event
// layer, the same shape useApplyJob.test.ts uses, so no real IPC and no Tauri
// bridge are involved and a test can emit any event synchronously.
//
// The centre of gravity is STOP. `AC-11` requires cancellation at safe
// boundaries, and the backend deliberately emits NO job event for a cancelled
// job: `run_job_to_terminal` takes an `on_cancelled` callback and
// `dupes_hash_verify` passes `|| {}` for it, exactly as `scan_start` does.
// Library.tsx's `stopScan` states the resulting rule outright:
//
//   "the backend never emits a job:completed/failed event for a cancelled job,
//    so this is the one place that transitions local state to the honest
//    'stopped' outcome."
//
// That rule is a property of the shared job wrapper, not of the scan, so it
// binds every caller of it. These tests hold this hook to it.
import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useDuplicates } from "../useDuplicates";
import { commands } from "@/lib/bindings";
import type { DuplicatesReviewView } from "@/lib/bindings";

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
    dupesReview: vi.fn(),
    dupesHashVerify: vi.fn(),
    dupesConfirm: vi.fn().mockResolvedValue({ status: "ok", data: null }),
    dupesClearConfirmation: vi.fn().mockResolvedValue({ status: "ok", data: null }),
    dupesExportCsv: vi.fn().mockResolvedValue({ status: "ok", data: { path: "C:\\out.csv" } }),
    // scan_cancel returns a BARE boolean, not a Result wrapper (bindings.ts:48).
    scanCancel: vi.fn().mockResolvedValue(true),
  },
  events: {
    jobProgress: makeBus("job:progress"),
    jobCompleted: makeBus("job:completed"),
    jobFailed: makeBus("job:failed"),
  },
}));

const mockedReview = vi.mocked(commands.dupesReview);
const mockedVerify = vi.mocked(commands.dupesHashVerify);
const mockedCancel = vi.mocked(commands.scanCancel);

function emptyReview(): DuplicatesReviewView {
  return {
    group_count: 0,
    copy_count: 0,
    reclaimable_bytes: 0,
    groups: [],
  } as unknown as DuplicatesReviewView;
}

function emit(bus: string, payload: unknown) {
  for (const cb of buses[bus] ?? []) cb({ payload });
}

beforeEach(() => {
  vi.clearAllMocks();
  for (const k of Object.keys(buses)) delete buses[k];
  mockedReview.mockResolvedValue({ status: "ok", data: emptyReview() });
  mockedVerify.mockResolvedValue({ status: "ok", data: { job_id: 7 } } as never);
  mockedCancel.mockResolvedValue(true);
});

/** Render the hook and wait for its first load to settle. */
async function ready() {
  const h = renderHook(() => useDuplicates(1));
  await waitFor(() => expect(h.result.current.status).toBe("ready"));
  return h;
}

/** Render, then start a check job and wait for progress to appear. */
async function checking() {
  const h = await ready();
  await act(async () => {
    await h.result.current.check();
  });
  await waitFor(() => expect(h.result.current.progress).not.toBeNull());
  return h;
}

describe("starting a check", () => {
  it("shows progress from the moment the job starts, before any event", async () => {
    const h = await checking();
    expect(mockedVerify).toHaveBeenCalledWith(1);
    expect(h.result.current.progress).toEqual({ done: 0, total: null, label: "" });
  });

  it("tracks progress events for its own job and ignores another job's", async () => {
    const h = await checking();

    act(() => {
      emit("job:progress", { job_id: 999, done: 5, total_estimate: 10, current_label: "other" });
    });
    expect(h.result.current.progress).toEqual({ done: 0, total: null, label: "" });

    act(() => {
      emit("job:progress", { job_id: 7, done: 3, total_estimate: 12, current_label: "Dune.m4b" });
    });
    expect(h.result.current.progress).toEqual({ done: 3, total: 12, label: "Dune.m4b" });
  });

  it("clears progress when its job completes", async () => {
    const h = await checking();
    act(() => {
      emit("job:completed", { job_id: 7, scan_id: 1 });
    });
    await waitFor(() => expect(h.result.current.progress).toBeNull());
  });
});

describe("stopping a check", () => {
  it("signals cancellation for the running job", async () => {
    const h = await checking();
    await act(async () => {
      h.result.current.stopCheck();
    });
    expect(mockedCancel).toHaveBeenCalledWith(7);
  });

  // THE DEFECT. The backend emits no event for a cancelled job, so if the hook
  // does not transition its own state here, nothing ever will: the progress UI
  // and its Stop button stay on screen for a job that already stopped.
  it("clears progress after Stop, because no event is coming", async () => {
    const h = await checking();
    expect(h.result.current.progress).not.toBeNull();

    await act(async () => {
      h.result.current.stopCheck();
    });

    await waitFor(() => expect(h.result.current.progress).toBeNull());
  });

  // The same failure seen from the other side, and the worse half: `check()`
  // refuses to start while it believes a job is running, so a stuck job id
  // makes the Check control permanently dead for the life of the mounted
  // screen. Recovering needs navigating away and back.
  it("allows a new check to start after Stop", async () => {
    const h = await checking();

    await act(async () => {
      h.result.current.stopCheck();
    });
    await waitFor(() => expect(h.result.current.progress).toBeNull());

    await act(async () => {
      await h.result.current.check();
    });

    expect(mockedVerify).toHaveBeenCalledTimes(2);
  });

  // A cancelled pass KEEPS every hash it finished (the job reports Cancelled
  // rather than Failed for exactly this reason). Those hashes are only visible
  // after a reload, so a Stop that does not reload hides work that was done.
  it("reloads after Stop so the hashes the pass finished become visible", async () => {
    const h = await checking();
    const before = mockedReview.mock.calls.length;

    await act(async () => {
      h.result.current.stopCheck();
    });

    await waitFor(() => expect(mockedReview.mock.calls.length).toBeGreaterThan(before));
  });

  it("does nothing when no check is running", async () => {
    const h = await ready();
    await act(async () => {
      h.result.current.stopCheck();
    });
    expect(mockedCancel).not.toHaveBeenCalled();
  });
});

describe("a failed check", () => {
  it("surfaces the job's own error code and clears progress", async () => {
    const h = await checking();
    act(() => {
      emit("job:failed", { job_id: 7, code: "duplicate-verify-failed" });
    });
    await waitFor(() => expect(h.result.current.progress).toBeNull());
    expect(h.result.current.actionError?.code).toBe("duplicate-verify-failed");
  });
});
