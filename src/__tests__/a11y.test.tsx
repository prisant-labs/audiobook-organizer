import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import axe from "axe-core";
import { Library } from "@/routes/Library";
import { Review } from "@/routes/Review";
import { Apply } from "@/routes/Apply";
import { InterruptionNotice } from "@/components/states/InterruptionNotice";
import { commands } from "@/lib/bindings";
import { getCover } from "@/lib/covers";
import { usePlanReview } from "@/hooks/usePlanReview";
import { useApplyJob, type UseApplyJob } from "@/hooks/useApplyJob";
import type { HistoryEntry, LibraryOverview, PlanGroupView, PlanOpView } from "@/lib/bindings";
import type { UseHealthMetrics } from "@/hooks/useHealthMetrics";
import type { UsePlanReview } from "@/hooks/usePlanReview";

// T-36 (v0.4.0 Phase 8, AC-41, design-system Section 8): axe-core smoke on the
// primary surfaces (home, review), run inside Vitest/jsdom rather than a
// browser (test-strategy.md Section 5: E2E GUI automation is deliberately
// deferred - this is the mechanical half of FD-21, the keyboard walkthrough
// itself stays a manual QA item, docs/internal/qa/v0.4.0-manual-qa.md).
//
// Same "mocked bindings"/"mocked hook" pattern as Library.test.tsx and
// Review.test.tsx: each surface is rendered in its populated, ready state (the
// richest DOM each screen produces) so axe has the most to check, not the
// thinnest loading/empty placeholder.

vi.mock("@/lib/bindings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/bindings")>();
  return {
    ...actual,
    commands: { scanStart: vi.fn(), scanCancel: vi.fn() },
    events: {
      jobCompleted: { listen: vi.fn().mockResolvedValue(() => {}) },
      jobFailed: { listen: vi.fn().mockResolvedValue(() => {}) },
      jobProgress: { listen: vi.fn().mockResolvedValue(() => {}) },
    },
  };
});
vi.mock("@/lib/covers", () => ({ getCover: vi.fn() }));
vi.mock("@/hooks/usePlanReview", () => ({ usePlanReview: vi.fn() }));
vi.mock("@/hooks/useApplyJob", () => ({ useApplyJob: vi.fn() }));

const mockedScanStart = vi.mocked(commands.scanStart);
const mockedScanCancel = vi.mocked(commands.scanCancel);
const mockedGetCover = vi.mocked(getCover);
const mockedUsePlanReview = vi.mocked(usePlanReview);
const mockedUseApplyJob = vi.mocked(useApplyJob);

afterEach(cleanup);
beforeEach(() => {
  mockedScanStart.mockReset();
  mockedScanCancel.mockReset();
  mockedGetCover.mockReset().mockResolvedValue(null);
  mockedUsePlanReview.mockReset();
  mockedUseApplyJob.mockReset();
});

function health(overview: LibraryOverview | null): UseHealthMetrics {
  return { overview, status: "ready", error: null, reload: vi.fn() };
}

const OVERVIEW: LibraryOverview = {
  scan_id: 9,
  total_books: 1022,
  total_bytes: 297 * 1024 ** 3,
  needs_tidy_books: 412,
  worth_a_look: [
    { entry_id: 1, title: "Sapiens", author: "Y. N. Harari", reason: { kind: "warn", text: "loose file" } },
    { entry_id: 2, title: "Dune", author: "Frank Herbert", reason: { kind: "alert", text: "2 copies" } },
  ],
  series: [{ name: "The Dresden Files", author: "Jim Butcher", book_count: 20 }],
  good_news: {
    already_tidy_books: 582,
    series_shelved: 34,
    empty_folders: 20,
    duplicate_groups: 403,
    duplicate_bytes: Math.round(10.1 * 1024 ** 3),
  },
  metrics: { per_class: [], problems: [], total_bytes: 297 * 1024 ** 3 },
};

const SEVEN_SLUGS = [
  "staging",
  "loose-books",
  "messy-names",
  "box-sets",
  "bundles",
  "copies",
  "empty-folders",
];

function group(overrides: Partial<PlanGroupView> = {}): PlanGroupView {
  const merged = {
    group: "loose-books",
    label: "loose books",
    headline: "Give 3 loose books their own folders",
    reason: "These audiobooks are sitting as single files instead of their own folder.",
    op_count: 3,
    byte_size: 3000,
    status: "included" as const,
    warning_count: 0,
    blocked_count: 0,
    ...overrides,
  };
  return { ...merged, actionable_count: overrides.actionable_count ?? merged.op_count };
}

