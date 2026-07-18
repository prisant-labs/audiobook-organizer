import type { AppError } from "./bindings";
import { type AppErrorCode, appErrorCode } from "./appError";

// The single AppError -> plain-language mapping (v0.4.0 Phase 7, F-908, AC-24).
// Every code in the taxonomy gets: a family-facing SENTENCE (what happened, in
// the plain-language register of design-system Section 6 - no machine code, no
// raw OS error), a suggested NEXT STEP (calm, one action), a RETRYABLE flag
// (whether re-attempting the same action is a sensible remediation), and a
// TONE (`danger` = something broke, `warn` = a soft "needs a decision" state;
// design-system Section 2.3 reserves the `--danger` pair for genuine failure,
// `--warn` for held/needs-you). The raw technical detail is never shown as the
// sentence; F-908 surfaces tuck `formatAppError` text inside "Show file
// details" for tier 1 (FD-13).
//
// This table is EXHAUSTIVE by construction: it is typed `Record<AppErrorCode,
// ErrorCopy>`, and `AppErrorCode` is derived structurally from the generated
// `AppError` (see appError.ts). Add a variant to the Rust taxonomy, regenerate
// bindings, and this record fails to type-check until the new code has copy -
// a future code can never reach a family without a family-safe surface. The
// table-driven test (errorCopy.test.ts) also asserts this at runtime against
// the codes parsed straight out of bindings.ts, so the guarantee holds even
// where a type error alone would slip past.

export type ErrorTone = "danger" | "warn";

export interface ErrorCopy {
  /** What happened, in plain language (never a machine code or raw OS error). */
  sentence: string;
  /** The one calm next step (design-system "one primary action"). */
  nextStep: string;
  /** Whether re-attempting the SAME action is a sensible remediation. */
  retryable: boolean;
  /** `danger` (something broke) vs `warn` (a held / needs-a-decision state). */
  tone: ErrorTone;
}

