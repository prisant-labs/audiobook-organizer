import { useEffect, useRef, useState } from "react";
import { DEFAULT_PLAN_FILTER, type PlanFilterState } from "@/lib/planFilter";
import { ERROR_COPY } from "@/lib/errorCopy";
import type { Theme } from "@/lib/theme";

import { Cover } from "@/components/Cover";
import { FallbackTile } from "@/components/FallbackTile";
import { ScanProgress } from "@/components/ScanProgress";
import { BookSlot } from "@/components/library/BookSlot";
import { GoodNewsLine } from "@/components/library/GoodNewsLine";
import { LibraryLede } from "@/components/library/LibraryLede";
import { LibrarySkeleton } from "@/components/library/LibrarySkeleton";
import { ShelfSection } from "@/components/library/ShelfSection";
import { SpineCluster } from "@/components/library/SpineCluster";
import { DuplicateCard } from "@/components/duplicates/DuplicateCard";
import { PolicySelector } from "@/components/duplicates/PolicySelector";
import { ConfirmInline } from "@/components/review/ConfirmInline";
import { FileDetails } from "@/components/review/FileDetails";
import { GroupCard } from "@/components/review/GroupCard";
import { GroupDetail } from "@/components/review/GroupDetail";
import { OpRow } from "@/components/review/OpRow";
import { PlanFilter } from "@/components/review/PlanFilter";
import { ReviewFooter } from "@/components/review/ReviewFooter";
import { UnverifiedArchiveConfirm } from "@/components/review/UnverifiedArchiveConfirm";
import { Sidebar } from "@/components/shell/Sidebar";
import { ThemeToggle } from "@/components/shell/ThemeToggle";
import { Titlebar } from "@/components/shell/Titlebar";
import { BuildingThePlan } from "@/components/states/BuildingThePlan";
import { EmptyState } from "@/components/states/EmptyState";
import { ErrorCallout } from "@/components/states/ErrorCallout";
import { InterruptionNotice } from "@/components/states/InterruptionNotice";
import { LoadingSkeleton } from "@/components/states/LoadingSkeleton";

import { Section, Specimen } from "./Specimen";
import * as fx from "./fixtures";

// The dev-only component gallery.
//
// WHY THIS EXISTS. The app's inconsistency is invisible until someone opens six
// screens and remembers what the first one looked like. Side by side on one
// page, wrong is obvious at a glance. This replaces the hand-written HTML
// pattern sheet as the review surface, and the difference is the whole point:
// the sheet was a DRAWING of the app that had already drifted from it (it showed
// four filter chips where PlanFilter actually ships a search box and three
// facet dropdowns), while this renders the real components from the real source.
// A pattern library that lives beside the product is structurally identical to
// the drift it claims to solve; it is only newer.
//
// WHY TWO IFRAMES. The theme tokens are scoped `:root[data-theme="day"]` and
// `:root[data-theme="evening"]`, so two themes cannot coexist in one document.
// The alternative was to copy the token values under a nestable selector, which
// would create an eighth copy of the design language and is exactly the drift
// this file is meant to expose. Two iframes of this same page, each setting its
// own root theme, keeps the token file the single source of truth.
//
// This never ships: gallery.html is not in Vite's build input (vite.config.ts
// declares no rollupOptions.input, so only index.html is built), so it exists
// under `pnpm dev` and nowhere else.

const PANE_THEMES: readonly Theme[] = ["day", "evening"];

export interface GalleryProps {
  /** Set when this document is one themed pane inside the chrome. */
  paneTheme: Theme | null;
}

export function Gallery({ paneTheme }: GalleryProps) {
  return paneTheme ? <Pane /> : <Chrome />;
}

// -- the chrome: both themes, side by side ------------------------------------

