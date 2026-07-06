import { STRINGS } from "@/lib/strings";

export interface ScanProgressProps {
  /** Entries read so far, when known (`job:progress`'s `done`). */
  done?: number;
  /** Best-known total, when known; an indeterminate scan omits it. */
  total?: number;
  /** Cooperative Stop (F-104, AC-36): flips the backend's cancel flag for this job. */
  onStop: () => void;
}

// Scan/re-scan progress (design-system Section 4.16 "progress set", scaled to
// this release's minimal treatment): a tabular-nums status line plus a real
// Stop control. Every progress screen in this release carries a working Stop
// (AC-36); there is deliberately no "Skip ahead" affordance (FD-02) - the
// prototype's demo-only shortcut never ships. The full `.centercol`/big-
// percent/rolling-feed treatment and the DISTINCT "Building the tidy-up plan"
// loading state (design-system Section 5.3) are Phase 7 scope (T-29..T-32);
// this component is the scan-side piece T-28 asks for and is meant to be
// reused (not duplicated) when that phase lands.
export function ScanProgress({ done, total, onStop }: ScanProgressProps) {
  const s = STRINGS.library;
  const statusLine =
    total != null ? `${(done ?? 0).toLocaleString("en-US")} of ${total.toLocaleString("en-US")} read` : s.scanning.heading;

  return (
    <div className="mt-2 flex items-center gap-3">
      <p className="tabular-nums text-[12.5px] text-ink-3">{statusLine}</p>
      <button
        type="button"
        onClick={onStop}
        className="rounded border border-border-2 px-2.5 py-1 text-[11.5px] font-semibold text-ink-2 transition-colors hover:border-ink-3 hover:text-ink"
      >
        {s.stopScan}
      </button>
    </div>
  );
}
