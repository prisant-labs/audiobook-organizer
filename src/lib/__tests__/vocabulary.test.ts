import { describe, expect, it } from "vitest";
import { STRINGS } from "@/lib/strings";
import { ERROR_COPY } from "@/lib/errorCopy";

// T-33 (v0.4.0 Phase 8, AC-37/AC-38): the copy sweep. FD-23 centralizes ALL
// user-facing copy in `strings.ts` (nav, screens) and `errorCopy.ts` (the
// AppError -> plain-language map), so a mechanical sweep of these two modules
// covers every rendered sentence in the app - there is no third place a
// user-facing string could hide. The exported HTML report (F-506, v0.3.0) is
// a separate Rust-generated artifact with its own `no_banned_vocabulary` test
// (crates/abo-core/src/plan/report.rs); this suite is this module's half of
// the same gate (design-system Section 6, standing rule 3).

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

const BANNED_PATTERN = new RegExp(`(${BANNED_WORDS.join("|")})`, "i");

/** Recursively collect every string leaf of a nested copy object (skips
 * functions like `review.moreOps`, which are copy TEMPLATES, not literal
 * copy - their own call sites are covered by the component tests that render
 * them). */
function collectStrings(value: unknown, path: string, out: Map<string, string>): void {
  if (typeof value === "string") {
    out.set(path, value);
  } else if (Array.isArray(value)) {
    value.forEach((v, i) => collectStrings(v, `${path}[${i}]`, out));
  } else if (value && typeof value === "object") {
    for (const [key, v] of Object.entries(value)) {
      collectStrings(v, path ? `${path}.${key}` : key, out);
    }
  }
  // functions, numbers, booleans: not literal copy, skip.
}

describe("copy sweep (T-33, AC-37/AC-38, design-system Section 6)", () => {
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
