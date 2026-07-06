import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { render, screen, cleanup, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Review } from "../Review";
import { usePlanReview, type UsePlanReview } from "@/hooks/usePlanReview";
import type { PlanGroupView, PlanOpView, PlanReview } from "@/lib/bindings";

// "Mocked bindings" pattern: the route consumes `usePlanReview` (itself unit
// tested against the mocked `lib/plan` client, see
// hooks/__tests__/usePlanReview.test.tsx), so this suite mocks the HOOK the
// same way Library.test.tsx is handed a constructed `health` value rather
// than mocking the fetch layer underneath it.
vi.mock("@/hooks/usePlanReview", () => ({ usePlanReview: vi.fn() }));
const mockedUsePlanReview = vi.mocked(usePlanReview);

afterEach(cleanup);
beforeEach(() => {
  mockedUsePlanReview.mockReset();
});

const SEVEN_SLUGS = [
  "staging",
  "loose-books",
  "messy-names",
  "box-sets",
  "bundles",
  "copies",
  "empty-folders",
];

function group(overrides: Partial<PlanGroupView> = {}): PlanGroupView {
  const merged = {
    group: "loose-books",
    label: "loose books",
    headline: "Give 3 loose books their own folders",
    reason: "These audiobooks are sitting as single files instead of their own folder.",
    op_count: 3,
    byte_size: 3000,
    status: "included" as const,
    warning_count: 0,
    blocked_count: 0,
    ...overrides,
  };
  // Default the actionable count to the full op_count unless a test overrides
  // it, so footer-sum assertions that only set op_count still add up.
  return { ...merged, actionable_count: overrides.actionable_count ?? merged.op_count };
}

function sevenGroups(overrides: Partial<Record<string, Partial<PlanGroupView>>> = {}): PlanGroupView[] {
  return SEVEN_SLUGS.map((slug) =>
    group({
      group: slug,
      label: slug.replace("-", " "),
      headline: `Headline for ${slug}`,
      ...overrides[slug],
    }),
  );
}

function op(overrides: Partial<PlanOpView> = {}): PlanOpView {
  return {
    id: 1,
    group: "loose-books",
    kind: "move",
    kind_reason: null,
    source_path: "E:\\lib\\Sapiens.m4b",
    target_path: "E:\\lib\\Author\\Sapiens.m4b",
    rationale: "Move this loose book into its own folder.",
    confidence: "high",
    byte_size: 1000,
    validation: "valid",
    validation_reason: null,
    warning_text: null,
    approval: "pending",
    matched_pattern: null,
    extracted_fields: [],
    stripped_noise: null,
    ...overrides,
  };
}

function reviewData(overrides: Partial<PlanReview> = {}): PlanReview {
  return { plan_id: 1, scan_id: 9, groups: sevenGroups(), ...overrides };
}

