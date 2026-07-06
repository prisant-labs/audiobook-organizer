import type { RouteId } from "@/routes";
import type { LibraryOverview } from "@/lib/bindings";

// Sidebar nav badge data (T-03, AC-3, FD-08). The canonical unit for
// Duplicates is the GROUP, never member copies.
export interface NavCounts {
  /** Duplicate GROUP count (FD-08). `undefined` when not yet known. */
  duplicateGroups?: number;
  /** History entry count. `undefined` when not yet known. */
  historyCount?: number;
  /** Tidy-up readiness label ("ready"), matching prototype 04/07. */
  tidyUpStatus?: string;
}

const DUPLICATE_GROUPS_PROBLEM = "duplicate-candidate-groups";

// Derives sidebar badge counts from the SAME `LibraryOverview` the Library
// home renders (F-202/F-902, T-15, v0.4.0 Phase 4): `AppShell` owns ONE
// `useHealthMetrics()` call and hands its `overview` both to `Sidebar` (via
// this function) and to `Library` (as a prop), so the badge and the home's
// own duplicate count can never disagree, and a completed scan's `reload()`
// updates both at once. (An earlier version had `Sidebar` and `Library` each
// call `useHealthMetrics()` independently; that left the badge stuck at
// whatever it read on mount, since only `Library`'s own instance ever
// reloaded - caught in the v0.4.0 Phase 4 headed walkthrough.) Before the
// first scan (or on load/error), `overview` is `null` and every count here
// stays `undefined` - the Sidebar renders no badge rather than a fabricated
// number (AC-7/FD-27).
//
// A plain function, not a hook: it has no internal state or effects of its
// own, just a derivation from the value its one caller already holds.
export function navCountsFrom(overview: LibraryOverview | null): NavCounts {
  if (!overview) return {};
  const duplicateGroups = overview.metrics.problems.find(
    (p) => p.problem === DUPLICATE_GROUPS_PROBLEM,
  )?.count;
  return { duplicateGroups };
}

// Maps a RouteId to the badge value the Sidebar should render for it, or
// `undefined` for no badge. Kept as a pure function (not inlined in the
// component) so it is unit-testable independent of rendering.
export function badgeForRoute(route: RouteId, counts: NavCounts): string | undefined {
  switch (route) {
    case "duplicates":
      return counts.duplicateGroups === undefined ? undefined : String(counts.duplicateGroups);
    case "history":
      return counts.historyCount === undefined ? undefined : String(counts.historyCount);
    case "tidy-up":
      return counts.tidyUpStatus;
    case "library":
    case "settings":
      return undefined;
  }
}
