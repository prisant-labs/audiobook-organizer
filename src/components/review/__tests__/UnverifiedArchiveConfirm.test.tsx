import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UnverifiedArchiveConfirm } from "../UnverifiedArchiveConfirm";
import { STRINGS } from "@/lib/strings";

afterEach(cleanup);

const S = STRINGS.duplicatesOverride;

describe("UnverifiedArchiveConfirm (v0.6.0 hardening, AC-13)", () => {
  // The criterion in one test. AC-13 calls the override "a deliberate two-step
  // affordance (not a default)", and the property that makes it deliberate is
  // that the first press cannot archive anything. A one-press version would
  // still LOOK like a warning and would have lost the whole guarantee.
  it("does not archive on the first press", async () => {
    const onConfirm = vi.fn();
    render(<UnverifiedArchiveConfirm onConfirm={onConfirm} />);

    await userEvent.click(screen.getByRole("button", { name: S.start }));

    expect(onConfirm).not.toHaveBeenCalled();
  });

  // AC-12 requires the override be "confirmed through a warning ... that states
  // the copies were not content-verified". Asserting the sentence is present,
  // not merely that some warning appeared: the whole point of the affordance is
  // that the reader is told WHY this is risky before they agree to it.
  it("states that the copies were not compared before asking for agreement", async () => {
    render(<UnverifiedArchiveConfirm onConfirm={vi.fn()} />);

    await userEvent.click(screen.getByRole("button", { name: S.start }));

    expect(screen.getByText(S.warning)).toBeInTheDocument();
  });

  it("archives on the second press", async () => {
    const onConfirm = vi.fn();
    render(<UnverifiedArchiveConfirm onConfirm={onConfirm} />);

    await userEvent.click(screen.getByRole("button", { name: S.start }));
    await userEvent.click(screen.getByRole("button", { name: S.goAhead }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  // Backing out must be free. A confirm step that can only go forward is a trap
  // rather than a decision, and this one guards a destructive-adjacent action.
  it("backs out without archiving, and can be reopened", async () => {
    const onConfirm = vi.fn();
    render(<UnverifiedArchiveConfirm onConfirm={onConfirm} />);

    await userEvent.click(screen.getByRole("button", { name: S.start }));
    await userEvent.click(screen.getByRole("button", { name: S.cancel }));

    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: S.start })).toBeInTheDocument();
  });

  // Design-system Section 8: status is never carried by colour alone. A red
  // button that reads the same as a safe one fails anyone who cannot see the
  // red, and this is the most consequential control in the duplicates flow.
  it("carries its warning in text, not only in colour", async () => {
    render(<UnverifiedArchiveConfirm onConfirm={vi.fn()} />);

    await userEvent.click(screen.getByRole("button", { name: S.start }));

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(S.warning);
  });

  // Disabled is a real state here: the surface will render this beside groups
  // it cannot act on, and a control that looks pressable but silently does
  // nothing is worse than one that says it is unavailable.
  it("cannot be opened while disabled", async () => {
    const onConfirm = vi.fn();
    render(<UnverifiedArchiveConfirm onConfirm={onConfirm} disabled />);

    const start = screen.getByRole("button", { name: S.start });
    expect(start).toBeDisabled();

    await userEvent.click(start);
    expect(screen.queryByText(S.warning)).not.toBeInTheDocument();
  });
});
