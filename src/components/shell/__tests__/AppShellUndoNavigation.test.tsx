import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppShell } from "../AppShell";
import type { AppSettings } from "@/lib/settings";
import { STRINGS } from "@/lib/strings";

// Regression coverage for the adversarial-review finding that `openPlanId` was
// never cleared. Once History prepared an undo, the id stayed set on the shell
// forever: `usePlanReview` prefers it over the scan on EVERY run, so navigating
// away and back to Tidy-up reopened the undo, and "build the plan again" rebuilt
// the undo rather than a forward plan. The user could not return to forward
// planning without restarting the app.
//
// These tests assert the lifecycle at the seam that owns it (the shell), by
// observing which backend call the review surface makes: `planGenerate` means a
// forward plan was built from the scan, `planGet` means a persisted plan was
// opened.

const planGenerate = vi.fn();
const planGet = vi.fn();
const planListOps = vi.fn();
const historyList = vi.fn();
const rollbackPrepare = vi.fn();

vi.mock("../../../lib/bindings", () => ({
  commands: {
    dbStatus: vi.fn().mockResolvedValue({ recovered: false, backup_path: null }),
    scanStart: vi.fn(),
    scanEntries: vi.fn(),
    // A completed scan, so the Tidy-up route has a scan_id and builds a FORWARD
    // plan (the behaviour these tests check the shell can return to).
    classifyOverview: vi.fn().mockResolvedValue({
      status: "ok",
      data: {
        scan_id: 5,
        total_books: 0,
        total_bytes: 0,
        needs_tidy_books: 0,
        worth_a_look: [],
        series: [],
        good_news: {
          already_tidy_books: 0,
          series_shelved: 0,
          empty_folders: 0,
          duplicate_groups: 0,
          duplicate_bytes: 0,
        },
        metrics: { per_class: [], problems: [], total_bytes: 0 },
      },
    }),
    planGenerate: (...a: unknown[]) => planGenerate(...a),
    planGet: (...a: unknown[]) => planGet(...a),
    planListOps: (...a: unknown[]) => planListOps(...a),
    historyList: (...a: unknown[]) => historyList(...a),
    rollbackPrepare: (...a: unknown[]) => rollbackPrepare(...a),
    rollbackPreparePartial: vi.fn(),
    rulesetGetActive: vi.fn().mockResolvedValue({ status: "error", error: {} }),
    rulesetPresetExamples: vi.fn().mockResolvedValue([]),
  },
  events: {
    jobCompleted: { listen: vi.fn().mockResolvedValue(() => {}) },
    jobFailed: { listen: vi.fn().mockResolvedValue(() => {}) },
    jobProgress: { listen: vi.fn().mockResolvedValue(() => {}) },
  },
}));

const SETTINGS: AppSettings = {
  library_root: "E:\\Books",
  set_aside_root: null,
  reports_root: null,
  theme: "day",
  scan_retention_count: 10,
};

const PLAN = {
  status: "ok" as const,
  data: { plan_id: 1, groups: [], op_count: 0, blocked_count: 0, stats: null },
};

beforeEach(() => {
  vi.clearAllMocks();
  planGenerate.mockResolvedValue(PLAN);
  planGet.mockResolvedValue({ ...PLAN, data: { ...PLAN.data, plan_id: 77 } });
  planListOps.mockResolvedValue({ status: "ok", data: { ops: [], truncated: false } });
  rollbackPrepare.mockResolvedValue({ status: "ok", data: { plan_id: 77, op_count: 2 } });
  historyList.mockResolvedValue({
    status: "ok",
    data: [
      {
        jobId: 1,
        mode: "real",
        state: "done",
        startedAt: "2026-07-29T10:00:00Z",
        finishedAt: "2026-07-29T10:05:00Z",
        changesMade: 2,
        undo: { kind: "put-everything-back", manifest_id: 9 },
      },
    ],
  });
});

async function goTo(label: string) {
  await userEvent.click(screen.getByRole("button", { name: label }));
}

describe("undo plan navigation lifecycle", () => {
  it("opens the prepared undo plan on the review surface", async () => {
    render(<AppShell settings={SETTINGS} onUpdate={vi.fn()} />);

    await goTo("History");
    await userEvent.click(
      await screen.findByRole("button", {
        name: new RegExp(STRINGS.history.putEverythingBack),
      }),
    );

    // The review surface LOADS plan 77 rather than generating a forward plan.
    await waitFor(() => expect(planGet).toHaveBeenCalledWith(77));
  });

  // The finding: after this sequence the user was stuck on the undo plan.
  it("returns to forward planning after navigating away from an undo", async () => {
    render(<AppShell settings={SETTINGS} onUpdate={vi.fn()} />);

    await goTo("History");
    await userEvent.click(
      await screen.findByRole("button", {
        name: new RegExp(STRINGS.history.putEverythingBack),
      }),
    );
    await waitFor(() => expect(planGet).toHaveBeenCalledWith(77));

    planGenerate.mockClear();
    planGet.mockClear();

    // Leave Tidy-up and come back the ordinary way.
    await goTo("Library");
    await goTo("Tidy-up");

    await waitFor(() => expect(planGenerate).toHaveBeenCalled());
    expect(planGet).not.toHaveBeenCalled();
  });

  it("does not open an undo plan when Tidy-up is reached by normal navigation", async () => {
    render(<AppShell settings={SETTINGS} onUpdate={vi.fn()} />);

    await goTo("Tidy-up");

    await waitFor(() => expect(planGenerate).toHaveBeenCalled());
    expect(planGet).not.toHaveBeenCalled();
  });
});
