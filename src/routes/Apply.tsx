// Apply surface (F-904, P8, AC-27..AC-31).
//
// One screen, one running job, one unambiguous state, one primary action
// per state (design-system Section 5, "one calm primary action per screen").
//
// State machine (driven by useApplyJob):
//   running (not paused) -> Pause between books | Stop
//   running (paused)     -> [Paused between books] Resume | Stop
//   stopped              -> [Stopped between books] (no buttons)
//   completed (real)     -> [Organizing complete] FD-10 reassurance
//   completed (dry-run)  -> [Rehearsal complete] body
//   blocked              -> [Needs a look] Got it
//   failed               -> ErrorCallout (FD-04 surface)
//
// Copy register (CLAUDE.md, PRODUCT.md, design-system Section 6.1):
//   "books", "library", "duplicates", "organize", "Archive", "undo",
//   "Pause between books", "stopped between books" are the canonical terms.
//   NEVER: operations, ops, journal, manifest, rollback, quarantine, dashboard.
//   Four of the six nouns this comment used to list had been RETIRED by a
//   decision while it still taught them as canon: shelves (FD-47), copies
//   (FD-46), set aside (FD-42) and tidy up (FD-48). Nothing swept it, because
//   the vocabulary gates read shipped copy and three governance files, never
//   code comments. A stale list of retired words presented as the rule is worse
//   than no list, so this one now carries its decision ids.
//   "Organize" has no noun form (FD-48): use "the plan", "the changes", or
//   "run" where copy needs one.
//
// AC-30 (FD-10 canon string): must appear character-for-character when a real
//   apply completes. Pulled from STRINGS.library.reassurance - see strings.ts.
// AC-31: --danger / --danger-bg tokens distinct from --alert; WCAG AA verified
//   by scripts/check-contrast.mjs.
import { useEffect, useRef } from "react";
import { CheckCircle2, AlertTriangle, PauseCircle, StopCircle } from "lucide-react";
import { STRINGS } from "@/lib/strings";
import { copyForCode } from "@/lib/errorCopy";
import { useApplyJob } from "@/hooks/useApplyJob";

export interface ApplyProps {
  jobId: number;
  /** "dry-run" | "real" - set at apply_start time by the parent caller. */
  mode?: "dry-run" | "real";
  /** Called when the user is done with the apply surface and wants to navigate away. */
  onDone: () => void;
}

// ---------- sub-components ----------

/** Primary action button (one per state, calm weight). */
function PrimaryButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded bg-primary px-5 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
    >
      {label}
    </button>
  );
}

/** Secondary action button (Stop - never the primary action, always the escape hatch). */
function SecondaryButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded border border-border-2 bg-surface px-5 py-2 text-[13px] font-semibold text-ink transition-colors hover:border-ink-3"
    >
      {label}
    </button>
  );
}

// ---------- main component ----------

