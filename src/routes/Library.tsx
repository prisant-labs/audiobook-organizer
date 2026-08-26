import { useCallback, useEffect, useRef, useState } from "react";
import { Library as LibraryIcon } from "lucide-react";
import { commands, events } from "@/lib/bindings";
import { appErrorCode, formatAppError } from "@/lib/appError";
import { copyForCode } from "@/lib/errorCopy";
import { STRINGS } from "@/lib/strings";
import type { UseHealthMetrics } from "@/hooks/useHealthMetrics";
import { LibrarySkeleton } from "@/components/library/LibrarySkeleton";
import { LibraryLede } from "@/components/library/LibraryLede";
import { ShelfSection } from "@/components/library/ShelfSection";
import { BookSlot } from "@/components/library/BookSlot";
import { SpineCluster } from "@/components/library/SpineCluster";
import { GoodNewsLine } from "@/components/library/GoodNewsLine";
import { ScanProgress } from "@/components/ScanProgress";
import { ErrorCallout } from "@/components/states/ErrorCallout";
import { EmptyState } from "@/components/states/EmptyState";
import type { RouteId } from "@/routes";

export interface LibraryProps {
  onNavigate: (route: RouteId) => void;
  // Owned by `AppShell` (ONE `useHealthMetrics()` call for the whole shell)
  // and passed down here, rather than this route calling the hook itself: the
  // Sidebar badges (`navCountsFrom`) read the SAME `overview`, so a completed
  // scan's `reload()` (called below) updates both at once. See useNavCounts.ts.
  health: UseHealthMetrics;
  // F-909 re-pick (AC-31): open the OS folder picker and persist the new
  // library root, resolving `true` when a folder was chosen. Provided by
  // AppShell (which owns settings + the dialog seam). Optional so the route
  // still renders in isolation (tests); the root-missing surface falls back to
  // a plain retry when it is absent.
  onRepickRoot?: () => Promise<boolean>;
}

// Two codes mean "the persisted library folder isn't usable and the fix is to
// re-pick it" (F-909, AC-31), as opposed to a transient scan failure.
const ROOT_MISSING_CODES = new Set(["root-not-found", "root-not-directory"]);

type ScanRunState =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "running"; jobId: number; done?: number; total?: number }
  | { phase: "stopped" }
  // The persisted root is gone (F-909 re-pick surface, AC-31).
  | { phase: "root-missing"; code: string; detail: string | null }
  // Any other scan failure (F-908 scan-failure surface, AC-24).
  | { phase: "failed"; code: string | null; detail: string | null };

// F-902 library home (T-15..T-18, AC-6..AC-9) with the F-908 error/empty
// states wired in (Phase 7, T-30..T-32). Every count/byte figure comes from
// `health.overview` (`classify_overview`) at render time (AC-7); the scan
// affordance wires `scan_start` and the job events so a fresh scan reloads the
// same data. Loading uses the library skeleton; the pre-first-scan and
// load-failure cases render through the shared F-908 state components rather
// than ad-hoc inline markup.
export function Library({ onNavigate, health, onRepickRoot }: LibraryProps) {
  const { overview, status, error, errorCode, reload } = health;
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
      // The job:failed event carries only the stable code, not the structured
      // payload; that is enough to map it to family-safe copy (F-908). A root
      // that went missing mid-scan routes to the re-pick surface (AC-31).
      const code = event.payload.code;
      if (ROOT_MISSING_CODES.has(code)) {
        setScanState({ phase: "root-missing", code, detail: null });
      } else {
        setScanState({ phase: "failed", code, detail: null });
      }
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
      const code = appErrorCode(result.error);
      const detail = formatAppError(result.error);
      // A missing/invalid persisted root is the F-909 re-pick case (AC-31);
      // every other synchronous scan failure is the F-908 scan-failure surface.
      if (ROOT_MISSING_CODES.has(code)) {
        setScanState({ phase: "root-missing", code, detail });
      } else {
        setScanState({ phase: "failed", code, detail });
      }
    }
  }, []);

  // Scan Stop control (F-104, AC-36): cooperative cancel at the next safe
  // boundary. `scan_cancel` is synchronous and returns whether a running job
  // was actually signalled; the backend never emits a job:completed/failed
  // event for a cancelled job, so this is the one place that transitions local
  // state to the honest "stopped" outcome.
  const stopScan = useCallback(async () => {
    if (scanState.phase !== "running") return;
    const stopped = await commands.scanCancel(scanState.jobId);
    if (!mountedRef.current || !stopped) return;
    currentJobIdRef.current = null;
    setScanState({ phase: "stopped" });
  }, [scanState]);

  // F-909 re-pick action (AC-31): open the picker, persist, and on success
  // clear the failure and reload the home from the new root.
  const repickRoot = useCallback(async () => {
    if (!onRepickRoot) return;
    const chosen = await onRepickRoot();
    if (!mountedRef.current || !chosen) return;
    setScanState({ phase: "idle" });
    reload();
  }, [onRepickRoot, reload]);

  if (status === "loading") {
    return <LibrarySkeleton />;
  }

  if (status === "error") {
    // The whole home failed to load (a rare classify_overview read failure).
    // Family-safe surface mapped from the code when carried; the raw cause
    // rides in the disclosure, never as the sentence (AC-24).
    return (
      <ErrorCallout copy={errorCode ? copyForCode(errorCode) : null} detail={error} onRetry={reload} />
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
      {scanState.phase === "root-missing" && (
        <ErrorCallout
          compact
          copy={copyForCode(scanState.code)}
          detail={scanState.detail}
          action={onRepickRoot ? { label: STRINGS.states.rootMissingRepick, onClick: () => void repickRoot() } : null}
          onRetry={() => void startScan()}
          retryLabel={STRINGS.states.tryAgain}
        />
      )}
      {scanState.phase === "failed" && (
        <ErrorCallout
          compact
          copy={scanState.code ? copyForCode(scanState.code) : null}
          detail={scanState.detail}
          onRetry={() => void startScan()}
        />
      )}
    </>
  );

  if (!overview) {
    // The honest pre-first-scan state (AC-6): "never scanned" is never shown as
    // a library of zero books. Design-system Section 5.2 empty register.
    return (
      <EmptyState
        icon={<LibraryIcon size={22} />}
        heading={STRINGS.library.noScanYet.heading}
        body={STRINGS.library.noScanYet.body}
        action={{ label: scanning ? STRINGS.library.scanning.heading : STRINGS.library.scanNow, onClick: startScan, disabled: scanning }}
      >
        {scanStatusArea}
      </EmptyState>
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
            onClick={() => onNavigate("organize")}
            className="rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
          >
            {STRINGS.library.startOrganizing}
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
            <SpineCluster key={series.entry_id} series={series} />
          ))}
        </ShelfSection>
      )}

      <GoodNewsLine goodNews={overview.good_news} />

      <p className="mt-4 max-w-[1060px] text-[12px] text-ink-3">{STRINGS.library.reassurance}</p>
    </div>
  );
}
