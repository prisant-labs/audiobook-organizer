import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ErrorCallout } from "../ErrorCallout";
import { EmptyState } from "../EmptyState";
import { ERROR_COPY } from "@/lib/errorCopy";

afterEach(cleanup);

// F-908 (AC-24): the family-safe error surface renders the mapped plain
// sentence + next step, never a bare OS error on a family-facing line; the raw
// technical detail lives only inside the "Show file details" disclosure.
describe("ErrorCallout", () => {
  it("shows the mapped sentence and next step, with the raw detail only in the disclosure", () => {
    render(
      <ErrorCallout
        copy={ERROR_COPY["scan-failed"]}
        detail={"scan-failed: C:\\lib exploded at frame 3"}
      />,
    );

    expect(screen.getByText(ERROR_COPY["scan-failed"].sentence)).toBeInTheDocument();
    expect(screen.getByText(ERROR_COPY["scan-failed"].nextStep)).toBeInTheDocument();
    // The raw detail is present (for tier 1) but tucked behind the disclosure,
    // never as the family-facing sentence.
    expect(screen.getByText("Show file details")).toBeInTheDocument();
    expect(screen.getByText(/exploded at frame 3/)).toBeInTheDocument();
    // The disclosure content is inside a <details> that is closed by default.
    expect(screen.getByText(/exploded at frame 3/).closest("details")).not.toHaveAttribute("open");
  });

  it("falls back to a generic family-safe surface when no mapped copy is given", () => {
    const onRetry = vi.fn();
    render(<ErrorCallout detail="overview-failed: database is locked" onRetry={onRetry} />);
    expect(screen.getByText("Something went wrong.")).toBeInTheDocument();
    expect(screen.getByText(/overview-failed/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();
  });

  it("renders a re-pick action distinct from retry (F-909 root-missing)", async () => {
    const user = userEvent.setup();
    const onAction = vi.fn();
    const onRetry = vi.fn();
    render(
      <ErrorCallout
        copy={ERROR_COPY["root-not-found"]}
        action={{ label: "Choose your library folder again", onClick: onAction }}
        onRetry={onRetry}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Choose your library folder again" }));
    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onRetry).not.toHaveBeenCalled();
  });

  it("carries an icon alongside the text so status is never color-alone (Section 8)", () => {
    const { container } = render(<ErrorCallout copy={ERROR_COPY["scan-failed"]} />);
    // lucide renders an <svg>; its presence plus the text label satisfies the
    // icon-plus-label rule.
    expect(container.querySelector("svg")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});

// F-908 (AC-25): the empty/edge surface keeps a disabled primary action
// visible with a reason beside it (the all-groups-excluded rule, design-system
// Section 5.2/7), never hiding it.
describe("EmptyState", () => {
  it("shows a disabled primary action with its reason rather than hiding it", () => {
    render(
      <EmptyState
        heading="Nothing selected"
        action={{ label: "Organize now", onClick: vi.fn(), disabled: true, reason: "Turn on at least one group." }}
      />,
    );
    expect(screen.getByRole("button", { name: "Organize now" })).toBeDisabled();
    expect(screen.getByText("Turn on at least one group.")).toBeInTheDocument();
  });
});