export function Apply({ jobId, mode = "dry-run", onDone }: ApplyProps) {
  const {
    phase,
    paused,
    feed,
    doneCount,
    total,
    mode: effectiveMode,
    errorCode,
    error,
    discrepancyCount,
    actions,
  } = useApplyJob(jobId, mode);

  // Auto-scroll the feed tail to the bottom whenever new sentences arrive.
  const feedRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (feedRef.current) {
      feedRef.current.scrollTop = feedRef.current.scrollHeight;
    }
  }, [feed]);

  return (
    <div className="flex h-full flex-col gap-0 overflow-hidden">
      {/* Header */}
      <div className="flex shrink-0 items-start justify-between border-b border-border px-6 py-4">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="font-serif text-[22px] font-medium leading-tight tracking-[-0.01em]">
              {STRINGS.apply.heading}
            </h1>
            {effectiveMode === "dry-run" && (
              <span className="rounded-full bg-warn-bg px-2.5 py-0.5 text-[11.5px] font-semibold text-warn">
                {STRINGS.apply.rehearsalBadge}
              </span>
            )}
          </div>
          {/* Progress count - shown while running and in terminal states */}
          <p className="mt-0.5 text-[13px] text-ink-3">
            {STRINGS.apply.progressLine(doneCount, total)}
          </p>
        </div>
      </div>

      {/* Scrolling sentence feed */}
      <div
        ref={feedRef}
        className="flex-1 overflow-y-auto px-6 py-4"
        role="log"
        aria-label={STRINGS.apply.activityLabel}
        aria-live="polite"
        aria-relevant="additions"
      >
        {feed.length === 0 && phase === "running" && !paused && (
          <p className="text-[13.5px] text-ink-3">
            {effectiveMode === "dry-run" ? STRINGS.apply.rehearsalSubline : STRINGS.apply.subline}
          </p>
        )}
        <ul className="space-y-1.5 list-none">
          {feed.map((item) => (
            <li key={item.id} className="text-[13.5px] leading-relaxed text-ink-2">
              {item.sentence}
            </li>
          ))}
        </ul>
      </div>

      {/* State panel: phase-specific content + action buttons */}
      <div className="shrink-0 border-t border-border px-6 py-4">
        {/* Running - paused */}
        {phase === "running" && paused && (
          <StatePanel
            icon={<PauseCircle size={18} aria-hidden className="text-warn" />}
            heading={STRINGS.apply.pausedHeading}
            body={STRINGS.apply.pausedBody}
          >
            <div className="mt-3 flex flex-wrap gap-2.5">
              <PrimaryButton label={STRINGS.apply.resumeAction} onClick={actions.resume} />
              <SecondaryButton label={STRINGS.apply.stopAction} onClick={actions.stop} />
            </div>
          </StatePanel>
        )}

        {/* Running - active */}
        {phase === "running" && !paused && (
          <div className="flex flex-wrap gap-2.5">
            <PrimaryButton label={STRINGS.apply.pauseAction} onClick={actions.pause} />
            <SecondaryButton label={STRINGS.apply.stopAction} onClick={actions.stop} />
          </div>
        )}

        {/* Stopped */}
        {phase === "stopped" && (
          <StatePanel
            icon={<StopCircle size={18} aria-hidden className="text-ink-3" />}
            heading={STRINGS.apply.stoppedHeading}
            body={
              effectiveMode === "dry-run"
                ? STRINGS.apply.rehearsalStoppedBody
                : STRINGS.apply.stoppedBody
            }
          >
            <div className="mt-3">
              <PrimaryButton label={STRINGS.apply.doneAction} onClick={onDone} />
            </div>
          </StatePanel>
        )}

        {/* Completed - real apply */}
        {phase === "completed" && effectiveMode === "real" && (
          <StatePanel
            icon={<CheckCircle2 size={18} aria-hidden className="text-good" />}
            heading={STRINGS.apply.completedHeading}
            body={STRINGS.apply.completedBody}
          >
            {/* AC-30: FD-10 canon string must appear exactly */}
            <p className="mt-2 text-[13px] text-ink-3 max-w-[52ch] text-pretty">
              {STRINGS.library.reassurance}
            </p>
            <p className="mt-1 text-[12.5px] text-ink-3">{STRINGS.apply.undoSaved}</p>
            <div className="mt-3">
              <PrimaryButton label={STRINGS.apply.doneAction} onClick={onDone} />
            </div>
          </StatePanel>
        )}

        {/* Completed - rehearsal (dry-run) */}
        {phase === "completed" && effectiveMode === "dry-run" && (
          <StatePanel
            icon={<CheckCircle2 size={18} aria-hidden className="text-good" />}
            heading={STRINGS.apply.rehearsalCompletedHeading}
            body={STRINGS.apply.rehearsalCompletedBody}
          >
            <div className="mt-3">
              <PrimaryButton label={STRINGS.apply.doneAction} onClick={onDone} />
            </div>
          </StatePanel>
        )}

        {/* Blocked (completed but needs acknowledge) */}
        {phase === "blocked" && (
          <StatePanel
            icon={<AlertTriangle size={18} aria-hidden className="text-warn" />}
            heading={STRINGS.apply.blockedHeading}
            body={STRINGS.apply.blockedBody}
          >
            {/* IMPORTANT 5: name how many differences the check found, so "Got it"
                is no longer the only content. The technical pointer to the saved
                report sits behind "Show file details" (FD-13). */}
            {discrepancyCount > 0 && (
              <p className="mt-1.5 text-[13px] font-medium text-ink-2">
                {STRINGS.apply.blockedCountLine(discrepancyCount)}
              </p>
            )}
            <details className="mt-2 max-w-[52ch]">
              <summary className="cursor-pointer text-[12.5px] font-semibold text-link">
                {STRINGS.states.showFileDetails}
              </summary>
              <p className="mt-2 text-[12.5px] leading-relaxed text-ink-3">
                {STRINGS.apply.blockedReportPointer}
              </p>
            </details>
            <div className="mt-3">
              <PrimaryButton label={STRINGS.apply.acknowledgeAction} onClick={actions.acknowledge} />
            </div>
          </StatePanel>
        )}

        {/* Failed (FD-04 surface: what happened / what is safe / what to do next) */}
        {phase === "failed" && (
          <FailedPanel errorCode={errorCode} detail={error} onDone={onDone} />
        )}
      </div>
    </div>
  );
}

