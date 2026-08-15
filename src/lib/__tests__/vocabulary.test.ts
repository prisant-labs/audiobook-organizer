import { describe, expect, it } from "vitest";
import { STRINGS } from "@/lib/strings";
import { ERROR_COPY } from "@/lib/errorCopy";

// T-33 (v0.4.0 Phase 8, AC-37/AC-38): the copy sweep over `strings.ts` (nav,
// screens) and `errorCopy.ts` (the AppError -> plain-language map).
//
// SCOPE, stated honestly. This file used to claim that sweeping these two modules
// "covers every rendered sentence in the app - there is no third place a
// user-facing string could hide". That was false, and believing it cost real
// defects: an adversarial review found FD-47's retired "shelf" still live in TWO
// Rust-side surfaces that render in this app and are swept by nothing here.
//
// User-facing text is produced in at least four places, each gated separately:
//
//   1. strings.ts / errorCopy.ts        -> this file
//   2. the exported HTML report          -> report.rs `no_banned_vocabulary`
//   3. AppError::remediation             -> error.rs `no_remediation_carries_retired_vocabulary`
//   4. campaign group headlines/reasons  -> query.rs `no_group_copy_carries_retired_vocabulary`
//      (plus stored rationale sentences from builder.rs, which reach the review
//      surface through the plan itself)
//
// The lesson behind that list: these gates are keyed on WHICH FILE text lives in,
// when the property that matters is whether the text REACHES A USER. Adding a
// fifth producer would silently be ungated again. Anyone adding one should add its
// sweep in the same change.

// Design-system Section 6.1 "Forbidden on primary surfaces" list, plus the
// "Not this" column terms from the vocabulary map. Word-boundary matched,
// case-insensitive, so "operations" and "batches" are caught without also
// flagging unrelated words that happen to share a substring.
const BANNED_WORDS = [
  "operations?",
  "\\bops\\b",
  "dedupe",
  "manifest",
  "quarantine",
  "dashboard",
  "batch(?:es)?",
] as const;

/**
 * Words RETIRED by a ledger decision, kept deliberately separate from
 * `BANNED_WORDS` above.
 *
 * The distinction is worth preserving: `BANNED_WORDS` are engineering terms that
 * were never allowed on a user-facing surface, while these were the *approved*
 * word until a decision replaced them. A reader hitting a failure needs to know
 * which kind they have, because the fix differs: banned jargon is rewritten,
 * a retired word is swapped for its successor.
 *
 * This list is the half of the vocabulary contract a machine can check. Its
 * existence is why FD-46 and FD-47 could sit unimplemented for a day: the guard
 * only knew forbidden words and had no opinion about retired ones.
 *
 * NOT retired: "copies". FD-46 renamed the GROUP to "Duplicates" but kept
 * "copies" for the members inside one, which reads naturally ("this book has
 * four copies, and they are duplicates of each other"). Banning it would be
 * wrong.
 */
const RETIRED_WORDS = [
  // FD-42 (2026-08-05): the product term is "Archive"; "quarantine" stays
  // internal-only and is already covered by BANNED_WORDS above.
  // NOT just the adjacent form. `\bset[- ]aside\b` missed "Set it aside" and
  // "sets the copy aside", both of which were LIVE in shipped copy when an
  // adversarial review found them: the first rendered in the ruleset editor, the
  // second in a stored rationale on the review surface. This matches
  // set/sets/setting followed by "aside" within a short window, verified against
  // the retired phrasings and against safe strings like "Set the theme to
  // Evening", "a settings page" and "Reset the filter".
  "\\bset(?:s|ting)?\\b[^.!?]{0,24}\\baside\\b",
  // FD-47 (2026-08-06): the word for where books live is "library". The word
  // boundary matters: it must NOT fire on "Audiobookshelf", the product this
  // app complements.
  //
  // "ved" and "ving" were added by FD-48's sweep: the original pattern knew only
  // the NOUN forms, so "34 series shelved together" was live in the shipped
  // good-news line and swept by nothing. A retirement covers a word family, not
  // one inflection of it.
  "\\bshel(?:f|ves|ved|ving)\\b",
  // FD-48 (2026-08-14, superseding FD-43): the action is "organize". Every
  // inflection is retired, not just the bare verb, because "tidy-ups", "already
  // tidy" and "carry on tidying up" were all live in shipped copy and a pattern
  // matching only "tidy up" would have left the majority of them standing.
  //
  // "tidy-up" needs no separate alternative: a hyphen is a word boundary, so
  // \btidy\b already matches the first half of it.
  //
  // NOT retired: the ENGINEERING identifiers. `needs_tidy_books`,
  // `already_tidy_books`, `blocks_further_tidying` and the `tidying-blocked`
  // error code are IPC and schema names that no user reads, and renaming them is
  // a migration rather than a copy change. This sweep covers STRINGS and
  // ERROR_COPY values, so those identifiers are out of its reach by
  // construction, which is the correct division: the vocabulary rule governs
  // surfaces a user reads, never the names the engine calls things internally.
  "\\btid(?:y|ies|ied|ying)\\b",
] as const;

