import { describe, expect, it } from "vitest";
// The generated bindings, imported as raw source text (Vite `?raw`), so this
// test parses the ACTUAL AppError union it ships against - no node:fs, no
// hand-maintained mirror.
import bindingsSourceText from "@/lib/bindings.ts?raw";
import { ALL_ERROR_CODES, ERROR_COPY, copyForCode, copyForError } from "@/lib/errorCopy";
import type { AppError } from "@/lib/bindings";

// F-908 (AC-24) exhaustiveness guard. The real defence against "a future
// AppError variant ships with no family-safe copy" has two independent walls:
//   1. Compile-time: `ERROR_COPY` is `Record<AppErrorCode, ErrorCopy>` and
//      `AppErrorCode` is derived from the generated `AppError`, so a new code
//      breaks `tsc` until it has copy (see errorCopy.ts / appError.ts).
//   2. Runtime (this suite): the set of codes is parsed straight out of the
//      generated `bindings.ts` `AppError` union and cross-checked against the
//      copy table, so the test fails the moment bindings gains a code the
//      table doesn't cover - no hand-maintained list to forget.
//
// The parser is deliberately narrow: it reads ONLY the `AppError` type block,
// then takes structured codes from the `({ "the-code": {` member heads and the
// bare unit-variant strings on their own union line. Both patterns are unique
// to the taxonomy's members and never match the `"other"?: never` guards.

/** Slice out just the `export type AppError = ... ;` declaration. */
function appErrorBlock(source: string): string {
  const start = source.indexOf("export type AppError =");
  expect(start, "bindings.ts must declare `export type AppError`").toBeGreaterThanOrEqual(0);
  // The declaration ends at the next top-level `export` after it.
  const rest = source.slice(start + "export type AppError =".length);
  const end = rest.indexOf("\nexport ");
  expect(end, "AppError declaration must be followed by another export").toBeGreaterThan(0);
  return rest.slice(0, end);
}

function codesFromBindings(): string[] {
  const block = appErrorBlock(bindingsSourceText);
  const codes = new Set<string>();
  // Structured members: `({ "the-code": {`
  for (const m of block.matchAll(/\(\{\s*"([a-z0-9-]+)":/g)) {
    codes.add(m[1]);
  }
  // Unit variants: a bare quoted string as its own union member, e.g.
  // `\n"nothing-approved" | \n`. Excludes the `"x"?: never` guards (they carry
  // a `?` and never start a line as a lone union member).
  for (const m of block.matchAll(/^\s*"([a-z0-9-]+)"\s*[|;]/gm)) {
    codes.add(m[1]);
  }
  return [...codes].sort();
}

describe("errorCopy exhaustiveness (F-908 / AC-24)", () => {
  const codes = codesFromBindings();

  it("parses a plausible number of codes out of the AppError union", () => {
    // A sanity floor so a broken parser (0 codes) can't make the suite pass
    // vacuously; the exact count is asserted by the set-equality test below.
    expect(codes.length).toBeGreaterThanOrEqual(20);
    expect(codes).toContain("nothing-approved");
    expect(codes).toContain("scan-failed");
    expect(codes).toContain("db-corrupt-recovered");
  });

  it("the copy table covers EVERY AppError code and carries no extras", () => {
    const covered = [...ALL_ERROR_CODES].sort();
    // Three-way equality: bindings union == ALL_ERROR_CODES == ERROR_COPY keys.
    expect(covered).toEqual(codes);
    expect(Object.keys(ERROR_COPY).sort()).toEqual(codes);
  });

  // Table-driven: each code renders real, family-safe copy. This is what makes
  // the test fail when a future code is added without copy - the new code
  // parsed from bindings appears here and its assertions run.
  it.each(codesFromBindings())("code %s has complete, plain-language copy", (code) => {
    const copy = copyForCode(code);
    expect(copy.sentence.length, `${code} needs a family-facing sentence`).toBeGreaterThan(0);
    expect(copy.nextStep.length, `${code} needs a next step`).toBeGreaterThan(0);
    expect(["danger", "warn"]).toContain(copy.tone);
    expect(typeof copy.retryable).toBe("boolean");
    // Never leak the raw machine code or a bare OS-error shape as the sentence
    // (AC-24: never a bare OS error / code as the family-facing line).
    expect(copy.sentence).not.toContain(code);
    expect(copy.sentence).not.toMatch(/[A-Z]:\\/); // no raw Windows path
  });
});

describe("copyForError maps a structured AppError", () => {
  it("keys off the active tag of an externally-tagged AppError", () => {
    const err: AppError = { "scan-failed": { detail: "disk fell over" } } as AppError;
    expect(copyForError(err)).toBe(ERROR_COPY["scan-failed"]);
  });

  it("keys off a bare unit-variant string", () => {
    expect(copyForError("nothing-approved" as AppError)).toBe(ERROR_COPY["nothing-approved"]);
  });

  it("falls back to a generic family-safe surface for an unknown code", () => {
    const copy = copyForCode("some-code-that-does-not-exist");
    expect(copy.sentence).toBe("Something went wrong.");
    expect(copy.tone).toBe("danger");
  });
});
