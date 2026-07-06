import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PlanFilter } from "../PlanFilter";
import { DEFAULT_PLAN_FILTER } from "@/lib/planFilter";
import type { PlanGroupView } from "@/lib/bindings";

afterEach(cleanup);

const GROUPS: PlanGroupView[] = [
  {
    group: "loose-books",
    label: "loose books",
    headline: "h",
    reason: "r",
    op_count: 1,
    byte_size: 0,
    status: "included",
    warning_count: 0,
    blocked_count: 0,
  },
  {
    group: "bundles",
    label: "bundles",
    headline: "h",
    reason: "r",
    op_count: 1,
    byte_size: 0,
    status: "included",
    warning_count: 0,
    blocked_count: 0,
  },
];

describe("PlanFilter", () => {
  it("calls onChange with updated text as the user types (AC-16 free text)", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PlanFilter filter={DEFAULT_PLAN_FILTER} onChange={onChange} groups={GROUPS} />);

    await user.type(screen.getByPlaceholderText("Search by name..."), "s");
    expect(onChange).toHaveBeenCalledWith({ ...DEFAULT_PLAN_FILTER, text: "s" });
  });

  it("lists every group as a facet option", () => {
    render(<PlanFilter filter={DEFAULT_PLAN_FILTER} onChange={vi.fn()} groups={GROUPS} />);
    expect(screen.getByRole("option", { name: "loose books" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "bundles" })).toBeInTheDocument();
  });

  it("calls onChange when a facet changes, never mutating approval (AC-17: this component owns no plan state)", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PlanFilter filter={DEFAULT_PLAN_FILTER} onChange={onChange} groups={GROUPS} />);

    await user.selectOptions(screen.getByLabelText("Filter by group"), "bundles");
    expect(onChange).toHaveBeenCalledWith({ ...DEFAULT_PLAN_FILTER, group: "bundles" });
  });

  it("shows a Clear control only when the filter is active, and it resets to the default", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const { rerender } = render(
      <PlanFilter filter={DEFAULT_PLAN_FILTER} onChange={onChange} groups={GROUPS} />,
    );
    expect(screen.queryByText("Clear")).toBeNull();

    rerender(
      <PlanFilter
        filter={{ ...DEFAULT_PLAN_FILTER, group: "bundles" }}
        onChange={onChange}
        groups={GROUPS}
      />,
    );
    await user.click(screen.getByText("Clear"));
    expect(onChange).toHaveBeenCalledWith(DEFAULT_PLAN_FILTER);
  });
});
