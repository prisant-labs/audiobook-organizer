import { STRINGS } from "@/lib/strings";
import { OpRow } from "./OpRow";
import type { PlanGroupView, PlanOpView } from "@/lib/bindings";

const MAX_SHOWN = 40;

export interface GroupDetailProps {
  group: PlanGroupView | null;
  ops: PlanOpView[];
  onExclude: (planOpId: number) => void;
}

// The right-pane single-group detail (F-502/F-504, design-system Section
// 4.10): the selected group's plain-language headline plus its curated
// operations (`OpRow`, capped at `MAX_SHOWN` with a "...and N more" note
// pointing to the exported report, mirroring the design-system's own report
// example cap). Used when the F-503 filter is inactive; the filter's own
// cross-group results list is rendered directly by `Review.tsx` with the
// same `OpRow`.
export function GroupDetail({ group, ops, onExclude }: GroupDetailProps) {
  if (!group) {
    return <p className="text-[13px] text-ink-2">{STRINGS.review.detailEmpty}</p>;
  }

  if (ops.length === 0) {
    return (
      <div>
        <h2 className="font-serif text-[20px] font-medium">{group.headline}</h2>
        <p className="mt-2 max-w-[56ch] text-[13px] leading-relaxed text-ink-2">
          {STRINGS.review.detailNoOps}
        </p>
      </div>
    );
  }

  const shown = ops.slice(0, MAX_SHOWN);
  const more = ops.length - shown.length;

  return (
    <div>
      <h2 className="font-serif text-[20px] font-medium">{group.headline}</h2>
      <p className="mt-1.5 max-w-[56ch] text-[13px] leading-relaxed text-ink-2">{group.reason}</p>
      <div className="mt-5">
        {shown.map((op, i) => (
          <OpRow key={op.id} op={op} first={i === 0} onExclude={() => onExclude(op.id)} />
        ))}
      </div>
      {more > 0 && (
        <p className="mt-3 border-t border-border pt-3.5 text-[12px] text-ink-3">
          {STRINGS.review.moreOps(more)}
        </p>
      )}
    </div>
  );
}
