// Apply surface (F-904, P8, AC-27..AC-31).
//
// One screen, one running job, one unambiguous state, one primary action
// per state (design-system Section 5, "one calm primary action per screen").
//
// State machine (driven by useApplyJob):
//   running (not paused) -> Pause between books | Stop
//   running (paused)     -> [Paused between books] Resume | Stop
//   stopped              -> [Stopped between books] (no buttons)
//   completed (real)     -> [Tidy-up complete] FD-10 reassurance
//   completed (dry-run)  -> [Rehearsal complete] body
//   blocked              -> [Needs a look] Got it
//   failed               -> ErrorCallout (FD-04 surface)
//
// Copy register (CLAUDE.md, PRODUCT.md, design-system Section 6.1):
//   "books", "shelves", "copies", "tidy up", "set aside", "undo",
//   "Pause between books", "stopped between books" are the canonical terms.
//   NEVER: operations, ops, journal, manifest, rollback, quarantine, dashboard.
//
// AC-30 (FD-10 canon string): must appear character-for-character when a real
//   apply completes. Pulled from STRINGS.library.reassurance - see strings.ts.
// AC-31: --danger / --danger-bg tokens distinct from --alert; WCAG AA verified
//   by scripts/check-contrast.mjs.
import { useEffect, useRef } from "react";
import { CheckCircle2, AlertTriangle, PauseCircle, StopCircle } from "lucide-react";
import { STRINGS } from "@/lib/strings";
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
  const { phase, paused, feed, doneCount, total, mode: effectiveMode, error, actions } =
    useApplyJob(jobId, mode);

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
        aria-label="Activity"
        aria-live="polite"
        aria-relevant="additions"
      >
        {feed.length === 0 && phase === "running" && !paused && (
          <p className="text-[13.5px] text-ink-3">{STRINGS.apply.subline}</p>
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
            body={STRINGS.apply.stoppedBody}
          >
            <div className="mt-3">
              <SecondaryButton label="Done" onClick={onDone} />
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
              <PrimaryButton label="Done" onClick={onDone} />
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
              <PrimaryButton label="Done" onClick={onDone} />
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
            <div className="mt-3">
              <PrimaryButton label={STRINGS.apply.acknowledgeAction} onClick={actions.acknowledge} />
            </div>
          </StatePanel>
        )}

        {/* Failed (FD-04 surface: what happened / what is safe / what to do next) */}
        {phase === "failed" && (
          <FailedPanel detail={error} />
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

/** FD-04 failure surface: plain language, what stopped the tidy-up, what is safe, disclosure. */
function FailedPanel({ detail }: { detail: string | null }) {
  return (
    <div className="flex items-start gap-2.5" role="alert">
      <AlertTriangle size={18} aria-hidden className="mt-0.5 flex-none text-danger" />
      <div>
        <p className="text-[14px] font-semibold text-danger">{STRINGS.apply.failedHeading}</p>
        <p className="mt-1 max-w-[52ch] text-[13px] leading-relaxed text-ink-2 text-pretty">
          {STRINGS.apply.failedSafeNote}
        </p>
        {/* FD-13 disclosure: raw technical code for tier-1 support; never shown
            as the primary sentence. Summary text never contains a path. */}
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
