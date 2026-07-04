// Tracer slice UI for the v0.1.0 spine (Phase 6).
//
// TODO(v0.4.0-seeing): delete this entire component. It is a disposable
// proof that the seam works end to end (scan_start -> job:completed ->
// scan_entries -> render), not a product surface. See CHANGELOG.md 0.1.0
// and docs/internal/releases/v0.1.0-spine/spec.md AC-19 for the deletion
// reminder and the behavior this proves.
//
// No raw `invoke`: every call goes through the generated bindings in
// src/lib/bindings.ts (eslint enforces this).
import { useEffect, useState } from "react";
import { commands, events, type EntryRow, type DbStatus } from "./lib/bindings";

// Hardcoded fixture root for the tracer button. This folder is NOT created
// by the app; prepare it by hand before running the tracer (see the manual
// QA checklist and the Phase 6 report for the exact layout used to verify
// this). Throwaway path, deleted with the rest of this component.
const TRACER_ROOT = "E:\\tmp\\abo-tracer";

type ScanState =
  | { phase: "idle" }
  | { phase: "running"; jobId: number }
  | { phase: "done"; jobId: number; scanId: number; entries: EntryRow[] }
  | { phase: "failed"; jobId: number; code: string };

function App() {
  const [state, setState] = useState<ScanState>({ phase: "idle" });
  const [dbStatus, setDbStatus] = useState<DbStatus | null>(null);

  useEffect(() => {
    commands.dbStatus().then(setDbStatus);
  }, []);

  useEffect(() => {
    const unlistenCompleted = events.jobCompleted.listen(async (event) => {
      const { job_id, scan_id } = event.payload;
      setState((prev) => {
        if (prev.phase !== "running" || prev.jobId !== job_id) return prev;
        return prev;
      });
      const result = await commands.scanEntries(scan_id);
      if (result.status === "ok") {
        setState({ phase: "done", jobId: job_id, scanId: scan_id, entries: result.data });
      } else {
        setState({ phase: "failed", jobId: job_id, code: result.error["scan-failed"]?.detail ?? "scan-failed" });
      }
    });

    const unlistenFailed = events.jobFailed.listen((event) => {
      const { job_id, code } = event.payload;
      setState({ phase: "failed", jobId: job_id, code });
    });

    return () => {
      unlistenCompleted.then((f) => f());
      unlistenFailed.then((f) => f());
    };
  }, []);

  async function runTracerScan() {
    const result = await commands.scanStart(TRACER_ROOT);
    if (result.status === "ok") {
      setState({ phase: "running", jobId: result.data.job_id });
    } else {
      setState({ phase: "failed", jobId: -1, code: JSON.stringify(result.error) });
    }
  }

  return (
    <main style={{ fontFamily: "sans-serif", padding: "1rem" }}>
      <h1>Audiobook Organizer - spine scaffold</h1>
      <p style={{ fontWeight: "bold" }}>
        Tracer slice - disposable, deleted at v0.4.0 (seeing)
      </p>
      <p style={{ fontSize: "0.85rem", color: "#555" }}>
        db_status(): {dbStatus ? JSON.stringify(dbStatus) : "loading..."}
      </p>
      <button onClick={runTracerScan} disabled={state.phase === "running"}>
        Run tracer scan
      </button>
      {state.phase === "running" && <p>Scan running (job {state.jobId})...</p>}
      {state.phase === "failed" && (
        <p style={{ color: "red" }}>Scan failed (job {state.jobId}): {state.code}</p>
      )}
      {state.phase === "done" && (
        <>
          <p>
            Scan {state.scanId} complete (job {state.jobId}), {state.entries.length} entries.
          </p>
          <pre style={{ background: "#f5f5f5", padding: "0.5rem", overflow: "auto" }}>
            {JSON.stringify(state.entries, null, 2)}
          </pre>
        </>
      )}
    </main>
  );
}

export default App;
