// A minimal, plain-language mapping from a `job:failed` event's bare `code`
// string to a family-safe sentence (design-system Section 5.1 "Scan failure").
//
// This is an INTERIM measure for the library home's scan affordance (T-15):
// the full `AppError`-family-to-surface mapping (`src/components/states/*`,
// e.g. `ScanFailure`) is Phase 7's brief (T-30). `job:failed` carries only the
// stable `code`, not the full structured `AppError` payload `formatAppError`
// expects, so this stays a small local dictionary rather than a duplicate of
// that later mapping; Phase 7 should route through its real state components
// instead of extending this file.
export function scanFailureMessage(code: string): string {
  switch (code) {
    case "root-not-found":
      return "Your library folder could not be found. Check that the drive is connected, then choose it again in Settings.";
    case "permission-denied":
      return "Some of your library could not be read because Windows denied access. Try again, or adjust the folder's permissions.";
    default:
      return "The scan stopped before it finished. Nothing was changed - a scan only reads.";
  }
}
