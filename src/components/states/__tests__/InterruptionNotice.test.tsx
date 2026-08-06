import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InterruptionNotice } from "../InterruptionNotice";
import { interruptionStateOf, type StartupInterruption } from "@/lib/interruption";
import type { HistoryEntry } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";

afterEach(cleanup);

const S = STRINGS.interruption;
const H = STRINGS.history;

function interruption(over: Partial<StartupInterruption> = {}): StartupInterruption {
  return {
    job_id: 14,
    mode: "real",
    interrupted: true,
    outcome: "completed",
    in_doubt_op_id: 142,
    resume_offered: true,
    done_count: 142,
    ...over,
  };
}

function entry(over: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    jobId: 14,
    mode: "real",
    state: "failed",
    startedAt: "2026-08-04T00:18:15Z",
    finishedAt: null,
    changesMade: 142,
    undo: { kind: "put-recent-changes-back", op_ids: [140, 141, 142] },
    ...over,
  };
}

function renderNotice(props: Partial<Parameters<typeof InterruptionNotice>[0]> = {}) {
  const handlers = {
    onGoToLibrary: vi.fn(),
    onUndo: vi.fn(),
    onOpenHistory: vi.fn(),
  };
  render(
    <InterruptionNotice
      interruption={interruption()}
      entry={entry()}
      preparing={false}
      {...handlers}
      {...props}
    />,
  );
  return handlers;
}

// The three states are a state machine over ReconcileResult, not three
// designs. `resume_offered` is the ENGINE's answer to "is carrying on safe",
// so the component reads it rather than re-deriving one (FD-36).
describe("interruptionStateOf", () => {
  it("classifies a rehearsal as the practice-run state whatever else it says", () => {
    expect(interruptionStateOf(interruption({ mode: "dry-run", resume_offered: true }))).toBe(
      "practice-run",
    );
  });

  it("classifies a real run with resume offered as decisive", () => {
    expect(interruptionStateOf(interruption({ resume_offered: true }))).toBe("stopped-decisive");
  });

  it("classifies a real run without resume offered as ambiguous", () => {
    expect(interruptionStateOf(interruption({ resume_offered: false }))).toBe("stopped-ambiguous");
  });
});

describe("InterruptionNotice, practice run stopped early", () => {
  it("says nothing on the shelves was touched and offers only the way back", async () => {
    const h = renderNotice({
      interruption: interruption({ mode: "dry-run", resume_offered: false, done_count: 0 }),
      entry: entry({ mode: "dry-run", undo: { kind: "practice-run" } }),
    });

    expect(screen.getByText(S.practiceHeading)).toBeInTheDocument();
    expect(screen.getByText(S.practiceBody)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: S.carryOn })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: H.putRecentChangesBack })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: S.practiceAction }));
    expect(h.onGoToLibrary).toHaveBeenCalledOnce();
  });
});

describe("InterruptionNotice, real run stopped early with a decisive outcome", () => {
  it("offers both carrying on and putting the changes back", async () => {
    const h = renderNotice();

    expect(screen.getByText(S.stoppedHeading)).toBeInTheDocument();
    expect(screen.getByText(S.booksMoved(142))).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: S.carryOn }));
    expect(h.onGoToLibrary).toHaveBeenCalledOnce();

    await userEvent.click(screen.getByRole("button", { name: H.putRecentChangesBack }));
    expect(h.onUndo).toHaveBeenCalledOnce();
  });

  it("disables the undo while one is being prepared", () => {
    renderNotice({ preparing: true });
    expect(screen.getByRole("button", { name: H.preparing })).toBeDisabled();
  });

  it("offers carrying on but no undo when the History row could not be read", () => {
    renderNotice({ entry: null });
    expect(screen.getByRole("button", { name: S.carryOn })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: H.putRecentChangesBack })).not.toBeInTheDocument();
  });
});

describe("InterruptionNotice, real run stopped early with an ambiguous outcome", () => {
  // The safety-critical assertion in this file. A cross-volume copy killed
  // mid-write leaves a target that exists but may be truncated, and a fresh
  // scan would read it as a whole book. Carrying on must never be offered when
  // the engine says the outcome is unconfirmed.
  it("never offers carrying on, and sends the user to History instead", async () => {
    const h = renderNotice({
      interruption: interruption({ resume_offered: false }),
      entry: entry({ undo: { kind: "needs-a-look" } }),
    });

    expect(screen.getByText(S.ambiguousHeading)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: S.carryOn })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: H.putRecentChangesBack })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: S.openHistory }));
    expect(h.onOpenHistory).toHaveBeenCalledOnce();
  });

  it("uses the danger token pair, while the calm states use warn (FD-09)", () => {
    const { container: ambiguous } = render(
      <InterruptionNotice
        interruption={interruption({ resume_offered: false })}
        entry={entry({ undo: { kind: "needs-a-look" } })}
        preparing={false}
        onGoToLibrary={vi.fn()}
        onUndo={vi.fn()}
        onOpenHistory={vi.fn()}
      />,
    );
    expect(ambiguous.querySelector(".text-danger")).not.toBeNull();
    cleanup();

    const { container: calm } = render(
      <InterruptionNotice
        interruption={interruption()}
        entry={entry()}
        preparing={false}
        onGoToLibrary={vi.fn()}
        onUndo={vi.fn()}
        onOpenHistory={vi.fn()}
      />,
    );
    expect(calm.querySelector(".text-warn")).not.toBeNull();
    expect(calm.querySelector(".text-danger")).toBeNull();
  });
});

describe("InterruptionNotice details disclosure", () => {
  it("holds only plain facts: no paths, no ids (FD-13, AC-6)", () => {
    renderNotice();
    expect(screen.getByText(S.showDetails)).toBeInTheDocument();
    expect(screen.getByText(S.detailChanges(142))).toBeInTheDocument();
    expect(screen.getByText(S.detailLastStepChecked)).toBeInTheDocument();
  });

  it("omits the started line when the History row could not be read", () => {
    renderNotice({ entry: null });
    expect(screen.queryByText(/^Started:/)).toBeNull();
    // The change count still shows: it comes from the interruption itself,
    // not from History, so losing History must not lose it.
    expect(screen.getByText(S.detailChanges(142))).toBeInTheDocument();
  });
});
