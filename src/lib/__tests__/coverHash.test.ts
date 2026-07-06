import { describe, expect, it } from "vitest";
import {
  fallbackTileStyle,
  hashTitle,
  hashTitleToHsl,
  hashTitleToHue,
} from "@/lib/coverHash";

// F-907 / AC-23: the fallback tile's color is derived deterministically from a
// hash of the title, so the SAME title always yields the SAME tile. This is the
// determinism test the plan's Phase 3 verification calls for ("same title yields
// same fallback color across renders").
describe("coverHash", () => {
  it("hashes the same title to the same value every time", () => {
    expect(hashTitle("The Way of Kings")).toBe(hashTitle("The Way of Kings"));
    expect(hashTitleToHsl("The Way of Kings")).toBe(hashTitleToHsl("The Way of Kings"));
  });

  it("produces the same full tile style for the same title (AC-23)", () => {
    const a = fallbackTileStyle("Project Hail Mary");
    const b = fallbackTileStyle("Project Hail Mary");
    expect(a).toEqual(b);
    expect(a.background).toBe(b.background);
    expect(a.color).toBe(b.color);
  });

  it("varies the tint across different titles", () => {
    // Not a strict guarantee for every pair (hues can collide mod 360), but these
    // distinct titles must not all collapse to one tint - a shelf needs variety.
    const hues = new Set(
      ["Dune", "Mistborn", "The Hobbit", "Neuromancer", "Hyperion"].map(hashTitleToHue),
    );
    expect(hues.size).toBeGreaterThan(1);
  });

  it("emits a well-formed hsl() tint in the muted library range", () => {
    const hsl = hashTitleToHsl("Some Book");
    // hsl(<hue> 34% 42%): fixed saturation/lightness, only the hue varies.
    expect(hsl).toMatch(/^hsl\(\d{1,3} 34% 42%\)$/);
    const hue = hashTitleToHue("Some Book");
    expect(hue).toBeGreaterThanOrEqual(0);
    expect(hue).toBeLessThan(360);
  });

  it("chooses one of the two palette inks for contrast", () => {
    const { color } = fallbackTileStyle("Any Title At All");
    expect(["#f7f3ec", "#241f1a"]).toContain(color);
  });

  it("handles empty and unicode titles deterministically", () => {
    expect(fallbackTileStyle("")).toEqual(fallbackTileStyle(""));
    expect(fallbackTileStyle("素晴らしい本")).toEqual(fallbackTileStyle("素晴らしい本"));
  });
});
