import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { usePlanReview } from "../usePlanReview";
import {
  excludeOp,
  generatePlan,
  getPlanReview,
  listPlanOps,
  setGroupApproval,
} from "@/lib/plan";
import type { PlanOpView, PlanOpsPage, PlanReview } from "@/lib/bindings";

// "Mocked bindings" pattern (test-strategy Frontend layer): mock the
// lib/plan client, matching useHealthMetrics.test.tsx's precedent for
// lib/overview.
vi.mock("@/lib/plan", () => ({
  generatePlan: vi.fn(),
  getPlanReview: vi.fn(),
  listPlanOps: vi.fn(),
  setGroupApproval: vi.fn(),
  excludeOp: vi.fn(),
}));

const mockedGenerate = vi.mocked(generatePlan);
const mockedGetReview = vi.mocked(getPlanReview);
const mockedListOps = vi.mocked(listPlanOps);
const mockedSetGroupApproval = vi.mocked(setGroupApproval);
const mockedExcludeOp = vi.mocked(excludeOp);

function group(overrides: Partial<PlanReview["groups"][number]> = {}) {
  return {
    group: "loose-books",
    label: "loose books",
    headline: "Give 1 loose book its own folder",
    reason: "reason",
    op_count: 1,
    actionable_count: 1,
    byte_size: 100,
    status: "included" as const,
    warning_count: 0,
    blocked_count: 0,
    ...overrides,
  };
}

function review(overrides: Partial<PlanReview> = {}): PlanReview {
  return { plan_id: 1, scan_id: 9, groups: [group()], ...overrides };
}

function op(overrides: Partial<PlanOpView> = {}): PlanOpView {
  return {
    id: 1,
    group: "loose-books",
    kind: "move",
    kind_reason: null,
    source_path: "E:\\lib\\A.m4b",
    target_path: "E:\\lib\\Author\\A.m4b",
    rationale: "r",
    confidence: "high",
    byte_size: 100,
    validation: "valid",
    validation_reason: null,
    warning_text: null,
    approval: "pending",
    matched_pattern: null,
    extracted_fields: [],
    stripped_noise: null,
    ...overrides,
  };
}

function page(ops: PlanOpView[], truncated = false): PlanOpsPage {
  return { ops, total: ops.length, truncated };
}

beforeEach(() => {
  mockedGenerate.mockReset();
  mockedGetReview.mockReset();
  mockedListOps.mockReset();
  mockedSetGroupApproval.mockReset();
  mockedExcludeOp.mockReset();
});

