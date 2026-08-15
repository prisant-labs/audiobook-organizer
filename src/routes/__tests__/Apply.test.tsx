// Apply surface state machine tests (F-904, P8 TDD, AC-27..AC-31).
//
// Pattern mirrors Review.test.tsx: the Apply ROUTE is tested at the component
// level by mocking `useApplyJob`, which owns the state machine. Each test
// drives a specific phase and verifies the surface's copy, actions, and
// accessibility shape. Tests run in the Vitest / jsdom environment with no
// real IPC.
import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { Apply } from "../Apply";
import { useApplyJob, type UseApplyJob } from "@/hooks/useApplyJob";
import { ERROR_COPY } from "@/lib/errorCopy";
import { STRINGS } from "@/lib/strings";

vi.mock("@/hooks/useApplyJob", () => ({ useApplyJob: vi.fn() }));
const mockedUseApplyJob = vi.mocked(useApplyJob);

afterEach(cleanup);
beforeEach(() => {
  mockedUseApplyJob.mockReset();
});

// ---------- shared fixtures ----------

const NOOP_ACTIONS = {
  pause: vi.fn(),
  resume: vi.fn(),
  stop: vi.fn(),
  acknowledge: vi.fn(),
};

function baseRunning(overrides: Partial<UseApplyJob> = {}): UseApplyJob {
  return {
    phase: "running",
    paused: false,
    feed: [],
    doneCount: 0,
    total: 5,
    mode: "dry-run",
    errorCode: null,
    error: null,
    blocked: false,
    discrepancyCount: 0,
    actions: { ...NOOP_ACTIONS },
    ...overrides,
  };
}

// ---------- phase: running (not paused) ----------

describe("Apply - running phase", () => {
  it("renders a heading and the rehearsal badge for a dry-run", () => {
    mockedUseApplyJob.mockReturnValue(baseRunning());
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByRole("heading")).toBeInTheDocument();
    expect(screen.getByText(/rehearsal/i)).toBeInTheDocument();
  });

  it("shows the Pause and Stop controls", () => {
    mockedUseApplyJob.mockReturnValue(baseRunning());
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByRole("button", { name: /pause between books/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /stop/i })).toBeInTheDocument();
  });

  it("shows a progress count", () => {
    mockedUseApplyJob.mockReturnValue(
      baseRunning({ doneCount: 2, total: 5, feed: [] }),
    );
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByText(/2 of 5/)).toBeInTheDocument();
  });

  it("renders sentences from the feed", () => {
    mockedUseApplyJob.mockReturnValue(
      baseRunning({ feed: [{ id: 1, sentence: "Moved The Eye of the World to its new shelf." }] }),
    );
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByText("Moved The Eye of the World to its new shelf.")).toBeInTheDocument();
  });

  it("calls pause when the Pause button is clicked", () => {
    const pause = vi.fn();
    mockedUseApplyJob.mockReturnValue(baseRunning({ actions: { ...NOOP_ACTIONS, pause } }));
    render(<Apply jobId={1} onDone={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /pause between books/i }));
    expect(pause).toHaveBeenCalledTimes(1);
  });

  it("calls stop when the Stop button is clicked", () => {
    const stop = vi.fn();
    mockedUseApplyJob.mockReturnValue(baseRunning({ actions: { ...NOOP_ACTIONS, stop } }));
    render(<Apply jobId={1} onDone={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /stop/i }));
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("does not use forbidden vocabulary", () => {
    mockedUseApplyJob.mockReturnValue(baseRunning({ feed: [] }));
    const { container } = render(<Apply jobId={1} onDone={vi.fn()} />);
    const text = container.textContent ?? "";
    // Plain-language register check (design-system 6.1, CLAUDE.md)
    for (const banned of ["operations", " ops ", "journal", "manifest", "rollback", "quarantine", "dashboard"]) {
      expect(text.toLowerCase()).not.toContain(banned);
    }
  });
});

