import { useCallback, useEffect, useRef, useState } from "react";
import {
  excludeOp as excludeOpApi,
  generatePlan,
  getPlanReview,
  listPlanOps,
  setGroupApproval as setGroupApprovalApi,
  type PlanOpView,
  type PlanReview,
} from "@/lib/plan";

// F-903 plan review surface (T-19..T-22, v0.4.0 Phase 5): owns the review
// screen's data. On mount (and whenever `scanId` changes to a real value) it
// generates a REAL plan from the given scan (`plan_generate`: build ->
// validate -> persist) and loads the flat op listing the group-detail pane
// and the F-503 filter both read from. Mutations (`toggleGroup`,
// `excludeOp`) are optimistic-free: they await the mutating command and
// replace state with exactly what the backend persisted, the same posture
// `useAppSettings`/`useHealthMetrics` already established in this codebase.

export type PlanReviewStatus =
  | "no-scan"
  | "generating"
  | "loading"
  | "ready"
  | "error";

export interface UsePlanReview {
  status: PlanReviewStatus;
  review: PlanReview | null;
  /** The flat op listing every group's detail pane and the filter box read from. */
  ops: PlanOpView[];
  /** Whether `ops` was capped below the plan's real total (see `PlanOpsPage.truncated`). */
  opsTruncated: boolean;
  error: string | null;
  /** Re-run generation from the current `scanId` (a fresh plan; the prior one is untouched). */
  regenerate: () => void;
  /** Toggle one campaign group's include/skip switch (F-502, AC-11). */
  toggleGroup: (group: string, included: boolean) => Promise<void>;
  /** Exclude one operation (F-502, AC-13); patches just that row in `ops`. */
  excludeOp: (planOpId: number) => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function usePlanReview(scanId: number | null): UsePlanReview {
  const [review, setReview] = useState<PlanReview | null>(null);
  const [ops, setOps] = useState<PlanOpView[]>([]);
  const [opsTruncated, setOpsTruncated] = useState(false);
  // `phase` tracks only the in-flight generate/load sequence; the `no-scan`
  // status is derived from `scanId` below rather than stored, so the effect
  // never needs to set state synchronously for that branch (react-hooks
  // set-state-in-effect: only the async IIFE's own setState calls run,
  // matching the pattern `useHealthMetrics`/`useAppSettings` already use).
  const [phase, setPhase] = useState<"idle" | "generating" | "loading" | "ready" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const [regenNonce, setRegenNonce] = useState(0);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (scanId == null) return;
    let cancelled = false;
    void (async () => {
      setPhase("generating");
      setError(null);
      try {
        const generated = await generatePlan(scanId);
        if (cancelled || !mountedRef.current) return;
        setReview(generated);
        setPhase("loading");
        const page = await listPlanOps(generated.plan_id);
        if (cancelled || !mountedRef.current) return;
        setOps(page.ops);
        setOpsTruncated(page.truncated);
        setPhase("ready");
      } catch (e) {
        if (cancelled || !mountedRef.current) return;
        setError(messageOf(e));
        setPhase("error");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [scanId, regenNonce]);

  const status: PlanReviewStatus = scanId == null ? "no-scan" : phase === "idle" ? "generating" : phase;

  const regenerate = useCallback(() => setRegenNonce((n) => n + 1), []);

  const toggleGroup = useCallback(
    async (group: string, included: boolean) => {
      if (!review) return;
      const updated = await setGroupApprovalApi(review.plan_id, group, included);
      if (!mountedRef.current) return;
      setReview(updated);
      // Approval transitions never rewrite an op's descriptive columns, only
      // the mutable approval pair; re-fetch the listing so each row's own
      // `approval` (used for its own exclude/include state in the detail
      // pane) stays in lockstep with what the group toggle just persisted.
      const page = await listPlanOps(updated.plan_id);
      if (!mountedRef.current) return;
      setOps(page.ops);
      setOpsTruncated(page.truncated);
    },
    [review],
  );

  const excludeOp = useCallback(
    async (planOpId: number) => {
      const updated = await excludeOpApi(planOpId);
      if (!mountedRef.current) return;
      setOps((prev) => prev.map((op) => (op.id === planOpId ? updated : op)));
      // Excluding an op can change its group's status (e.g. every remaining
      // op is now blocked-or-excluded, AC-15) and the footer totals, so the
      // seven cards are re-fetched too.
      if (review) {
        const refreshed = await getPlanReview(review.plan_id);
        if (!mountedRef.current) return;
        setReview(refreshed);
      }
    },
    [review],
  );

  return { status, review, ops, opsTruncated, error, regenerate, toggleGroup, excludeOp };
}
