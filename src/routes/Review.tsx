import { useMemo, useState } from "react";
import { CheckCircle2 } from "lucide-react";
import { STRINGS } from "@/lib/strings";
import { copyForCode } from "@/lib/errorCopy";
import { usePlanReview } from "@/hooks/usePlanReview";
import { DEFAULT_PLAN_FILTER, filterPlanOps, isPlanFilterActive, type PlanFilterState } from "@/lib/planFilter";
import { GroupCard } from "@/components/review/GroupCard";
import { GroupDetail } from "@/components/review/GroupDetail";
import { OpRow } from "@/components/review/OpRow";
import { PlanFilter } from "@/components/review/PlanFilter";
import { ReviewFooter } from "@/components/review/ReviewFooter";
import { BuildingThePlan } from "@/components/states/BuildingThePlan";
import { EmptyState } from "@/components/states/EmptyState";
import { ErrorCallout } from "@/components/states/ErrorCallout";

export interface ReviewProps {
  // The most recently completed scan's id (from `classify_overview`, owned by
  // AppShell's single `useHealthMetrics()` call - see Library.tsx's own
  // comment on why this is shared rather than re-fetched here). `null` before
  // any scan has ever completed (AC-28-adjacent honest empty state).
  scanId: number | null;
}

// F-903 plan review surface (T-19..T-25, AC-10..AC-20): the two-pane review.
// Left: the seven FD-26 campaign-group cards with include/skip switches
// (F-502). Right: the selected group's curated operations with rationale,
// "needs you" flags, per-op exclude (F-502/AC-13), and the "Show file
// details" disclosure (F-504). The F-503 filter narrows the right pane
// across every group at once when active; clearing it returns to the
// selected card's own detail. Loading/error/no-scan states here are minimal
// (matching Library.tsx's precedent): the full F-908 states are Phase 7's
// brief (T-29..T-32) and are meant to supersede these, not be duplicated by
// them.
export function Review({ scanId }: ReviewProps) {
  const { status, review, ops, error, errorCode, regenerate, cancel, toggleGroup, excludeOp } =
    usePlanReview(scanId);
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);
  const [filter, setFilter] = useState<PlanFilterState>(DEFAULT_PLAN_FILTER);

  const filterActive = isPlanFilterActive(filter);
  const filteredOps = useMemo(() => filterPlanOps(ops, filter), [ops, filter]);
  // Default to the first card so the detail pane is never empty once a plan
  // has loaded; derived rather than synced via an effect, so it recomputes
  // for free whenever a fresh plan replaces the reviewed one.
  const effectiveSelectedGroup = selectedGroup ?? review?.groups[0]?.group ?? null;
  const groupOps = useMemo(
    () => ops.filter((op) => op.group === effectiveSelectedGroup),
    [ops, effectiveSelectedGroup],
  );

  if (status === "no-scan") {
    // Honest pre-scan empty state (design-system Section 5.2): nothing to
    // review until the library has been scanned.
    return (
      <EmptyState
        icon={<CheckCircle2 size={22} />}
        heading={STRINGS.review.noScan.heading}
        body={STRINGS.review.noScan.body}
      />
    );
  }

  if (status === "generating" || status === "loading") {
    // The DISTINCT plan-building loading state with a real Stop (AC-26,
    // design-system Section 5.3/5.4). Stop honestly abandons the wait and
    // drops to the stopped surface below; no library change happens here.
    return <BuildingThePlan onStop={cancel} />;
  }

  if (status === "stopped") {
    // The user stopped the plan build (AC-26). Calm, reversible: build again.
    return (
      <EmptyState
        heading={STRINGS.states.planBuildStopped.heading}
        body={STRINGS.states.planBuildStopped.body}
        action={{ label: STRINGS.states.planBuildStopped.action, onClick: regenerate }}
      />
    );
  }

  if (status === "error" || !review) {
    // F-908 plan-failure surface (AC-24): family-safe copy mapped from the
    // code when carried, generic otherwise; the raw cause stays in the
    // disclosure. Retry re-runs generation.
    return (
      <ErrorCallout
        copy={errorCode ? copyForCode(errorCode) : null}
        detail={error}
        onRetry={regenerate}
      />
    );
  }

  const activeGroup = review.groups.find((g) => g.group === effectiveSelectedGroup) ?? null;

  return (
    <div>
      <div className="grid gap-8 lg:grid-cols-[420px_1fr]">
        <div>
          <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em] [text-wrap:balance]">
            {STRINGS.review.heading}
          </h1>
          <p className="mt-2 max-w-[46ch] text-[13.5px] leading-[1.55] text-ink-2 [text-wrap:pretty]">
            {STRINGS.review.lede}
          </p>
          <div className="mt-6">
            {review.groups.map((group) => (
              <GroupCard
                key={group.group}
                group={group}
                selected={group.group === effectiveSelectedGroup}
                onSelect={() => setSelectedGroup(group.group)}
                onToggle={(included) => void toggleGroup(group.group, included)}
              />
            ))}
          </div>
        </div>

        <div>
          <PlanFilter filter={filter} onChange={setFilter} groups={review.groups} />
          {filterActive ? (
            filteredOps.length === 0 ? (
              <p className="text-[13px] text-ink-2">{STRINGS.review.filterNoMatches}</p>
            ) : (
              <div>
                {filteredOps.map((op, i) => (
                  <OpRow
                    key={op.id}
                    op={op}
                    first={i === 0}
                    groupLabel={review.groups.find((g) => g.group === op.group)?.label}
                    onExclude={() => void excludeOp(op.id)}
                  />
                ))}
              </div>
            )
          ) : (
            <GroupDetail group={activeGroup} ops={groupOps} onExclude={(id) => void excludeOp(id)} />
          )}
        </div>
      </div>

      <ReviewFooter groups={review.groups} />
    </div>
  );
}