const BANNED_PATTERN = new RegExp(`(${[...BANNED_WORDS, ...RETIRED_WORDS].join("|")})`, "i");

/**
 * Argument tuples used to render copy TEMPLATES so their output can be swept.
 *
 * Templates take either counts or labels, and several call `.toLocaleString()`
 * on their argument, which throws for a string. So there is no single probe that
 * renders all of them: we try each tuple and keep every result that renders.
 *
 * `1` and `2` exist as separate probes deliberately. Several templates branch on
 * singular versus plural (`n === 1 ? "1 book moved" : ...`), and probing only one
 * side would sweep only one of the two sentences a reader can actually see.
 */
const TEMPLATE_PROBES: readonly unknown[] = [1, 2, "Dune"];

/**
 * Recursively collect every user-visible string in a copy object.
 *
 * Includes copy TEMPLATES (function-valued entries such as
 * `review.moreOps`) by rendering them. This closes a real hole: the sweep
 * previously skipped functions on the grounds that "their own call sites are
 * covered by the component tests that render them", which is a weaker guarantee
 * than a mechanical sweep and depends on a component test both rendering the
 * template AND asserting on its text. Four templates were carrying retired
 * vocabulary that no sweep could see (FD-42's "set aside" and FD-47's "shelf").
 *
 * A template that renders under no probe is reported to `unrenderable` rather
 * than dropped. Silently skipping what cannot be swept is exactly the failure
 * this function exists to prevent.
 */
function collectStrings(
  value: unknown,
  path: string,
  out: Map<string, string>,
  unrenderable: string[] = [],
): void {
  if (typeof value === "string") {
    out.set(path, value);
  } else if (typeof value === "function") {
    const fn = value as (...args: unknown[]) => unknown;
    const arity = Math.max(fn.length, 1);
    let rendered = 0;
    for (const probe of TEMPLATE_PROBES) {
      try {
        const result = fn(...(Array.from({ length: arity }, () => probe) as unknown[]));
        if (typeof result === "string") {
          out.set(`${path}(${String(probe)})`, result);
          rendered += 1;
        }
      } catch {
        // This probe's type is wrong for this template (e.g. a string passed to
        // a template that calls .toLocaleString()). Another probe may still work.
      }
    }
    if (rendered === 0) unrenderable.push(path);
  } else if (Array.isArray(value)) {
    value.forEach((v, i) => collectStrings(v, `${path}[${i}]`, out, unrenderable));
  } else if (value && typeof value === "object") {
    for (const [key, v] of Object.entries(value)) {
      collectStrings(v, path ? `${path}.${key}` : key, out, unrenderable);
    }
  }
  // numbers, booleans, null: not copy, skip.
}

