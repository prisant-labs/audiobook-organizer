import { describe, expect, it, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FileDetails } from "../FileDetails";
import type { PlanOpView } from "@/lib/bindings";

afterEach(cleanup);

function op(overrides: Partial<PlanOpView> = {}): PlanOpView {
  return {
    id: 1,
    group: "loose-books",
    kind: "move",
    kind_reason: null,
    source_path: "E:\\lib\\Sapiens by Yuval Noah Harari.m4b",
    target_path: "E:\\lib\\Yuval Noah Harari\\Sapiens\\Sapiens by Yuval Noah Harari.m4b",
    rationale: "Move this loose book into its own folder.",
    confidence: "high",
    byte_size: 1000,
    validation: "valid",
    validation_reason: null,
    warning_text: null,
    approval: "pending",
    matched_pattern: { id: "pattern-1", name: "Title by Author (loose root file)" },
    extracted_fields: [
      { field: "title", value: "Sapiens", confidence: "high" },
      { field: "author", value: "Yuval Noah Harari", confidence: "high" },
    ],
    stripped_noise: null,
    ...overrides,
  };
}

describe("FileDetails", () => {
  it("is closed by default (raw paths not visible until opened, FD-13/AC-19)", () => {
    const { container } = render(<FileDetails op={op()} />);
    expect(screen.getByText("Show file details")).toBeInTheDocument();
    expect(container.querySelector("details")).not.toHaveAttribute("open");
  });

  it("reveals the raw source and target paths, the matched pattern, and extracted fields when opened (AC-18)", async () => {
    const user = userEvent.setup();
    const { container } = render(<FileDetails op={op()} />);

    await user.click(screen.getByText("Show file details"));

    expect(container.querySelector("details")).toHaveAttribute("open");
    const pathBlock = container.querySelector("pre")?.textContent ?? "";
    expect(pathBlock).toContain("E:\\lib\\Sapiens by Yuval Noah Harari.m4b");
    expect(pathBlock).toContain("E:\\lib\\Yuval Noah Harari\\Sapiens\\Sapiens");
    expect(screen.getByText("Title by Author (loose root file)")).toBeInTheDocument();
    expect(screen.getByText("(pattern-1)")).toBeInTheDocument();
    const items = screen.getAllByRole("listitem").map((li) => li.textContent ?? "");
    expect(items.some((text) => text.includes("title") && text.includes("Sapiens"))).toBe(true);
    expect(items.some((text) => text.includes("author") && text.includes("Yuval Noah Harari"))).toBe(
      true,
    );
  });

  it("shows a low-confidence field rather than hiding it (AC-20)", async () => {
    const user = userEvent.setup();
    render(
      <FileDetails
        op={op({
          extracted_fields: [{ field: "series", value: "unclear guess", confidence: "low" }],
        })}
      />,
    );
    await user.click(screen.getByText("Show file details"));
    expect(screen.getByText("unclear guess")).toBeInTheDocument();
    expect(screen.getByText("(low)")).toBeInTheDocument();
  });

  it("shows the stripped-noise line for a rename op when present", async () => {
    const user = userEvent.setup();
    render(<FileDetails op={op({ stripped_noise: "14.0 - [320k 18;48;03 2.52GB]" })} />);
    await user.click(screen.getByText("Show file details"));
    expect(screen.getByText("14.0 - [320k 18;48;03 2.52GB]")).toBeInTheDocument();
  });

  it("omits the matched-pattern and fields sections when there is nothing to show", () => {
    render(
      <FileDetails
        op={op({ matched_pattern: null, extracted_fields: [], stripped_noise: null })}
      />,
    );
    expect(screen.queryByText(/Matched pattern/)).toBeNull();
    expect(screen.queryByText(/Stripped from the name/)).toBeNull();
  });
});
