import { describe, expect, it, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { Cover } from "../Cover";
import type { CoverImage } from "@/lib/covers";

afterEach(cleanup);

// A tiny 1x1 pixel's worth of fake base64; the component only composes it into a
// data URL, it does not decode it in jsdom.
const IMAGE: CoverImage = { mime: "image/jpeg", base64: "AAECAwQF" };

// F-907 / T-13: Cover renders the real art at 1:1 (AC-22) when present, and the
// deterministic fallback tile when there is no cover OR the bytes fail to decode
// (AC-23).
describe("Cover", () => {
  it("renders the cover image as a data URL when art is present", () => {
    render(<Cover title="Hyperion" author="Dan Simmons" image={IMAGE} />);
    const img = screen.getByRole("img", { name: "Hyperion by Dan Simmons" });
    expect(img).toHaveAttribute("src", "data:image/jpeg;base64,AAECAwQF");
    expect(img).toHaveClass("aspect-square");
  });

  it("renders the fallback tile when there is no cover", () => {
    render(<Cover title="Kindred" author="Octavia E. Butler" image={null} />);
    // The fallback tile is a role=img labeled by title/author, and it renders the
    // title text (the real cover <img> has no visible text node).
    expect(screen.getByText("Kindred")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Kindred by Octavia E. Butler" })).toBeInTheDocument();
  });

  it("falls back to the tile when the image fails to decode (onError)", () => {
    render(<Cover title="Blindsight" author="Peter Watts" image={IMAGE} />);
    // Initially the real image renders.
    const img = screen.getByRole("img", { name: "Blindsight by Peter Watts" });
    expect(img.tagName).toBe("IMG");

    // Simulate a decode failure: the component swaps to the fallback tile.
    fireEvent.error(img);
    expect(screen.getByText("Blindsight")).toBeInTheDocument();
  });
});
