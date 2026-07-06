import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OpRow } from "../OpRow";
import type { PlanOpView } from "@/lib/bindings";

afterEach(cleanup);

function op(overrides: Partial<PlanOpView> = {}): PlanOpView {
  return {
    id: 7,
    group: "loose-books",
    kind: "move",
    kind_reason: null,
    source_path: "E:\\lib\\Sapiens.m4b",
    target_path: "E:\\lib\\Yuval Noah Harari\\Sapiens\\Sapiens.m4b",
    rationale: "This audiobook is sitting loose at the library root; move it into its own folder.",
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

describe("OpRow", () => {
  it("renders the plain-language rationale, with the raw path confined to the disclosure (AC-19)", () => {
    const { container } = render(<OpRow op={op()} onExclude={vi.fn()} />);
    const rationale = screen.getByText(/sitting loose at the library root/);
    expect(rationale.textContent).not.toContain("E:\\lib");
    // The path DOES exist, but only inside the "Show file details" disclosure
    // (FD-13's one sanctioned location) - never as a sibling of the rationale.
    const details = container.querySelector("details");
    expect(details?.textContent).toContain("E:\\lib\\Sapiens.m4b");
  });

  it("calls onExclude when 'Leave this one out' is clicked (AC-13)", async () => {
    const user = userEvent.setup();
    const onExclude = vi.fn();
    render(<OpRow op={op()} onExclude={onExclude} />);

    await user.click(screen.getByText("Leave this one out"));
    expect(onExclude).toHaveBeenCalledTimes(1);
  });

  it("shows an already-excluded row as struck through with no exclude button", () => {
    render(<OpRow op={op({ approval: "excluded" })} onExclude={vi.fn()} />);
    expect(screen.queryByText("Leave this one out")).toBeNull();
    expect(screen.getByText("Left out")).toBeInTheDocument();
  });

  it("hides the exclude control for a no-op row (nothing to exclude)", () => {
    render(<OpRow op={op({ kind: "no-op", kind_reason: "staging" })} onExclude={vi.fn()} />);
    expect(screen.queryByText("Leave this one out")).toBeNull();
  });

  it("shows a warning pill with plain-language text when the op carries one", () => {
    render(
      <OpRow
        op={op({ validation: "warning", warning_text: "This move crosses drives, so the file is copied and checked." })}
        onExclude={vi.fn()}
      />,
    );
    expect(screen.getByText(/crosses drives/)).toBeInTheDocument();
  });

  it("offers exclude (never include) on a blocked op (AC-14)", () => {
    render(
      <OpRow
        op={op({ validation: "blocked", warning_text: "Scan again to refresh the plan." })}
        onExclude={vi.fn()}
      />,
    );
    expect(screen.getByText(/Scan again to refresh/)).toBeInTheDocument();
    expect(screen.getByText("Leave this one out")).toBeInTheDocument();
  });

  it("renders the 'Show file details' disclosure for every row", () => {
    render(<OpRow op={op()} onExclude={vi.fn()} />);
    expect(screen.getByText("Show file details")).toBeInTheDocument();
  });
});