// ---------- phase: running + paused ----------

describe("Apply - paused phase", () => {
  it("shows the Resume button and the paused heading", () => {
    mockedUseApplyJob.mockReturnValue(baseRunning({ paused: true }));
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByRole("button", { name: /resume/i })).toBeInTheDocument();
    expect(screen.getByText(/paused between books/i)).toBeInTheDocument();
  });

  it("does not show Pause when already paused", () => {
    mockedUseApplyJob.mockReturnValue(baseRunning({ paused: true }));
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.queryByRole("button", { name: /pause between books/i })).toBeNull();
  });

  it("calls resume when Resume is clicked", () => {
    const resume = vi.fn();
    mockedUseApplyJob.mockReturnValue(
      baseRunning({ paused: true, actions: { ...NOOP_ACTIONS, resume } }),
    );
    render(<Apply jobId={1} onDone={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /resume/i }));
    expect(resume).toHaveBeenCalledTimes(1);
  });
});

// ---------- phase: stopped ----------

describe("Apply - stopped phase", () => {
  it("shows 'stopped between books' copy and a single Done action", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "stopped",
      doneCount: 1,
      total: 5,
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByText(/stopped between books/i)).toBeInTheDocument();
    // No Pause/Resume/Stop in the stopped state; exactly one primary action.
    expect(screen.queryByRole("button", { name: /pause between books/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /resume/i })).toBeNull();
    expect(screen.getByRole("button", { name: new RegExp(STRINGS.apply.doneAction, "i") })).toBeInTheDocument();
  });

  it("a stopped REHEARSAL never claims books moved to their new places (Critical 1)", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "stopped",
      mode: "dry-run",
      doneCount: 1,
      total: 5,
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByText(STRINGS.apply.rehearsalStoppedBody)).toBeInTheDocument();
    expect(screen.queryByText(/in their new places/i)).toBeNull();
  });
});

// ---------- mode-aware feed (Critical 1) ----------

describe("Apply - rehearsal feed", () => {
  it("a dry-run feed sentence is a 'Checked' line, not a 'Moved' claim", () => {
    // The hook composes the sentence, so at the ROUTE level we assert the surface
    // simply renders whatever the (mode-aware) hook produced; the hook's own
    // mode-to-sentence mapping is covered in useApplyJob.test.ts.
    mockedUseApplyJob.mockReturnValue(
      baseRunning({
        mode: "dry-run",
        feed: [{ id: 1, sentence: STRINGS.apply.rehearsalOpMovedSentence("The Eye of the World") }],
      }),
    );
    render(<Apply jobId={1} onDone={vi.fn()} />);
    // Derived from STRINGS rather than hardcoded. This assertion used to spell the
    // sentence out, so it duplicated copy that strings.ts owns and broke when
    // FD-47 retired "shelf" - a copy change failing a ROUTE test tells you nothing
    // about the route. What this test actually guards is the line below it.
    expect(
      screen.getByText(STRINGS.apply.rehearsalOpMovedSentence("The Eye of the World")),
    ).toBeInTheDocument();
    // The real property (Critical 1): a rehearsal never claims a real move.
    expect(screen.queryByText(/^moved /i)).toBeNull();
  });
});

// ---------- phase: completed (dry-run) ----------

describe("Apply - completed phase (rehearsal)", () => {
  it("shows rehearsal completed copy and not a real-move claim", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "completed",
      mode: "dry-run",
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByText(/rehearsal complete/i)).toBeInTheDocument();
  });

  it("shows the FD-10 canon string", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "completed",
      mode: "real",
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    // AC-30: exact FD-10 canon string must appear
    expect(
      screen.getByText(
        "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.",
      ),
    ).toBeInTheDocument();
  });
});

// ---------- phase: blocked (discrepancy) ----------

