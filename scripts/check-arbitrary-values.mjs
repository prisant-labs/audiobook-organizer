#!/usr/bin/env node
/**
 * Ratchet: the app's Tailwind arbitrary values must never grow in number.
 *
 * WHY THIS EXISTS. The design system governs colour exactly (hex tokens in
 * tokens.css) and copy mechanically (vocabulary.test.ts), and neither drifts. It
 * states type as RANGES ("h1 26-30px") and says nothing at all about spacing, and
 * both have drifted badly: the app ships 71 distinct arbitrary values across 285
 * uses, including text-[6.8px], text-[10.5px], text-[12.5px], text-[13.5px] and
 * text-[14.5px]. A system produces exactly the consistency it enforces, and this
 * dimension was enforced by nothing.
 *
 * WHAT THIS IS NOT. It is not the fix. The fix is a real spacing and type scale in
 * tokens.css under @theme, so the Tailwind utilities the app already uses BECOME
 * the scale. That scale is not ratified yet. This gate is the cheap half that
 * needs no design decision: it freezes the sprawl where it is, today, so the
 * problem stops growing while the scale is decided. Every number here is expected
 * to fall.
 *
 * WHY IT FAILS ON IMPROVEMENT TOO. A ratchet that only fails upward is not a
 * ratchet: the baseline goes stale, someone removes ten values, and the slack is
 * silently available for ten new ones. Failing in both directions costs one
 * number edit per improvement and buys two things: the baseline can never lie,
 * and every improvement shows up in its own diff as a number going down.
 *
 * WHAT COUNTS, precisely. A Tailwind arbitrary VALUE on a utility: text-[13px],
 * max-w-[52ch], hover:bg-[#c4262e], lg:grid-cols-[420px_1fr]. These are design
 * decisions taken inline, one class at a time, which is how 71 of them accumulated
 * without anyone choosing 71 of anything.
 *
 * WHAT DELIBERATELY DOES NOT COUNT, and why the distinction is load-bearing:
 *
 *   1. Arbitrary VARIANTS: data-[state=open]:, [&>svg]:, supports-[display:grid]:,
 *      max-[600px]:. These are selector logic, not design values. shadcn/ui
 *      generates them heavily, and the agreed order is scale first THEN shadcn, so
 *      a gate that taxed shadcn adoption would punish the next step for the sins
 *      of the last one. The discriminator is mechanical rather than a hardcoded
 *      prefix list: a variant is always followed by a colon, a value never is.
 *      This matters because the prefix alone is ambiguous, max-[600px]: is a media
 *      query while max-w-[52ch] is a width.
 *   2. Arbitrary PROPERTIES: [text-wrap:balance]. These reach a CSS feature that
 *      has no Tailwind utility at all, so there is no scale step they could be
 *      spending instead. Five uses, two distinct, both legitimate.
 *
 * WHAT THIS DOES NOT GUARANTEE, stated so nobody over-trusts it. It counts
 * arbitrary values only. It has no opinion on whether p-4 or p-6 was the right
 * choice, which is the larger half of the consistency problem and needs the scale
 * plus a review surface, not a regex. A component can be perfectly free of
 * arbitrary values and still be inconsistent with every other component.
 *
 * ON THE BASELINE NUMBER. A previous session recorded 254 uses and 58 distinct
 * using a different, unrecorded pattern. This file measures 285 and 71 because its
 * pattern also catches variant-prefixed utilities (hover:bg-[#c4262e],
 * lg:grid-cols-[420px_1fr]) that a simpler regex misses. The number that governs
 * is the one this script produces, because this script is what runs.
 *
 * Run locally:  node scripts/check-arbitrary-values.mjs
 * See them all: node scripts/check-arbitrary-values.mjs --report
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

/**
 * The frozen sprawl, measured on 2026-08-14 against main at 62684bf.
 *
 * BOTH numbers are ratcheted, because each one alone has an obvious hole. Uses
 * alone would let forty new copies of an existing bad value in. Distinct alone
 * would let a value be swapped for a different one at no cost. Together they say:
 * no new inline design decisions, and no wider spread of the ones already made.
 *
 * LOWER THESE when a change removes arbitrary values. That edit is the point.
 */
const BASELINE_USES = 285;
const BASELINE_DISTINCT = 71;

