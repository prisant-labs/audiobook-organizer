import { useCallback, useEffect, useRef, useState } from "react";
import {
  commands,
  events,
  type DuplicatesReviewView,
  type ResolutionPolicy,
} from "@/lib/bindings";
import { codeOf, formatAppError, type AppErrorCode } from "@/lib/appError";

// The duplicates screen's data (F-905, v0.6.0 P5).
//
// Follows the same plain load/reload shape as `useHistory` and
// `useHealthMetrics` rather than introducing a data-fetching library for one
// screen. What it adds is a JOB: checking copies reads every candidate file end
// to end, which FD-49 measured at the speed of the drive rather than the CPU, so
// it reports progress and can be stopped.
//
// WHAT THIS HOOK DELIBERATELY DOES NOT DECIDE. It never works out whether a
// group may be archived. `group.content_verified` arrives already resolved from
// the engine, and the backend refuses a confirmation that fails AC-12's gate
// whatever this hook believes. A view that re-derived the rule would be a second
// implementation of the one thing that must not be got wrong.

export type DuplicatesStatus = "loading" | "ready" | "error";

export interface CheckProgress {
  done: number;
  total: number | null;
  label: string;
}

export interface ActionError {
  code: AppErrorCode | null;
  detail: string;
}

export interface UseDuplicates {
  review: DuplicatesReviewView | null;
  /** Which copy each group suggests keeping (AC-28). */
  policy: ResolutionPolicy;
  setPolicy: (policy: ResolutionPolicy) => void;
  status: DuplicatesStatus;
  errorCode: AppErrorCode | null;
  errorDetail: string | null;
  /** Non-null while a check job is running. */
  progress: CheckProgress | null;
  /** The most recent failure from an action (check, confirm, save). */
  actionError: ActionError | null;
  dismissActionError: () => void;
  /** Where the last saved list landed, if one was saved this visit. */
  savedTo: string | null;
  reload: () => void;
  check: () => Promise<void>;
  stopCheck: () => Promise<void>;
  confirm: (
    group: { group_key: string; method: string },
    keeperEntryId: number,
    loserEntryIds: number[],
    unverifiedOverride: boolean,
  ) => Promise<void>;
  clearConfirmation: (group: { group_key: string; method: string }) => Promise<void>;
  save: () => Promise<void>;
}