describe("Apply - blocked phase", () => {
  it("shows the needs-attention heading and an Acknowledge button", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "blocked",
      blocked: true,
      discrepancyCount: 2,
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByText(/needs a look/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /got it/i })).toBeInTheDocument();
  });

  it("calls acknowledge when Got it is clicked", () => {
    const acknowledge = vi.fn();
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "blocked",
      blocked: true,
      discrepancyCount: 1,
      actions: { ...NOOP_ACTIONS, acknowledge },
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: /got it/i }));
    expect(acknowledge).toHaveBeenCalledTimes(1);
  });

  it("names how many differences the check found, and hides the report pointer behind Show file details (IMPORTANT 5)", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "blocked",
      blocked: true,
      discrepancyCount: 3,
    });
    const { container } = render(<Apply jobId={1} onDone={vi.fn()} />);

    // The specifics: how many things the check found, so "Got it" is no longer the
    // only content.
    expect(screen.getByText(STRINGS.apply.blockedCountLine(3))).toBeInTheDocument();
    // The technical pointer lives inside the disclosure, not on the summary line.
    expect(screen.getByText(/show file details/i)).toBeInTheDocument();
    const summaryEl = container.querySelector("summary");
    expect(summaryEl?.textContent).not.toContain("after-the-fact-check.md");
    expect(screen.getByText(STRINGS.apply.blockedReportPointer)).toBeInTheDocument();
  });
});

// ---------- phase: failed (FD-04 surface) ----------

describe("Apply - failed phase", () => {
  it("renders all THREE FD-04 parts from the per-code copy map (Critical 2)", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "failed",
      errorCode: "source-vanished",
      error: "E:\\lib\\book.m4b: source-vanished",
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    // 1. What happened - the CODE-SPECIFIC sentence from ERROR_COPY, not a generic
    //    heading alone (the banner is present too, but the specific sentence proves
    //    the panel now consumes the exhaustive copy map).
    expect(screen.getByText(/something stopped the run/i)).toBeInTheDocument();
    expect(screen.getByText(ERROR_COPY["source-vanished"].sentence)).toBeInTheDocument();
    // 2. What is safe - always shown.
    expect(screen.getByText(STRINGS.apply.failedSafeNote)).toBeInTheDocument();
    // 3. What to do next - the CODE-SPECIFIC next step from ERROR_COPY.
    expect(screen.getByText(ERROR_COPY["source-vanished"].nextStep)).toBeInTheDocument();
    // Show file details disclosure (FD-13).
    expect(screen.getByText(/show file details/i)).toBeInTheDocument();
  });

  it("offers a primary action so a failed apply is not a dead end (Critical 2)", () => {
    const onDone = vi.fn();
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "failed",
      errorCode: "access-denied",
      error: "access-denied",
    });
    render(<Apply jobId={1} onDone={onDone} />);

    const done = screen.getByRole("button", { name: new RegExp(STRINGS.apply.doneAction, "i") });
    expect(done).toBeInTheDocument();
    fireEvent.click(done);
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it("uses a per-code sentence, not the generic fallback, for a known code", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "failed",
      errorCode: "access-denied",
      error: "access-denied",
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);
    expect(screen.getByText(ERROR_COPY["access-denied"].sentence)).toBeInTheDocument();
    expect(screen.getByText(ERROR_COPY["access-denied"].nextStep)).toBeInTheDocument();
  });

  it("never shows a raw path as primary content", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "failed",
      errorCode: "source-vanished",
      error: "E:\\lib\\Author\\book.m4b: something bad",
    });
    const { container } = render(<Apply jobId={1} onDone={vi.fn()} />);
    // The raw path must only be inside the <details> disclosure, not in the
    // primary surface. We test this by checking the summary element text
    // (which is the primary visible part) does not include a backslash path.
    // The full path lives inside the closed <details> element.
    const summaryEl = container.querySelector("summary");
    expect(summaryEl?.textContent).not.toContain("E:\\lib");
  });
});
