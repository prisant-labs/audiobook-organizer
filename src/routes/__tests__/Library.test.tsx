import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Library } from "../Library";
import { commands } from "@/lib/bindings";
import { getCover } from "@/lib/covers";
import type { LibraryOverview } from "@/lib/bindings";
import type { UseHealthMetrics } from "@/hooks/useHealthMetrics";

// "Mocked bindings" pattern: the raw scan_start/job-event bindings the scan
// affordance calls directly, plus the cover fetch a worth-a-look BookSlot
// triggers as a side effect of rendering. `health` (AppShell's ONE
// `useHealthMetrics()` call, see useNavCounts.ts) is passed straight in as a
// prop, so this test constructs it directly rather than mocking the fetch
// layer underneath it (see useHealthMetrics.test.tsx for that hook's own
// tests).
vi.mock("@/lib/bindings", () => ({
  commands: { scanStart: vi.fn(), scanCancel: vi.fn() },
  events: {
    jobCompleted: { listen: vi.fn().mockResolvedValue(() => {}) },
    jobFailed: { listen: vi.fn().mockResolvedValue(() => {}) },
    jobProgress: { listen: vi.fn().mockResolvedValue(() => {}) },
  },
}));
vi.mock("@/lib/covers", () => ({ getCover: vi.fn() }));

const mockedScanStart = vi.mocked(commands.scanStart);
const mockedScanCancel = vi.mocked(commands.scanCancel);
const mockedGetCover = vi.mocked(getCover);

afterEach(cleanup);
beforeEach(() => {
  mockedScanStart.mockReset();
  mockedScanCancel.mockReset();
  mockedGetCover.mockReset().mockResolvedValue(null);
});

function health(overview: LibraryOverview | null, extra: Partial<UseHealthMetrics> = {}): UseHealthMetrics {
  return { overview, status: "ready", error: null, reload: vi.fn(), ...extra };
}

