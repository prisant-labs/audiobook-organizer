import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PolicySelector } from "../PolicySelector";

describe("PolicySelector (F-704, AC-28)", () => {
  it("offers the three surviving policies, not the four the plan still names", () => {
    render(<PolicySelector value="flag-only" onChange={vi.fn()} />);
    expect(screen.getAllByRole("radio")).toHaveLength(3);
  });

  it("reports the choice rather than deciding anything itself", async () => {
    const onChange = vi.fn();
    render(<PolicySelector value="flag-only" onChange={onChange} />);

    await userEvent.click(screen.getByRole("radio", { name: "The biggest copy" }));

    expect(onChange).toHaveBeenCalledWith("keep-larger");
  });

  /// Two selectors on one page must not share a radio group.
  ///
  /// THIS ASSERTS THE MECHANISM, NOT THE SYMPTOM, and that is deliberate rather
  /// than lazy. The first version hardcoded `name="duplicate-policy"`, so the two
  /// gallery specimens formed ONE native radio group and the browser rendered
  /// both with nothing selected. The obvious test (assert each is checked) was
  /// written first and PASSED WITH THE BUG STILL IN, because React sets `checked`
  /// as a controlled property and jsdom never applies the native grouping that
  /// unchecks the other one. A test that passes against the defect it was written
  /// for is worse than no test: it claims coverage it does not have.
  ///
  /// Distinct `name` attributes are the actual fix and they ARE observable here,
  /// so that is what this pins. The symptom was caught by rendering the gallery
  /// and looking at it, which is the only thing that could have caught it.
  it("gives each instance its own radio group", () => {
    render(
      <>
        <div data-testid="first">
          <PolicySelector value="flag-only" onChange={vi.fn()} />
        </div>
        <div data-testid="second">
          <PolicySelector value="keep-larger" onChange={vi.fn()} />
        </div>
      </>,
    );

    const nameOf = (testId: string) =>
      within(screen.getByTestId(testId))
        .getAllByRole("radio")
        .map((r) => r.getAttribute("name"));

    const first = nameOf("first");
    const second = nameOf("second");

    expect(new Set(first).size).toBe(1);
    expect(new Set(second).size).toBe(1);
    expect(first[0]).not.toBe(second[0]);
  });

  /// The note is the honest part: on an exact group, keyed on name AND size,
  /// there is usually nothing for a size rule to choose between. A person who
  /// switched policies and saw nothing move would otherwise think it was broken.
  it("says that it often changes nothing", () => {
    render(<PolicySelector value="flag-only" onChange={vi.fn()} />);
    expect(screen.getByText(/often leave nothing to choose between/)).toBeInTheDocument();
  });
});