describe("copy sweep (T-33, AC-37/AC-38, design-system Section 6)", () => {
  // The sweep is only as good as its coverage, so prove the coverage first.
  // These two tests guard the COLLECTOR; the ones after them use it.
  it("renders every copy template, leaving none unswept", () => {
    const strings = new Map<string, string>();
    const unrenderable: string[] = [];
    collectStrings(STRINGS, "", strings, unrenderable);

    expect(
      unrenderable,
      `these copy templates rendered under no probe, so nothing sweeps them. ` +
        `Add a probe to TEMPLATE_PROBES that matches their argument types: ` +
        `${JSON.stringify(unrenderable)}`,
    ).toEqual([]);

    // A template's rendered output is keyed `path(probe)`, so their presence is
    // observable rather than assumed.
    const rendered = [...strings.keys()].filter((k) => k.includes("("));
    expect(rendered.length, "no copy templates were rendered at all").toBeGreaterThan(0);
  });

  it("catches a banned word hidden inside a copy template", () => {
    // The regression test for the hole this collector was changed to close.
    // Before, a function-valued entry was skipped outright, so a banned word
    // inside one passed the sweep. This fixture fails if that ever regresses.
    // `innocent` must contain NO banned or retired word. If it did, this test
    // would pass on that string alone and would keep passing even if templates
    // were skipped again, which is the exact regression it exists to catch.
    const fixture = {
      innocent: "Books and duplicates",
      sneaky: (label: string) => `Running dedupe on ${label}.`,
    };
    const strings = new Map<string, string>();
    collectStrings(fixture, "", strings);

    const offenders = [...strings.entries()].filter(([, text]) => BANNED_PATTERN.test(text));
    expect(
      offenders.length,
      "a banned word inside a copy template escaped the sweep",
    ).toBeGreaterThan(0);
  });

  // FD-47's pattern is the one most likely to be made wrong by a later edit:
  // "Audiobookshelf" is the product this app complements, it appears throughout
  // the docs, and a pattern without word boundaries would flag it forever. This
  // pins both directions so a well-meaning simplification to /shelf/i fails here
  // rather than in someone's PR description.
  it("flags a retired word without flagging Audiobookshelf", () => {
    expect(BANNED_PATTERN.test("Nothing on your shelves was touched")).toBe(true);
    expect(BANNED_PATTERN.test("ready for its new shelf")).toBe(true);
    expect(BANNED_PATTERN.test("Set aside 3 copies")).toBe(true);
    expect(BANNED_PATTERN.test("the set-aside folder")).toBe(true);

    expect(BANNED_PATTERN.test("imports cleanly into Audiobookshelf")).toBe(false);
    expect(BANNED_PATTERN.test("your Audiobookshelf library")).toBe(false);
    // FD-46 kept "copies" for the members of a group; only the group was renamed.
    expect(BANNED_PATTERN.test("this book has four copies")).toBe(false);
  });

  // FD-48 (superseding FD-43) retired the whole "tidy" family for "organize",
  // and closed an FD-47 gap in the same sweep. Each inflection is pinned
  // separately: the bug being prevented is a pattern that knows only the base
  // form, which is exactly how "shelved" and "already tidy" stayed live under a
  // guard that was passing.
  it("flags every retired inflection, not just the base word", () => {
    // FD-48: the verb, the noun, and the participles.
    expect(BANNED_PATTERN.test("Tidy up now")).toBe(true);
    expect(BANNED_PATTERN.test("No tidy-ups yet")).toBe(true);
    expect(BANNED_PATTERN.test("Your library is already tidy")).toBe(true);
    expect(BANNED_PATTERN.test("Carry on tidying up")).toBe(true);
    expect(BANNED_PATTERN.test("the books have been tidied")).toBe(true);
    expect(BANNED_PATTERN.test("Tidying up your books")).toBe(true);

    // FD-47's gap: the verb form the original noun-only pattern missed.
    expect(BANNED_PATTERN.test("34 series shelved together")).toBe(true);

    // The replacements must pass, or the sweep would forbid its own successor.
    expect(BANNED_PATTERN.test("Organize now")).toBe(false);
    expect(BANNED_PATTERN.test("Organizing your books")).toBe(false);
    expect(BANNED_PATTERN.test("Nothing organized yet")).toBe(false);
    expect(BANNED_PATTERN.test("34 series kept together")).toBe(false);
    expect(BANNED_PATTERN.test("Real run")).toBe(false);
  });

  it("STRINGS carries no Section 6.1 banned vocabulary", () => {
    const strings = new Map<string, string>();
    collectStrings(STRINGS, "", strings);
    expect(strings.size).toBeGreaterThan(0);

    const offenders = [...strings.entries()].filter(([, text]) => BANNED_PATTERN.test(text));
    expect(offenders, `banned vocabulary found in strings.ts: ${JSON.stringify(offenders)}`).toEqual(
      [],
    );
  });

  it("ERROR_COPY (the AppError plain-language map) carries no banned vocabulary", () => {
    const strings = new Map<string, string>();
    collectStrings(ERROR_COPY, "", strings);
    expect(strings.size).toBeGreaterThan(0);

    const offenders = [...strings.entries()].filter(([, text]) => BANNED_PATTERN.test(text));
    expect(
      offenders,
      `banned vocabulary found in errorCopy.ts: ${JSON.stringify(offenders)}`,
    ).toEqual([]);
  });

  // AC-38: the FD-10 deletion-guarantee sentence is used verbatim, not
  // paraphrased, wherever the full guarantee enumeration appears.
  it("carries the FD-10 deletion-guarantee sentence verbatim", () => {
    expect(STRINGS.library.reassurance).toBe(
      "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.",
    );
  });

  // AC-37: the removed prototype line never reappears.
  it("never claims genre folders become tags", () => {
    const strings = new Map<string, string>();
    collectStrings(STRINGS, "", strings);
    const genreClaim = /old genre view lives on as tags/i;
    const offenders = [...strings.values()].filter((text) => genreClaim.test(text));
    expect(offenders).toEqual([]);
  });
});
