// Deterministic fallback-tile color for F-907 (FD-03, design-system Section 4.5).
//
// AC-23: when a book has no cover, the fallback tile's background is derived
// deterministically from a hash of the title, so the SAME title always yields
// the SAME tile. A shelf of all-fallback tiles then stays stable across scans
// and reads as a warm bookshelf, not a broken grid. This module is pure and
// dependency-free, so it is trivially unit-testable (the determinism test the
// plan's Phase 3 verification calls for).
//
// Only the HUE varies with the title; saturation and lightness are fixed in a
// muted, warm-library range so no title ever produces a harsh or washed-out
// tile. The plan names the seam `hashTitleToHsl`; the component consumes
// `fallbackTileStyle`, which pairs the tint with a legible ink chosen by the
// tint's own luminance (so the title text stays readable on every hue).

// Fixed tint saturation/lightness (percent). Muted and mid-toned so the tile is
// calm and the title text is legible on top (design-system 4.5).
const TILE_SATURATION = 34;
const TILE_LIGHTNESS = 42;

// A near-white and a near-black ink drawn from the warm palette, chosen per tile
// by the tint's luminance so the title always has adequate contrast.
const LIGHT_INK = "#f7f3ec";
const DARK_INK = "#241f1a";

// FNV-1a (32-bit) over the title's UTF-16 code units. Deterministic, stable
// across runs and platforms, and dependency-free. `Math.imul` keeps the prime
// multiply inside 32 bits; the final `>>> 0` yields an unsigned result.
export function hashTitle(title: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < title.length; i++) {
    hash ^= title.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

// The deterministic tint hue (0-359) for a title.
export function hashTitleToHue(title: string): number {
  return hashTitle(title) % 360;
}

// The deterministic tile tint as a CSS `hsl()` string (the plan's
// `hashTitleToHsl` seam). Same title in, same string out.
export function hashTitleToHsl(title: string): string {
  return `hsl(${hashTitleToHue(title)} ${TILE_SATURATION}% ${TILE_LIGHTNESS}%)`;
}

// Convert HSL (h in [0,360), s/l in [0,1]) to RGB in [0,255]. Used only to pick
// the tile's ink by luminance; the rendered tint itself is emitted as `hsl()`.
function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = h / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let rgb: [number, number, number];
  if (hp < 1) rgb = [c, x, 0];
  else if (hp < 2) rgb = [x, c, 0];
  else if (hp < 3) rgb = [0, c, x];
  else if (hp < 4) rgb = [0, x, c];
  else if (hp < 5) rgb = [x, 0, c];
  else rgb = [c, 0, x];
  const m = l - c / 2;
  return [
    Math.round((rgb[0] + m) * 255),
    Math.round((rgb[1] + m) * 255),
    Math.round((rgb[2] + m) * 255),
  ];
}

// WCAG relative luminance of an sRGB color in [0,1].
function relativeLuminance([r, g, b]: [number, number, number]): number {
  const lin = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export interface FallbackTileStyle {
  // The tile background (a deterministic `hsl()` tint of the title).
  background: string;
  // A legible ink for the title text on that tint.
  color: string;
}

// The full deterministic style for a title's fallback tile: the tint plus a
// title ink chosen by the tint's luminance so the text is readable on every hue.
// Deterministic (same title, same style), which is what AC-23 requires.
export function fallbackTileStyle(title: string): FallbackTileStyle {
  const hue = hashTitleToHue(title);
  const rgb = hslToRgb(hue, TILE_SATURATION / 100, TILE_LIGHTNESS / 100);
  const ink = relativeLuminance(rgb) > 0.4 ? DARK_INK : LIGHT_INK;
  return {
    background: `hsl(${hue} ${TILE_SATURATION}% ${TILE_LIGHTNESS}%)`,
    color: ink,
  };
}
