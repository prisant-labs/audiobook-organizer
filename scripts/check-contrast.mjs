#!/usr/bin/env node
// Token-contrast script (v0.4.0 Phase 8, T-35, AC-40; design-system Section 8,
// FD-21). Parses the REAL token values straight out of src/styles/tokens.css
// (no hand-copied hex table to drift out of sync) and computes the WCAG AA
// (>= 4.5:1) contrast ratio for every informational token pair this app
// actually renders, in both Day and Evening. Exits non-zero and prints the
// failing pairs if any ratio is under the threshold; wired into `pnpm test`
// (package.json), which is what ci.yml's `test` job runs (ci-plan.md Section 5
// gate registry: "Contrast token check ... ci.yml test (script in pnpm test)").
//
// Pair selection (the "full matrix" design-system Section 8 promises beyond
// its own load-bearing table): every fg/bg combination actually used in a
// component today (grep'd against src/components and src/routes for
// text-<token> beside bg-<token> classes), plus the two token pairs the
// design system defines but no component consumes yet (`--danger-ink` on
// `--danger`, sanctioned for a future destructive-confirm button per
// design-system Section 2.3). `--ink-3` is deliberately checked ONLY against
// the backgrounds the design system's own Section 8 table sanctions it for
// (`--bg` in both themes, `--surface-2` in Day, `--surface` in Evening) - this
// IS the mechanical form of the "--ink-3 restricted to decorative use"
// rule: ink-3 is simply never checked against any background the design
// system has not already computed as passing, so a future component putting
// ink-3 information-bearing text on a new background must add that pair here
// (and it will fail loudly if it does not clear 4.5:1) rather than silently
// riding on an unchecked assumption.
//
// Alpha compositing: `--good-bg`/`--warn-bg`/`--alert-bg`/`--danger-bg` are
// semi-transparent in Evening (rgba with alpha < 1). A semi-transparent
// background's effective color depends on what sits behind it, and these
// chips render on ordinary cards (`--surface`) as well as directly on the
// page (`--bg`) depending on which shelf/list they are in (BookSlot vs.
// OpRow/GroupCard). This script composites over BOTH ambient backgrounds and
// keeps the WORSE (lower) ratio, so the gate holds regardless of where a chip
// ends up - a real-world-conservative check, not merely a token-math one.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TOKENS_PATH = path.join(__dirname, "..", "src", "styles", "tokens.css");
const AA_BODY_TEXT_RATIO = 4.5;

// ---- CSS token parsing -----------------------------------------------------

function extractBlock(css, selectorPattern) {
  const match = css.match(selectorPattern);
  if (!match) {
    throw new Error(`tokens.css: could not find a block for ${selectorPattern}`);
  }
  const start = match.index + match[0].length;
  const end = css.indexOf("\n}", start);
  if (end === -1) throw new Error(`tokens.css: unterminated block for ${selectorPattern}`);
  return css.slice(start, end);
}

function parseTokens(block) {
  const tokens = {};
  // `--name: value;` - value may itself contain parens (rgba(...), linear-gradient(...)).
  const re = /--([a-z0-9-]+):\s*([^;]+);/gi;
  let m;
  while ((m = re.exec(block))) {
    tokens[m[1]] = m[2].trim();
  }
  return tokens;
}

/** Parse a `#rrggbb` or `rgba(r, g, b, a)` token value into {r,g,b,a} (0-255, 0-1). */
function parseColor(value) {
  const hex = value.match(/^#([0-9a-f]{6})$/i);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a: 1 };
  }
  const rgba = value.match(
    /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/i,
  );
  if (rgba) {
    return {
      r: Number(rgba[1]),
      g: Number(rgba[2]),
      b: Number(rgba[3]),
      a: rgba[4] === undefined ? 1 : Number(rgba[4]),
    };
  }
  throw new Error(`Not a solid or rgba color token: ${value}`);
}

/** Composite a (possibly transparent) color over an opaque base. */
function compositeOver(fg, base) {
  if (fg.a >= 1) return fg;
  return {
    r: fg.a * fg.r + (1 - fg.a) * base.r,
    g: fg.a * fg.g + (1 - fg.a) * base.g,
    b: fg.a * fg.b + (1 - fg.a) * base.b,
    a: 1,
  };
}

// ---- WCAG relative luminance / contrast ratio ------------------------------

