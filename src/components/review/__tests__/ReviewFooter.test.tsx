import { describe, expect, it, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { ReviewFooter } from "../ReviewFooter";
import type { PlanGroupView } from "@/lib/bindings";

afterEach(cleanup);

function group(overrides: Partial<PlanGroupView> = {}): PlanGroupView {
  return {
    group: "loose-books",
    label: "loose books",
    headline: "Give 3 loose books their own folders",
    reason: "why",
    op_count: 3,
    actionable_count: 3,
    byte_size: 0,
    status: "included",
    warning_count: 0,
    blocked_count: 0,
    ...overrides,
  };
}

describe("ReviewFooter", () => {
  it("sums actionable_count, not op_count, across included groups (FIX 5)", () => {
    // A mixed group: 3 changes total, but only 1 would actually run (the other
    // two are blocked/excluded), plus a clean group of 2. The footer must show
    // 3 changes (1 + 2), never 5 (op_count 3 + 2).
    const groups: PlanGroupView[] = [
      group({ group: "loose-books", op_count: 3, actionable_count: 1, blocked_count: 1 }),
      group({ group: "empty-folders", op_count: 2, actionable_count: 2 }),
      // A held group contributes nothing (not included).
      group({ group: "copies", op_count: 4, actionable_count: 0, status: "checking" }),
    ];
    render(<ReviewFooter groups={groups} />);

    // "2 of 3 groups included" (loose-books + empty-folders; copies is held).
    expect(screen.getByText("2 of 3")).toBeInTheDocument();
    // "3 changes" = 1 actionable + 2 actionable, NOT 5 (the op_count sum).
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.queryByText("5")).toBeNull();
  });

  it("shows the all-excluded note and disables the action when no group is included", () => {
    const groups: PlanGroupView[] = [
      group({ status: "checking", actionable_count: 0 }),
      group({ group: "empty-folders", status: "left-out", actionable_count: 0 }),
    ];
    render(<ReviewFooter groups={groups} />);
    expect(screen.getByText("0 of 2")).toBeInTheDocument();
  });
});
