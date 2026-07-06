import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GroupCard } from "../GroupCard";
import type { PlanGroupView } from "@/lib/bindings";

afterEach(cleanup);

function group(overrides: Partial<PlanGroupView> = {}): PlanGroupView {
  return {
    group: "loose-books",
    label: "loose books",
    headline: "Give 238 loose books their own folders",
    reason: "These audiobooks are sitting as single files instead of their own folder.",
    op_count: 238,
    actionable_count: 238,
    byte_size: 67.9 * 1024 ** 3,
    status: "included",
    warning_count: 0,
    blocked_count: 0,
    ...overrides,
  };
}

describe("GroupCard", () => {
  it("renders the headline, reason, and tabular-nums change count", () => {
    render(<GroupCard group={group()} selected={false} onSelect={vi.fn()} onToggle={vi.fn()} />);
    expect(screen.getByText("Give 238 loose books their own folders")).toBeInTheDocument();
    expect(screen.getByText(/sitting as single files/)).toBeInTheDocument();
    expect(screen.getByText(/238 changes/)).toBeInTheDocument();
  });

  it("reflects `included` status as a checked switch and the 'included' tag", () => {
    render(<GroupCard group={group({ status: "included" })} selected={false} onSelect={vi.fn()} onToggle={vi.fn()} />);
    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("included")).toBeInTheDocument();
  });

  it("reflects `left-out` status as an unchecked switch and the 'left out' tag", () => {
    render(<GroupCard group={group({ status: "left-out" })} selected={false} onSelect={vi.fn()} onToggle={vi.fn()} />);
    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("left out")).toBeInTheDocument();
  });

  it("disables the switch and shows 'checking' when the group is held (AC-15/OQ-3)", () => {
    render(<GroupCard group={group({ status: "checking" })} selected={false} onSelect={vi.fn()} onToggle={vi.fn()} />);
    expect(screen.getByRole("switch")).toBeDisabled();
    expect(screen.getByText("checking")).toBeInTheDocument();
  });

  it("calls onToggle with the opposite of the current included state (AC-11)", async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();
    render(<GroupCard group={group({ status: "included" })} selected={false} onSelect={vi.fn()} onToggle={onToggle} />);

    await user.click(screen.getByRole("switch"));
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("never calls onToggle when the switch is disabled", async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();
    render(<GroupCard group={group({ status: "checking" })} selected={false} onSelect={vi.fn()} onToggle={onToggle} />);

    await user.click(screen.getByRole("switch"));
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("selects the card (not the switch) on click, and toggling the switch does not also select", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onToggle = vi.fn();
    render(<GroupCard group={group()} selected={false} onSelect={onSelect} onToggle={onToggle} />);

    await user.click(screen.getByRole("switch"));
    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();

    await user.click(screen.getByText("Give 238 loose books their own folders"));
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("is keyboard-selectable (Enter selects, Section 7)", () => {
    const onSelect = vi.fn();
    render(<GroupCard group={group()} selected={false} onSelect={onSelect} onToggle={vi.fn()} />);
    const card = screen.getByRole("button", { name: /Give 238 loose books/i });
    card.focus();
    card.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(onSelect).toHaveBeenCalled();
  });

  it("marks the selected card with aria-selected", () => {
    render(<GroupCard group={group()} selected onSelect={vi.fn()} onToggle={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Give 238 loose books/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
