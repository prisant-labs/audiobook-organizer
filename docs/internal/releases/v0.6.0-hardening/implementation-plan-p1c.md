---
title: "Implementation plan: P1c, the interruption recovery surface"
type: implementation-plan
release: v0.6.0-hardening
phase: P1c
feature: F-606
date: 2026-08-04
status: ready
owner: jprisant
design: design-p1c-interruption-surface.md
satisfies: AC-6, AC-7 (v0.6.0 hardening, AC-7 as amended by FD-39)
---

# P1c interruption recovery surface: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a killed tidy-up visible to the user, by building the surface that renders what the startup reconciler already worked out and never says.

**Architecture:** Frontend only. A hook reads `startup_interruption` once on mount and pairs it with the matching `history_list` row, which is where the engine has already resolved what can be done about the run. A presentational component renders one of three states from that pair. `AppShell` shows it in the main screen area ahead of the Apply screen, leaving the sidebar live. No backend command, no migration, no schema change.

**Tech stack:** React 19, TypeScript, Tailwind v4 with CSS custom-property tokens, Vitest with Testing Library, axe-core, lucide-react icons.

## Global constraints

Copied verbatim from `CLAUDE.md`, `PRODUCT.md`, and the design system. Every task's requirements implicitly include this section.

- **No em-dashes (U+2014) or en-dashes (U+2013) anywhere**: code, comments, copy, commit messages. Use " - ", a comma, a colon, or a sentence break. Numeric ranges use plain hyphens. A PreToolUse hook enforces this on writes.
- **Every reference ID carries its handle on first use per section**: "AC-6 (v0.6.0 hardening, the resume-or-rollback offer)", never a bare "AC-6". Acceptance-criteria numbers restart per release, so always name the release.
- **Plain-language register on every user-facing surface**: books, shelves, copies, tidy up, set aside, undo. Never operations, ops, dedupe, manifest, journal, quarantine, dashboard, dry run, reconcile.
- **All user-facing copy lives in `src/lib/strings.ts`** (`FD-23`, English-only v1 with one strings module). No string literals in components.
- **Branch-first**: this work happens on `feat/v0.6.0-p1c-resume-surface`. Never commit to `main`.
- **Agents do not self-merge.** `D-11`'s allowance was scoped to the private repo and lapsed with `FD-38` (the public flip). Open the PR and stop.
- **`READ-ONLY` absolute against `E:\Books - Audio`.** Nothing in this plan touches a real library; every test uses mocked bindings.
- **Conventional commit prefixes**: `feat:`, `fix:`, `docs:`, `chore:`.
- **Do not change `AppShell.tsx:107`'s dry-run pin.** Enabling real applies is a separate human-gated decision.
- **The package manager is pnpm, not npm.** `pnpm-lock.yaml` is the lockfile and CI runs `pnpm install --frozen-lockfile`. Every command in this plan is `pnpm`; running `npm install` would generate a competing lockfile and break the CI cache.
- **Typed IPC only** (`FD-29`, enforced by the `no-raw-invoke` ESLint rule): the frontend calls the tauri-specta generated bindings in `src/lib/bindings.ts` and never imports `invoke` from `@tauri-apps/api`, statically or dynamically. Everything in this plan goes through `commands.*`, so this holds by construction; it is stated because `pnpm lint` fails the build if it is ever broken.

## File structure

| File | Status | Responsibility |
|---|---|---|
| `src/lib/format.ts` | modify | Gains `formatWhen`, moved out of `History.tsx` so two surfaces share one date format |
| `src/lib/strings.ts` | modify | Gains `STRINGS.interruption` |
| `src/components/states/InterruptionNotice.tsx` | **create** | The presentational surface. Props in, callbacks out, no IPC |
| `src/hooks/useStartupInterruption.ts` | **create** | The one place that talks to the backend for this feature |
| `src/routes/History.tsx` | modify | Imports `formatWhen` instead of defining it |
| `src/components/shell/AppShell.tsx` | modify | Renders the notice ahead of `activeJob` |
| `docs/internal/design-system.md` | modify | Section 5 gains this state family |
| `src/components/states/__tests__/InterruptionNotice.test.tsx` | **create** | Three states, tone, action visibility |
| `src/hooks/__tests__/useStartupInterruption.test.tsx` | **create** | Five load paths |
| `src/components/shell/__tests__/AppShellInterruption.test.tsx` | **create** | Precedence, dismissal, sidebar stays |
| `src/__tests__/a11y.test.tsx` | modify | axe smoke on all three states |

The component and the hook are split so the surface stays directly renderable in a test with hand-built data, which is how every other state component in this codebase is tested.

---

## Task 1: Shared date formatting

Small and first, because Task 2's component needs it and `History.tsx` currently owns a copy that is not exported.