function srgbChannel(c) {
  const v = c / 255;
  return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance({ r, g, b }) {
  return 0.2126 * srgbChannel(r) + 0.7152 * srgbChannel(g) + 0.0722 * srgbChannel(b);
}

function contrastRatio(c1, c2) {
  const l1 = relativeLuminance(c1);
  const l2 = relativeLuminance(c2);
  const [lighter, darker] = l1 >= l2 ? [l1, l2] : [l2, l1];
  return (lighter + 0.05) / (darker + 0.05);
}

// ---- the checked pairs ------------------------------------------------------
// [fgToken, bgToken, label]. `bgToken` may be a token name or the special
// ambient markers below, resolved per-theme against the parsed token map.

const AMBIENT_BACKGROUNDS = ["bg", "surface"]; // chips render on the page or on a card

const PAIRS = [
  // Body text on the two ordinary surfaces (design-system Section 8 table).
  ["ink", "bg", "primary body text on the page"],
  ["ink", "surface", "primary body text on a card"],
  ["ink-2", "bg", "secondary body text on the page"],
  ["ink-2", "surface", "secondary body text on a card"],

  // --ink-3: checked ONLY against the backgrounds Section 8 sanctions (see
  // header comment). Do not add another background here without also
  // computing it passes - that is the whole point of the restriction.
  ["ink-3", "bg", "tertiary text on the page (Section 8 sanctioned)"],
  ["ink-3", "surface-2", "tertiary text on a sunken surface (Section 8 sanctioned, Day)"],
  ["ink-3", "surface", "tertiary text on a card (Section 8 sanctioned, Evening)"],

  // Buttons.
  ["primary-ink", "primary", "primary button text"],
  ["primary-ink", "primary-hover", "primary button text, hovered"],

  // Links (FileDetails summary, OpRow exclude action, PlanFilter clear link,
  // ErrorCallout disclosure summary).
  ["link", "bg", "link text on the page"],
  ["link", "surface", "link text on a card"],

  // Status tone text, standalone (RulesetEditor/ErrorCallout/Settings render
  // --danger/--warn/--good text directly on the ambient surface, no chip).
  ["danger", "bg", "danger text on the page"],
  ["danger", "surface", "danger text on a card"],
  ["warn", "bg", "warn text on the page"],
  ["warn", "surface", "warn text on a card"],
  ["good", "bg", "good text on the page"],
  ["good", "surface", "good text on a card"],

  // Status tone chips: text-<tone> on bg-<tone>-bg (BookSlot reason chip,
  // GroupCard status tag, OpRow warning/blocked pill). The *-bg tokens are
  // semi-transparent in Evening; composited over both ambient backgrounds.
  ["warn", "warn-bg", "warn chip (BookSlot .why, GroupCard status tag)"],
  ["alert", "alert-bg", "alert chip (BookSlot .why structural)"],
  ["good", "good-bg", "good chip (GroupCard included tag)"],
  ["danger", "danger-bg", "danger chip (OpRow blocked pill)"],

  // Defined by design-system Section 2.3 but not yet consumed by a component
  // (a future destructive-confirm button): checked anyway so the token pair
  // can never silently regress before its first use.
  ["danger-ink", "danger", "danger-ink solid button text (reserved, Section 2.3)"],
];

// ---- run --------------------------------------------------------------------

function run() {
  const css = readFileSync(TOKENS_PATH, "utf8");
  const dayTokens = parseTokens(extractBlock(css, /:root\[data-theme="day"\]\s*\{/));
  const eveningTokens = parseTokens(extractBlock(css, /:root\[data-theme="evening"\]\s*\{/));

  const themes = [
    { name: "Day", tokens: dayTokens },
    { name: "Evening", tokens: eveningTokens },
  ];

  /** @type {{theme:string,label:string,fg:string,bg:string,ratio:number}[]} */
  const rows = [];
  const failures = [];

  for (const { name, tokens } of themes) {
    for (const [fgToken, bgToken, label] of PAIRS) {
      const fgValue = tokens[fgToken];
      if (!fgValue) throw new Error(`Unknown token --${fgToken} in ${name} theme`);
      const fg = parseColor(fgValue);

      const bgIsAmbientCandidate = AMBIENT_BACKGROUNDS.includes(bgToken) === false;
      const bgValue = tokens[bgToken];
      if (!bgValue) throw new Error(`Unknown token --${bgToken} in ${name} theme`);
      const bgRaw = parseColor(bgValue);

      if (bgRaw.a >= 1 || !bgIsAmbientCandidate) {
        // Opaque background, or the pair IS one of the ambient backgrounds
        // itself (bg/surface are always opaque in this token set) - a single
        // ratio suffices.
        const ratio = contrastRatio(fg, bgRaw);
        rows.push({ theme: name, label, fg: fgToken, bg: bgToken, ratio });
        if (ratio < AA_BODY_TEXT_RATIO) failures.push({ theme: name, label, fg: fgToken, bg: bgToken, ratio });
        continue;
      }

      // Semi-transparent background: composite over every ambient surface
      // and keep the worst (lowest) ratio (see header comment).
      let worst = Infinity;
      for (const ambientName of AMBIENT_BACKGROUNDS) {
        const ambient = parseColor(tokens[ambientName]);
        const composited = compositeOver(bgRaw, ambient);
        const ratio = contrastRatio(fg, composited);
        worst = Math.min(worst, ratio);
      }
      rows.push({ theme: name, label, fg: fgToken, bg: bgToken, ratio: worst });
      if (worst < AA_BODY_TEXT_RATIO) failures.push({ theme: name, label, fg: fgToken, bg: bgToken, ratio: worst });
    }
  }

  const width = Math.max(...rows.map((r) => r.label.length));
  console.log(`Token-contrast check (WCAG AA, >= ${AA_BODY_TEXT_RATIO}:1)\n`);
  for (const row of rows) {
    const pass = row.ratio >= AA_BODY_TEXT_RATIO;
    console.log(
      `  [${pass ? "PASS" : "FAIL"}] ${row.theme.padEnd(8)} ${row.label.padEnd(width)}  --${row.fg} on --${row.bg}  ${row.ratio.toFixed(2)}:1`,
    );
  }

  if (failures.length > 0) {
    console.error(`\n${failures.length} pair(s) failed WCAG AA (>= ${AA_BODY_TEXT_RATIO}:1):`);
    for (const f of failures) {
      console.error(`  --${f.fg} on --${f.bg} (${f.theme}, ${f.label}): ${f.ratio.toFixed(2)}:1`);
    }
    process.exitCode = 1;
    return;
  }

  console.log(`\nOK: all ${rows.length} token pairs (${themes.length} themes) pass WCAG AA.`);
}

run();
