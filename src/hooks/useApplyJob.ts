// useApplyJob hook (F-904, P8, AC-27..AC-31).
//
// Owns the apply-surface state machine for a live apply job:
//  - Subscribes to `apply:op-executed` to build a scrolling sentence feed.
//  - Subscribes to `job:completed` / `job:failed` / `job:stopped` to detect every
//    terminal transition reliably (the walk fires no op event after a Stop, and a
//    halt fires none for the failed op - so a status poll alone would race them).
//  - Refreshes `job_status` after each event to derive the current phase (including
//    the `paused` flag, which lives only in memory on the backend registry) and to
//    backfill the progress counters from the durable journal on mount.
//  - Exposes `pause`, `resume`, `stop`, and `acknowledge` actions.
//
// Phase mapping (from JobStatus.state + .paused + .blocks_further_tidying):
//   state="running", paused=false  -> phase="running", paused=false
//   state="running", paused=true   -> phase="running", paused=true
//   state="stopped"                -> phase="stopped"
//   state="completed", !blocked    -> phase="completed"
//   state="completed", blocked     -> phase="blocked"
//   state="failed"                 -> phase="failed"
//
// Feed sentence templates come from STRINGS.apply so every sentence
// is in the centralized copy register (FD-23). No raw paths ever enter the
// feed - `ApplyOpExecutedPayload.label` is the last path component only,
// stripped of extension for audio files, as the backend guarantees.
import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";

// ---------- public types ----------

export type ApplyPhase =
  | "running"
  | "stopped"
  | "completed"
  | "blocked"
  | "failed";

export interface FeedItem {
  /** `op_id` from the backend - unique within a job, stable key for React lists. */
  id: number;
  sentence: string;
}

export interface ApplyActions {
  /** Request the backend to pause after the current op boundary. */
  pause: () => void;
  /** Request the backend to resume from a paused state. */
  resume: () => void;
  /** Cooperatively stop the walk at the next op boundary. */
  stop: () => void;
  /** Acknowledge a completed-but-blocked state, clearing the block flag. */
  acknowledge: () => void;
}

export interface UseApplyJob {
  phase: ApplyPhase;
  /** True when phase="running" and the walk is paused at an op boundary. */
  paused: boolean;
  /** Scrolling tail: one sentence per completed op (no-ops are omitted). */
  feed: FeedItem[];
  doneCount: number;
  total: number;
  /** "dry-run" | "real" - forwarded from the parent's apply_start call. */
  mode: "dry-run" | "real";
  /** Stable machine error code when phase="failed", else null. */
  errorCode: string | null;
  /** Raw technical detail for the FD-13 disclosure (error code or null). */
  error: string | null;
  /** True when the completed job raised an unacknowledged discrepancy block. */
  blocked: boolean;
  discrepancyCount: number;
  actions: ApplyActions;
}

// ---------- private helpers ----------

/**
 * Convert a completed op into a plain-language sentence (FD-23, no raw paths).
 *
 * MODE-AWARE (Critical 1): a dry-run rehearsal moves nothing, so its feed says
 * what was CHECKED, never what was moved. Only the dry-run mode is reachable this
 * release, so a mode-agnostic "Moved X" sentence would be false on every line the
 * user actually sees. The real-apply templates ship ready for when a Real apply
 * becomes reachable.
 */
function opToSentence(
  kind: string,
  label: string,
  mode: "dry-run" | "real",
): string | null {
  const rehearsal = mode === "dry-run";
  switch (kind) {
    case "move":
    case "rename":
      return rehearsal
        ? STRINGS.apply.rehearsalOpMovedSentence(label)
        : STRINGS.apply.opMovedSentence(label);
    case "quarantine":
      return rehearsal
        ? STRINGS.apply.rehearsalOpSetAsideSentence(label)
        : STRINGS.apply.opSetAsideSentence(label);
    case "rmdir-empty":
      return rehearsal
        ? STRINGS.apply.rehearsalOpRemovedEmpty
        : STRINGS.apply.opRemovedEmpty;
    case "mkdir":
      return rehearsal
        ? STRINGS.apply.rehearsalOpCreatedFolder
        : STRINGS.apply.opCreatedFolder;
    case "no-op":
      // No-ops complete silently - not shown in the feed (they are
      // bookkeeping rows, not user-visible actions).
      return null;
    default:
      // Unknown kind: show a generic sentence rather than leaking an internal
      // token. The label is still safe (last path component only).
      return rehearsal
        ? STRINGS.apply.rehearsalOpMovedSentence(label)
        : STRINGS.apply.opMovedSentence(label);
  }
}

// ---------- hook ----------