**Files:**
- Modify: `src/lib/format.ts`
- Modify: `src/routes/History.tsx:239-247` (delete the local `formatWhen`), and its import block at the top
- Test: `src/lib/__tests__/format.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: `export function formatWhen(iso: string): string` in `src/lib/format.ts`. Returns a locale-formatted date such as "4 Aug 2026", or the empty string when `iso` does not parse.

- [ ] **Step 1: Write the failing test**

`src/lib/__tests__/format.test.ts` already exists and covers `formatBytes`. Add `formatWhen` to its existing import from `../format`, then append this describe block:

```ts
describe("formatWhen", () => {
  it("formats an ISO timestamp as a readable date", () => {
    expect(formatWhen("2026-08-04T00:18:15Z")).not.toBe("");
    expect(formatWhen("2026-08-04T00:18:15Z")).toMatch(/2026/);
  });

  it("returns an empty string for an unparseable value rather than 'Invalid Date'", () => {
    expect(formatWhen("not a date")).toBe("");
    expect(formatWhen("")).toBe("");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm vitest run src/lib/__tests__/format.test.ts`
Expected: FAIL, `formatWhen` is not exported from `../format`.

- [ ] **Step 3: Move the implementation**

Append to `src/lib/format.ts`:

```ts
/**
 * A past timestamp as a family-readable date (History, and the interruption
 * surface's details disclosure). Returns "" rather than "Invalid Date" for an
 * unparseable value, so a bad row degrades to a blank instead of shouting.
 */
export function formatWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
```

Then in `src/routes/History.tsx`, delete the local `formatWhen` function (lines 239 to 247) and add it to the existing import from `@/lib/format` if there is one, or add this import beside the other `@/lib` imports:

```ts
import { formatWhen } from "@/lib/format";
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm vitest run src/lib/__tests__/format.test.ts src/routes/__tests__/History.test.tsx`
Expected: PASS on both. The History tests must still pass unchanged, which is the proof the move was behaviour-preserving.

- [ ] **Step 5: Commit**

```bash
git add src/lib/format.ts src/lib/__tests__/format.test.ts src/routes/History.tsx
git commit -m "refactor: move formatWhen into lib/format so two surfaces share it"
```

---

## Task 2: The copy

Copy lands before the component so the component has no string literals to inline and no temptation to invent wording mid-build.

**Files:**
- Modify: `src/lib/strings.ts`

**Interfaces:**
- Produces: `STRINGS.interruption`, with the exact keys listed below. Task 3 consumes every one of them.

- [ ] **Step 1: Add the copy block**

Insert into `src/lib/strings.ts` immediately after the `history: { ... },` block, keeping the surrounding comment style:

```ts
  // The interruption recovery surface (v0.6.0 P1c, F-606 interruption safety).
  // Shown when a prior session was killed mid-tidy-up and the startup
  // reconciler repaired the record before the app opened.
  //
  // Copy register notes. "Stopped early" rather than "crashed" or "aborted";
  // "practice run" never "dry run", matching STRINGS.history. The engine words
  // this surface is closest to - reconcile, journal, operation - never appear.
  // Order is deliberate: the heading says what happened, the next line says
  // what is safe, so a reader who stops after one sentence still learns the
  // thing that matters most.
  interruption: {
    // State 1: an interrupted practice run. The real shelves were never touched.
    practiceHeading: "The practice run stopped early",
    practiceBody:
      "Your computer stopped before the practice run finished. Nothing on your shelves was touched: a practice run only shows you what would happen.",
    practiceAction: "Back to your library",

    // State 2: a real tidy-up stopped early, and the app confirmed where it got to.
    stoppedHeading: "The tidy-up stopped early",
    stoppedBody:
      "Your computer stopped partway through. Nothing was left half-done, and every book that was moved is safe.",
    booksMoved: (n: number) => (n === 1 ? "1 book was moved" : `${n} books were moved`),
    nothingMoved: "No books had been moved yet.",
    carryOn: "Carry on tidying up",
    carryOnNote:
      "The app takes a fresh look at your library first, so books that were already tidied stay where they are.",

    // State 3: a real tidy-up stopped early and one step could not be confirmed.
    ambiguousHeading: "The tidy-up stopped early, and one book needs a look",
    ambiguousBody:
      "The app could not tell for certain what happened to one book when it stopped, so it will not carry on by itself.",
    openHistory: "Open History",

    // Shared details disclosure (FD-13): no paths, no ids, nothing raw.
    showDetails: "Show details",
    detailStarted: (when: string) => `Started: ${when}`,
    detailChanges: (n: number) =>
      n === 1 ? "Recorded: 1 change" : `Recorded: ${n} changes`,
    detailLastStepChecked: "The last step was checked and the record was put right.",
    detailNothingInDoubt: "No step was left in doubt.",
  },
```

- [ ] **Step 2: Run the mechanical vocabulary gate**

Run: `pnpm vitest run src/lib/__tests__/vocabulary.test.ts`
Expected: PASS.

This is not a formality. `vocabulary.test.ts` walks every string leaf of `STRINGS` and fails on the design-system Section 6.1 banned list: `operations`, `ops`, `dedupe`, `manifest`, `quarantine`, `dashboard`, `batch`. It is the mechanical half of the plain-language rule, and this surface sits closer to the engine's vocabulary than any other, so it is the one most likely to trip it. Note the sweep skips function-valued entries, because those are templates rather than literal copy, so `booksMoved`, `detailStarted`, and `detailChanges` are covered by the component tests in Task 3 instead.

- [ ] **Step 3: Verify the module still typechecks and no dashes crept in**

Run: `pnpm typecheck`
Expected: PASS, no errors.

Run: `node -e "const s=require('fs').readFileSync('src/lib/strings.ts','utf8'); process.exit(/[\u2013\u2014]/.test(s)?1:0)"`
Expected: exit 0. A non-zero exit means an em-dash or en-dash is present and must be replaced with " - ", a comma, or a colon.

- [ ] **Step 4: Commit**

```bash
git add src/lib/strings.ts
git commit -m "feat: copy for the interruption recovery surface"
```

---

## Task 3: The `InterruptionNotice` component

**Files:**
- Create: `src/components/states/InterruptionNotice.tsx`
- Test: `src/components/states/__tests__/InterruptionNotice.test.tsx`

**Interfaces:**
- Consumes: `STRINGS.interruption` and `STRINGS.history` from Task 2, `formatWhen` from Task 1, and `HistoryEntry` / `UndoOffer` from `@/lib/bindings`.
- Produces:
  - `export type StartupInterruption` - the non-null result of `commands.startupInterruption()`, derived from the generated bindings so it cannot drift.
  - `export type InterruptionState = "practice-run" | "stopped-decisive" | "stopped-ambiguous"`
  - `export function interruptionStateOf(i: StartupInterruption): InterruptionState`
  - `export function InterruptionNotice(props: InterruptionNoticeProps)` where `InterruptionNoticeProps` is `{ interruption: StartupInterruption; entry: HistoryEntry | null; preparing: boolean; onGoToLibrary: () => void; onUndo: () => void; onOpenHistory: () => void }`

Task 4 (the hook) re-exports `StartupInterruption` for convenience; Task 5 (`AppShell`) imports `InterruptionNotice` and passes all six props.

Two design points the implementer must not quietly change:

1. **The component never decides what can be undone.** It reads `entry.undo`, which the engine resolved. This is `FD-36`'s rule (the undo offer is resolved in the engine, not derived in the shell) and it is why `entry` is a prop rather than something the component fetches.
2. **"Carry on tidying up" navigates to the library; it does not start a scan.** Auto-starting work from a recovery screen is the kind of surprise this product exists to avoid, and the library screen is where the tidy-up action already lives.

- [ ] **Step 1: Write the failing tests**

Create `src/components/states/__tests__/InterruptionNotice.test.tsx`:

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  InterruptionNotice,
  interruptionStateOf,
  type StartupInterruption,
} from "../InterruptionNotice";
import type { HistoryEntry } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";

afterEach(cleanup);

const S = STRINGS.interruption;
const H = STRINGS.history;

function interruption(over: Partial<StartupInterruption> = {}): StartupInterruption {
  return {
    job_id: 14,
    mode: "real",
    interrupted: true,
    outcome: "completed",
    in_doubt_op_id: 142,
    resume_offered: true,
    done_count: 142,
    ...over,
  };
}

function entry(over: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    jobId: 14,
    mode: "real",
    state: "failed",
    startedAt: "2026-08-04T00:18:15Z",
    finishedAt: null,
    changesMade: 142,
    undo: { kind: "put-recent-changes-back", op_ids: [140, 141, 142] },
    ...over,
  };
}

function renderNotice(props: Partial<Parameters<typeof InterruptionNotice>[0]> = {}) {
  const handlers = {
    onGoToLibrary: vi.fn(),
    onUndo: vi.fn(),
    onOpenHistory: vi.fn(),
  };
  render(
    <InterruptionNotice
      interruption={interruption()}
      entry={entry()}
      preparing={false}
      {...handlers}
      {...props}
    />,
  );
  return handlers;
}

// The three states are a state machine over ReconcileResult, not three
// designs. `resume_offered` is the engine's own answer to "is carrying on
// safe", so the component reads it rather than re-deriving one.
describe("interruptionStateOf", () => {
  it("classifies a rehearsal as the practice-run state whatever else it says", () => {
    expect(interruptionStateOf(interruption({ mode: "dry-run", resume_offered: true }))).toBe(
      "practice-run",
    );
  });

  it("classifies a real run with resume offered as decisive", () => {
    expect(interruptionStateOf(interruption({ resume_offered: true }))).toBe("stopped-decisive");
  });

  it("classifies a real run without resume offered as ambiguous", () => {
    expect(interruptionStateOf(interruption({ resume_offered: false }))).toBe("stopped-ambiguous");
  });
});

describe("InterruptionNotice, practice run stopped early", () => {
  it("says nothing on the shelves was touched and offers only the way back", async () => {
    const h = renderNotice({
      interruption: interruption({ mode: "dry-run", resume_offered: false, done_count: 0 }),
      entry: entry({ mode: "dry-run", undo: { kind: "practice-run" } }),
    });

    expect(screen.getByText(S.practiceHeading)).toBeInTheDocument();
    expect(screen.getByText(S.practiceBody)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: S.carryOn })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: H.putRecentChangesBack })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: S.practiceAction }));
    expect(h.onGoToLibrary).toHaveBeenCalledOnce();
  });
});

describe("InterruptionNotice, real run stopped early with a decisive outcome", () => {
  it("offers both carrying on and putting the changes back", async () => {
    const h = renderNotice();

    expect(screen.getByText(S.stoppedHeading)).toBeInTheDocument();
    expect(screen.getByText(S.booksMoved(142))).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: S.carryOn }));
    expect(h.onGoToLibrary).toHaveBeenCalledOnce();

    await userEvent.click(screen.getByRole("button", { name: H.putRecentChangesBack }));
    expect(h.onUndo).toHaveBeenCalledOnce();
  });

  it("disables the undo while one is being prepared", () => {
    renderNotice({ preparing: true });
    expect(screen.getByRole("button", { name: H.preparing })).toBeDisabled();
  });

  it("offers carrying on but no undo when the History row could not be read", () => {
    renderNotice({ entry: null });
    expect(screen.getByRole("button", { name: S.carryOn })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: H.putRecentChangesBack })).not.toBeInTheDocument();
  });
});

describe("InterruptionNotice, real run stopped early with an ambiguous outcome", () => {
  // The safety-critical assertion in this file. A truncated cross-volume copy
  // reads as a whole book on the next scan, so carrying on must never be
  // offered when the engine says the outcome is unconfirmed.
  it("never offers carrying on, and sends the user to History instead", async () => {
    const h = renderNotice({
      interruption: interruption({ resume_offered: false }),
      entry: entry({ undo: { kind: "needs-a-look" } }),
    });

    expect(screen.getByText(S.ambiguousHeading)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: S.carryOn })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: H.putRecentChangesBack })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: S.openHistory }));
    expect(h.onOpenHistory).toHaveBeenCalledOnce();
  });

  it("uses the danger token pair, while the calm states use warn (FD-09)", () => {
    const { container: ambiguous } = render(
      <InterruptionNotice
        interruption={interruption({ resume_offered: false })}
        entry={entry({ undo: { kind: "needs-a-look" } })}
        preparing={false}
        onGoToLibrary={vi.fn()}
        onUndo={vi.fn()}
        onOpenHistory={vi.fn()}
      />,
    );
    expect(ambiguous.querySelector(".text-danger")).not.toBeNull();
    cleanup();

    const { container: calm } = render(
      <InterruptionNotice
        interruption={interruption()}
        entry={entry()}
        preparing={false}
        onGoToLibrary={vi.fn()}
        onUndo={vi.fn()}
        onOpenHistory={vi.fn()}
      />,
    );
    expect(calm.querySelector(".text-warn")).not.toBeNull();
    expect(calm.querySelector(".text-danger")).toBeNull();
  });
});

describe("InterruptionNotice details disclosure", () => {
  it("holds only plain facts: no paths, no ids (FD-13, AC-6)", () => {
    renderNotice();
    expect(screen.getByText(S.showDetails)).toBeInTheDocument();
    expect(screen.getByText(S.detailChanges(142))).toBeInTheDocument();
    expect(screen.getByText(S.detailLastStepChecked)).toBeInTheDocument();
    // The in-doubt op id is deliberately never rendered.
    expect(screen.queryByText(/142\b.*op|op.*142/i)).toBeNull();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm vitest run src/components/states/__tests__/InterruptionNotice.test.tsx`
Expected: FAIL, cannot resolve `../InterruptionNotice`.

- [ ] **Step 3: Write the component**

Create `src/components/states/InterruptionNotice.tsx`:

```tsx
import { AlertTriangle, ArrowRight, FlaskConical, Undo2 } from "lucide-react";
import { commands, type HistoryEntry } from "@/lib/bindings";
import { formatWhen } from "@/lib/format";
import { STRINGS } from "@/lib/strings";

// The interruption recovery surface (v0.6.0 P1c, F-606 interruption safety,
// AC-6 and AC-7 as amended by FD-39).
//
// # What this screen is for
//
// The startup reconciler finds a tidy-up a previous session was killed in the
// middle of, verifies from disk what actually happened to the single operation
// that could have been in doubt, and repairs its own record. Until this screen
// existed it then said nothing at all, so a killed tidy-up was invisible.
//
// # Why it decides almost nothing
//
// Two questions look like this component's to answer and are not. Whether
// carrying on is SAFE is `resume_offered`, which the reconciler sets from the
// verified on-disk outcome. What can be UNDONE is `entry.undo`, which the
// engine resolves from invariants the view cannot see (was an undo file
// exported, are the operations reversible, did reconciliation leave an
// ambiguity). FD-36 put that decision in the engine deliberately. This
// component renders both answers and derives neither.
//
// # Why "carry on" does not start a scan
//
// It routes to the library, where the tidy-up action already lives. Starting
// work off the back of a recovery screen is the sort of surprise this product
// exists to avoid, and re-planning from a fresh scan (FD-39) is what makes
// carrying on correct: books already tidied simply produce no operation the
// second time.

const S = STRINGS.interruption;
const H = STRINGS.history;

/**
 * The non-null result of `startup_interruption`, derived from the generated
 * bindings rather than restated, so a change to `ReconcileResult` breaks this
 * at compile time instead of silently.
 */
export type StartupInterruption = NonNullable<
  Awaited<ReturnType<typeof commands.startupInterruption>>
>;

export type InterruptionState = "practice-run" | "stopped-decisive" | "stopped-ambiguous";

/**
 * Which story this run tells. A rehearsal is always the practice-run state
 * whatever else the result says: its effects lived in a MemFs that died with
 * the process, so there is nothing on disk to carry on from or put back.
 */
export function interruptionStateOf(i: StartupInterruption): InterruptionState {
  if (i.mode === "dry-run") return "practice-run";
  return i.resume_offered ? "stopped-decisive" : "stopped-ambiguous";
}

export interface InterruptionNoticeProps {
  interruption: StartupInterruption;
  /** The matching History row, or null when it could not be read. */
  entry: HistoryEntry | null;
  /** True while an undo plan is being prepared. */
  preparing: boolean;
  onGoToLibrary: () => void;
  onUndo: () => void;
  onOpenHistory: () => void;
}

const PRIMARY =
  "inline-flex items-center gap-1.5 rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-60";
const SECONDARY =
  "inline-flex items-center gap-1.5 rounded border border-border-2 bg-surface px-4 py-2 text-[13px] font-semibold text-ink transition-colors hover:bg-surface-2 disabled:opacity-60";

export function InterruptionNotice({
  interruption,
  entry,
  preparing,
  onGoToLibrary,
  onUndo,
  onOpenHistory,
}: InterruptionNoticeProps) {
  const state = interruptionStateOf(interruption);
  const isPractice = state === "practice-run";
  const isAmbiguous = state === "stopped-ambiguous";

  const Icon = isPractice ? FlaskConical : AlertTriangle;
  const heading = isPractice
    ? S.practiceHeading
    : isAmbiguous
      ? S.ambiguousHeading
      : S.stoppedHeading;
  const body = isPractice ? S.practiceBody : isAmbiguous ? S.ambiguousBody : S.stoppedBody;

  // Icon-plus-label always, never colour alone (design-system Section 8).
  const glyph = isAmbiguous ? "bg-danger-bg text-danger" : "bg-warn-bg text-warn";

  const offer = entry?.undo;
  const undoLabel =
    offer?.kind === "put-everything-back"
      ? H.putEverythingBack
      : offer?.kind === "put-recent-changes-back"
        ? H.putRecentChangesBack
        : null;

  return (
    <div className="max-w-[56ch]">
      <span
        aria-hidden
        className={`mb-4 flex size-11 items-center justify-center rounded-[10px] ${glyph}`}
      >
        <Icon size={22} />
      </span>

      <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em] text-balance">
        {heading}
      </h1>
      <p className="mt-2 text-[14px] leading-relaxed text-ink-2">{body}</p>

      {!isPractice && (
        <p className="mt-3 text-[14px] font-semibold text-ink">
          {interruption.done_count > 0 ? S.booksMoved(interruption.done_count) : S.nothingMoved}
        </p>
      )}

      <div className="mt-5 flex flex-wrap gap-2.5">
        {isPractice && (
          <button type="button" onClick={onGoToLibrary} className={PRIMARY}>
            {S.practiceAction}
          </button>
        )}

        {state === "stopped-decisive" && (
          <button type="button" onClick={onGoToLibrary} className={PRIMARY}>
            <ArrowRight size={15} aria-hidden />
            {S.carryOn}
          </button>
        )}

        {!isPractice && undoLabel && (
          <button type="button" onClick={onUndo} disabled={preparing} className={SECONDARY}>
            <Undo2 size={15} aria-hidden />
            {preparing ? H.preparing : undoLabel}
          </button>
        )}

        {isAmbiguous && (
          <button type="button" onClick={onOpenHistory} className={PRIMARY}>
            {S.openHistory}
          </button>
        )}
      </div>

      {state === "stopped-decisive" && (
        <p className="mt-2.5 max-w-[48ch] text-[12.5px] leading-relaxed text-ink-3">
          {S.carryOnNote}
        </p>
      )}

      {!isPractice && undoLabel && (
        <p className="mt-1.5 max-w-[48ch] text-[12.5px] leading-relaxed text-ink-3">
          {H.undoIsReviewedFirst}
        </p>
      )}

      <details className="mt-5">
        <summary className="cursor-pointer text-[13px] text-ink-3">{S.showDetails}</summary>
        <ul className="mt-2 space-y-1 text-[12.5px] leading-relaxed text-ink-3">
          {entry?.startedAt && formatWhen(entry.startedAt) && (
            <li>{S.detailStarted(formatWhen(entry.startedAt))}</li>
          )}
          <li>{S.detailChanges(interruption.done_count)}</li>
          <li>
            {interruption.interrupted ? S.detailLastStepChecked : S.detailNothingInDoubt}
          </li>
        </ul>
      </details>
    </div>
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm vitest run src/components/states/__tests__/InterruptionNotice.test.tsx`
Expected: PASS, all 10 tests.

If the danger-token test fails because the class is on a different element than expected, fix the assertion to query the element that actually carries it. Do not remove the assertion: it is the mechanical check that the ambiguous state is visually distinct.

- [ ] **Step 5: Typecheck and lint**

Run: `pnpm typecheck && pnpm lint`
Expected: PASS on both.

- [ ] **Step 6: Commit**

```bash
git add src/components/states/InterruptionNotice.tsx src/components/states/__tests__/InterruptionNotice.test.tsx
git commit -m "feat: the interruption recovery surface component"
```

---

## Task 4: The `useStartupInterruption` hook

**Files:**
- Create: `src/hooks/useStartupInterruption.ts`
- Test: `src/hooks/__tests__/useStartupInterruption.test.tsx`

**Interfaces:**
- Consumes: `commands.startupInterruption`, `commands.historyList` from `@/lib/bindings`; `StartupInterruption` from Task 3.
- Produces:
  ```ts
  export interface UseStartupInterruption {
    interruption: StartupInterruption | null;
    entry: HistoryEntry | null;
    status: "loading" | "ready";
    dismiss: () => void;
  }
  export function useStartupInterruption(): UseStartupInterruption
  ```
  Task 5 consumes all four fields.

Behaviour the tests below pin down:

- `startup_interruption` returns a raw promise, **not** the `{status, data}` envelope other commands use, because the Rust command returns `Option<ReconcileResult>` with no `Result`. It resolves to the object or `null`, and rejects only on a transport failure.
- A failure anywhere resolves to "no interruption" or "no entry". A recovery offer that cannot be read must never block the app from opening.
- `dismiss()` clears the local copy. The backend value is a process-lifetime snapshot and never changes, so clearing locally is the only way this surface goes away for the session.

- [ ] **Step 1: Write the failing tests**

Create `src/hooks/__tests__/useStartupInterruption.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useStartupInterruption } from "../useStartupInterruption";
import { commands } from "@/lib/bindings";
import type { HistoryEntry } from "@/lib/bindings";

vi.mock("@/lib/bindings", () => ({
  commands: {
    startupInterruption: vi.fn(),
    historyList: vi.fn(),
  },
}));

const mockedStartup = vi.mocked(commands.startupInterruption);
const mockedHistory = vi.mocked(commands.historyList);

const RESULT = {
  job_id: 14,
  mode: "real" as const,
  interrupted: true,
  outcome: "completed" as const,
  in_doubt_op_id: 142,
  resume_offered: true,
  done_count: 142,
};

const ROW: HistoryEntry = {
  jobId: 14,
  mode: "real",
  state: "failed",
  startedAt: "2026-08-04T00:18:15Z",
  finishedAt: null,
  changesMade: 142,
  undo: { kind: "put-recent-changes-back", op_ids: [140, 141, 142] },
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("useStartupInterruption", () => {
  it("reports no interruption on a clean start, and never calls History", async () => {
    mockedStartup.mockResolvedValue(null);

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toBeNull();
    expect(result.current.entry).toBeNull();
    expect(mockedHistory).not.toHaveBeenCalled();
  });

  it("pairs the interruption with its History row", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockResolvedValue({ status: "ok", data: [ROW] });

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toEqual(RESULT);
    expect(result.current.entry).toEqual(ROW);
  });

  it("keeps the interruption but no entry when no History row matches", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockResolvedValue({ status: "ok", data: [{ ...ROW, jobId: 99 }] });

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toEqual(RESULT);
    expect(result.current.entry).toBeNull();
  });

  // A recovery offer that cannot be fully read still tells the user the run was
  // interrupted; it just cannot offer an undo. Failing to null here would hide
  // the interruption entirely, which is the worse outcome.
  it("still reports the interruption when History fails", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockRejectedValue(new Error("db locked"));

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toEqual(RESULT);
    expect(result.current.entry).toBeNull();
  });

  it("reports no interruption when the command itself fails, rather than blocking the app", async () => {
    mockedStartup.mockRejectedValue(new Error("ipc gone"));

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.interruption).toBeNull();
  });

  it("dismiss clears the surface for the rest of the session", async () => {
    mockedStartup.mockResolvedValue(RESULT);
    mockedHistory.mockResolvedValue({ status: "ok", data: [ROW] });

    const { result } = renderHook(() => useStartupInterruption());
    await waitFor(() => expect(result.current.interruption).not.toBeNull());

    act(() => result.current.dismiss());

    expect(result.current.interruption).toBeNull();
    expect(result.current.entry).toBeNull();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm vitest run src/hooks/__tests__/useStartupInterruption.test.tsx`
Expected: FAIL, cannot resolve `../useStartupInterruption`.

- [ ] **Step 3: Write the hook**

Create `src/hooks/useStartupInterruption.ts`:

```ts
import { useCallback, useEffect, useState } from "react";
import { commands, type HistoryEntry } from "@/lib/bindings";
import type { StartupInterruption } from "@/components/states/InterruptionNotice";

// The one place that asks the backend about a tidy-up a previous session was
// killed in the middle of (v0.6.0 P1c, F-606 interruption safety).
//
// # Two reads, and why the second one exists
//
// `startup_interruption` says what HAPPENED: which job, whether it was a real
// tidy-up or a rehearsal, how far it got, and whether carrying on is safe. It
// does not say what can be DONE about it. That is `entry.undo`, resolved in the
// engine per FD-36 because it depends on invariants the view cannot see. So the
// hook reads both and hands the pair to the surface.
//
// # Everything fails soft
//
// A recovery offer that cannot be read must never stop the app from opening. A
// failed `startup_interruption` resolves to "no interruption"; a failed
// `history_list` keeps the interruption and drops only the undo action. In both
// cases the run is still visible in History, which reads the same rows.
//
// # Why dismiss is local
//
// The backend value is captured once before `manage` and cloned on every call,
// so it is a snapshot for the life of the process and cannot be cleared by
// acting on it. Clearing the local copy is the only way this surface goes away.

export interface UseStartupInterruption {
  interruption: StartupInterruption | null;
  /** The matching History row, or null when it could not be read. */
  entry: HistoryEntry | null;
  status: "loading" | "ready";
  dismiss: () => void;
}

export function useStartupInterruption(): UseStartupInterruption {
  const [interruption, setInterruption] = useState<StartupInterruption | null>(null);
  const [entry, setEntry] = useState<HistoryEntry | null>(null);
  const [status, setStatus] = useState<"loading" | "ready">("loading");

  const dismiss = useCallback(() => {
    setInterruption(null);
    setEntry(null);
  }, []);

  // Every setState happens inside the async run, never synchronously in the
  // effect body (react-hooks/set-state-in-effect), matching useHistory and
  // useHealthMetrics. Runs once: the backing value never changes in a session.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let found: StartupInterruption | null = null;
      try {
        found = await commands.startupInterruption();
      } catch {
        found = null;
      }
      if (cancelled) return;

      if (!found) {
        setStatus("ready");
        return;
      }

      let row: HistoryEntry | null = null;
      try {
        const history = await commands.historyList(null);
        if (history.status === "ok") {
          row = history.data.find((e) => e.jobId === found.job_id) ?? null;
        }
      } catch {
        row = null;
      }
      if (cancelled) return;

      setInterruption(found);
      setEntry(row);
      setStatus("ready");
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  return { interruption, entry, status, dismiss };
}
```

If TypeScript complains that `found` is possibly null inside the `.find` callback (narrowing does not always survive a closure), hoist it: `const job = found.job_id;` immediately after the null check, then compare against `job`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm vitest run src/hooks/__tests__/useStartupInterruption.test.tsx`
Expected: PASS, all 6 tests.

- [ ] **Step 5: Typecheck and lint**

Run: `pnpm typecheck && pnpm lint`
Expected: PASS on both.

- [ ] **Step 6: Commit**

```bash
git add src/hooks/useStartupInterruption.ts src/hooks/__tests__/useStartupInterruption.test.tsx
git commit -m "feat: read the startup interruption and pair it with its History row"
```

---

## Task 5: Wire it into `AppShell`

**Files:**
- Modify: `src/components/shell/AppShell.tsx`
- Test: `src/components/shell/__tests__/AppShellInterruption.test.tsx` (new file, so the existing `AppShell.test.tsx` stays focused)

**Interfaces:**
- Consumes: `useStartupInterruption` (Task 4), `InterruptionNotice` (Task 3), and the existing `navigate` / `openUndoPlan` callbacks already defined in `AppShell`.
- Produces: nothing new for later tasks.

Placement matters. The notice goes **ahead of** `activeJob` in the existing conditional chain, because a session that opens with an unresolved interruption has no live job, and an interruption that somehow coexists with one is the more important thing to show.

- [ ] **Step 1: Write the failing tests**

Create `src/components/shell/__tests__/AppShellInterruption.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppShell } from "../AppShell";
import { useStartupInterruption } from "@/hooks/useStartupInterruption";
import { useHealthMetrics } from "@/hooks/useHealthMetrics";
import { STRINGS } from "@/lib/strings";
import type { AppSettings } from "@/lib/settings";

vi.mock("@/hooks/useStartupInterruption", () => ({ useStartupInterruption: vi.fn() }));
vi.mock("@/hooks/useHealthMetrics", () => ({ useHealthMetrics: vi.fn() }));
vi.mock("@/lib/bindings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/bindings")>();
  return {
    ...actual,
    commands: { rollbackPreparePartial: vi.fn(), applyStart: vi.fn() },
  };
});

const mockedInterruption = vi.mocked(useStartupInterruption);
const mockedHealth = vi.mocked(useHealthMetrics);

const S = STRINGS.interruption;

const SETTINGS: AppSettings = {
  library_root: "E:\\Books - Audio",
  theme: "day",
} as AppSettings;

const RESULT = {
  job_id: 14,
  mode: "dry-run" as const,
  interrupted: true,
  outcome: null,
  in_doubt_op_id: 7,
  resume_offered: false,
  done_count: 0,
};

beforeEach(() => {
  vi.clearAllMocks();
  mockedHealth.mockReturnValue({
    overview: null,
    status: "ready",
    reload: vi.fn(),
  } as unknown as ReturnType<typeof useHealthMetrics>);
});
afterEach(cleanup);

function shell() {
  return render(<AppShell settings={SETTINGS} onUpdate={vi.fn()} />);
}

describe("AppShell interruption handling", () => {
  it("shows the notice instead of the route content, with the sidebar still there", () => {
    mockedInterruption.mockReturnValue({
      interruption: RESULT,
      entry: null,
      status: "ready",
      dismiss: vi.fn(),
    });

    shell();

    expect(screen.getByText(S.practiceHeading)).toBeInTheDocument();
    // The soft-panel decision: navigation stays available, so the sidebar must
    // still render. A hard gate was rejected; this asserts it stayed rejected.
    expect(screen.getByRole("navigation")).toBeInTheDocument();
  });

  it("renders the ordinary route content when there is no interruption", () => {
    mockedInterruption.mockReturnValue({
      interruption: null,
      entry: null,
      status: "ready",
      dismiss: vi.fn(),
    });

    shell();

    expect(screen.queryByText(S.practiceHeading)).not.toBeInTheDocument();
  });

  it("dismisses when the user takes the way out", async () => {
    const dismiss = vi.fn();
    mockedInterruption.mockReturnValue({
      interruption: RESULT,
      entry: null,
      status: "ready",
      dismiss,
    });

    shell();
    await userEvent.click(screen.getByRole("button", { name: S.practiceAction }));

    expect(dismiss).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm vitest run src/components/shell/__tests__/AppShellInterruption.test.tsx`
Expected: FAIL. The notice is not rendered, so `getByText(S.practiceHeading)` throws.

If `getByRole("navigation")` fails because `Sidebar` renders a plain `<aside>` or `<div>`, check `src/components/shell/Sidebar.tsx` and assert against whatever landmark or test id it actually provides. Do not add a role to `Sidebar` purely to satisfy the test.

- [ ] **Step 3: Wire it in**

In `src/components/shell/AppShell.tsx`, add to the import block:

```tsx
import { useStartupInterruption } from "@/hooks/useStartupInterruption";
import { InterruptionNotice } from "@/components/states/InterruptionNotice";
```

Inside `AppShell`, beside the other hook calls (after `const health = useHealthMetrics();`):

```tsx
  // A tidy-up a previous session was killed in the middle of (v0.6.0 P1c,
  // F-606). Shown in the screen area ahead of everything else, with the sidebar
  // left live: the dangerous action is starting a NEW tidy-up, not using the
  // app, and blocking navigation would be a procedural gate that stops nothing
  // an IPC caller can reach. See design-p1c-interruption-surface.md.
  const interruption = useStartupInterruption();
  const [preparingUndo, setPreparingUndo] = useState(false);

  // Carrying on is a fresh scan and a fresh plan, not a replay of the
  // interrupted job (FD-39): books already tidied produce no operation the
  // second time. So this is just navigation to where the tidy-up action lives.
  const onInterruptionGoToLibrary = useCallback(() => {
    interruption.dismiss();
    navigate("library");
  }, [interruption, navigate]);

  const onInterruptionOpenHistory = useCallback(() => {
    interruption.dismiss();
    navigate("history");
  }, [interruption, navigate]);

  // Same two-step the History screen uses (D-09): prepare the inverse plan,
  // then hand the user to the review surface. Nothing moves on this click.
  const onInterruptionUndo = useCallback(async () => {
    const offer = interruption.entry?.undo;
    if (offer?.kind !== "put-recent-changes-back") return;
    setPreparingUndo(true);
    try {
      const result = await commands.rollbackPreparePartial(
        interruption.entry!.jobId,
        offer.op_ids,
      );
      if (result.status === "ok") {
        interruption.dismiss();
        openUndoPlan(result.data.plan_id);
      }
    } finally {
      setPreparingUndo(false);
    }
  }, [interruption, openUndoPlan]);
```

Then change the render conditional inside `<ScreenContainer>` so the notice comes first:

```tsx
        <ScreenContainer>
          {interruption.interruption ? (
            <InterruptionNotice
              interruption={interruption.interruption}
              entry={interruption.entry}
              preparing={preparingUndo}
              onGoToLibrary={onInterruptionGoToLibrary}
              onUndo={() => void onInterruptionUndo()}
              onOpenHistory={onInterruptionOpenHistory}
            />
          ) : activeJob ? (
            <Apply jobId={activeJob.jobId} mode={activeJob.mode} onDone={onApplyDone} />
          ) : startError ? (
```

The rest of the chain is unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm vitest run src/components/shell/__tests__/AppShellInterruption.test.tsx src/components/shell/__tests__/AppShell.test.tsx src/components/shell/__tests__/AppShellUndoNavigation.test.tsx`
Expected: PASS on all three files. The two pre-existing shell test files must still pass, which is the proof the new branch did not disturb the old ones.

If the existing shell tests now fail because `useStartupInterruption` is unmocked and calls real bindings, add this mock to those files:

```tsx
vi.mock("@/hooks/useStartupInterruption", () => ({
  useStartupInterruption: () => ({
    interruption: null,
    entry: null,
    status: "ready",
    dismiss: () => {},
  }),
}));
```

- [ ] **Step 5: Typecheck and lint**

Run: `pnpm typecheck && pnpm lint`
Expected: PASS on both.

- [ ] **Step 6: Commit**

```bash
git add src/components/shell/AppShell.tsx src/components/shell/__tests__/
git commit -m "feat: surface an interrupted tidy-up when the app opens"
```

---

## Task 6: Accessibility smoke and the design-system entry

**Files:**
- Modify: `src/__tests__/a11y.test.tsx`
- Modify: `docs/internal/design-system.md` (Section 5, the F-908 state catalogue)

**Interfaces:**
- Consumes: `InterruptionNotice` from Task 3.
- Produces: nothing consumed by later tasks.

`FD-21` (accessibility verified, not promised) requires three things for a new surface: an axe-core smoke test, a mechanical contrast check of the token pairs in both themes, and a keyboard walkthrough in the manual QA checklist. The contrast half already passes, because this surface introduces no new tokens: it uses `--warn`, `--warn-bg`, `--danger`, and `--danger-bg`, all of which `scripts/check-contrast.mjs` already covers. Only the axe half is new.

- [ ] **Step 1: Add the axe smoke test**

Append to `src/__tests__/a11y.test.tsx`, following the existing describe blocks:

```tsx
describe("InterruptionNotice a11y", () => {
  const RESULT = {
    job_id: 14,
    mode: "real" as const,
    interrupted: true,
    outcome: "completed" as const,
    in_doubt_op_id: 142,
    resume_offered: true,
    done_count: 142,
  };

  const ROW: HistoryEntry = {
    jobId: 14,
    mode: "real",
    state: "failed",
    startedAt: "2026-08-04T00:18:15Z",
    finishedAt: null,
    changesMade: 142,
    undo: { kind: "put-recent-changes-back", op_ids: [140, 141, 142] },
  };

  // All three states, because the ambiguous one uses a different token pair and
  // a different action set: passing on the calm state proves nothing about it.
  it.each([
    ["practice run", { ...RESULT, mode: "dry-run" as const, resume_offered: false }],
    ["stopped, decisive", RESULT],
    ["stopped, ambiguous", { ...RESULT, resume_offered: false }],
  ])("has no axe violations: %s", async (_label, interruption) => {
    const { container } = render(
      <InterruptionNotice
        interruption={interruption}
        entry={ROW}
        preparing={false}
        onGoToLibrary={() => {}}
        onUndo={() => {}}
        onOpenHistory={() => {}}
      />,
    );
    const results = await axe.run(container);
    expect(results.violations).toEqual([]);
  });
});
```

Add to that file's imports:

```tsx
import { InterruptionNotice } from "@/components/states/InterruptionNotice";
import type { HistoryEntry } from "@/lib/bindings";
```

- [ ] **Step 2: Run the accessibility tests**

Run: `pnpm vitest run src/__tests__/a11y.test.tsx`
Expected: PASS, including the three new cases.

A likely first failure is a heading-order violation, because `InterruptionNotice` renders an `<h1>` while the surrounding shell may already own one. If axe reports `page-has-heading-one` or `heading-order`, resolve it by checking what `ScreenContainer` and its sibling routes do (`EmptyState` also renders an `h1`) and matching that precedent, not by deleting the heading.

- [ ] **Step 3: Add the design-system entry**

In `docs/internal/design-system.md`, Section 5 (the F-908 state catalogue), insert after the "Apply failure plus resume choice" entry:

```markdown
Interrupted tidy-up, recovery choice (v0.6.0 P1c, F-606). Trigger: a previous session was killed mid-apply and the startup reconciler repaired the record before the app opened. Layout: replaces the screen area, sidebar left live (navigation is not blocked; the gate that matters is the engine's forward-tidying gate, not a UI trap). Three states from one component. (a) An interrupted practice run: `--warn`, a flask glyph, "Nothing on your shelves was touched", one calm action back to the library. (b) A real tidy-up stopped early with a verified outcome: `--warn`, warning glyph, the count of books moved, primary "Carry on tidying up" (which re-scans and re-plans, FD-39, never replays), secondary the engine-resolved undo. (c) A real tidy-up stopped early with an unconfirmed outcome: `--danger`, warning glyph, carrying on is NOT offered, the only actions are the undo the engine allows and opening History. All three carry a "Show details" disclosure holding plain facts only: no paths, no ids, no journal (AC-6, FD-13). Distinct from "Apply failure plus resume choice" above, which is the in-session halt and uses F-608 pause/resume semantics; this one is the across-a-restart case.
```

- [ ] **Step 4: Give the keyboard walkthrough a home**

`FD-21` requires a keyboard-walkthrough item in the release's manual QA checklist, and v0.6.0 has no checklist yet: `docs/internal/qa/` holds only `v0.1.0-manual-qa.md` and `v0.4.0-manual-qa.md`. Create `docs/internal/qa/v0.6.0-manual-qa.md` following that convention:

```markdown
# v0.6.0 (hardening) manual QA: the human half of the release gate

The automated half of `FD-21` (accessibility verified, not promised) runs in
`pnpm test`: the axe-core smoke over each surface and the token-contrast script
over both themes. This file is the half a person performs on a real Windows
WebView2 build, following the convention set by
[`v0.4.0-manual-qa.md`](./v0.4.0-manual-qa.md).

## Walkthrough

1. **Interrupted tidy-up, keyboard only** (`AC-6`, v0.6.0 hardening; `FD-21`).
   - Start a practice tidy-up, then kill the app process while it runs
     (Task Manager, End task). Reopen it.
   - Confirm the recovery notice appears in the screen area with the sidebar
     still visible and navigable.
   - Reach every control with Tab alone: the action button, the "Show details"
     disclosure, and the sidebar items. Confirm focus is visible on each,
     Enter and Space both operate the button, and Enter opens and closes the
     disclosure.
   - Confirm the wording is about a practice run and says nothing on the
     shelves was touched. If it offers to carry on or to put changes back,
     that is a defect: a practice run has neither.
   - Repeat in both Day and Evening themes.

2. **Cancel mid-tidy-up** (`AC-8`, v0.6.0 hardening). Still open; owned by jp.
   Cancel a run in the app and confirm it stops between changes, never
   mid-file-move, and leaves a state that can be picked up again. The automated
   half already passes; this is the by-hand half.
```

Item 2 is a placeholder for an existing open gate, not new work: `AC-8` (v0.6.0 hardening, the cancel-mid-tidy-up hand walkthrough) has been outstanding with no home to be written down in.

- [ ] **Step 5: Verify no dashes entered the doc**

Run: `node -e "const s=require('fs').readFileSync('docs/internal/design-system.md','utf8'); process.exit(/[\u2013\u2014]/.test(s)?1:0)"`
Expected: exit 0.

- [ ] **Step 6: Commit**

```bash
git add src/__tests__/a11y.test.tsx docs/internal/design-system.md docs/internal/qa/v0.6.0-manual-qa.md
git commit -m "test: axe smoke on all three interruption states, plus the design-system and QA entries"
```

---

## Task 7: Full verification and the pull request

**Files:** none changed unless a check fails.

- [ ] **Step 1: Run the whole frontend suite**

Run: `pnpm test`
Expected: PASS. This runs `vitest run` over all files plus `scripts/check-contrast.mjs`. Before this task the baseline was 278 frontend tests across 37 files; this plan adds roughly 22 across 4 files, so expect about 300. Record the actual number.

- [ ] **Step 2: Confirm the backend is genuinely untouched**

Run: `git diff --stat main -- crates/ src-tauri/`
Expected: **empty output.** This plan claims zero backend changes; a non-empty diff means something drifted and must be explained or reverted before the PR.

- [ ] **Step 3: Confirm the generated bindings did not drift**

Run: `pnpm bindings:check`
Expected: PASS. This exports the bindings from Rust and fails if `src/lib/bindings.ts` differs. It should pass trivially, since no Rust changed; it is here because a silent bindings edit is exactly the kind of thing this gate exists to catch.

- [ ] **Step 4: Run the Rust suite once, as a regression check**

Run: `cargo test -p abo-core`
Expected: PASS. Baseline is 502 lib tests plus 3 kill-recovery integration tests. Nothing here should move that number; running it proves the claim rather than asserting it.

- [ ] **Step 5: Sweep for dashes across everything changed**

Run:
```bash
git diff --name-only main | while read -r f; do
  [ -f "$f" ] && node -e "
    const s = require('fs').readFileSync(process.argv[1], 'utf8');
    if (/[\u2013\u2014]/.test(s)) { console.log('DASH: ' + process.argv[1]); process.exit(1); }
  " "$f"
done
```
Expected: no output. Any file listed must be fixed before committing.

- [ ] **Step 6: Push and open the pull request**

```bash
git push -u origin feat/v0.6.0-p1c-resume-surface
gh pr create --title "feat: P1c, the interruption recovery surface" --body "$(cat <<'BODY'
Closes P1c, the last open piece of F-606 (interruption safety) apart from the
AC-8 hand walkthrough. `startup_interruption` has shipped and been tested since
2026-07-31 with nothing calling it, so a killed tidy-up was invisible to the
user. This builds the surface that speaks.

Three states from one component, driven by `ReconcileResult` and the engine's
own `UndoOffer`:

- an interrupted practice run (the only state reachable while apply is pinned
  to rehearsal): nothing on the shelves was touched, one way back
- a real tidy-up stopped early with a verified outcome: carry on, or put the
  changes back
- a real tidy-up stopped early with an unconfirmed outcome: carrying on is not
  offered, because a cross-volume copy killed mid-write leaves a target that
  exists but may be truncated, and a fresh scan would read it as a tidy book

**Zero backend changes and zero migrations.** `git diff main -- crates/ src-tauri/`
is empty, and `pnpm bindings:check` passes.

AC-7 (v0.6.0 hardening) is satisfied as amended by FD-39: carrying on re-plans
from a fresh scan rather than replaying the interrupted job, because a replayed
plan is validated against a snapshot the interrupted run itself invalidated.

Design: `docs/internal/releases/v0.6.0-hardening/design-p1c-interruption-surface.md`
Plan: `docs/internal/releases/v0.6.0-hardening/implementation-plan-p1c.md`

Not merged by an agent: D-11's self-merge allowance lapsed with FD-38.

BODY
)"
```

- [ ] **Step 7: Stop**

Do not merge. Report the PR number, the final test counts, and the result of the backend-untouched check. `AC-8` (v0.6.0 hardening, the cancel-mid-tidy-up hand walkthrough) and the state-1 walkthrough are jp's to run.

---

## What this plan does not do, on purpose

- **No engine gate on forward tidying while an interruption is unresolved.** The state where that matters cannot occur while apply is pinned to rehearsal. It is recorded in `STATUS.md` as the fourth precondition for enabling real changes, next to the power-loss threat model, the cross-volume move policy, and the mechanical authorization boundary.
- **No book names in the details disclosure.** `in_doubt_op_id` is an id, and resolving it to a title needs a `plan_list_ops` call this surface does not otherwise make. The disclosure carries plain facts instead. A follow-up can add titles if the disclosure proves too thin once a real interruption is reachable.
- **No persistence of the notice across navigation.** Dismissing is local and one-shot. The run stays visible in History with the same undo offer, so nothing is lost but prominence.
- **No change to `AppShell.tsx:107`.** The dry-run pin stays.
