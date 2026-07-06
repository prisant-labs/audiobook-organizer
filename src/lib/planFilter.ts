import type { PlanOpView } from "./bindings";

// F-503 (search and filter, view-only per AC-17): a single filter box over
// the whole plan's op listing. Narrows by free text over source/target
// names plus group, confidence, and warning-type facets (AC-16); clearing it
// (every field back to its default) restores the full plan. This module
// only ever READS `ops` and returns a new filtered array - it never touches
// approval or exclude state (AC-17).

export interface PlanFilterState {
  text: string;
  group: string | "all";
  confidence: "all" | "high" | "medium" | "low";
  status: "all" | "warning" | "blocked";
}

export const DEFAULT_PLAN_FILTER: PlanFilterState = {
  text: "",
  group: "all",
  confidence: "all",
  status: "all",
};

export function isPlanFilterActive(filter: PlanFilterState): boolean {
  return (
    filter.text.trim() !== "" ||
    filter.group !== "all" ||
    filter.confidence !== "all" ||
    filter.status !== "all"
  );
}

export function filterPlanOps(ops: readonly PlanOpView[], filter: PlanFilterState): PlanOpView[] {
  const needle = filter.text.trim().toLowerCase();
  return ops.filter((op) => {
    if (filter.group !== "all" && op.group !== filter.group) return false;
    if (filter.confidence !== "all" && op.confidence !== filter.confidence) return false;
    if (filter.status !== "all" && op.validation !== filter.status) return false;
    if (needle) {
      const haystack = `${op.source_path} ${op.target_path} ${op.rationale}`.toLowerCase();
      if (!haystack.includes(needle)) return false;
    }
    return true;
  });
}
