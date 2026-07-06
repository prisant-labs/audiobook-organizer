import {
  commands,
  type PlanGroupView,
  type PresetExampleView,
  type Ruleset,
  type RulesetDetail,
  type RulesetSaveRequest,
  type RulesetSummary,
} from "./bindings";
import { formatAppError } from "./appError";

// Frontend client for the F-906 ruleset editor (v0.4.0 Phase 6). Wraps the
// generated `ruleset_*`/`plan_preview` bindings and unwraps their `Result`
// shape, mirroring `src/lib/plan.ts`'s pattern: an `AppError` is thrown as a
// typed `RulesetError` (never swallowed), never returned as a sentinel.

export type { PresetExampleView, Ruleset, RulesetDetail, RulesetSaveRequest, RulesetSummary };

export class RulesetError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RulesetError";
  }
}

/** The active ruleset's full editable detail, seeding the shipped default if nothing has ever been saved. */
export async function getActiveRuleset(): Promise<RulesetDetail> {
  const result = await commands.rulesetGetActive();
  if (result.status === "error") throw new RulesetError(formatAppError(result.error));
  return result.data;
}

/** Every saved ruleset (F-801 CRUD), lightest-weight shape. */
export async function listRulesets(): Promise<RulesetSummary[]> {
  const result = await commands.rulesetList();
  if (result.status === "error") throw new RulesetError(formatAppError(result.error));
  return result.data;
}

/** One ruleset's full editable detail. */
export async function getRuleset(rulesetId: number): Promise<RulesetDetail> {
  const result = await commands.rulesetGet(rulesetId);
  if (result.status === "error") throw new RulesetError(formatAppError(result.error));
  return result.data;
}

/** Create (id: null) or update (id: set) a ruleset and make it the active one (AC-32/AC-33). */
export async function saveRuleset(request: RulesetSaveRequest): Promise<RulesetDetail> {
  const result = await commands.rulesetSave(request);
  if (result.status === "error") throw new RulesetError(formatAppError(result.error));
  return result.data;
}

/** Delete a saved ruleset; rejects if it is the active one. */
export async function deleteRuleset(rulesetId: number): Promise<void> {
  const result = await commands.rulesetDelete(rulesetId);
  if (result.status === "error") throw new RulesetError(formatAppError(result.error));
}

/** The three shipped F-401 presets, each with a rendered example path. */
export async function getPresetExamples(seriesIndexWidth: number): Promise<PresetExampleView[]> {
  return commands.rulesetPresetExamples(seriesIndexWidth);
}

/** The F-906 live re-plan preview (AC-33): projected per-group counts for a draft ruleset, never persisted. */
export async function previewPlan(scanId: number, ruleset: Ruleset): Promise<PlanGroupView[]> {
  const result = await commands.planPreview(scanId, ruleset);
  if (result.status === "error") throw new RulesetError(formatAppError(result.error));
  return result.data.groups;
}
