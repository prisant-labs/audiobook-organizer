import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { History } from "../History";
import { STRINGS } from "@/lib/strings";
import type { HistoryEntry } from "@/lib/bindings";

const historyList = vi.fn();
const rollbackPrepare = vi.fn();
const rollbackPreparePartial = vi.fn();

vi.mock("@/lib/bindings", () => ({
  commands: {
    historyList: (...a: unknown[]) => historyList(...a),
    rollbackPrepare: (...a: unknown[]) => rollbackPrepare(...a),
    rollbackPreparePartial: (...a: unknown[]) => rollbackPreparePartial(...a),
  },
}));

function entry(over: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    jobId: 1,
    mode: "real",
    state: "done",
    startedAt: "2026-07-29T10:00:00Z",
    finishedAt: "2026-07-29T10:05:00Z",
    changesMade: 3,
    undo: { kind: "put-everything-back", manifest_id: 9 },
    ...over,
  } as HistoryEntry;
}

const S = STRINGS.history;

beforeEach(() => {
  vi.clearAllMocks();
});

describe("History", () => {
  it("shows the honest empty state before anything has been organized", async () => {
    historyList.mockResolvedValue({ status: "ok", data: [] });
    render(<History onOpenPlan={vi.fn()} />);
    expect(await screen.findByText(S.emptyHeading)).toBeInTheDocument();
  });

  it("offers the whole-run undo for a completed real run", async () => {
    historyList.mockResolvedValue({ status: "ok", data: [entry()] });
    render(<History onOpenPlan={vi.fn()} />);

    expect(await screen.findByRole("button", { name: new RegExp(S.putEverythingBack) }))
      .toBeInTheDocument();
    expect(screen.getByText(S.changesMade(3))).toBeInTheDocument();
  });

  // The safety-relevant case: a practice run is visible (so the record does not
  // lie by omission) but can never be "put back", because nothing moved.
  it("lists a practice run and offers it no undo", async () => {
    historyList.mockResolvedValue({
      status: "ok",
      data: [entry({ mode: "dry-run", undo: { kind: "practice-run" }, changesMade: 4 })],
    });
    render(<History onOpenPlan={vi.fn()} />);

    expect(await screen.findByText(S.practiceRun)).toBeInTheDocument();
    expect(screen.getByText(S.practiceRunNote)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: new RegExp(S.putEverythingBack) }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: new RegExp(S.putRecentChangesBack) }),
    ).not.toBeInTheDocument();
  });

  // An ambiguous outcome must never present a one-click reversal.
  it("shows needs-a-look instead of an undo button when the outcome is ambiguous", async () => {
    historyList.mockResolvedValue({
      status: "ok",
      data: [entry({ undo: { kind: "needs-a-look" } })],
    });
    render(<History onOpenPlan={vi.fn()} />);

    expect(await screen.findByText(S.needsALook)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: new RegExp(S.putEverythingBack) }),
    ).not.toBeInTheDocument();
  });

  it("says there is nothing to put back when a run changed nothing", async () => {
    historyList.mockResolvedValue({
      status: "ok",
      data: [entry({ changesMade: 0, undo: { kind: "nothing-to-put-back" } })],
    });
    render(<History onOpenPlan={vi.fn()} />);

    expect(await screen.findByText(S.noChangesMade)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /put/i })).not.toBeInTheDocument();
  });

  it("prepares a whole-run undo and hands the plan to the review surface", async () => {
    historyList.mockResolvedValue({ status: "ok", data: [entry()] });
    rollbackPrepare.mockResolvedValue({ status: "ok", data: { plan_id: 77, op_count: 3 } });
    const onOpenPlan = vi.fn();
    render(<History onOpenPlan={onOpenPlan} />);

    await userEvent.click(
      await screen.findByRole("button", { name: new RegExp(S.putEverythingBack) }),
    );

    await waitFor(() => expect(rollbackPrepare).toHaveBeenCalledWith(9));
    expect(onOpenPlan).toHaveBeenCalledWith(77);
  });

  // The partial path must forward exactly the op ids the engine resolved; the
  // view never assembles that list itself.
  it("prepares a partial undo with the op ids the engine supplied", async () => {
    historyList.mockResolvedValue({
      status: "ok",
      data: [
        entry({
          state: "stopped",
          undo: { kind: "put-recent-changes-back", op_ids: [4, 5, 6] },
        }),
      ],
    });
    rollbackPreparePartial.mockResolvedValue({
      status: "ok",
      data: { plan_id: 88, op_count: 3 },
    });
    const onOpenPlan = vi.fn();
    render(<History onOpenPlan={onOpenPlan} />);

    await userEvent.click(
      await screen.findByRole("button", { name: new RegExp(S.putRecentChangesBack) }),
    );

    await waitFor(() => expect(rollbackPreparePartial).toHaveBeenCalledWith(1, [4, 5, 6]));
    expect(onOpenPlan).toHaveBeenCalledWith(88);
  });

  it("renders the family-safe surface when the record cannot be read", async () => {
    historyList.mockResolvedValue({
      status: "error",
      error: { "history-unavailable": { detail: "disk gone" } },
    });
    render(<History onOpenPlan={vi.fn()} />);

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    // The raw detail is never the family-facing sentence (FD-13).
    expect(screen.queryByText("disk gone")).not.toBeInTheDocument();
  });

  it("does not navigate when preparing an undo fails", async () => {
    historyList.mockResolvedValue({ status: "ok", data: [entry()] });
    rollbackPrepare.mockResolvedValue({
      status: "error",
      error: { "rollback-prepare-failed": { detail: "undo file missing" } },
    });
    const onOpenPlan = vi.fn();
    render(<History onOpenPlan={onOpenPlan} />);

    await userEvent.click(
      await screen.findByRole("button", { name: new RegExp(S.putEverythingBack) }),
    );

    await waitFor(() => expect(rollbackPrepare).toHaveBeenCalled());
    expect(onOpenPlan).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });
});