/**
 * A utility arbitrary value, NOT an arbitrary variant.
 *
 * Anatomy, matching `lg:hover:max-w-[52ch]`:
 *   (?:[a-z0-9-]+:)*   variant chain, lg: and hover:
 *   [a-z][a-z0-9-]*-   the utility, max-w-
 *   \[([^\]\s]+)\]     the value, [52ch]. No whitespace: Tailwind requires
 *                      underscores for spaces, so this also stops a runaway
 *                      match from eating across lines.
 *   (?!:)              NOT followed by a colon, which is what makes this a value
 *                      rather than a variant. This single lookahead replaces a
 *                      hardcoded list of variant prefixes and cannot fall out of
 *                      date as Tailwind adds more.
 *
 * The leading boundary keeps `record[key]` and `[status, setStatus]` out: both are
 * ordinary JavaScript and neither is preceded by a utility ending in a hyphen.
 *
 * KNOWN LIMIT: an arbitrary value used as an object KEY, `{ "text-[13px]": cond }`,
 * is followed by a colon and so reads as a variant and is skipped. No such key
 * exists in this tree. If clsx-style conditional objects with arbitrary keys ever
 * appear, this pattern needs revisiting rather than trusting.
 */
const ARBITRARY_VALUE = /(?:^|[\s"'`{(])((?:[a-z0-9-]+:)*[a-z][a-z0-9-]*-)\[([^\]\s]+)\](?!:)/g;

/** Tracked app source. Tests are excluded: a test asserting on a class string is
 *  mirroring a decision made in a component, not making one. */
function appSourceFiles() {
  const out = execFileSync("git", ["ls-files", "-z", "--", "src"], {
    maxBuffer: 64 * 1024 * 1024,
  });
  return out
    .toString("utf8")
    .split("\0")
    .filter(Boolean)
    .filter((f) => f.endsWith(".tsx") || f.endsWith(".ts"))
    .filter((f) => !f.includes("__tests__") && !f.includes(".test."));
}

const wantReport = process.argv.includes("--report");

const files = appSourceFiles();

// A glob that matches nothing is a broken gate reporting a clean tree, which is
// the worst failure mode available to a check. It is an error, never a pass.
if (files.length === 0) {
  console.error(
    "::error::the arbitrary-value ratchet found no source files to scan, so it cannot pass",
  );
  process.exit(1);
}

/** value -> [{ file, line }], so a failure can say exactly where. */
const occurrences = new Map();
const unreadable = [];
let uses = 0;

for (const file of files) {
  let text;
  try {
    text = readFileSync(file, "utf8");
  } catch (err) {
    // Never swallow this. An unreadable file is an unknown, and an unknown must
    // not be reported as a pass.
    unreadable.push(`${file}: ${err.message}`);
    continue;
  }

  text.split(/\r?\n/).forEach((line, i) => {
    for (const m of line.matchAll(ARBITRARY_VALUE)) {
      const value = `${m[1]}[${m[2]}]`;
      if (!occurrences.has(value)) occurrences.set(value, []);
      occurrences.get(value).push({ file, line: i + 1 });
      uses += 1;
    }
  });
}

if (unreadable.length > 0) {
  console.error(
    "::error::the arbitrary-value ratchet could not read some source files, so it cannot pass",
  );
  unreadable.forEach((u) => console.error(`  ${u}`));
  process.exit(1);
}

const distinct = occurrences.size;

if (wantReport) {
  const sorted = [...occurrences.entries()].sort(
    (a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]),
  );
  console.log(`${uses} uses of ${distinct} distinct arbitrary values in ${files.length} files:\n`);
  for (const [value, hits] of sorted) {
    console.log(`  ${String(hits.length).padStart(3)}  ${value}`);
    for (const h of hits.slice(0, 4)) console.log(`         ${h.file}:${h.line}`);
    if (hits.length > 4) console.log(`         ... and ${hits.length - 4} more`);
  }
  console.log("");
}

const grew = uses > BASELINE_USES || distinct > BASELINE_DISTINCT;
const shrank = uses < BASELINE_USES || distinct < BASELINE_DISTINCT;

if (grew) {
  console.error(
    `::error::arbitrary Tailwind values grew: ${uses} uses of ${distinct} distinct ` +
      `(baseline ${BASELINE_USES} uses of ${BASELINE_DISTINCT} distinct). ` +
      `Use an existing Tailwind step instead of inventing a value. If no step fits, ` +
      `that is a design decision and belongs in the scale in src/styles/tokens.css, ` +
      `not inline in a className. Run 'node scripts/check-arbitrary-values.mjs --report' ` +
      `to see every one and where it lives.`,
  );
  process.exit(1);
}

if (shrank) {
  console.error(
    `::error::arbitrary Tailwind values FELL to ${uses} uses of ${distinct} distinct ` +
      `(baseline ${BASELINE_USES} uses of ${BASELINE_DISTINCT} distinct). ` +
      `This is good news and still fails: lower BASELINE_USES to ${uses} and ` +
      `BASELINE_DISTINCT to ${distinct} in scripts/check-arbitrary-values.mjs so the ` +
      `ratchet holds the new ground. A baseline nobody lowers is slack nobody sees.`,
  );
  process.exit(1);
}

console.log(
  `OK: ${uses} uses of ${distinct} distinct arbitrary Tailwind values across ` +
    `${files.length} app source files, exactly at the baseline. Expected to fall, never to rise.`,
);
