import { ShieldCheck } from "lucide-react";
import { STRINGS } from "@/lib/strings";
import { ConfirmInline } from "./ConfirmInline";
import type { PlanGroupView } from "@/lib/bindings";

export interface ReviewFooterProps {
  groups: readonly PlanGroupView[];
  /**
   * The plan_id of the currently loaded plan. Required to start an apply;
   * when null (plan not yet generated) the confirm button stays disabled.
   */
  planId: number | null;
  /**
   * Called when the user confirms the tidy-up. The shell starts the apply
   * job and navigates to the Apply screen. Optional: when omitted the pre-
   * v0.5.0 stub message ("not available yet") is shown on confirm.
   */
  onStartApply?: (planId: number) => Promise<void>;
}

// The sticky footer action bar (F-502, design-system Section 4.13): a live
// "N of 7 groups included, M changes" total (AC-11; states which quantity
// each number reports, FD-08, never mixing "copies/pairs/groups"), the
// reassurance line, and the two-step confirm (T-21). When every group is
// left out or checking, the primary action is present but disabled with an
// explanatory line beside it (design-system Section 5.2 "All-groups-
// excluded", Section 7).
export function ReviewFooter({ groups, planId, onStartApply }: ReviewFooterProps) {
  const included = groups.filter((g) => g.status === "included");
  // Sum only ops that would ACTUALLY run: `actionable_count` already excludes
  // each group's blocked and individually-excluded ops (FIX 5), so a group
  // holding held ops never inflates the "M changes" total.
  const totalChanges = included.reduce((sum, g) => sum + g.actionable_count, 0);
  const allExcluded = included.length === 0;

  // Wire the confirm to the real apply start when planId is available and a
  // handler was provided by the shell. When either is absent the stub path
  // in ConfirmInline shows the honest "not available yet" message.
  const onConfirm =
    planId != null && onStartApply ? () => onStartApply(planId) : undefined;

  return (
    <div className="sticky bottom-0 mt-6 flex flex-wrap items-center gap-4 border-t border-border bg-titlebar px-5 py-3.5">
      <span className="flex items-center gap-2 text-[12.5px] text-ink-2">
        <ShieldCheck aria-hidden="true" className="h-[15px] w-[15px] flex-none text-good" strokeWidth={2.2} />
        {STRINGS.review.footerReassurance}
      </span>
      <span className="ml-auto tabular-nums text-[12.5px] text-ink-3">
        <b className="font-semibold text-ink">
          {included.length} of {groups.length}
        </b>{" "}
        groups included &middot; <b className="font-semibold text-ink">{totalChanges.toLocaleString("en-US")}</b>{" "}
        changes
      </span>
      {allExcluded && <span className="text-[12px] text-ink-3">{STRINGS.review.allExcludedNote}</span>}
      <ConfirmInline disabled={allExcluded} onConfirm={onConfirm} />
    </div>
  );
}