describe("usePlanReview", () => {
  it("reports 'no-scan' without calling any backend when scanId is null", () => {
    const { result } = renderHook(() => usePlanReview(null));
    expect(result.current.status).toBe("no-scan");
    expect(mockedGenerate).not.toHaveBeenCalled();
  });

  it("generates a real plan and loads its ops for a real scanId", async () => {
    mockedGenerate.mockResolvedValue(review());
    mockedListOps.mockResolvedValue(page([op()]));

    const { result } = renderHook(() => usePlanReview(9));

    expect(result.current.status).toBe("generating");
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(mockedGenerate).toHaveBeenCalledWith(9);
    expect(result.current.review?.plan_id).toBe(1);
    expect(result.current.ops).toHaveLength(1);
    expect(result.current.opsTruncated).toBe(false);
  });

  it("surfaces an error status when generation fails", async () => {
    mockedGenerate.mockRejectedValue(new Error("plan-generation-failed: database is locked"));

    const { result } = renderHook(() => usePlanReview(9));
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.error).toMatch(/plan-generation-failed/);
  });

  it("toggleGroup persists via setGroupApproval and refreshes both the cards and the op listing", async () => {
    mockedGenerate.mockResolvedValue(review());
    mockedListOps.mockResolvedValue(page([op()]));
    const { result } = renderHook(() => usePlanReview(9));
    await waitFor(() => expect(result.current.status).toBe("ready"));

    const toggledOff = review({ groups: [group({ status: "left-out" })] });
    mockedSetGroupApproval.mockResolvedValue(toggledOff);
    mockedListOps.mockResolvedValue(page([op({ approval: "rejected" })]));

    await act(async () => {
      await result.current.toggleGroup("loose-books", false);
    });

    expect(mockedSetGroupApproval).toHaveBeenCalledWith(1, "loose-books", false);
    expect(result.current.review?.groups[0].status).toBe("left-out");
    expect(result.current.ops[0].approval).toBe("rejected");
  });

  it("excludeOp patches just that row and refreshes the group cards", async () => {
    mockedGenerate.mockResolvedValue(review());
    mockedListOps.mockResolvedValue(page([op({ id: 1 }), op({ id: 2 })]));
    const { result } = renderHook(() => usePlanReview(9));
    await waitFor(() => expect(result.current.status).toBe("ready"));

    mockedExcludeOp.mockResolvedValue(op({ id: 1, approval: "excluded" }));
    mockedGetReview.mockResolvedValue(review({ groups: [group({ status: "checking" })] }));

    await act(async () => {
      await result.current.excludeOp(1);
    });

    expect(mockedExcludeOp).toHaveBeenCalledWith(1);
    expect(result.current.ops.find((o) => o.id === 1)?.approval).toBe("excluded");
    expect(result.current.ops.find((o) => o.id === 2)?.approval).toBe("pending");
    expect(result.current.review?.groups[0].status).toBe("checking");
  });

  it("regenerate() re-runs generation against the same scanId", async () => {
    mockedGenerate.mockResolvedValue(review());
    mockedListOps.mockResolvedValue(page([]));
    const { result } = renderHook(() => usePlanReview(9));
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => result.current.regenerate());
    await waitFor(() => expect(mockedGenerate).toHaveBeenCalledTimes(2));
  });

  it("cancel() stops an in-flight plan build and a stopped build never revives the screen (AC-26)", async () => {
    // A generation that has not settled keeps the build in flight.
    let resolveGenerate: (r: PlanReview) => void = () => {};
    mockedGenerate.mockReturnValue(
      new Promise<PlanReview>((resolve) => {
        resolveGenerate = resolve;
      }),
    );
    mockedListOps.mockResolvedValue(page([op()]));

    const { result } = renderHook(() => usePlanReview(9));
    expect(result.current.status).toBe("generating");

    // Stop while it is still building: the honest cooperative stop (no backend
    // cancel; building moves no files) drops to the stopped surface.
    act(() => result.current.cancel());
    expect(result.current.status).toBe("stopped");

    // Even when the abandoned generation later resolves, the stopped build is
    // NOT revived into a ready review (stoppedRef guards the async result).
    await act(async () => {
      resolveGenerate(review());
    });
    expect(result.current.status).toBe("stopped");
    expect(result.current.review).toBeNull();

    // A regenerate afterwards builds again from scratch.
    mockedGenerate.mockResolvedValue(review());
    act(() => result.current.regenerate());
    await waitFor(() => expect(result.current.status).toBe("ready"));
  });

  it("cancel() is a no-op when no generation is in flight", async () => {
    mockedGenerate.mockResolvedValue(review());
    mockedListOps.mockResolvedValue(page([op()]));
    const { result } = renderHook(() => usePlanReview(9));
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => result.current.cancel());
    expect(result.current.status).toBe("ready");
  });

  it("ignores overlapping regenerate() calls while a generation is in flight", async () => {
    // A generation that has not settled keeps the sequence in flight.
    let resolveGenerate: (r: PlanReview) => void = () => {};
    mockedGenerate.mockReturnValue(
      new Promise<PlanReview>((resolve) => {
        resolveGenerate = resolve;
      }),
    );
    mockedListOps.mockResolvedValue(page([]));

    const { result } = renderHook(() => usePlanReview(9));
    // The mount generation is running (one call so far).
    expect(result.current.status).toBe("generating");
    expect(mockedGenerate).toHaveBeenCalledTimes(1);

    // A double-invoke while it is still running is ignored: no second
    // concurrent plan_generate is started.
    act(() => result.current.regenerate());
    act(() => result.current.regenerate());
    expect(mockedGenerate).toHaveBeenCalledTimes(1);

    // Once the in-flight run settles, a later regenerate proceeds normally.
    await act(async () => {
      resolveGenerate(review());
    });
    await waitFor(() => expect(result.current.status).toBe("ready"));
    act(() => result.current.regenerate());
    await waitFor(() => expect(mockedGenerate).toHaveBeenCalledTimes(2));
  });
});
