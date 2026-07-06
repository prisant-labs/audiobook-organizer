import {
  commands,
  type AppError,
  type PlanGroupView,
  type PlanOpView,
  type PlanOpsPage,
  type PlanReview,
} from "./bindings";
import { AppErrorException } from "./appError";

// Frontend client for the F-903 plan review surface (T-19..T-22, v0.4.0
// Phase 5). Wraps the generated `plan_*` bindings and unwraps their `Result`
// shape, mirroring `src/lib/overview.ts`'s pattern: an `AppError` is thrown
// as a typed `PlanError` (never swallowed), never returned as a sentinel. The
// error carries the STRUCTURED AppError (AppErrorException) so the F-908
// review surface can map its `code` to family-safe copy (errorCopy.ts).

export type { PlanGroupView, PlanOpView, PlanOpsPage, PlanReview };

export class PlanError extends AppErrorException {
  constructor(error: AppError) {
    super(error);
    this.name = "PlanError";
  }
}

/** Generate a real plan from a completed scan (build -> validate -> persist), returning its seven-card review surface. */
export async function generatePlan(scanId: number): Promise<PlanReview> {
  const result = await commands.planGenerate(scanId);
  if (result.status === "error") throw new PlanError(result.error);
  return result.data;
}

/** Re-fetch a previously generated plan's review surface. */
export async function getPlanReview(planId: number): Promise<PlanReview> {
  const result = await commands.planGet(planId);
  if (result.status === "error") throw new PlanError(result.error);
  return result.data;
}

/** The flat, filterable op listing for one plan (F-503/F-504). */
export async function listPlanOps(planId: number): Promise<PlanOpsPage> {
  const result = await commands.planListOps(planId);
  if (result.status === "error") throw new PlanError(result.error);
  return result.data;
}

/** Toggle a campaign group's include/skip switch (F-502, AC-11). */
export async function setGroupApproval(
  planId: number,
  group: string,
  included: boolean,
): Promise<PlanReview> {
  const result = await commands.planSetGroupApproval(planId, group, included);
  if (result.status === "error") throw new PlanError(result.error);
  return result.data;
}

/** Exclude one operation from a plan (F-502, AC-13). */
export async function excludeOp(planOpId: number): Promise<PlanOpView> {
  const result = await commands.planExcludeOp(planOpId);
  if (result.status === "error") throw new PlanError(result.error);
  return result.data;
}
