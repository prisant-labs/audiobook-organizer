import type { RouteId } from "@/routes";

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

// TODO(Phase 4, T-15 `useHealthMetrics()`): wire this to the `classify_overview`
// IPC command once it exists. As of this phase, `classify_overview` (F-202) is
// implemented as pure logic in `crates/abo-core/src/classify/metrics.rs`, but
// it is NOT yet exposed as a tauri-specta command in src/lib/bindings.ts.
// Adding that command is out of this phase's brief (app shell/theme/nav only,
// no new IPC surface). Returning an all-`undefined` shape here means the
// Sidebar renders no badge rather than a fabricated number, which keeps
// AC-7/FD-27 (no hardcoded sample numbers) intact until the real binding
// lands.
export function useNavCounts(): NavCounts {
  return {};
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
