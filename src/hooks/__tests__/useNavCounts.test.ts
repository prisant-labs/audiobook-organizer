import { describe, expect, it } from "vitest";
import { badgeForRoute, navCountsFrom } from "../useNavCounts";
import type { LibraryOverview } from "@/lib/bindings";

function overviewWithDuplicateGroups(count: number): LibraryOverview {
  return {
    scan_id: 1,
    total_books: 10,
    total_bytes: 1,
    needs_tidy_books: count,
    worth_a_look: [],
    series: [],
    good_news: {
      already_tidy_books: 0,
      series_shelved: 0,
      empty_folders: 0,
      duplicate_groups: count,
      duplicate_bytes: 0,
    },
    metrics: {
      per_class: [],
      problems: [{ problem: "duplicate-candidate-groups", unit: "groups", count, byte_total: 0 }],
      total_bytes: 1,
    },
  };
}

describe("navCountsFrom", () => {
  it("returns no counts before any scan has completed (overview is null)", () => {
    expect(navCountsFrom(null)).toEqual({});
  });

  it("reads the duplicate GROUP count from the overview's health metrics (FD-08)", () => {
    expect(navCountsFrom(overviewWithDuplicateGroups(403))).toEqual({ duplicateGroups: 403 });
  });

  it("omits the count when the problem is absent from the metrics", () => {
    const overview = overviewWithDuplicateGroups(0);
    overview.metrics.problems = [];
    expect(navCountsFrom(overview)).toEqual({ duplicateGroups: undefined });
  });
});

describe("badgeForRoute", () => {
  it("renders no badge for library/settings regardless of counts", () => {
    const counts = { duplicateGroups: 403, historyCount: 12, organizeStatus: "ready" };
    expect(badgeForRoute("library", counts)).toBeUndefined();
    expect(badgeForRoute("settings", counts)).toBeUndefined();
  });

  it("stringifies the duplicate GROUP count for the Duplicates badge", () => {
    expect(badgeForRoute("duplicates", { duplicateGroups: 403 })).toBe("403");
    expect(badgeForRoute("duplicates", {})).toBeUndefined();
  });
});
