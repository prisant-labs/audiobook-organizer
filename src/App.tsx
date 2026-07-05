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
import { useEffect, useRef, useState } from "react";
import { commands, events, type EntryRow, type DbStatus, type AppError } from "./lib/bindings";

// Hardcoded fixture root for the tracer button. This folder is NOT created
// by the app; prepare it by hand before running the tracer (see the manual
// QA checklist and the Phase 6 report for the exact layout used to verify
// this). Throwaway path, deleted with the rest of this component.
const TRACER_ROOT = "E:\\tmp\\abo-tracer";

// AppError is an externally tagged enum: exactly one machine-code key is
// present (the rest are typed `never`). Render it as "code: detail" (or just
// "code" when the payload has no readable field) instead of dumping the raw
// object shape.
function formatAppError(error: AppError): string {
  const record = error as Record<string, { detail?: string; path?: string; backup_path?: string } | undefined>;
  const code = Object.keys(record).find((key) => record[key] !== undefined);
  if (!code) return "unknown-error";
  const payload = record[code];
  const detail = payload?.detail ?? payload?.path ?? payload?.backup_path;
  return detail ? `${code}: ${detail}` : code;
}

type ScanState =
  | { phase: "idle"; error?: string }
  | { phase: "running"; jobId: number | "pending" }
  | { phase: "done"; jobId: number; scanId: number; entries: EntryRow[] }
  | { phase: "failed"; jobId: number; code: string };

function App() {
  const [state, setState] = useState<ScanState>({ phase: "idle" });
  const [dbStatus, setDbStatus] = useState<DbStatus | null>(null);
  // Tracks the job_id this component is currently waiting on, so the
  // job:completed / job:failed listeners can ignore events for a stale job.
  // A ref (not just state) avoids stale-closure reads inside the listener
  // callbacks, which close over the render they were registered in.
  const currentJobIdRef = useRef<number | "pending" | null>(null);
  // Guards setState calls that land after an await, once the component has
  // unmounted (this tracer has no route away from it today, but the guard
  // is cheap hygiene against a future dev-mode double-mount or unmount).
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    commands.dbStatus().then((status) => {
      if (mountedRef.current) setDbStatus(status);
    });
  }, []);

  useEffect(() => {
    const unlistenCompleted = events.jobCompleted.listen(async (event) => {
      const { job_id, scan_id } = event.payload;
      if (currentJobIdRef.current !== job_id) return;
      const result = await commands.scanEntries(scan_id);
      if (!mountedRef.current || currentJobIdRef.current !== job_id) return;
      if (result.status === "ok") {
        setState({ phase: "done", jobId: job_id, scanId: scan_id, entries: result.data });
      } else {
        // AppError serializes externally tagged ({ "some-code": { ... } }); the
        // union's exclusion markers forbid indexing a fixed key, so read the
        // tag generically. Disposable tracer only; real surfaces are v0.4.0.
        const err: unknown = result.error;
        const code =
          typeof err === "string" ? err : (Object.keys(err as object)[0] ?? "unknown-error");
        setState({ phase: "failed", jobId: job_id, code });
      }
    });

    const unlistenFailed = events.jobFailed.listen((event) => {
      const { job_id, code } = event.payload;
      if (currentJobIdRef.current !== job_id) return;
      setState({ phase: "failed", jobId: job_id, code });
    });

    return () => {
      unlistenCompleted.then((f) => f());
      unlistenFailed.then((f) => f());
    };
  }, []);

  async function runTracerScan() {
    // Set the running phase synchronously, before the IPC round-trip, so the
    // disabled={phase === "running"} guard engages on the same click instead
    // of only after scan_start resolves.
    currentJobIdRef.current = "pending";
    setState({ phase: "running", jobId: "pending" });

    const result = await commands.scanStart(TRACER_ROOT);
    if (!mountedRef.current) return;

    if (result.status === "ok") {
      const jobId = result.data.job_id;
      currentJobIdRef.current = jobId;
      setState({ phase: "running", jobId });
    } else {
      currentJobIdRef.current = null;
      setState({ phase: "idle", error: formatAppError(result.error) });
    }
  }

  return (
    // A plain <div>, not <main>: this tracer now mounts inside AppShell's
    // ScreenContainer (v0.4.0 Phase 1), which already owns the page's one
    // <main> landmark. Nesting two <main> elements is invalid; this is the
    // only change made to this disposable file for that reason.
    <div style={{ fontFamily: "sans-serif", padding: "1rem" }}>
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
      {state.phase === "idle" && state.error && (
        <p style={{ color: "red" }}>Scan could not start: {state.error}</p>
      )}
      {state.phase === "running" && (
        <p>Scan running (job {state.jobId === "pending" ? "starting..." : state.jobId})...</p>
      )}
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
    </div>
  );
}

export default App;
