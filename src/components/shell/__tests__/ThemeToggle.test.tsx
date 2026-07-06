import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeToggle } from "../ThemeToggle";

// AC-2 / design-system Section 4.2, 7: the theme control is a two-button
// segmented control; the active theme's button carries `aria-pressed="true"`
// and the other `false`; clicking calls back with EXACTLY "day" or "evening".
describe("ThemeToggle", () => {
  it("marks the current theme pressed and the other not", () => {
    render(<ThemeToggle theme="day" onChange={vi.fn()} />);

    expect(screen.getByRole("button", { name: "Day" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Evening" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("flips when the theme prop flips", () => {
    const { rerender } = render(<ThemeToggle theme="day" onChange={vi.fn()} />);
    rerender(<ThemeToggle theme="evening" onChange={vi.fn()} />);

    expect(screen.getByRole("button", { name: "Day" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: "Evening" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("calls onChange with the canonical identifier only (never calm/prose)", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<ThemeToggle theme="day" onChange={onChange} />);

    await user.click(screen.getByRole("button", { name: "Evening" }));
    expect(onChange).toHaveBeenCalledExactlyOnceWith("evening");

    await user.click(screen.getByRole("button", { name: "Day" }));
    expect(onChange).toHaveBeenCalledWith("day");
    expect(onChange).not.toHaveBeenCalledWith("calm");
    expect(onChange).not.toHaveBeenCalledWith("prose");
  });

  it("groups the two buttons under one accessible label", () => {
    render(<ThemeToggle theme="day" onChange={vi.fn()} />);
    expect(screen.getByRole("group", { name: "Theme" })).toBeInTheDocument();
  });
});
