import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ScanProgress } from "../ScanProgress";

// F-104 scan Stop control (T-28, AC-36): direct component coverage,
// independent of Library.tsx's own job-lifecycle wiring (see its test file
// for the end-to-end wiring through `scan_cancel`).
describe("ScanProgress", () => {
  it("shows a tabular-nums count when the total is known", () => {
    render(<ScanProgress done={120} total={500} onStop={vi.fn()} />);
    expect(screen.getByText("120 of 500 read")).toBeInTheDocument();
  });

  it("shows an indeterminate status when the total is not yet known", () => {
    render(<ScanProgress onStop={vi.fn()} />);
    expect(screen.getByText(/reading your library/i)).toBeInTheDocument();
  });

  it("calls onStop when Stop is clicked", async () => {
    const user = userEvent.setup();
    const onStop = vi.fn();
    render(<ScanProgress done={1} total={10} onStop={onStop} />);

    await user.click(screen.getByRole("button", { name: "Stop" }));

    expect(onStop).toHaveBeenCalledTimes(1);
  });

  it("never renders a 'Skip ahead' affordance (FD-02: demo-only, does not ship)", () => {
    render(<ScanProgress done={1} total={10} onStop={vi.fn()} />);
    expect(screen.queryByText(/skip ahead/i)).toBeNull();
  });
});
