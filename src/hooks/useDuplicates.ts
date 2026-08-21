import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events, type DuplicatesReviewView } from "@/lib/bindings";
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
  stopCheck: () => void;
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

  // The running check's job id, for Stop. A ref rather than state: nothing
  // renders it, and a re-render between starting the job and the user pressing
  // Stop would be a needless hazard.
  const jobId = useRef<number | null>(null);

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
        const result = await commands.dupesReview(scanId);
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
  }, [scanId, nonce]);

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

  const stopCheck = useCallback(() => {
    if (jobId.current === null) return;
    // The same cooperative Stop the scan uses: the flag registry is keyed by job
    // id and ids are unique across kinds, so one control serves both.
    void commands.scanCancel(jobId.current);
  }, []);

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