export function useDuplicates(scanId: number | null): UseDuplicates {
  const [review, setReview] = useState<DuplicatesReviewView | null>(null);
  const [status, setStatus] = useState<DuplicatesStatus>("loading");
  const [errorCode, setErrorCode] = useState<AppErrorCode | null>(null);
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  const [progress, setProgress] = useState<CheckProgress | null>(null);
  const [actionError, setActionError] = useState<ActionError | null>(null);
  const [savedTo, setSavedTo] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  // Flag-only is the default (AC-23) and the honest starting point: the app has
  // no opinion until a person expresses one.
  const [policy, setPolicy] = useState<ResolutionPolicy>("flag-only");

  // The running check's job id, for Stop. A ref rather than state: nothing
  // renders it, and a re-render between starting the job and the user pressing
  // Stop would be a needless hazard.
  const jobId = useRef<number | null>(null);

  // `stopCheck` sets state after awaiting the cancel command, so it needs to
  // know whether the screen is still there. Same guard `Library` keeps for the
  // same reason on the same code path.
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  const dismissActionError = useCallback(() => setActionError(null), []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (scanId === null) {
        // No scan is not an error: it is the first-run state, and the screen has
        // its own words for it.
        setReview(null);
        setStatus("ready");
        return;
      }
      setStatus("loading");
      setErrorCode(null);
      setErrorDetail(null);
      try {
        const result = await commands.dupesReview(scanId, policy);
        if (cancelled) return;
        if (result.status === "ok") {
          setReview(result.data);
          setStatus("ready");
        } else {
          setErrorCode(codeOf(result.error));
          setErrorDetail(formatAppError(result.error));
          setStatus("error");
        }
      } catch (e: unknown) {
        if (cancelled) return;
        setErrorCode(null);
        setErrorDetail(String(e));
        setStatus("error");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [scanId, nonce, policy]);

  // Job events. Listeners are set up once and filter by the job id this hook
  // started, so another job's completion never moves this screen.
  useEffect(() => {
    const unlisten: Array<() => void> = [];
    void (async () => {
      unlisten.push(
        await events.jobProgress.listen((e) => {
          if (e.payload.job_id !== jobId.current) return;
          setProgress({
            done: e.payload.done,
            total: e.payload.total_estimate,
            label: e.payload.current_label,
          });
        }),
      );
      const finish = () => {
        jobId.current = null;
        setProgress(null);
        reload();
      };
      unlisten.push(
        await events.jobCompleted.listen((e) => {
          if (e.payload.job_id === jobId.current) finish();
        }),
      );
      unlisten.push(
        await events.jobFailed.listen((e) => {
          if (e.payload.job_id !== jobId.current) return;
          // The job's own code, surfaced through the same family-safe path as
          // every other failure rather than as a bare string.
          setActionError({
            code: e.payload.code as AppErrorCode,
            detail: e.payload.code,
          });
          finish();
        }),
      );
    })();
    return () => unlisten.forEach((u) => u());
  }, [reload]);

  const check = useCallback(async () => {
    if (scanId === null || jobId.current !== null) return;
    setActionError(null);
    // Shown from the moment the job starts rather than from the first progress
    // event, so pressing the button visibly does something even while the first
    // file is still being read.
    setProgress({ done: 0, total: null, label: "" });
    const result = await commands.dupesHashVerify(scanId);
    if (result.status === "ok") {
      jobId.current = result.data.job_id;
    } else {
      setProgress(null);
      setActionError({ code: codeOf(result.error), detail: formatAppError(result.error) });
    }
  }, [scanId]);

  // Stop, and then transition this screen's own state, because NOTHING ELSE
  // WILL. `dupes_hash_verify` passes an empty `on_cancelled` to
  // `run_job_to_terminal`, exactly as `scan_start` does, so a cancelled job
  // marks its `jobs` row and emits no `job:completed` or `job:failed`. Without
  // the three lines below, the progress bar and its Stop button stay on screen
  // for a job that already stopped, and `check`'s own `jobId.current !== null`
  // guard then refuses to start another one for the life of the mounted screen.
  //
  // `Library`'s `stopScan` carries this rule already: "the backend never emits a
  // job:completed/failed event for a cancelled job, so this is the one place
  // that transitions local state to the honest stopped outcome." That rule is a
  // property of the shared job wrapper rather than of the scan, so it binds
  // every caller of it, and this one was not honouring it.
  const stopCheck = useCallback(async () => {
    const id = jobId.current;
    if (id === null) return;
    // The same cooperative Stop the scan uses: the flag registry is keyed by job
    // id and ids are unique across kinds, so one control serves both.
    const stopped = await commands.scanCancel(id);
    if (!mounted.current) return;
    // `false` means there was no running job to signal, which means it already
    // reached a terminal state and its own event has already done this work.
    if (!stopped) return;
    jobId.current = null;
    setProgress(null);
    // A cancelled pass KEEPS every hash it finished, which is why the job
    // reports Cancelled rather than Failed. Those hashes only become visible on
    // a re-read, so a Stop that skipped this would hide completed work.
    reload();
  }, [reload]);

  const confirm = useCallback<UseDuplicates["confirm"]>(
    async (group, keeperEntryId, loserEntryIds, unverifiedOverride) => {
      if (scanId === null) return;
      setActionError(null);
      const result = await commands.dupesConfirm(
        scanId,
        group.method,
        group.group_key,
        keeperEntryId,
        loserEntryIds,
        unverifiedOverride,
      );
      if (result.status === "ok") reload();
      else setActionError({ code: codeOf(result.error), detail: formatAppError(result.error) });
    },
    [scanId, reload],
  );

  const clearConfirmation = useCallback<UseDuplicates["clearConfirmation"]>(
    async (group) => {
      if (scanId === null) return;
      setActionError(null);
      const result = await commands.dupesClearConfirmation(scanId, group.method, group.group_key);
      if (result.status === "ok") reload();
      else setActionError({ code: codeOf(result.error), detail: formatAppError(result.error) });
    },
    [scanId, reload],
  );

  const save = useCallback(async () => {
    if (scanId === null) return;
    setActionError(null);
    const result = await commands.dupesExportCsv(scanId);
    if (result.status === "ok") setSavedTo(result.data.path);
    else setActionError({ code: codeOf(result.error), detail: formatAppError(result.error) });
  }, [scanId]);

  return {
    review,
    policy,
    setPolicy,
    status,
    errorCode,
    errorDetail,
    progress,
    actionError,
    dismissActionError,
    savedTo,
    reload,
    check,
    stopCheck,
    confirm,
    clearConfirmation,
    save,
  };
}