export const ERROR_COPY: Record<AppErrorCode, ErrorCopy> = {
  // -- Startup / storage (the app's own records; never the audiobooks) -------
  "db-migration-failed": {
    sentence: "The app couldn't open its own records.",
    nextStep:
      "Restart the app. Your audiobooks are untouched - only the app's own notes are affected.",
    retryable: false,
    tone: "danger",
  },
  "db-corrupt-recovered": {
    sentence: "The app's memory of your last scan couldn't be opened, so it started fresh.",
    nextStep:
      "Scan your library again to rebuild it. Your audiobooks are untouched - this is only the app's own notes.",
    retryable: false,
    tone: "danger",
  },
  "settings-failed": {
    sentence: "The app couldn't read or save your settings.",
    nextStep:
      "Try again. If it keeps happening, restart the app - your audiobooks are untouched.",
    retryable: true,
    tone: "danger",
  },

  // -- Library root (F-909 re-pick) ------------------------------------------
  "root-not-found": {
    sentence: "Your library folder couldn't be found.",
    nextStep:
      "Check that the drive is connected, then choose your library folder again.",
    retryable: false,
    tone: "danger",
  },
  "root-not-directory": {
    sentence: "The library location isn't a folder.",
    nextStep: "Choose your library folder again - pick a folder, not a single file.",
    retryable: false,
    tone: "danger",
  },

  // -- Scan-time (a scan only reads) -----------------------------------------
  "permission-denied": {
    sentence: "Windows wouldn't let the app read part of your library.",
    nextStep: "Give the app permission to that folder, then try again.",
    retryable: true,
    tone: "danger",
  },
  "junction-skipped": {
    sentence: "A shortcut-style folder was skipped so the scan wouldn't loop back on itself.",
    nextStep: "Nothing to do - the rest of your library was read normally.",
    retryable: false,
    tone: "warn",
  },
  "scan-failed": {
    sentence: "The scan stopped before it finished.",
    nextStep: "Try the scan again. Nothing was changed - a scan only reads.",
    retryable: true,
    tone: "danger",
  },
  "csv-parse": {
    sentence: "That file list couldn't be read.",
    nextStep: "Check the file, or scan your library directly instead.",
    retryable: false,
    tone: "danger",
  },

  // -- Organizing choices (rulesets, shown in plain terms) -------------------
  "ruleset-invalid": {
    sentence: "Those organizing choices couldn't be saved.",
    nextStep: "Undo your last change and try again.",
    retryable: false,
    tone: "warn",
  },
  "ruleset-not-found": {
    sentence: "That set of organizing choices is no longer there.",
    nextStep: "Pick your organizing choices again in Settings.",
    retryable: false,
    tone: "warn",
  },
  "ruleset-in-use": {
    sentence: "These organizing choices are the ones in use, so they can't be removed.",
    nextStep: "Switch to a different set first, then remove this one.",
    retryable: false,
    tone: "warn",
  },
  "ruleset-operation-failed": {
    sentence: "Your organizing choices couldn't be saved.",
    nextStep: "Try again. If it keeps happening, restart the app.",
    retryable: true,
    tone: "danger",
  },

  // -- Plan build / validation (nothing is moved while planning) -------------
  "plan-generation-failed": {
    sentence: "The tidy-up plan couldn't be built.",
    nextStep:
      "Build the plan again. Nothing was changed - building a plan only reads your library.",
    retryable: true,
    tone: "danger",
  },
  "plan-not-found": {
    sentence: "That tidy-up plan is no longer available.",
    nextStep: "Build the plan again from your library.",
    retryable: false,
    tone: "danger",
  },
  "snapshot-stale": {
    sentence: "Your library changed since this plan was made.",
    nextStep: "Re-check now so the plan stays accurate.",
    retryable: true,
    tone: "warn",
  },
  "nothing-approved": {
    sentence: "Nothing is turned on to tidy up.",
    nextStep: "Turn on at least one group, then try again.",
    retryable: false,
    tone: "warn",
  },

  // -- Structural conflicts a plain retry can't fix (leave out / adjust) -----
  "collision-in-plan": {
    sentence: "Two changes would end up with the same name.",
    nextStep: "Leave one of them out, or adjust your organizing choices.",
    retryable: false,
    tone: "warn",
  },
  "collision-on-disk": {
    sentence: "A change would land where something already exists.",
    nextStep: "Leave it out, or move the existing item first.",
    retryable: false,
    tone: "warn",
  },
  "path-too-long": {
    sentence: "The new location's name would be too long for Windows.",
    nextStep: "Shorten a folder name in your organizing choices, or leave this one out.",
    retryable: false,
    tone: "warn",
  },
  "illegal-component": {
    sentence: "The new name would use characters Windows doesn't allow.",
    nextStep: "Leave this one out - the app won't create a name Windows can't use.",
    retryable: false,
    tone: "warn",
  },
  "reserved-name": {
    sentence: "The new name matches a name Windows reserves.",
    nextStep: "Leave this one out, or rename it in your organizing choices.",
    retryable: false,
    tone: "warn",
  },
  "cross-volume-space-insufficient": {
    sentence: "There isn't enough room on the drive for these changes.",
    nextStep: "Free up space on the drive, then try again.",
    retryable: true,
    tone: "danger",
  },
  "cycle-detected": {
    sentence: "A change would move a folder inside itself.",
    nextStep: "Leave this one out - the app won't make a folder that contains itself.",
    retryable: false,
    tone: "warn",
  },

  // -- Applying changes (v0.5.0; making changes for real is not on yet) -------
  "apply-not-supported": {
    sentence: "Making changes for real isn't available in this version yet.",
    nextStep:
      "You can preview what a tidy-up would do. Making changes for real arrives in a later version.",
    retryable: false,
    tone: "warn",
  },
  "apply-failed": {
    sentence: "The app couldn't record this tidy-up run.",
    nextStep: "Try again. If it keeps happening, restart the app - your audiobooks are untouched.",
    retryable: true,
    tone: "danger",
  },
  "journal-write-failed": {
    sentence: "The app stopped before making any change because it couldn't first note what it was about to do.",
    nextStep: "Nothing was moved. Try again. If it keeps happening, restart the app - your audiobooks are untouched.",
    retryable: true,
    tone: "danger",
  },

  // -- Making changes for real (v0.5.0 executor: safety-first, never overwrite) --
  "job-already-running": {
    sentence: "A tidy-up is already in progress.",
    nextStep: "Wait for it to finish, then start the next one. Only one tidy-up runs at a time.",
    retryable: false,
    tone: "warn",
  },
  "source-vanished": {
    sentence: "Something this tidy-up was going to change is no longer where it was.",
    nextStep: "Scan your library again to refresh it, then review and run the tidy-up once more.",
    retryable: true,
    tone: "warn",
  },
  "target-appeared": {
    sentence: "Something already exists where a change would go, so it was left alone.",
    nextStep:
      "Nothing was overwritten. Scan your library again, then review and run the tidy-up once more.",
    retryable: true,
    tone: "warn",
  },
  "copy-verify-mismatch": {
    sentence: "A file copied to another drive didn't match the original, so the change was stopped.",
    nextStep:
      "Your original file is safe and untouched. Check the other drive for errors, then try the tidy-up again.",
    retryable: true,
    tone: "danger",
  },
  "access-denied": {
    sentence: "Windows wouldn't let the app change an item, even after a second try.",
    nextStep:
      "Close any program using that file or folder, or give the app permission to it, then try again.",
    retryable: true,
    tone: "danger",
  },

  // -- Undoing a tidy-up (v0.5.0: putting things back the way they were) -------
  "rollback-not-reversible": {
    sentence: "That tidy-up was a practice run, so nothing actually moved.",
    nextStep:
      "There's nothing to undo. Run a real tidy-up first if you want changes you can put back.",
    retryable: false,
    tone: "warn",
  },
  "rollback-selection-not-contiguous": {
    sentence: "An undo can only take back the most recent changes, in order.",
    nextStep:
      "Choose an unbroken run of the latest changes to undo, without skipping any in the middle.",
    retryable: false,
    tone: "warn",
  },
  "rollback-prepare-failed": {
    sentence: "The undo couldn't be prepared.",
    nextStep:
      "The undo file may be missing or the tidy-up it refers to may be gone. Scan your library again, then build and review a fresh plan instead.",
    retryable: true,
    tone: "danger",
  },
};

