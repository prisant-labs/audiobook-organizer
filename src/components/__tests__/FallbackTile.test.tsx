import { describe, expect, it, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { FallbackTile } from "../FallbackTile";
import { fallbackTileStyle } from "@/lib/coverHash";

afterEach(cleanup);

// F-907 / design-system 4.5: the fallback tile renders the title in serif on a
// deterministic tint, with the author in micro-caps, and is square 1:1 (AC-22).
describe("FallbackTile", () => {
  it("renders the title text and the author", () => {
    render(<FallbackTile title="The Left Hand of Darkness" author="Ursula K. Le Guin" />);
    expect(screen.getByText("The Left Hand of Darkness")).toBeInTheDocument();
    expect(screen.getByText("Ursula K. Le Guin")).toBeInTheDocument();
  });

  it("exposes an accessible image label combining title and author", () => {
    render(<FallbackTile title="Dune" author="Frank Herbert" />);
    expect(screen.getByRole("img", { name: "Dune by Frank Herbert" })).toBeInTheDocument();
  });

  it("labels by title alone when there is no author", () => {
    render(<FallbackTile title="Beowulf" />);
    expect(screen.getByRole("img", { name: "Beowulf" })).toBeInTheDocument();
  });

  it("is a square 1:1 tile (AC-22)", () => {
    render(<FallbackTile title="Foundation" />);
    expect(screen.getByRole("img")).toHaveClass("aspect-square");
  });

  it("uses the deterministic tint for the title (AC-23)", () => {
    const { background } = fallbackTileStyle("Snow Crash");
    render(<FallbackTile title="Snow Crash" />);
    // The inline background is the hashed tint, so the same title always paints
    // the same tile.
    expect(screen.getByRole("img")).toHaveStyle({ background });
  });

  it("paints the same tint on repeated independent renders", () => {
    render(<FallbackTile title="Anathem" />);
    const first = screen.getByRole("img").getAttribute("style");
    cleanup();
    render(<FallbackTile title="Anathem" />);
    const second = screen.getByRole("img").getAttribute("style");
    expect(first).toBe(second);
  });
});
