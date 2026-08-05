import { AlertTriangle, ArrowRight, FlaskConical, Undo2 } from "lucide-react";
import type { HistoryEntry } from "@/lib/bindings";
import { formatWhen } from "@/lib/format";
import { interruptionStateOf, type StartupInterruption } from "@/lib/interruption";
import { STRINGS } from "@/lib/strings";

// The interruption recovery surface (v0.6.0 P1c, F-606 interruption safety,
// AC-6 and AC-7 as amended by FD-39).
//
// # What this screen is for
//
// The startup reconciler finds a tidy-up a previous session was killed in the
// middle of, verifies from disk what actually happened to the single operation
// that could have been in doubt, and repairs its own record. Until this screen
// existed it then said nothing at all, so a killed tidy-up was invisible to the
// person it happened to.
//
// # Why it decides almost nothing
//
// Two questions look like this component's to answer and are not. Whether
// carrying on is SAFE is `resume_offered`, which the reconciler sets from the
// verified on-disk outcome. What can be UNDONE is `entry.undo`, which the
// engine resolves from invariants the view cannot see: was an undo file
// exported, are the operations reversible, did reconciliation leave an
// ambiguity. FD-36 put that decision in the engine deliberately, because
// re-deriving it here would place a safety decision in the layer with the least
// context. This component renders both answers and derives neither.
//
// # Why "carry on" does not start a scan
//
// It routes to the library, where the tidy-up action already lives. Starting
// work off the back of a recovery screen is the sort of surprise this product
// exists to avoid. Re-planning from a fresh scan is also what makes carrying on
// correct at all (FD-39): books already tidied produce no operation the second
// time, so the next plan covers exactly the work that remains.

const S = STRINGS.interruption;
const H = STRINGS.history;

export interface InterruptionNoticeProps {
  interruption: StartupInterruption;
  /** The matching History row, or null when it could not be read. */
  entry: HistoryEntry | null;
  /** True while an undo plan is being prepared. */
  preparing: boolean;
  onGoToLibrary: () => void;
  onUndo: () => void;
  onOpenHistory: () => void;
}

const PRIMARY =
  "inline-flex items-center gap-1.5 rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-60";
const SECONDARY =
  "inline-flex items-center gap-1.5 rounded border border-border-2 bg-surface px-4 py-2 text-[13px] font-semibold text-ink transition-colors hover:bg-surface-2 disabled:opacity-60";

export function InterruptionNotice({
  interruption,
  entry,
  preparing,
  onGoToLibrary,
  onUndo,
  onOpenHistory,
}: InterruptionNoticeProps) {
  const state = interruptionStateOf(interruption);
  const isPractice = state === "practice-run";
  const isAmbiguous = state === "stopped-ambiguous";

  const Icon = isPractice ? FlaskConical : AlertTriangle;
  const heading = isPractice
    ? S.practiceHeading
    : isAmbiguous
      ? S.ambiguousHeading
      : S.stoppedHeading;
  const body = isPractice ? S.practiceBody : isAmbiguous ? S.ambiguousBody : S.stoppedBody;

  // Icon plus label always, never colour alone (design-system Section 8). The
  // danger pair is reserved for the one state where carrying on is unsafe;
  // the other two are "needs you", which is what --warn is for (Section 2.2).
  const glyph = isAmbiguous ? "bg-danger-bg text-danger" : "bg-warn-bg text-warn";

  const offer = entry?.undo;
  const undoLabel =
    offer?.kind === "put-everything-back"
      ? H.putEverythingBack
      : offer?.kind === "put-recent-changes-back"
        ? H.putRecentChangesBack
        : null;

  const startedWhen = entry?.startedAt ? formatWhen(entry.startedAt) : "";

  return (
    <div className="max-w-[56ch]">
      <span
        aria-hidden
        className={`mb-4 flex size-11 items-center justify-center rounded-[10px] ${glyph}`}
      >
        <Icon size={22} />
      </span>

      <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em] text-balance">
        {heading}
      </h1>
      <p className="mt-2 text-[14px] leading-relaxed text-ink-2">{body}</p>

      {!isPractice && (
        <p className="mt-3 text-[14px] font-semibold text-ink">
          {interruption.done_count > 0 ? S.booksMoved(interruption.done_count) : S.nothingMoved}
        </p>
      )}

      <div className="mt-5 flex flex-wrap gap-2.5">
        {isPractice && (
          <button type="button" onClick={onGoToLibrary} className={PRIMARY}>
            {S.practiceAction}
          </button>
        )}

        {state === "stopped-decisive" && (
          <button type="button" onClick={onGoToLibrary} className={PRIMARY}>
            <ArrowRight size={15} aria-hidden />
            {S.carryOn}
          </button>
        )}

        {!isPractice && undoLabel && (
          <button type="button" onClick={onUndo} disabled={preparing} className={SECONDARY}>
            <Undo2 size={15} aria-hidden />
            {preparing ? H.preparing : undoLabel}
          </button>
        )}

        {isAmbiguous && (
          <button type="button" onClick={onOpenHistory} className={PRIMARY}>
            {S.openHistory}
          </button>
        )}
      </div>

      {state === "stopped-decisive" && (
        <p className="mt-2.5 max-w-[48ch] text-[12.5px] leading-relaxed text-ink-3">
          {S.carryOnNote}
        </p>
      )}

      {!isPractice && undoLabel && (
        <p className="mt-1.5 max-w-[48ch] text-[12.5px] leading-relaxed text-ink-3">
          {H.undoIsReviewedFirst}
        </p>
      )}

      <details className="mt-5">
        <summary className="cursor-pointer text-[13px] text-ink-3">{S.showDetails}</summary>
        <ul className="mt-2 space-y-1 text-[12.5px] leading-relaxed text-ink-3">
          {startedWhen && <li>{S.detailStarted(startedWhen)}</li>}
          <li>{S.detailChanges(interruption.done_count)}</li>
          <li>{interruption.interrupted ? S.detailLastStepChecked : S.detailNothingInDoubt}</li>
        </ul>
      </details>
    </div>
  );
}