// The runtime list of every code, mirroring the `AppError` union. A compile-
// time guard (below) fails if this drifts from `AppErrorCode` in either
// direction, so the table-driven test can iterate a real array while the type
// system keeps that array honest against the generated bindings.
export const ALL_ERROR_CODES = Object.keys(ERROR_COPY) as AppErrorCode[];

// Compile-time exhaustiveness: this assignment fails if any `AppErrorCode` is
// missing from `ALL_ERROR_CODES` (a new code with no copy), and each element
// being an `AppErrorCode` already forbids extras. Kept as a typed const so
// `tsc --noEmit` (the CI/local typecheck gate) is the second wall behind the
// runtime test.
const _EXHAUSTIVE: readonly AppErrorCode[] = ALL_ERROR_CODES;
type _AllCovered = AppErrorCode extends (typeof ALL_ERROR_CODES)[number] ? true : never;
const _ALL_COVERED: _AllCovered = true;
void _EXHAUSTIVE;
void _ALL_COVERED;

// A generic, family-safe fallback for the (defensive) case of a code with no
// entry - never expected for a real `AppError`, but keeps `copyForCode` total
// so an unexpected string from an event payload still renders a calm surface
// rather than crashing or leaking a raw value.
export const GENERIC_ERROR_COPY: ErrorCopy = {
  sentence: "Something went wrong.",
  nextStep:
    "You can try again. If it keeps happening, restart the app - your audiobooks are untouched.",
  retryable: true,
  tone: "danger",
};

/** Family-safe copy for a machine code string (e.g. a `job:failed` payload). */
export function copyForCode(code: string): ErrorCopy {
  return ERROR_COPY[code as AppErrorCode] ?? GENERIC_ERROR_COPY;
}

/** Family-safe copy for a structured `AppError`. */
export function copyForError(error: AppError): ErrorCopy {
  return copyForCode(appErrorCode(error));
}
