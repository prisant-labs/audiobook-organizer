import { Search } from "lucide-react";
import { STRINGS } from "@/lib/strings";
import { DEFAULT_PLAN_FILTER, type PlanFilterState } from "@/lib/planFilter";
import type { PlanGroupView } from "@/lib/bindings";

export interface PlanFilterProps {
  filter: PlanFilterState;
  onChange: (next: PlanFilterState) => void;
  groups: readonly PlanGroupView[];
}

// The F-503 filter box (AC-16): free text plus group/confidence/warning-type
// facets. View-only (AC-17): every change here only calls `onChange`, never
// a plan mutation.
export function PlanFilter({ filter, onChange, groups }: PlanFilterProps) {
  return (
    <div className="mb-4 flex flex-wrap items-center gap-2">
      <div className="relative flex-1 basis-[200px]">
        <Search
          aria-hidden="true"
          className="pointer-events-none absolute left-2.5 top-1/2 h-[13px] w-[13px] -translate-y-1/2 text-ink-3"
        />
        <input
          type="search"
          value={filter.text}
          onChange={(e) => onChange({ ...filter, text: e.target.value })}
          placeholder={STRINGS.review.filterPlaceholder}
          aria-label={STRINGS.review.filterPlaceholder}
          className="w-full rounded border border-border-2 bg-surface py-1.5 pl-7 pr-2.5 text-[12.5px] text-ink outline-none focus-visible:border-primary"
        />
      </div>
      <select
        value={filter.group}
        onChange={(e) => onChange({ ...filter, group: e.target.value })}
        aria-label="Filter by group"
        className="rounded border border-border-2 bg-surface px-2 py-1.5 text-[12px] text-ink"
      >
        <option value="all">{STRINGS.review.filterGroupAll}</option>
        {groups.map((g) => (
          <option key={g.group} value={g.group}>
            {g.label}
          </option>
        ))}
      </select>
      <select
        value={filter.confidence}
        onChange={(e) => onChange({ ...filter, confidence: e.target.value as PlanFilterState["confidence"] })}
        aria-label="Filter by confidence"
        className="rounded border border-border-2 bg-surface px-2 py-1.5 text-[12px] text-ink"
      >
        <option value="all">{STRINGS.review.filterConfidenceAll}</option>
        <option value="high">high confidence</option>
        <option value="medium">medium confidence</option>
        <option value="low">low confidence</option>
      </select>
      <select
        value={filter.status}
        onChange={(e) => onChange({ ...filter, status: e.target.value as PlanFilterState["status"] })}
        aria-label="Filter by status"
        className="rounded border border-border-2 bg-surface px-2 py-1.5 text-[12px] text-ink"
      >
        <option value="all">{STRINGS.review.filterWarningAll}</option>
        <option value="warning">{STRINGS.review.filterWarningOnly}</option>
        <option value="blocked">{STRINGS.review.filterBlockedOnly}</option>
      </select>
      {(filter.text || filter.group !== "all" || filter.confidence !== "all" || filter.status !== "all") && (
        <button
          type="button"
          onClick={() => onChange(DEFAULT_PLAN_FILTER)}
          className="text-[12px] text-link hover:underline"
        >
          Clear
        </button>
      )}
    </div>
  );
}
