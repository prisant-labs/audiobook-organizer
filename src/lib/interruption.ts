import type { commands } from "@/lib/bindings";

// The pure half of the interruption recovery surface (v0.6.0 P1c, F-606
// interruption safety): the shape the backend reports and the one decision the
// frontend makes about it.
//
// Split out of InterruptionNotice.tsx because a module that exports both a
// component and a function defeats fast refresh (react-refresh
// /only-export-components). Keeping the classification here also puts it beside
// the other pure view logic in lib/ (planFilter, overview) rather than inside a
// component, so the hook can type against it without importing a component.

/**
 * The non-null result of `startup_interruption`, derived from the generated
 * bindings rather than restated, so a change to `ReconcileResult` breaks this
 * at compile time instead of drifting silently.
 */
export type StartupInterruption = NonNullable<
  Awaited<ReturnType<typeof commands.startupInterruption>>
>;

export type InterruptionState = "practice-run" | "stopped-decisive" | "stopped-ambiguous";

/**
 * Which story an interrupted run tells.
 *
 * This is a reading of the engine's answer, not a second opinion on it.
 * `resume_offered` is set by the reconciler from the verified on-disk outcome,
 * and it is the only thing that decides whether carrying on is safe.
 *
 * A rehearsal is always the practice-run state whatever else the result says:
 * its effects lived in a `MemFs` that died with the process, so there is
 * nothing on disk to carry on from and nothing to put back.
 */
export function interruptionStateOf(i: StartupInterruption): InterruptionState {
  if (i.mode === "dry-run") return "practice-run";
  return i.resume_offered ? "stopped-decisive" : "stopped-ambiguous";
}
