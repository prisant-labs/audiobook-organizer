// The proposed spacing and type scale, as data.
//
// WHY THIS FILE IS DATA AND NOT CSS. `src/styles/tokens.css` is a tracked
// normative file: writing steps into it is a ratification, not a proposal, and
// inventing values there was caught and stopped once already. This file holds the
// proposal so it can be RENDERED and reacted to. If the scale is ratified, the
// steps below are transcribed into `@theme` in tokens.css and this file is
// deleted along with its gallery section. If it is not, deleting it costs
// nothing, because nothing outside the gallery imports it.
//
// HOW THE NUMBERS WERE PRODUCED, so they can be checked rather than believed:
//
//   node scripts/check-arbitrary-values.mjs --report
//     -> every inline value, its count and its call sites. The type figures
//        below are its text-size rows summed; its measure and box-size rows are
//        named out of scope at the bottom of this file.
//   The standard-utility figures came from a one-off scan of the same tracked
//     .ts/.tsx files for non-arbitrary spacing, size, leading and weight
//     utilities.
//
// Measured 2026-08-19 against main at 0c09c05.
//
// SCOPE: PRODUCT SOURCE, NOT THIS GALLERY. The standard-utility counts exclude
// `src/gallery`, because the gallery is a dev tool that never ships and its own
// furniture is not a product surface. That exclusion is not cosmetic: the only
// two uses of the 20px and 24px standard steps in this tree are this page's own
// headings, and counting them would have put a step in the scale that no product
// screen asks for. The ratchet scopes differently, all of src, and should: it is
// preventing sprawl in source rather than describing the product. The two agree
// where it matters, since `src/gallery` contains none of the 285 inline values.
//
// WHAT THE MEASUREMENT CHANGED ABOUT THE BRIEF. The plan was to derive both
// scales from the app's 71 inline values. That is right for TYPE, where 161 of
// those uses are sizes and the standard utilities are almost unused. It is wrong
// for SPACING: only 5 inline uses are spacing, while 316 uses of 17 standard
// steps carry the real decisions. A spacing scale derived from the inline values
// alone would have been derived from a sample of three numbers.

/** One size the app ships today, and how it is reached. */
export interface Fold {
  px: number;
  /** Uses via an inline size, the kind the ratchet counts. */
  inline: number;
  /** Uses via a standard utility step. */
  utility: number;
}

export interface TypeStep {
  /** Token suffix on ratification, and the name to argue about now. */
  name: string;
  px: number;
  /** Line box in px. Every one lands on the 4px grid; see GRID_RULE. */
  linePx: number;
  /** Letter spacing in em. Zero for everything below the title steps. */
  tracking: number;
  /** What this step is for, in one line, taken from where its sizes ship now. */
  role: string;
  /** Real app copy at this step, so a step is judged as text and not as a number. */
  sample: string;
  /** Render the sample serif, matching how the role ships today. */
  serif?: boolean;
  /** Render the sample semibold, matching how the role ships today. */
  strong?: boolean;
  folds: readonly Fold[];
}

export interface SpacingFold {
  px: number;
  /** The standard step that produces it. */
  tw: number;
  uses: number;
}

export interface SpacingStep {
  name: string;
  px: number;
  /** The standard step that already produces this value, so adopting it renames nothing. */
  tw: number;
  role: string;
  folds: readonly SpacingFold[];
}

/** The one rule the type scale is derived from, stated so it can be attacked. */
export const GRID_RULE =
  "Every line box is a multiple of 4px, so type and spacing compose instead of " +
  "fighting. The app's own 1.55 line height comes to 20.15px on 13px text, a " +
  "sixth of a pixel off the 20px this rule produces, which is where the rule came " +
  "from rather than a coincidence it can claim credit for.";

export const DERIVATION = [
  "Whole pixels only. A half-pixel step invites a neighbouring half-pixel step, " +
    "which is how this app came to ship both 12.5 and 13.",
  "No two steps closer than 2px. The disease being cured is 12.5 against 13 " +
    "being used as though they were different steps.",
  "Anchored on 13px, the app's most-used size at 42 inline uses, so the change is " +
    "smallest where the app is densest.",
  "The ratio holds near 1.18 across the ladder, and every step is a size the app " +
    "already ships or a whole pixel between two it ships.",
  "A size lands on the nearest step. Where it sits exactly between two, it goes " +
    "up, because on a desktop app a slightly larger word is a smaller mistake " +
    "than a slightly smaller hit target.",
] as const;

export const MEASURED = {
  at: "main 0c09c05, 2026-08-19",
  type: {
    inlineUses: 161,
    inlineDistinct: 18,
    utilityUses: 5,
    utilityDistinct: 1,
    distinctSizes: 18,
  },
  spacing: { inlineUses: 5, inlineDistinct: 3, utilityUses: 292, utilityDistinct: 16 },
  leading: { inlineUses: 3, inlineDistinct: 1, utilityUses: 44, utilityDistinct: 3 },
  weight: { utilityUses: 72, utilityDistinct: 2 },
} as const;