function hookState(overrides: Partial<UsePlanReview> = {}): UsePlanReview {
  return {
    status: "ready",
    review: reviewData(),
    ops: [op()],
    opsTruncated: false,
    error: null,
    errorCode: null,
    regenerate: vi.fn(),
    cancel: vi.fn(),
    toggleGroup: vi.fn().mockResolvedValue(undefined),
    excludeOp: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

describe("Review", () => {
  it("shows the honest no-scan state without rendering any cards", () => {
    mockedUsePlanReview.mockReturnValue(hookState({ status: "no-scan", review: null, ops: [] }));
    render(<Review scanId={null} />);
    expect(screen.getByText(/Scan your library first/)).toBeInTheDocument();
    expect(screen.queryByRole("switch")).toBeNull();
  });

  it("shows a building-the-plan message while generating", () => {
    mockedUsePlanReview.mockReturnValue(hookState({ status: "generating", review: null, ops: [] }));
    render(<Review scanId={9} />);
    expect(screen.getByText(/Building the tidy-up plan/)).toBeInTheDocument();
  });

  it("the plan-building state carries a real Stop wired to cancel (AC-26)", async () => {
    const user = userEvent.setup();
    const cancel = vi.fn();
    mockedUsePlanReview.mockReturnValue(
      hookState({ status: "generating", review: null, ops: [], cancel }),
    );
    render(<Review scanId={9} />);

    await user.click(screen.getByRole("button", { name: "Stop" }));
    expect(cancel).toHaveBeenCalledTimes(1);
    // Never the demo-only "Skip ahead" affordance (FD-02).
    expect(screen.queryByText(/skip ahead/i)).toBeNull();
  });

  it("the stopped state offers to build the plan again, calling regenerate (AC-26)", async () => {
    const user = userEvent.setup();
    const regenerate = vi.fn();
    mockedUsePlanReview.mockReturnValue(
      hookState({ status: "stopped", review: null, ops: [], regenerate }),
    );
    render(<Review scanId={9} />);

    expect(screen.getByText(/Building the plan was stopped/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /build the plan again/i }));
    expect(regenerate).toHaveBeenCalledTimes(1);
  });

  it("surfaces a retry control on error, calling regenerate()", async () => {
    const user = userEvent.setup();
    const regenerate = vi.fn();
    mockedUsePlanReview.mockReturnValue(
      hookState({ status: "error", review: null, ops: [], error: "plan-generation-failed: boom", regenerate }),
    );
    render(<Review scanId={9} />);

    expect(screen.getByText(/plan-generation-failed/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /try again/i }));
    expect(regenerate).toHaveBeenCalledTimes(1);
  });

  it("renders exactly the seven canonical campaign groups as cards (AC-10)", () => {
    mockedUsePlanReview.mockReturnValue(hookState());
    render(<Review scanId={9} />);
    expect(screen.getAllByRole("switch")).toHaveLength(7);
  });

  it("selecting a card shows that group's operations in the detail pane", async () => {
    const user = userEvent.setup();
    mockedUsePlanReview.mockReturnValue(
      hookState({
        review: reviewData({ groups: sevenGroups({ bundles: { headline: "Unpack 2 books" } }) }),
        ops: [op({ id: 1, group: "bundles", rationale: "Extract this book from the bundle." })],
      }),
    );
    render(<Review scanId={9} />);

    await user.click(screen.getByText("Unpack 2 books"));
    expect(screen.getByText("Extract this book from the bundle.")).toBeInTheDocument();
  });

  it("toggling a card's switch calls toggleGroup with the group slug and desired state (AC-11)", async () => {
    const user = userEvent.setup();
    const toggleGroup = vi.fn().mockResolvedValue(undefined);
    mockedUsePlanReview.mockReturnValue(hookState({ toggleGroup }));
    render(<Review scanId={9} />);

    const switches = screen.getAllByRole("switch");
    await user.click(switches[0]); // staging card, default status "included" -> toggling off
    expect(toggleGroup).toHaveBeenCalledWith("staging", false);
  });

  it("excluding an operation calls excludeOp with its id (AC-13)", async () => {
    const user = userEvent.setup();
    const excludeOp = vi.fn().mockResolvedValue(undefined);
    mockedUsePlanReview.mockReturnValue(
      hookState({ ops: [op({ id: 42, group: "loose-books" })], excludeOp }),
    );
    render(<Review scanId={9} />);

    // Default selection is the first card (staging); switch to loose-books.
    await user.click(screen.getByText("Headline for loose-books"));
    await user.click(screen.getByText("Leave this one out"));
    expect(excludeOp).toHaveBeenCalledWith(42);
  });

  it("filtering narrows the visible operations across groups and never calls a mutation (AC-16/AC-17)", async () => {
    const user = userEvent.setup();
    const toggleGroup = vi.fn();
    const excludeOp = vi.fn();
    mockedUsePlanReview.mockReturnValue(
      hookState({
        ops: [
          op({ id: 1, group: "loose-books", rationale: "Move Sapiens into its own folder." }),
          op({ id: 2, group: "bundles", rationale: "Extract Dune from the bundle." }),
        ],
        toggleGroup,
        excludeOp,
      }),
    );
    render(<Review scanId={9} />);

    await user.type(screen.getByPlaceholderText("Search by name..."), "dune");

    expect(screen.getByText("Extract Dune from the bundle.")).toBeInTheDocument();
    expect(screen.queryByText("Move Sapiens into its own folder.")).toBeNull();
    expect(toggleGroup).not.toHaveBeenCalled();
    expect(excludeOp).not.toHaveBeenCalled();
  });

  it("computes footer totals from included groups only, stating groups and changes separately (FD-08/AC-11)", () => {
    mockedUsePlanReview.mockReturnValue(
      hookState({
        review: reviewData({
          groups: sevenGroups({
            staging: { status: "checking", op_count: 0 },
            "loose-books": { status: "included", op_count: 5 },
            "messy-names": { status: "included", op_count: 2 },
            "box-sets": { status: "left-out", op_count: 4 },
            bundles: { status: "checking", op_count: 0 },
            copies: { status: "checking", op_count: 0 },
            "empty-folders": { status: "left-out", op_count: 1 },
          }),
        }),
      }),
    );
    render(<Review scanId={9} />);

    const footer = within(screen.getByText(/Nothing is deleted/).closest("div")!);
    expect(footer.getByText(/2 of 7/)).toBeInTheDocument();
    expect(footer.getByText("groups included", { exact: false })).toBeInTheDocument();
    expect(footer.getByText("7", { selector: "b" })).toBeInTheDocument();
    expect(footer.getByText("changes", { exact: false })).toBeInTheDocument();
  });

  it("disables the primary confirm action and shows an explanatory line when every group is excluded (design-system 5.2)", () => {
    mockedUsePlanReview.mockReturnValue(
      hookState({
        review: reviewData({ groups: sevenGroups().map((g) => ({ ...g, status: "checking" })) }),
      }),
    );
    render(<Review scanId={9} />);

    expect(screen.getByRole("button", { name: "Tidy up now" })).toBeDisabled();
    expect(screen.getByText(/Turn on at least one group/)).toBeInTheDocument();
  });

  it("the confirm affordance leads to the honest not-yet state rather than applying anything", async () => {
    const user = userEvent.setup();
    mockedUsePlanReview.mockReturnValue(hookState());
    render(<Review scanId={9} />);

    await user.click(screen.getByRole("button", { name: "Tidy up now" }));
    await user.click(screen.getByRole("button", { name: "Go ahead" }));

    expect(screen.getByText(/isn't available in this version yet/)).toBeInTheDocument();
  });
});
