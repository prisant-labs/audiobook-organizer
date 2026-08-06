import { useCallback, useEffect, useState } from "react";
import { commands, type HistoryEntry } from "@/lib/bindings";
import type { StartupInterruption } from "@/lib/interruption";

// The one place that asks the backend about a tidy-up a previous session was
// killed in the middle of (v0.6.0 P1c, F-606 interruption safety).
//
// # Two reads, and why the second one exists
//
// `startup_interruption` says what HAPPENED: which job, whether it was a real
// tidy-up or a rehearsal, how far it got, and whether carrying on is safe. It
// does not say what can be DONE about it. That is `entry.undo`, resolved in the
// engine per FD-36 because it depends on invariants the view cannot see (was an
// undo file exported, are the operations reversible, did reconciliation leave
// an ambiguity). So the hook reads both and hands the pair to the surface.
//
// # Everything fails soft, in one direction
//
// A recovery offer that cannot be read must never stop the app from opening,
// but a PARTIAL read should not hide the interruption either. So the two
// failures resolve differently on purpose: a failed `startup_interruption`
// means no surface at all, while a failed `history_list` keeps the surface and
// drops only the undo action. Losing the undo button costs the user one route
// to a remedy they can still reach from History; losing the whole surface costs
// them the knowledge that anything happened.
//
// # Why dismiss is local
//
// The backend value is captured once before `manage` and cloned on every call,
// so it is a snapshot for the life of the process and cannot be cleared by
// acting on it. Clearing the local copy is the only way this surface goes away.

export interface UseStartupInterruption {
  interruption: StartupInterruption | null;
  /** The matching History row, or null when it could not be read. */
  entry: HistoryEntry | null;
  status: "loading" | "ready";
  dismiss: () => void;
}

export function useStartupInterruption(): UseStartupInterruption {
  const [interruption, setInterruption] = useState<StartupInterruption | null>(null);
  const [entry, setEntry] = useState<HistoryEntry | null>(null);
  const [status, setStatus] = useState<"loading" | "ready">("loading");

  const dismiss = useCallback(() => {
    setInterruption(null);
    setEntry(null);
  }, []);

  // Every setState happens inside the async run, never synchronously in the
  // effect body (react-hooks/set-state-in-effect), matching useHistory and
  // useHealthMetrics. Runs once: the backing value never changes in a session.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let found: StartupInterruption | null;
      try {
        found = await commands.startupInterruption();
      } catch {
        found = null;
      }
      if (cancelled) return;

      if (!found) {
        setStatus("ready");
        return;
      }

      // Hoisted so the narrowing survives into the callback below.
      const jobId = found.job_id;
      let row: HistoryEntry | null = null;
      try {
        const history = await commands.historyList(null);
        if (history.status === "ok") {
          row = history.data.find((e) => e.jobId === jobId) ?? null;
        }
      } catch {
        row = null;
      }
      if (cancelled) return;

      setInterruption(found);
      setEntry(row);
      setStatus("ready");
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  return { interruption, entry, status, dismiss };
}
