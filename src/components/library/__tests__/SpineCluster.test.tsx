import { describe, expect, it, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { SpineCluster } from "../SpineCluster";
import type { SeriesCluster } from "@/lib/bindings";

afterEach(cleanup);

const DRESDEN: SeriesCluster = {
  name: "The Dresden Files",
  author: "Jim Butcher",
  book_count: 20,
};

// T-17 / AC-8: the series spine cluster keeps the stylized spine metaphor
// (D-06's deliberate exception), and its visuals are deterministic (same
// series in, same render out) so the shelf never flickers across re-renders.
describe("SpineCluster", () => {
  it("shows the series name, author, and book count in the caption", () => {
    render(<SpineCluster series={DRESDEN} />);
    // The series name also appears once, vertically, on the center spine
    // (design-system Section 4.8); assert the caption's own bold instance.
    expect(screen.getAllByText("The Dresden Files").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Jim Butcher/)).toBeInTheDocument();
    expect(screen.getByText(/20 books/)).toBeInTheDocument();
  });

  it("notes how many books are not shown when the cluster exceeds the spine cap", () => {
    render(<SpineCluster series={DRESDEN} />);
    expect(screen.getByText(/not shown/)).toBeInTheDocument();
  });

  it("omits the not-shown note when every book is drawn as a spine", () => {
    render(<SpineCluster series={{ ...DRESDEN, book_count: 3 }} />);
    expect(screen.queryByText(/not shown/)).toBeNull();
  });

  it("renders the identical markup for the same series across two renders (deterministic)", () => {
    const { container: first } = render(<SpineCluster series={DRESDEN} />);
    const firstHtml = first.innerHTML;
    cleanup();
    const { container: second } = render(<SpineCluster series={DRESDEN} />);
    expect(second.innerHTML).toBe(firstHtml);
  });
});
