import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events } from "@/lib/bindings";
import { formatAppError } from "@/lib/appError";
import { scanFailureMessage } from "@/lib/scanFailure";
import { STRINGS } from "@/lib/strings";
import type { UseHealthMetrics } from "@/hooks/useHealthMetrics";
import { LibrarySkeleton } from "@/components/library/LibrarySkeleton";
import { LibraryLede } from "@/components/library/LibraryLede";
import { ShelfSection } from "@/components/library/ShelfSection";
import { BookSlot } from "@/components/library/BookSlot";
import { SpineCluster } from "@/components/library/SpineCluster";
import { GoodNewsLine } from "@/components/library/GoodNewsLine";
import { ScanProgress } from "@/components/ScanProgress";
import type { RouteId } from "@/routes";

export interface LibraryProps {
  onNavigate: (route: RouteId) => void;
  // Owned by `AppShell` (ONE `useHealthMetrics()` call for the whole shell)
  // and passed down here, rather than this route calling the hook itself: the
  // Sidebar badges (`navCountsFrom`) read the SAME `overview`, so a completed
  // scan's `reload()` (called below) updates both at once. See useNavCounts.ts.
  health: UseHealthMetrics;
}

type ScanRunState =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "running"; jobId: number; done?: number; total?: number }
  | { phase: "stopped" }
  | { phase: "failed"; message: string };

