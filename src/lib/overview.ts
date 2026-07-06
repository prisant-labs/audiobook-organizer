import { commands, type LibraryOverview } from "./bindings";
import { formatAppError } from "./appError";

// Frontend client for the F-902 library home (T-15, v0.4.0 Phase 4). Wraps the
// generated `classify_overview` binding and unwraps its `Result` shape.
//
// `null` is a normal, expected outcome (no scan has ever completed yet - the
// honest pre-first-scan state, AC-6) and is returned as `null`, not thrown. An
// actual `AppError` (a rare database read failure) is surfaced as a throwable
// `OverviewError` so `useHealthMetrics` can distinguish "nothing to show yet"
// from "something went wrong reading it".

export type { LibraryOverview };

export class OverviewError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "OverviewError";
  }
}

export async function getLibraryOverview(): Promise<LibraryOverview | null> {
  const result = await commands.classifyOverview();
  if (result.status === "error") {
    throw new OverviewError(formatAppError(result.error));
  }
  return result.data;
}
