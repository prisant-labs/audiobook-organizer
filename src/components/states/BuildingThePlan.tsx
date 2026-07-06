import { STRINGS } from "@/lib/strings";

export interface BuildingThePlanProps {
  /**
   * Stop the plan build (AC-26 / design-system Section 5.4). Honest cooperative
   * stop: building a plan moves no files (it only reads the snapshot and writes
   * the plan rows), so there is nothing to cancel mid-file. Stopping abandons
   * the wait and returns the review to a stopped state the user can rebuild
   * from - the register is truthful about what Stop does (it never claims to
   * undo work, because no library change happens while planning).
   */
  onStop: () => void;
}

// The DISTINCT plan-building loading state (F-908, AC-26, design-system Section
// 5.3): shown between scan completion and the review surface, never a reuse of
// the scan screen. Carries a real Stop control (Section 5.4: "every progress
// screen carries a real Stop control"). The indeterminate bar is decorative;
// the copy carries the meaning.
export function BuildingThePlan({ onStop }: BuildingThePlanProps) {
  const s = STRINGS.states.buildingThePlan;
  return (
    <div className="mx-auto flex min-h-full max-w-[52ch] flex-col justify-center px-2 py-10">
      <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em] text-balance">
        {s.heading}
      </h1>
      <p className="mt-2 text-[13.5px] leading-relaxed text-ink-2">{s.subline}</p>
      <div
        aria-hidden
        className="mt-5 h-1.5 w-full max-w-[360px] overflow-hidden rounded-full bg-surface-2"
      >
        <div className="h-full w-1/3 animate-pulse rounded-full bg-primary" />
      </div>
      <div className="mt-5">
        <button
          type="button"
          onClick={onStop}
          className="rounded border border-border-2 bg-surface px-4 py-2 text-[13px] font-semibold text-ink transition-colors hover:border-ink-3"
        >
          {s.stop}
        </button>
      </div>
    </div>
  );
}