function op(overrides: Partial<PlanOpView> = {}): PlanOpView {
  return {
    id: 1,
    group: "loose-books",
    kind: "move",
    kind_reason: null,
    source_path: "E:\\lib\\Sapiens.m4b",
    target_path: "E:\\lib\\Author\\Sapiens.m4b",
    rationale: "Move this loose book into its own folder.",
    confidence: "high",
    byte_size: 1000,
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

function reviewHookState(): UsePlanReview {
  return {
    status: "ready",
    review: {
      plan_id: 1,
      scan_id: 9,
      groups: SEVEN_SLUGS.map((slug) => group({ group: slug, label: slug.replace("-", " ") })),
    },
    ops: [op()],
    opsTruncated: false,
    error: null,
    errorCode: null,
    regenerate: vi.fn(),
    cancel: vi.fn(),
    toggleGroup: vi.fn().mockResolvedValue(undefined),
    excludeOp: vi.fn().mockResolvedValue(undefined),
  };
}

// axe-core's own violation shape carries more than we need to assert on; keep
// only what a failure message benefits from.
function describeViolations(results: axe.AxeResults) {
  return results.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.impact} ${v.id}: ${v.description} (${v.nodes.length} node(s))`);
}

function applyJobState(overrides: Partial<UseApplyJob> = {}): UseApplyJob {
  return {
    phase: "running",
    paused: false,
    feed: [{ id: 1, sentence: "Moved Sapiens to its new shelf." }],
    doneCount: 1,
    total: 5,
    mode: "dry-run",
    errorCode: null,
    error: null,
    blocked: false,
    discrepancyCount: 0,
    actions: { pause: vi.fn(), resume: vi.fn(), stop: vi.fn(), acknowledge: vi.fn() },
    ...overrides,
  };
}

describe("axe-core accessibility smoke (T-36, AC-41)", () => {
  it("library home has zero serious/critical violations", async () => {
    const { container } = render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);
    const results = await axe.run(container);
    expect(describeViolations(results), JSON.stringify(describeViolations(results), null, 2)).toEqual(
      [],
    );
  });

  it("the review surface has zero serious/critical violations", async () => {
    mockedUsePlanReview.mockReturnValue(reviewHookState());
    const { container } = render(<Review scanId={9} />);
    const results = await axe.run(container);
    expect(describeViolations(results), JSON.stringify(describeViolations(results), null, 2)).toEqual(
      [],
    );
  });

  it("the apply surface has zero serious/critical violations", async () => {
    // Running phase with one feed sentence - the richest DOM the Apply screen
    // produces while active. Mocking useApplyJob keeps this a fast unit-level
    // smoke test (no IPC, no real Tauri bridge).
    mockedUseApplyJob.mockReturnValue(applyJobState());
    const { container } = render(<Apply jobId={1} onDone={vi.fn()} />);
    const results = await axe.run(container);
    expect(describeViolations(results), JSON.stringify(describeViolations(results), null, 2)).toEqual(
      [],
    );
  });

  it("the apply FAILED panel has zero serious/critical violations", async () => {
    // The FD-04 failure surface (Critical 2): danger/warn tone, disclosure, and a
    // primary action - the highest-stakes state, so it gets its own axe smoke.
    mockedUseApplyJob.mockReturnValue(
      applyJobState({
        phase: "failed",
        feed: [],
        errorCode: "access-denied",
        error: "access-denied",
      }),
    );
    const { container } = render(<Apply jobId={1} onDone={vi.fn()} />);
    const results = await axe.run(container);
    expect(describeViolations(results), JSON.stringify(describeViolations(results), null, 2)).toEqual(
      [],
    );
  });

  it("the apply BLOCKED panel has zero serious/critical violations", async () => {
    // The completed-but-blocked surface (IMPORTANT 5): count line, disclosure, and
    // the acknowledge action.
    mockedUseApplyJob.mockReturnValue(
      applyJobState({ phase: "blocked", feed: [], blocked: true, discrepancyCount: 2 }),
    );
    const { container } = render(<Apply jobId={1} onDone={vi.fn()} />);
    const results = await axe.run(container);
    expect(describeViolations(results), JSON.stringify(describeViolations(results), null, 2)).toEqual(
      [],
    );
  });
});

// The interruption recovery surface (v0.6.0 P1c, F-606). All THREE states, not
// just the calm one: the ambiguous state uses a different token pair and a
// different action set, so passing on the decisive state proves nothing about
// it. This is the mechanical half of FD-21 for this surface; the keyboard
// walkthrough is the human half, in docs/internal/qa/v0.6.0-manual-qa.md.
describe("InterruptionNotice a11y", () => {
  const BASE = {
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

  it.each([
    ["a practice run stopped early", { ...BASE, mode: "dry-run" as const, resume_offered: false }],
    ["a real run stopped early, decisive", BASE],
    ["a real run stopped early, ambiguous", { ...BASE, resume_offered: false }],
  ])("has zero serious/critical violations: %s", async (_label, interruption) => {
    const { container } = render(
      <InterruptionNotice
        interruption={interruption}
        entry={ROW}
        preparing={false}
        onGoToLibrary={vi.fn()}
        onUndo={vi.fn()}
        onOpenHistory={vi.fn()}
      />,
    );
    const results = await axe.run(container);
    expect(describeViolations(results), JSON.stringify(describeViolations(results), null, 2)).toEqual(
      [],
    );
  });
});