export function useApplyJob(jobId: number, mode: "dry-run" | "real"): UseApplyJob {
  const [phase, setPhase] = useState<ApplyPhase>("running");
  const [paused, setPaused] = useState(false);
  const [feed, setFeed] = useState<FeedItem[]>([]);
  const [doneCount, setDoneCount] = useState(0);
  const [total, setTotal] = useState(0);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [blocked, setBlocked] = useState(false);
  const [discrepancyCount, setDiscrepancyCount] = useState(0);

  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Refresh job_status and update all derived state. Called after each event
  // so the phase transitions (running->paused, running->stopped, etc.) are
  // driven by the authoritative backend state rather than event inference.
  const refreshStatus = useCallback(async () => {
    try {
      const result = await commands.jobStatus(jobId);
      // Unwrap the typedError<JobStatus, AppError> discriminated union
      // (same pattern as plan.ts: check status, then access .data).
      if (result.status === "error") return; // IPC error; stay at current phase
      const status = result.data;
      if (!mountedRef.current) return;

      const st = status.state;
      setPaused(status.paused);
      setBlocked(status.blocks_further_tidying);
      setDiscrepancyCount(status.discrepancy_count);
      setErrorCode(status.error_code);

      // Backfill the progress counters from the durable journal (IMPORTANT 4) so a
      // fast dry-run that finished before the listeners attached still shows the
      // true "X of Y books". `Math.max` keeps this monotonic: a live event that has
      // already advanced the counters is never dragged backwards by a slightly
      // older status read.
      setDoneCount((prev) => Math.max(prev, status.done_count));
      setTotal((prev) => Math.max(prev, status.total));

      if (st === "running") {
        setPhase("running");
      } else if (st === "stopped") {
        setPhase("stopped");
      } else if (st === "completed") {
        setPhase(status.blocks_further_tidying ? "blocked" : "completed");
      } else if (st === "failed") {
        setPhase("failed");
      }
      // Other states (e.g. "cancelled" from scan - should not appear for apply
      // jobs) are ignored; the phase stays at its current value.
    } catch {
      // Unexpected error (e.g. Tauri bridge not ready): do not crash the
      // surface. The phase stays at its current value; the next event retries.
    }
  }, [jobId]);

  // Mount: load initial status so the surface initialises correctly even when
  // some ops already ran before the component mounted. Uses async IIFE per the
  // project pattern (react-hooks/set-state-in-effect: setState must be inside
  // a callback, not the synchronous effect body).
  useEffect(() => {
    void (async () => {
      await refreshStatus();
    })();
  }, [refreshStatus]);

  // Event subscriptions: one listener each for op-executed, completed, failed.
  // All three are registered in a single effect to share a single cleanup path.
  useEffect(() => {
    const unlistenOp = events.applyOpExecuted.listen((event) => {
      const payload = event.payload;
      if (payload.job_id !== jobId) return;

      // Add the sentence for this op to the feed (omit no-ops). Mode-aware so a
      // rehearsal never claims a real move (Critical 1).
      const sentence = opToSentence(payload.kind, payload.label, mode);
      if (sentence !== null) {
        setFeed((prev) => [...prev, { id: payload.op_id, sentence }]);
      }

      // Update progress counters from the event payload (lower latency than
      // waiting for job_status to settle); `Math.max` keeps them monotonic against
      // the mount backfill.
      setDoneCount((prev) => Math.max(prev, payload.done_count));
      setTotal((prev) => Math.max(prev, payload.total));

      // Refresh status for the paused flag - the backend may have flipped it
      // after the last op in the current run.
      void refreshStatus();
    });

    const unlistenCompleted = events.jobCompleted.listen((event) => {
      if (event.payload.job_id !== jobId) return;
      void refreshStatus();
    });

    const unlistenFailed = events.jobFailed.listen((event) => {
      if (event.payload.job_id !== jobId) return;
      void refreshStatus();
    });

    // The DISTINCT cooperative-Stop terminal event (IMPORTANT 3): the walk fires no
    // `apply:op-executed` after a Stop, so without this the surface would race a
    // single status poll and often miss the `stopped` transition. The status
    // refresh here is the same authoritative read the other terminal events use.
    const unlistenStopped = events.jobStopped.listen((event) => {
      if (event.payload.job_id !== jobId) return;
      void refreshStatus();
    });

    return () => {
      unlistenOp.then((f) => f());
      unlistenCompleted.then((f) => f());
      unlistenFailed.then((f) => f());
      unlistenStopped.then((f) => f());
    };
  }, [jobId, mode, refreshStatus]);

  // ---------- actions ----------

  const pause = useCallback(() => {
    void commands.jobPause(jobId).then(() => refreshStatus());
  }, [jobId, refreshStatus]);

  const resume = useCallback(() => {
    void commands.jobResume(jobId).then(() => refreshStatus());
  }, [jobId, refreshStatus]);

  const stop = useCallback(() => {
    void commands.jobStop(jobId).then(() => refreshStatus());
  }, [jobId, refreshStatus]);

  const acknowledge = useCallback(() => {
    void commands.acknowledgeCheck(jobId).then(() => refreshStatus());
  }, [jobId, refreshStatus]);

  return {
    phase,
    paused,
    feed,
    doneCount,
    total,
    mode,
    errorCode,
    // `error` carries the code string as the technical disclosure detail;
    // a raw filesystem path is never available here (JobStatus has no message
    // field). The FD-13 disclosure will show the code to the user as a copyable
    // string for tier-1 support.
    error: errorCode,
    blocked,
    discrepancyCount,
    actions: { pause, resume, stop, acknowledge },
  };
}