export const TYPE_STEPS: readonly TypeStep[] = [
  {
    name: "meta",
    px: 11,
    linePx: 16,
    tracking: 0,
    role: "Counts, sizes, paths and timestamps. The line under a card that says how much.",
    sample: "12 changes / 1.4 GB / last run 3 days ago",
    folds: [
      { px: 10.5, inline: 8, utility: 0 },
      { px: 11.5, inline: 17, utility: 0 },
    ],
  },
  {
    name: "body",
    px: 13,
    linePx: 20,
    tracking: 0,
    role: "The sentence a user actually reads, inside a card, a row or a form.",
    sample:
      "These four copies are identical. The app keeps the one already in the right place and archives the rest.",
    folds: [
      { px: 12, inline: 11, utility: 0 },
      { px: 12.5, inline: 38, utility: 0 },
      { px: 13, inline: 42, utility: 0 },
      { px: 13.5, inline: 8, utility: 0 },
    ],
  },
  {
    name: "lead",
    px: 15,
    linePx: 24,
    tracking: 0,
    role: "A card headline, or the one line of explanation under a screen title.",
    sample: "Four copies of Dune, in three folders",
    strong: true,
    folds: [
      { px: 14, inline: 15, utility: 5 },
      { px: 14.5, inline: 3, utility: 0 },
      { px: 15, inline: 1, utility: 0 },
      { px: 16, inline: 1, utility: 0 },
    ],
  },
  {
    name: "heading",
    px: 18,
    linePx: 24,
    tracking: 0,
    role: "A heading inside a screen, above a group of rows.",
    sample: "What this run changed",
    serif: true,
    folds: [
      { px: 18, inline: 1, utility: 0 },
      { px: 19, inline: 1, utility: 0 },
    ],
  },
  {
    name: "title",
    px: 22,
    linePx: 28,
    tracking: -0.01,
    role: "A callout, or a screen that is mostly one message.",
    sample: "We could not reach that folder",
    serif: true,
    folds: [
      { px: 20, inline: 2, utility: 0 },
      { px: 22, inline: 2, utility: 0 },
    ],
  },
  {
    name: "display",
    px: 26,
    linePx: 32,
    tracking: -0.01,
    role: "The heading on a full-screen state: empty, building, interrupted.",
    sample: "Nothing to review yet",
    serif: true,
    folds: [{ px: 26, inline: 7, utility: 0 }],
  },
  {
    name: "hero",
    px: 30,
    linePx: 36,
    tracking: -0.01,
    role: "The one h1 a screen is allowed. Library, and the first-run welcome.",
    sample: "Your library",
    serif: true,
    folds: [{ px: 30, inline: 2, utility: 0 }],
  },
];

/**
 * Sizes deliberately left OUT of the scale, with the reason.
 *
 * Both are text inside simulated book art rather than interface text: each is
 * scaled to the drawing it sits in, and putting them on the interface ladder
 * would either blow up the artwork or add two steps nothing else can use.
 */
export const TYPE_EXCEPTIONS = [
  { px: 6.8, uses: 1, where: "SpineCluster, the rotated label on a drawn book spine" },
  { px: 9, uses: 1, where: "FallbackTile, the title on a drawn stand-in cover" },
] as const;

/**
 * The spacing scale.
 *
 * THE CHEAP HALF. Every step below is a standard utility step the app already
 * uses, so ratifying this adds no token and renames nothing: it is a rule about
 * WHICH of the 17 steps in the tree are allowed, and the other 10 fold into a
 * neighbour. That is why the type half is the real work and this half is a
 * decision plus, later, a lint rule.
 *
 * Folds are to the nearest step, ties upward, because on a desktop app growing a
 * gap is safer than shrinking a hit target.
 */
export const SPACING_STEPS: readonly SpacingStep[] = [
  {
    name: "nudge",
    px: 2,
    tw: 0.5,
    role: "Optical alignment only, never layout. Nine of its sixteen uses are a top margin lining a small icon up with a line of text.",
    folds: [{ px: 2, tw: 0.5, uses: 16 }],
  },
  {
    name: "tight",
    px: 4,
    tw: 1,
    role: "Between two things that are really one thing: an icon and its label.",
    folds: [{ px: 4, tw: 1, uses: 29 }],
  },
  {
    name: "snug",
    px: 8,
    tw: 2,
    role: "The workhorse. Padding inside a small control, gap between rows in a list.",
    folds: [
      { px: 6, tw: 1.5, uses: 25 },
      { px: 8, tw: 2, uses: 65 },
    ],
  },
  {
    name: "base",
    px: 12,
    tw: 3,
    role: "Between the parts of one card, and the padding of a dense one.",
    folds: [
      { px: 10, tw: 2.5, uses: 28 },
      { px: 12, tw: 3, uses: 37 },
    ],
  },
  {
    name: "roomy",
    px: 16,
    tw: 4,
    role: "Card padding, and the gap between cards.",
    folds: [
      { px: 14, tw: 3.5, uses: 7 },
      { px: 16, tw: 4, uses: 41 },
    ],
  },
  {
    name: "section",
    px: 24,
    tw: 6,
    role: "Between two groups of content on the same screen.",
    folds: [
      { px: 20, tw: 5, uses: 15 },
      { px: 24, tw: 6, uses: 13 },
    ],
  },
  {
    name: "screen",
    px: 32,
    tw: 8,
    role: "The screen's own breathing room, and the space under a hero.",
    folds: [
      { px: 28, tw: 7, uses: 2 },
      { px: 32, tw: 8, uses: 5 },
      { px: 36, tw: 9, uses: 2 },
      { px: 40, tw: 10, uses: 5 },
      { px: 44, tw: 11, uses: 1 },
      { px: 64, tw: 16, uses: 1 },
    ],
  },
];

/**
 * Adjacent axes this proposal does NOT cover, named so the boundary is visible
 * now rather than discovered later.
 */
export const OUT_OF_SCOPE = [
  "Measure: 7 distinct character widths across 38 uses. A real axis, and a separate decision.",
  "Icon and box sizes: 19 distinct pixel sizes hard-coded on width and height.",
  "Radius: three inline values against one token. Small, and it belongs with the box work.",
  "Which step to use where. This scale says what the steps ARE. A component using a legal step badly is still wrong, and no regex can see it.",
] as const;
