import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppShell } from "../AppShell";
import { useStartupInterruption } from "@/hooks/useStartupInterruption";
import { commands } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";
import type { AppSettings } from "@/lib/settings";
import type { HistoryEntry } from "@/lib/bindings";

// The shell's own bindings are mocked to the minimum the default Library route
// needs (same pattern as AppShell.test.tsx), plus the one command the undo
// action reaches. The interruption hook is mocked outright so each test states
// the recovery state it is exercising rather than staging a backend.
vi.mock("@/hooks/useStartupInterruption", () => ({ useStartupInterruption: vi.fn() }));
vi.mock("@/lib/bindings", () => ({
  commands: {
    dbStatus: vi.fn().mockResolvedValue({ recovered: false, backup_path: null }),
    scanStart: vi.fn(),
    scanEntries: vi.fn(),
    classifyOverview: vi.fn().mockResolvedValue({ status: "ok", data: null }),
    rollbackPreparePartial: vi.fn(),
    applyStart: vi.fn(),
  },
  events: {
    jobCompleted: { listen: vi.fn().mockResolvedValue(() => {}) },
    jobFailed: { listen: vi.fn().mockResolvedValue(() => {}) },
    jobProgress: { listen: vi.fn().mockResolvedValue(() => {}) },
  },
}));

const mockedInterruption = vi.mocked(useStartupInterruption);
const mockedRollback = vi.mocked(commands.rollbackPreparePartial);

const S = STRINGS.interruption;
const H = STRINGS.history;

const SETTINGS: AppSettings = {
  library_root: "E:\\Books",
  set_aside_root: null,
  reports_root: null,
  theme: "day",
  scan_retention_count: 10,
};

const PRACTICE = {
  job_id: 14,
  mode: "dry-run" as const,
  interrupted: true,
  outcome: null,
  in_doubt_op_id: 7,
  resume_offered: false,
  done_count: 0,
};

const REAL = {
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

function stage(over: Partial<ReturnType<typeof useStartupInterruption>> = {}) {
  const dismiss = vi.fn();
  mockedInterruption.mockReturnValue({
    interruption: null,
    entry: null,
    status: "ready",
    dismiss,
    ...over,
  });
  return dismiss;
}

function shell() {
  render(<AppShell settings={SETTINGS} onUpdate={vi.fn().mockResolvedValue(undefined)} />);
}

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(cleanup);

describe("AppShell interruption handling", () => {
  it("shows the notice instead of the route content, with the sidebar still there", () => {
    stage({ interruption: PRACTICE });
    shell();

    expect(screen.getByText(S.practiceHeading)).toBeInTheDocument();
    // The soft-panel decision, asserted rather than assumed. An earlier draft
    // blocked the whole app from AppRoot; that was rejected because a
    // navigation block is a procedural gate that stops nothing an IPC caller
    // can reach, and because it traps a reader in a screen to tell them
    // nothing happened. If someone reinstates the hard gate, this fails.
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
  });

  it("renders the ordinary route content when there is no interruption", () => {
    stage();
    shell();

    expect(screen.queryByText(S.practiceHeading)).not.toBeInTheDocument();
    expect(screen.queryByText(S.stoppedHeading)).not.toBeInTheDocument();
  });

  it("dismisses when the user takes the way out of a practice run", async () => {
    const dismiss = stage({ interruption: PRACTICE });
    shell();

    await userEvent.click(screen.getByRole("button", { name: S.practiceAction }));
    expect(dismiss).toHaveBeenCalledOnce();
  });

  it("prepares a partial undo from the journal tail and opens it for review", async () => {
    const dismiss = stage({ interruption: REAL, entry: ROW });
    mockedRollback.mockResolvedValue({ status: "ok", data: { plan_id: 77, op_count: 3 } });
    shell();

    await userEvent.click(screen.getByRole("button", { name: H.putRecentChangesBack }));

    // The op ids come from the engine's own UndoOffer, never recomputed here.
    await waitFor(() => expect(mockedRollback).toHaveBeenCalledWith(14, [140, 141, 142]));
    expect(dismiss).toHaveBeenCalledOnce();
  });

  it("keeps the notice up when preparing the undo fails", async () => {
    const dismiss = stage({ interruption: REAL, entry: ROW });
    mockedRollback.mockResolvedValue({
      status: "error",
      error: { kind: "rollback-prepare-failed", detail: "nope" },
    } as unknown as Awaited<ReturnType<typeof commands.rollbackPreparePartial>>);
    shell();

    await userEvent.click(screen.getByRole("button", { name: H.putRecentChangesBack }));

    await waitFor(() => expect(mockedRollback).toHaveBeenCalled());
    // Dismissing on a failed prepare would strand the user: the surface would
    // vanish and nothing would have been undone.
    expect(dismiss).not.toHaveBeenCalled();
    expect(screen.getByText(S.stoppedHeading)).toBeInTheDocument();
  });
});