function Chrome() {
  const [height, setHeight] = useState(2000);

  useEffect(() => {
    function onMessage(event: MessageEvent) {
      // Same-origin only, and shape-checked: this listener is open to the world
      // otherwise, and a dev tool is still not a reason to trust a stray message.
      if (event.origin !== window.location.origin) return;
      const data = event.data as { type?: unknown; height?: unknown } | null;
      if (!data || data.type !== "gallery:height" || typeof data.height !== "number") return;
      // Both panes render identical content, so the taller one wins and the two
      // columns stay row-aligned. Growing only avoids a feedback loop where a
      // shrinking iframe reports a smaller height forever.
      setHeight((current) => Math.max(current, Math.ceil(data.height as number)));
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);

  return (
    <div className="min-h-screen bg-bg px-8 py-6 font-sans text-ink">
      <header className="mb-6 border-b border-border pb-4">
        <h1 className="font-serif text-2xl font-medium">Component gallery</h1>
        <p className="mt-2 max-w-3xl text-sm text-ink-2">
          The real components from <code className="font-mono text-xs">src/components</code>,
          rendered in both themes. This is the review surface: variants get drawn here and
          picked, never described in prose. Dev-only, never bundled.
        </p>
        <p className="mt-2 text-sm text-ink-3">
          One theme on its own:{" "}
          <a className="text-link underline" href="?theme=day">
            Day
          </a>{" "}
          <a className="text-link underline" href="?theme=evening">
            Evening
          </a>
        </p>
      </header>

      <div className="grid grid-cols-2 gap-6">
        {PANE_THEMES.map((theme) => (
          <div key={theme}>
            <h2 className="mb-2 font-mono text-xs font-semibold uppercase tracking-wide text-ink-3">
              {theme}
            </h2>
            <iframe
              title={`Gallery, ${theme} theme`}
              src={`?theme=${theme}`}
              className="w-full rounded border border-border"
              style={{ height: `${height}px` }}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

// -- one themed pane ----------------------------------------------------------

function Pane() {
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const node = bodyRef.current;
    if (!node || window.parent === window) return;
    // Report our rendered height so the parent can size both iframes to the
    // taller one. An observer rather than a one-shot measure: fonts and the
    // cover images resolve after first paint and change the height.
    const report = () => {
      window.parent.postMessage(
        { type: "gallery:height", height: node.getBoundingClientRect().height + 64 },
        window.location.origin,
      );
    };
    const observer = new ResizeObserver(report);
    observer.observe(node);
    report();
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={bodyRef} className="bg-bg px-6 py-6 font-sans text-ink">
      <Specimens />
    </div>
  );
}

// -- the specimens ------------------------------------------------------------

function Specimens() {
  return (
    <>
      <Section
        title="Shell"
        blurb="The window frame and navigation. Present on every screen, so any inconsistency here is inconsistency everywhere."
      >
        <Specimen name="Titlebar" wide note="reaches the Tauri window API for the caption buttons">
          <Titlebar theme="day" onThemeChange={fx.noop} />
        </Specimen>
        <Specimen name="Sidebar" state="library active">
          <Sidebar
            active="library"
            onNavigate={fx.noop}
            counts={{ duplicateGroups: 403, historyCount: 12, organizeStatus: "ready" }}
          />
        </Specimen>
        <Specimen name="Sidebar" state="Organize active, no counts yet" note="FD-27: badges are omitted, never faked as 0">
          <Sidebar active="organize" onNavigate={fx.noop} counts={{}} />
        </Specimen>
        <Specimen name="ThemeToggle" state="day">
          <ThemeToggle theme="day" onChange={fx.noop} />
        </Specimen>
        <Specimen name="ThemeToggle" state="evening">
          <ThemeToggle theme="evening" onChange={fx.noop} />
        </Specimen>
      </Section>

      <Section
        title="Library"
        blurb="The home screen: what you have, what needs work, and what is already fine."
      >
        <Specimen name="LibraryLede" state="needs work" wide>
          <LibraryLede overview={fx.OVERVIEW} />
        </Specimen>
        <Specimen name="LibraryLede" state="nothing to do" wide>
          <LibraryLede overview={fx.OVERVIEW_TIDY} />
        </Specimen>
        <Specimen name="GoodNewsLine" wide>
          <GoodNewsLine goodNews={fx.GOOD_NEWS} />
        </Specimen>
        <Specimen name="Cover" state="no cover art, falls back">
          <div className="w-28">
            <Cover title="Sapiens" author="Y. N. Harari" image={null} />
          </div>
        </Specimen>
        <Specimen name="FallbackTile" state="deterministic tint by title" wide>
          <div className="flex flex-wrap gap-3">
            {fx.FALLBACK_TITLES.map((book) => (
              <div key={book.title} className="w-28">
                <FallbackTile title={book.title} author={book.author} />
              </div>
            ))}
          </div>
        </Specimen>
        <Specimen name="SpineCluster" state="20 books">
          <SpineCluster series={fx.SERIES} />
        </Specimen>
        <Specimen name="SpineCluster" state="41 books, over the 14-spine cap">
          <SpineCluster series={fx.SERIES_LONG} />
        </Specimen>
        <Specimen name="BookSlot" state="warn chip" note="resolves its cover over IPC">
          <BookSlot scanId={9} book={fx.BOOK_WARN} />
        </Specimen>
        <Specimen name="BookSlot" state="alert chip" note="resolves its cover over IPC">
          <BookSlot scanId={9} book={fx.BOOK_ALERT} />
        </Specimen>
        <Specimen name="ShelfSection" wide>
          <ShelfSection heading="Worth a look first" subline="a few examples of what organizing would fix">
            {fx.FALLBACK_TITLES.slice(0, 4).map((book) => (
              <div key={book.title} className="w-28 flex-none">
                <FallbackTile title={book.title} author={book.author} />
              </div>
            ))}
          </ShelfSection>
        </Specimen>
        <Specimen name="LibrarySkeleton" wide>
          <LibrarySkeleton />
        </Specimen>
      </Section>

      <Section
        title="Review"
        blurb="The densest surface in the app, and the one with the most states. Aligned columns are load-bearing here (FD-45)."
      >
        <Specimen
          name="PlanFilter"
          wide
          note="a search box and three facet dropdowns, which is what actually ships"
        >
          <StatefulPlanFilter />
        </Specimen>
        <Specimen name="GroupCard" state="included, selected">
          <GroupCard group={fx.GROUPS[1]} selected onSelect={fx.noop} onToggle={fx.noop} />
        </Specimen>
        <Specimen name="GroupCard" state="included, not selected">
          <GroupCard group={fx.GROUPS[0]} selected={false} onSelect={fx.noop} onToggle={fx.noop} />
        </Specimen>
        <Specimen name="GroupCard" state="left out">
          <GroupCard group={fx.GROUPS[3]} selected={false} onSelect={fx.noop} onToggle={fx.noop} />
        </Specimen>
        <Specimen name="GroupCard" state="with warnings">
          <GroupCard group={fx.GROUPS[2]} selected={false} onSelect={fx.noop} onToggle={fx.noop} />
        </Specimen>
        <Specimen name="GroupCard" state="fully blocked">
          <GroupCard group={fx.GROUPS[4]} selected={false} onSelect={fx.noop} onToggle={fx.noop} />
        </Specimen>
        <Specimen name="GroupCard" state="large counts, tabular alignment">
          <GroupCard group={fx.GROUPS[5]} selected={false} onSelect={fx.noop} onToggle={fx.noop} />
        </Specimen>
        <Specimen name="OpRow" state="plain" wide>
          <OpRow op={fx.op()} first onExclude={fx.noop} />
        </Specimen>
        <Specimen name="OpRow" state="warning" wide>
          <OpRow
            op={fx.op({
              id: 2,
              warning_text: "A file with this name is already in the target folder.",
              confidence: "medium",
            })}
            onExclude={fx.noop}
          />
        </Specimen>
        <Specimen name="OpRow" state="blocked, low confidence, cross-group label" wide>
          <OpRow
            op={fx.op({
              id: 3,
              validation: "blocked",
              validation_reason: "The target path is longer than Windows allows.",
              confidence: "low",
              rationale: "Rename this folder to drop the ripper tag.",
            })}
            groupLabel="messy names"
            onExclude={fx.noop}
          />
        </Specimen>
        <Specimen name="FileDetails" state="closed by default" wide>
          <FileDetails op={fx.op()} />
        </Specimen>
        <Specimen name="GroupDetail" state="group with ops" wide>
          <GroupDetail
            group={fx.GROUPS[1]}
            ops={[fx.op(), fx.op({ id: 2, confidence: "medium" })]}
            onExclude={fx.noop}
          />
        </Specimen>
        <Specimen name="GroupDetail" state="nothing selected" wide>
          <GroupDetail group={null} ops={[]} onExclude={fx.noop} />
        </Specimen>
        <Specimen name="ConfirmInline" state="enabled">
          <ConfirmInline disabled={false} />
        </Specimen>
        <Specimen name="ConfirmInline" state="disabled">
          <ConfirmInline disabled />
        </Specimen>
        <Specimen
          name="UnverifiedArchiveConfirm"
          state="step 1, the opener"
          note="AC-13: press it to see step 2. Not wired to anything yet; resolution is P3"
          wide
        >
          <UnverifiedArchiveConfirm onConfirm={fx.noop} />
        </Specimen>
        <Specimen name="UnverifiedArchiveConfirm" state="disabled">
          <UnverifiedArchiveConfirm onConfirm={fx.noop} disabled />
        </Specimen>
        <Specimen name="ReviewFooter" wide>
          <ReviewFooter groups={fx.GROUPS} planId={1} />
        </Specimen>
      </Section>

      <Section
        title="Duplicates"
        blurb="One group is one book with N copies (FD-08). The three states are the ones a decision actually passes through: unchecked, checked, decided."
      >
        <Specimen
          name="PolicySelector"
          state="flag-only"
          wide
          note="says out loud that it usually changes nothing, because on exact groups it cannot"
        >
          <PolicySelector value="flag-only" onChange={fx.noop} />
        </Specimen>
        <Specimen name="PolicySelector" state="keep-larger" wide>
          <PolicySelector value="keep-larger" onChange={fx.noop} />
        </Specimen>
        <Specimen name="DuplicateCard" state="not checked" wide note="keeping a copy here routes through the AC-13 two-step">
          <ul className="m-0 list-none p-0">
            <DuplicateCard group={fx.DUPES_UNCHECKED} onConfirm={fx.noop} onClear={fx.noop} />
          </ul>
        </Specimen>
        <Specimen name="DuplicateCard" state="checked and identical" wide note="the automatic path is open">
          <ul className="m-0 list-none p-0">
            <DuplicateCard group={fx.DUPES_CHECKED} onConfirm={fx.noop} onClear={fx.noop} />
          </ul>
        </Specimen>
        <Specimen name="DuplicateCard" state="decided" wide>
          <ul className="m-0 list-none p-0">
            <DuplicateCard group={fx.DUPES_DECIDED} onConfirm={fx.noop} onClear={fx.noop} />
          </ul>
        </Specimen>
        <Specimen name="DuplicateCard" state="one copy could not be read" wide note="the case AC-12 cares most about: not-knowing, with a reason">
          <ul className="m-0 list-none p-0">
            <DuplicateCard group={fx.DUPES_UNREADABLE} onConfirm={fx.noop} onClear={fx.noop} />
          </ul>
        </Specimen>
      </Section>

      <Section
        title="States"
        blurb="Loading, empty, error and interruption. The states nobody opens on purpose, which is why they drift furthest."
      >
        <Specimen name="ScanProgress" state="determinate">
          <ScanProgress done={412} total={1022} onStop={fx.noop} />
        </Specimen>
        <Specimen name="ScanProgress" state="indeterminate">
          <ScanProgress onStop={fx.noop} />
        </Specimen>
        <Specimen name="BuildingThePlan" wide>
          <BuildingThePlan onStop={fx.noop} />
        </Specimen>
        <Specimen name="LoadingSkeleton" state="3 rows">
          <LoadingSkeleton label="Loading your library" />
        </Specimen>
        <Specimen name="LoadingSkeleton" state="6 rows">
          <LoadingSkeleton label="Loading history" rows={6} />
        </Specimen>
        <Specimen name="EmptyState" state="good tone">
          <EmptyState
            heading="Your library is already organized"
            body="Nothing needs doing right now."
            tone="good"
          />
        </Specimen>
        <Specimen name="EmptyState" state="neutral, with a disabled action">
          <EmptyState
            heading="Nothing to review yet"
            body="Scan your library, then come back here."
            tone="neutral"
            action={{
              label: "Organize now",
              onClick: fx.noop,
              disabled: true,
              reason: "Turn on at least one group.",
            }}
          />
        </Specimen>
        <Specimen name="ErrorCallout" state="known error, retryable" wide>
          <ErrorCallout copy={ERROR_COPY["access-denied"]} onRetry={fx.noop} />
        </Specimen>
        <Specimen name="ErrorCallout" state="with heading and technical detail" wide>
          <ErrorCallout
            copy={ERROR_COPY["db-migration-failed"]}
            heading="Settings are unavailable"
            detail="os error 5: access is denied"
          />
        </Specimen>
        <Specimen name="InterruptionNotice" state="practice run stopped early" wide>
          <InterruptionNotice
            interruption={fx.INTERRUPTION_PRACTICE}
            entry={fx.HISTORY_ROW}
            preparing={false}
            onGoToLibrary={fx.noop}
            onUndo={fx.noop}
            onOpenHistory={fx.noop}
          />
        </Specimen>
        <Specimen name="InterruptionNotice" state="real run, decisive" wide>
          <InterruptionNotice
            interruption={fx.INTERRUPTION_DECISIVE}
            entry={fx.HISTORY_ROW}
            preparing={false}
            onGoToLibrary={fx.noop}
            onUndo={fx.noop}
            onOpenHistory={fx.noop}
          />
        </Specimen>
        <Specimen name="InterruptionNotice" state="real run, ambiguous" wide>
          <InterruptionNotice
            interruption={fx.INTERRUPTION_AMBIGUOUS}
            entry={fx.HISTORY_ROW}
            preparing={false}
            onGoToLibrary={fx.noop}
            onUndo={fx.noop}
            onOpenHistory={fx.noop}
          />
        </Specimen>
      </Section>
    </>
  );
}

/** PlanFilter is controlled, so the gallery owns the state to keep it usable. */
function StatefulPlanFilter() {
  const [filter, setFilter] = useState<PlanFilterState>(DEFAULT_PLAN_FILTER);
  return <PlanFilter filter={filter} onChange={setFilter} groups={fx.GROUPS} />;
}
