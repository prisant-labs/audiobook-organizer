import { useCallback, useEffect, useState } from "react";
import { commands, type HistoryEntry } from "@/lib/bindings";
import { codeOf, formatAppError, type AppErrorCode } from "@/lib/appError";

// The History screen's data (v0.6.0): past tidy-ups, newest first, each with the
// undo offer the ENGINE resolved. Follows the same plain load/reload pattern as
// `useHealthMetrics` and `useAppSettings` rather than introducing a data-fetching
// library for one screen.
//
// Note what this hook deliberately does NOT do: it never decides what can be
// undone. `entry.undo` arrives already resolved from `abo-core::exec::history`,
// because that decision depends on engine invariants (was a manifest exported,
// are its operations reversible, did reconciliation leave something ambiguous)
// that the view has no business re-deriving.

export type HistoryStatus = "loading" | "ready" | "error";

export interface UseHistory {
  entries: HistoryEntry[];
  status: HistoryStatus;
  /** The structured AppError code behind a failure, for family-safe copy. */
  errorCode: AppErrorCode | null;
  /** Raw technical detail for the "Show file details" disclosure (FD-13). */
  errorDetail: string | null;
  reload: () => void;
}

export function useHistory(): UseHistory {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [status, setStatus] = useState<HistoryStatus>("loading");
  const [errorCode, setErrorCode] = useState<AppErrorCode | null>(null);
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);

  const reload = useCallback(() => setNonce((n) => n + 1), []);

  // Every setState happens inside the async run, never synchronously in the
  // effect body (react-hooks/set-state-in-effect) - the same shape
  // `useHealthMetrics` and `usePlanReview` already use. `status` starts at
  // "loading" from useState, and a reload re-enters "loading" from within the
  // run below, so no synchronous reset is needed.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      setStatus("loading");
      setErrorCode(null);
      setErrorDetail(null);
      try {
        const result = await commands.historyList(null);
        if (cancelled) return;
        if (result.status === "ok") {
          setEntries(result.data);
          setStatus("ready");
        } else {
          setErrorCode(codeOf(result.error));
          setErrorDetail(formatAppError(result.error));
          setStatus("error");
        }
      } catch (e: unknown) {
        if (cancelled) return;
        setErrorDetail(String(e));
        setStatus("error");
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [nonce]);

  return { entries, status, errorCode, errorDetail, reload };
}