const OVERVIEW: LibraryOverview = {
  scan_id: 9,
  total_books: 1022,
  total_bytes: 297 * 1024 ** 3,
  needs_tidy_books: 412,
  worth_a_look: [
    {
      entry_id: 1,
      title: "Sapiens",
      author: "Y. N. Harari",
      reason: { kind: "warn", text: "loose file" },
    },
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

describe("Library", () => {
  it("shows the loading skeleton while the first load is pending", () => {
    const { container } = render(
      <Library onNavigate={vi.fn()} health={health(null, { status: "loading" })} />,
    );
    expect(container.querySelector('[aria-hidden="true"]')).toBeInTheDocument();
    expect(screen.queryByText(/audiobooks/)).toBeNull();
  });

  it("shows the honest pre-first-scan state when no scan has ever completed (AC-6)", () => {
    render(<Library onNavigate={vi.fn()} health={health(null)} />);

    expect(screen.getByRole("button", { name: /scan your library/i })).toBeInTheDocument();
    // No fabricated numbers appear pre-scan (AC-7/FD-27).
    expect(screen.queryByText(/audiobooks/)).toBeNull();
  });

  it("starts a scan from the pre-first-scan affordance", async () => {
    const user = userEvent.setup();
    mockedScanStart.mockResolvedValue({ status: "ok", data: { job_id: 5 } });
    render(<Library onNavigate={vi.fn()} health={health(null)} />);

    await user.click(screen.getByRole("button", { name: /scan your library/i }));

    await waitFor(() => expect(mockedScanStart).toHaveBeenCalledTimes(1));
  });

  it("renders the lede sentence, shelves, and the verbatim FD-10 reassurance once a scan exists", () => {
    render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);

    expect(screen.getByText(/1,022 audiobooks/)).toBeInTheDocument();
    expect(screen.getByText(/412/)).toBeInTheDocument();
    expect(screen.getByText("Sapiens")).toBeInTheDocument();
    // The series name also appears vertically on the center spine, so there
    // are two matches; either is proof the series shelf rendered.
    expect(screen.getAllByText("The Dresden Files").length).toBeGreaterThanOrEqual(1);
    // AC-9: the FD-10 guarantee, verbatim.
    expect(
      screen.getByText(
        "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.",
      ),
    ).toBeInTheDocument();
  });

  it("presents exactly one primary action alongside the secondary scan-again control (AC-6)", () => {
    const { container } = render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);

    expect(screen.getByRole("button", { name: "Start a tidy-up" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Scan again" })).toBeInTheDocument();
    // "One calm primary action per screen" (design-system Section 1.1/7): only
    // "Start a tidy-up" carries the primary button treatment.
    expect(container.querySelectorAll("button.bg-primary")).toHaveLength(1);
  });

  it("never renders a stat-band/hero-tile class name (D-06 anti-reference)", () => {
    const { container } = render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);
    expect(container.querySelector(".stat-band, .hero-metric, .kpi-tile")).toBeNull();
  });

  it("routes to Tidy-up when 'Start a tidy-up' is clicked", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(<Library onNavigate={onNavigate} health={health(OVERVIEW)} />);

    await user.click(screen.getByRole("button", { name: "Start a tidy-up" }));
    expect(onNavigate).toHaveBeenCalledWith("tidy-up");
  });

  it("surfaces a retry control when the overview failed to load, and retry calls reload()", async () => {
    const user = userEvent.setup();
    const reload = vi.fn();
    render(
      <Library
        onNavigate={vi.fn()}
        health={health(null, { status: "error", error: "overview-failed: database is locked", reload })}
      />,
    );

    expect(screen.getByText(/overview-failed/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /try again/i }));
    expect(reload).toHaveBeenCalledTimes(1);
  });

  // F-908 scan-failure and F-909 root-missing surfaces (T-30, AC-24/AC-31).
  describe("scan failure and root re-pick surfaces", () => {
    it("renders a family-safe scan-failure surface with retry when scan_start fails (AC-24)", async () => {
      const user = userEvent.setup();
      mockedScanStart.mockResolvedValue({
        status: "error",
        error: { "scan-failed": { detail: "the drive fell over" } },
      });
      render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);

      await user.click(screen.getByRole("button", { name: "Scan again" }));

      // The mapped plain sentence, not the raw code, is the family-facing line.
      expect(await screen.findByText("The scan stopped before it finished.")).toBeInTheDocument();
      // The raw cause is available only inside the disclosure.
      expect(screen.getByText(/the drive fell over/)).toBeInTheDocument();
      // Retry re-attempts the scan.
      mockedScanStart.mockResolvedValue({ status: "ok", data: { job_id: 3 } });
      await user.click(screen.getByRole("button", { name: /try again/i }));
      await waitFor(() => expect(mockedScanStart).toHaveBeenCalledTimes(2));
    });

    it("routes a missing persisted root to a re-pick surface wired to onRepickRoot (AC-31)", async () => {
      const user = userEvent.setup();
      const onRepickRoot = vi.fn().mockResolvedValue(true);
      mockedScanStart.mockResolvedValue({
        status: "error",
        error: { "root-not-found": { path: "E:\\Books" } },
      });
      render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} onRepickRoot={onRepickRoot} />);

      await user.click(screen.getByRole("button", { name: "Scan again" }));

      expect(await screen.findByText("Your library folder couldn't be found.")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /choose your library folder again/i }));
      expect(onRepickRoot).toHaveBeenCalledTimes(1);
    });
  });

  // F-104 scan Stop control (T-28, AC-36): cooperative cancel, never "Skip ahead".
  describe("scan Stop control", () => {
    it("shows a working Stop control once a scan is running, wired to scan_cancel", async () => {
      const user = userEvent.setup();
      mockedScanStart.mockResolvedValue({ status: "ok", data: { job_id: 7 } });
      mockedScanCancel.mockResolvedValue(true);
      render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);

      await user.click(screen.getByRole("button", { name: "Scan again" }));
      const stop = await screen.findByRole("button", { name: "Stop" });

      await user.click(stop);

      expect(mockedScanCancel).toHaveBeenCalledWith(7);
      expect(await screen.findByText(/stopped/i)).toBeInTheDocument();
      // The reassurance that a scan only reads accompanies the stopped note.
      expect(screen.getByText(/nothing was changed/i)).toBeInTheDocument();
    });

    it("does not transition to stopped when scan_cancel reports the job already finished", async () => {
      const user = userEvent.setup();
      mockedScanStart.mockResolvedValue({ status: "ok", data: { job_id: 8 } });
      mockedScanCancel.mockResolvedValue(false);
      render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);

      await user.click(screen.getByRole("button", { name: "Scan again" }));
      const stop = await screen.findByRole("button", { name: "Stop" });
      await user.click(stop);

      expect(mockedScanCancel).toHaveBeenCalledWith(8);
      expect(screen.queryByText(/^Stopped\./)).toBeNull();
    });

    it("never renders a 'Skip ahead' affordance (FD-02: demo-only, does not ship)", async () => {
      const user = userEvent.setup();
      mockedScanStart.mockResolvedValue({ status: "ok", data: { job_id: 9 } });
      render(<Library onNavigate={vi.fn()} health={health(OVERVIEW)} />);

      await user.click(screen.getByRole("button", { name: "Scan again" }));
      await screen.findByRole("button", { name: "Stop" });

      expect(screen.queryByText(/skip ahead/i)).toBeNull();
    });
  });
});
