import { TriangleAlert } from "lucide-react";
import { cn } from "@/lib/utils";
import { STRINGS } from "@/lib/strings";
import { FileDetails } from "./FileDetails";
import type { PlanOpView } from "@/lib/bindings";

export interface OpRowProps {
  op: PlanOpView;
  first?: boolean;
  /** Shown ahead of the rationale when a row is out of its own group's context (the F-503 cross-group filter view). */
  groupLabel?: string;
  onExclude: () => void;
}

// One operation row (F-502/F-504, design-system Section 4.10): a
// plain-language rationale sentence, a "needs you" flag when the op carries
// a warning or is blocked, a per-op exclude control (AC-13), and the "Show
// file details" disclosure (AC-18). Raw paths never appear outside that
// disclosure (AC-19). Shared by the single-group detail pane and the F-503
// cross-group filter results list.
export function OpRow({ op, first, groupLabel, onExclude }: OpRowProps) {
  const excluded = op.approval === "excluded";
  return (
    <div className={cn("py-4", !first && "border-t border-border")}>
      {groupLabel && (
        <p className="mb-1 text-[10.5px] uppercase tracking-[0.04em] text-ink-3">{groupLabel}</p>
      )}
      <div className="flex items-start justify-between gap-3">
        <p
          className={cn(
            "text-[13px] leading-relaxed",
            excluded ? "text-ink-3 line-through" : "text-ink",
          )}
        >
          {op.rationale}
        </p>
        {op.kind !== "no-op" &&
          (excluded ? (
            <span className="flex-none whitespace-nowrap text-[11.5px] text-ink-3">
              {STRINGS.review.excludeUndoNote}
            </span>
          ) : (
            <button
              type="button"
              onClick={onExclude}
              className="flex-none whitespace-nowrap text-[11.5px] text-link hover:underline"
            >
              {STRINGS.review.excludeAction}
            </button>
          ))}
      </div>
      {op.warning_text && (
        <span
          className={cn(
            "mt-2 inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11.5px]",
            op.validation === "blocked" ? "bg-danger-bg text-danger" : "bg-warn-bg text-warn",
          )}
        >
          <TriangleAlert aria-hidden="true" className="h-[11px] w-[11px] shrink-0" strokeWidth={2.2} />
          {op.warning_text}
        </span>
      )}
      <FileDetails op={op} />
    </div>
  );
}