// F-902 library home (T-15..T-18, AC-6..AC-9): the warm, cover-forward screen
// the family sees first. Every count/byte figure comes from `health.overview`
// (`classify_overview`) at render time (AC-7); the scan affordance wires
// `scan_start` and the job events so a fresh scan reloads the same data (T-15
// brief). Loading, error, and honest pre-first-scan states are handled inline
// here rather than through the full F-908 state components (`src/components/
// states/*`), which are Phase 7's brief (T-29..T-32) - this route's minimal
// versions are marked below and are meant to be superseded, not duplicated,
// when that phase lands.
export function Library({ onNavigate, health }: LibraryProps) {
  const { overview, status, error, reload } = health;
  const [scanState, setScanState] = useState<ScanRunState>({ phase: "idle" });
  const currentJobIdRef = useRef<number | "pending" | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    const unlistenCompleted = events.jobCompleted.listen((event) => {
      if (currentJobIdRef.current !== event.payload.job_id) return;
      currentJobIdRef.current = null;
      if (!mountedRef.current) return;
      setScanState({ phase: "idle" });
      reload();
    });
    const unlistenFailed = events.jobFailed.listen((event) => {
      if (currentJobIdRef.current !== event.payload.job_id) return;
      currentJobIdRef.current = null;
      if (!mountedRef.current) return;
      setScanState({ phase: "failed", message: scanFailureMessage(event.payload.code) });
    });
    const unlistenProgress = events.jobProgress.listen((event) => {
      if (currentJobIdRef.current !== event.payload.job_id) return;
      if (!mountedRef.current) return;
      setScanState((prev) =>
        prev.phase === "running" || prev.phase === "starting"
          ? {
              phase: "running",
              jobId: event.payload.job_id,
              done: event.payload.done,
              total: event.payload.total_estimate ?? undefined,
            }
          : prev,
      );
    });
    return () => {
      unlistenCompleted.then((f) => f());
      unlistenFailed.then((f) => f());
      unlistenProgress.then((f) => f());
    };
  }, [reload]);

  const startScan = useCallback(async () => {
    currentJobIdRef.current = "pending";
    setScanState({ phase: "starting" });
    const result = await commands.scanStart();
    if (!mountedRef.current) return;
    if (result.status === "ok") {
      currentJobIdRef.current = result.data.job_id;
      setScanState({ phase: "running", jobId: result.data.job_id });
    } else {
      currentJobIdRef.current = null;
      setScanState({ phase: "failed", message: formatAppError(result.error) });
    }
  }, []);

  // Scan Stop control (F-104, AC-36): cooperative cancel at the next safe
  // boundary. `scan_cancel` is synchronous and returns whether a running job
  // was actually signalled; the backend never emits a job:completed/failed
  // event for a cancelled job (the requester already knows), so this is the
  // one place that transitions local state to the honest "stopped" outcome.
  // Ignoring any later job:progress/completed/failed for this job_id (by
  // clearing currentJobIdRef) is what keeps that late traffic from silently
  // reviving a screen the user already asked to stop.
  const stopScan = useCallback(async () => {
    if (scanState.phase !== "running") return;
    const stopped = await commands.scanCancel(scanState.jobId);
    if (!mountedRef.current || !stopped) return;
    currentJobIdRef.current = null;
    setScanState({ phase: "stopped" });
  }, [scanState]);

  if (status === "loading") {
    return <LibrarySkeleton />;
  }

  if (status === "error") {
    return (
      <div className="max-w-[52ch]">
        <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em]">
          {STRINGS.library.heading}
        </h1>
        <p className="mt-3 text-[14px] leading-relaxed text-danger">{error}</p>
        <button
          type="button"
          onClick={reload}
          className="mt-4 rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
        >
          Try again
        </button>
      </div>
    );
  }

  const scanning = scanState.phase === "starting" || scanState.phase === "running";
  const scanStatusArea = (
    <>
      {scanState.phase === "starting" && (
        <p className="mt-2 tabular-nums text-[12.5px] text-ink-3">{STRINGS.library.scanning.heading}</p>
      )}
      {scanState.phase === "running" && (
        <ScanProgress done={scanState.done} total={scanState.total} onStop={() => void stopScan()} />
      )}
      {scanState.phase === "stopped" && (
        <p className="mt-2 text-[12.5px] text-ink-3">{STRINGS.library.stopped}</p>
      )}
      {scanState.phase === "failed" && (
        <p className="mt-2 text-[12.5px] text-danger">{scanState.message}</p>
      )}
    </>
  );

  if (!overview) {
    return (
      <div className="max-w-[52ch]">
        <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em] [text-wrap:balance]">
          {STRINGS.library.noScanYet.heading}
        </h1>
        <p className="mt-3 text-[14.5px] leading-[1.55] text-ink-2 [text-wrap:pretty]">
          {STRINGS.library.noScanYet.body}
        </p>
        <button
          type="button"
          onClick={startScan}
          disabled={scanning}
          className="mt-5 rounded bg-primary px-4 py-2.5 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-60"
        >
          {scanning ? STRINGS.library.scanning.heading : STRINGS.library.scanNow}
        </button>
        {scanStatusArea}
      </div>
    );
  }

  return (
    <div>
      <header className="flex max-w-[1060px] items-start justify-between gap-6">
        <div>
          <h1 className="font-serif text-[30px] font-medium tracking-[-0.01em] [text-wrap:balance]">
            {STRINGS.library.heading}
          </h1>
          <LibraryLede overview={overview} />
          {scanStatusArea}
        </div>
        <div className="flex flex-none gap-2.5 pt-1.5">
          <button
            type="button"
            onClick={startScan}
            disabled={scanning}
            className="rounded border border-border-2 bg-surface px-4 py-2 text-[13px] font-semibold text-ink transition-colors hover:border-ink-3 disabled:opacity-60"
          >
            {scanning ? STRINGS.library.scanning.heading : STRINGS.library.scanAgain}
          </button>
          <button
            type="button"
            onClick={() => onNavigate("tidy-up")}
            className="rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
          >
            {STRINGS.library.startTidyUp}
          </button>
        </div>
      </header>

      {overview.worth_a_look.length > 0 && (
        <ShelfSection
          heading={STRINGS.library.worthALookHeading}
          subline={STRINGS.library.worthALookSubline}
        >
          {overview.worth_a_look.map((book) => (
            <BookSlot key={book.entry_id} scanId={overview.scan_id} book={book} />
          ))}
        </ShelfSection>
      )}

      {overview.series.length > 0 && (
        <ShelfSection
          heading={STRINGS.library.seriesHeading}
          subline={STRINGS.library.seriesSubline}
        >
          {overview.series.map((series) => (
            <SpineCluster key={series.name} series={series} />
          ))}
        </ShelfSection>
      )}

      <GoodNewsLine goodNews={overview.good_news} />

      <p className="mt-4 max-w-[1060px] text-[12px] text-ink-3">{STRINGS.library.reassurance}</p>
    </div>
  );
}
