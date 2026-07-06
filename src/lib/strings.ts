// Centralized user-facing copy (FD-23: English-only v1, one strings module so
// later localization is possible). Seeded here with only what the app shell
// needs; Phase 4 (T-15/T-18, library home) and Phase 8 (T-33, copy sweep)
// grow this to the full copy register in docs/internal/design-system.md
// Section 6.
//
// The sidebar footer line is the design-system Section 6.4 standing
// reassurance line tagged "(nav / provenance)": "Everything stays on this
// computer." Section 4.3's illustrative second line ("Nothing is ever
// deleted.") is NOT used verbatim here: FD-10 permits negated "deleted"
// wording only inside a full guarantee enumeration ("No audiobook is ever
// deleted. Only empty folders are removed, and every change can be undone.").
// A bare "Nothing is ever deleted." outside that enumeration would violate
// FD-10's own restriction, so this module carries only the single line the
// copy register actually sanctions standalone.
export const STRINGS = {
  appName: "Audiobook Organizer",
  sidebarFooterProvenance: "Everything stays on this computer.",

  // First-run / library root selection (F-909, design-system Sections 4-5:
  // calm, one primary action; Section 6 plain-language register). The only
  // path forward is choosing a folder (AC-28). The reassurance avoids the word
  // "deleted" so it does not collide with the FD-10 guarantee-enumeration rule
  // (AC-38): first-run reassurance is a read-only promise, not a deletion
  // guarantee.
  firstRun: {
    heading: "Let's find your audiobooks",
    lede: "Choose the folder where your audiobooks live. The app only reads it, then shows you what a tidy-up would do. Nothing is moved or changed until you review and approve it.",
    chooseAction: "Choose your library folder",
    choosing: "Opening the folder picker...",
    reassurance: "Everything stays on this computer.",
    errorPrefix: "That folder could not be saved",
  },

  // Settings (F-803 + F-909 re-selection). Calm maintenance surface; the
  // library folder is the focal control. "Set aside" is the plain-language
  // vocabulary for the quarantine root (FD-31); "dashboard" and engine jargon
  // never appear (design-system Section 6.1).
  settings: {
    heading: "Settings",
    intro: "Where your library lives and how tidy-ups behave.",
    libraryLabel: "Your library folder",
    libraryHelp: "The folder the app reads your audiobooks from.",
    libraryChange: "Change folder",
    libraryChoose: "Choose a folder",
    setAsideLabel: "Set-aside folder",
    setAsideHelp: "Where duplicate copies and clutter are moved when you tidy up. Leave as the default to keep it beside your library.",
    reportsLabel: "Reports folder",
    reportsHelp: "Where saved tidy-up reports are written. Leave as the default to keep it beside the app's data.",
    defaultLocation: "Default location",
    change: "Change",
    themeLabel: "Appearance",
    themeHelp: "Day is light; Evening is warm and dark.",
    retentionLabel: "Scans to keep",
    retentionHelp: "The app remembers this many recent scans, then lets the oldest go.",
  },

  // Library home (F-902, v0.4.0 Phase 4, design-system Sections 4.6-4.8 and 6).
  // Numbers are NEVER literals here (FD-27, AC-7): every count/byte figure is
  // composed at render time from `classify_overview` by `Library.tsx`'s own
  // sentence-building helpers, not stored as a fixed string in this module.
  library: {
    heading: "Your library",
    worthALookHeading: "Worth a look first",
    worthALookSubline: "a few examples of what the tidy-up would fix",
    seriesHeading: "Series on your shelves",
    seriesSubline: "the tidy-up keeps each series together",
    scanAgain: "Scan again",
    startTidyUp: "Start a tidy-up",
    scanNow: "Scan your library",
    // AC-9: the FD-10 deletion-guarantee copy, verbatim. This is the exact
    // sanctioned string; do not paraphrase it here or at any call site.
    reassurance:
      "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.",
    // The honest pre-first-scan state (design-system Section 5.2 "Empty
    // library root" family; AC-6 forbids treating "never scanned" as a
    // library of zero books).
    noScanYet: {
      heading: "Let's take a first look",
      body: "Scan your library to see what's there. The app only reads it - nothing is moved or changed until you review and approve it.",
    },
    scanning: {
      heading: "Reading your library...",
    },
  },

  // Plan review surface (F-903/F-502/F-503/F-504, v0.4.0 Phase 5,
  // design-system Sections 4.9-4.14 and 6). Every group's headline/reason and
  // every count/byte figure comes from the real generated plan
  // (`plan_generate`/`plan_get`), never a literal here (FD-27).
  review: {
    heading: "Review the tidy-up",
    lede: "The plan looks at every book and works out what would tidy it up. Nothing happens until you say so, and you can leave any group out.",
    generating: "Building the tidy-up plan...",
    noScan: {
      heading: "Scan your library first",
      body: "There is nothing to review yet. Go to your library and scan it, then come back here to review a tidy-up.",
    },
    detailEmpty: "Choose a group on the left to see what it would change.",
    detailNoOps: "There is nothing to show for this group this time.",
    moreOps: (more: number) =>
      `...and ${more.toLocaleString("en-US")} more just like these. The full list is in the exported report.`,
    excludeAction: "Leave this one out",
    excludeUndoNote: "Left out",
    fileDetails: "Show file details",
    // F-504 honesty caveat (FIX 2): shown beside the re-derived pattern/fields
    // block so the reader knows it reflects only this item's own name, not any
    // detail inherited from a parent folder. Suppressed for box sets and
    // bundles (the backend omits the whole block there).
    ownNameCaveat: "Based on this item's own name.",
    filterPlaceholder: "Search by name...",
    filterGroupAll: "All groups",
    filterConfidenceAll: "Any confidence",
    filterWarningAll: "Any status",
    filterWarningOnly: "Needs a look",
    filterBlockedOnly: "Held",
    filterNoMatches: "Nothing matches that search.",
    // Design-system Section 4.13/6.4 standing footer reassurance line
    // (distinct from the library home's full FD-10 guarantee enumeration).
    footerReassurance: "Nothing is deleted. Every change can be undone.",
    tidyUpNow: "Tidy up now",
    confirmPrompt: "Ready to tidy up the included groups?",
    confirmGoAhead: "Go ahead",
    confirmNotYet: "Not yet",
    // Applying is v0.5.0 (acting); the confirm affordance is real but honest
    // about what it cannot do yet, never a fake success.
    confirmNotAvailable:
      "Applying a tidy-up isn't available in this version yet - that arrives in a later update. For now you can review the plan, include or leave out groups, and look at the file details.",
    allExcludedNote: "Turn on at least one group to tidy up.",
  },
} as const;