// ---------- helpers ----------

interface StatePanelProps {
  icon: React.ReactNode;
  heading: string;
  body: string;
  children?: React.ReactNode;
}

/** Compact state-panel row: icon + heading + body + optional slot for actions. */
function StatePanel({ icon, heading, body, children }: StatePanelProps) {
  return (
    <div className="flex items-start gap-2.5">
      <span className="mt-0.5 flex-none">{icon}</span>
      <div>
        <p className="text-[14px] font-semibold text-ink">{heading}</p>
        <p className="mt-0.5 max-w-[52ch] text-[13px] leading-relaxed text-ink-2 text-pretty">{body}</p>
        {children}
      </div>
    </div>
  );
}

/**
 * FD-04 failure surface (Critical 2): the three family-safe parts, driven by the
 * per-code copy map so the "what happened" and "what to do next" lines are
 * specific to the actual failure, not a generic heading. The machine code stays
 * behind "Show file details" (FD-13), and a primary action lets the user leave a
 * failed apply (the surface is otherwise a dead end - the shell clears the active
 * job only through this handler).
 *
 * The tone (`--danger` vs `--warn`) follows the code's own copy entry, exactly as
 * the shared ErrorCallout does everywhere else; both token pairs are WCAG AA in
 * both themes (AC-31), and both are distinct from `--alert`.
 */
function FailedPanel({
  errorCode,
  detail,
  onDone,
}: {
  errorCode: string | null;
  detail: string | null;
  onDone: () => void;
}) {
  const copy = copyForCode(errorCode ?? "");
  const toneText = copy.tone === "danger" ? "text-danger" : "text-warn";
  return (
    <div className="flex items-start gap-2.5" role="alert">
      <AlertTriangle size={18} aria-hidden className={`mt-0.5 flex-none ${toneText}`} />
      <div>
        {/* What happened: the plain-language banner + the code-specific sentence. */}
        <p className={`text-[14px] font-semibold ${toneText}`}>{STRINGS.apply.failedHeading}</p>
        <p className="mt-1 max-w-[52ch] text-[13px] leading-relaxed text-ink-2 text-pretty">
          {copy.sentence}
        </p>
        {/* What is safe: always true - no audiobook is ever left half-moved. */}
        <p className="mt-1.5 max-w-[52ch] text-[13px] leading-relaxed text-ink-2 text-pretty">
          {STRINGS.apply.failedSafeNote}
        </p>
        {/* What to do next: the one calm remediation for this specific code. */}
        <p className="mt-1.5 max-w-[52ch] text-[13px] leading-relaxed text-ink-2 text-pretty">
          {copy.nextStep}
        </p>
        <div className="mt-3">
          <PrimaryButton label={STRINGS.apply.doneAction} onClick={onDone} />
        </div>
        {/* FD-13 disclosure: the raw machine code for tier-1 support; never shown
            as a family-facing line. The summary text never contains a path. */}
        {detail && (
          <details className="mt-3 max-w-[52ch]">
            <summary className="cursor-pointer text-[12.5px] font-semibold text-link">
              {STRINGS.states.showFileDetails}
            </summary>
            <pre className="mt-2 overflow-x-auto rounded bg-surface-2 p-3 font-mono text-[11.5px] leading-relaxed text-ink-2">
              {detail}
            </pre>
          </details>
        )}
      </div>
    </div>
  );
}
