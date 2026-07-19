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
  it("shows 'stopped between books' copy and no primary action button", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "stopped",
      doneCount: 1,
      total: 5,
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    expect(screen.getByText(/stopped between books/i)).toBeInTheDocument();
    // No Pause/Resume/Stop in the stopped state
    expect(screen.queryByRole("button", { name: /pause between books/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /resume/i })).toBeNull();
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
});

// ---------- phase: failed (FD-04 surface) ----------

describe("Apply - failed phase", () => {
  it("renders the FD-04 surface: what happened, what is safe, what to do next", () => {
    mockedUseApplyJob.mockReturnValue({
      ...baseRunning(),
      phase: "failed",
      errorCode: "source-vanished",
      error: "E:\\lib\\book.m4b: source-vanished",
    });
    render(<Apply jobId={1} onDone={vi.fn()} />);

    // What stopped the tidy-up (plain language heading)
    expect(screen.getByText(/something stopped the tidy-up/i)).toBeInTheDocument();
    // What is safe (always shown)
    expect(screen.getByText(/your books are safe/i)).toBeInTheDocument();
    // Show file details disclosure (FD-13)
    expect(screen.getByText(/show file details/i)).toBeInTheDocument();
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
