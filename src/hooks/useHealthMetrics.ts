import { useCallback, useEffect, useRef, useState } from "react";
import { getLibraryOverview, type LibraryOverview } from "@/lib/overview";

// F-902 library home data (T-15, v0.4.0 Phase 4): loads `classify_overview`
// once on mount and exposes a reload, so the Library route and the sidebar
// badges (`useNavCounts`) always read the SAME numbers (AC-7) rather than each
// fetching and possibly disagreeing.
//
// The plan brief names this a "TanStack Query hook `useHealthMetrics()`"; this
// project has no TanStack Query dependency yet (Phase 5 brings TanStack
// Virtual for an unrelated reason - list virtualization, not data fetching),
// so this hook follows the SAME plain load/reload pattern `useAppSettings`
// already established rather than adding a new data-fetching library for one
// screen. The public shape (`status`, `overview`, `error`, `reload`) is what a
// TanStack Query consumer would see anyway, so swapping the implementation
// later is a non-breaking change if the project adopts the library broadly.

export type HealthMetricsStatus = "loading" | "ready" | "error";

export interface UseHealthMetrics {
  /** The library overview, or `null` while loading, on error, or before any
   * scan has ever completed (the honest pre-first-scan state, AC-6). */
  overview: LibraryOverview | null;
  status: HealthMetricsStatus;
  /** A plain-language error message when `status === "error"`. */
  error: string | null;
  /** Re-run the load (used after a scan completes, and by error-state retry). */
  reload: () => void;
}

export function useHealthMetrics(): UseHealthMetrics {
  const [overview, setOverview] = useState<LibraryOverview | null>(null);
  const [status, setStatus] = useState<HealthMetricsStatus>("loading");
  const [error, setError] = useState<string | null>(null);
  const [reloadNonce, setReloadNonce] = useState(0);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (!cancelled && mountedRef.current) {
        setStatus("loading");
        setError(null);
      }
      try {
        const loaded = await getLibraryOverview();
        if (cancelled || !mountedRef.current) return;
        setOverview(loaded);
        setStatus("ready");
      } catch (e) {
        if (cancelled || !mountedRef.current) return;
        setError(e instanceof Error ? e.message : String(e));
        setStatus("error");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [reloadNonce]);

  const reload = useCallback(() => setReloadNonce((n) => n + 1), []);

  return { overview, status, error, reload };
}
