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

  // Set-aside retention (F-904 history/undo surface and the dry-run report).
  // FD-34 + FD-10 + AC-23: the product NEVER auto-deletes anything in the
  // set-aside area; the user empties it themselves. Plain-language register:
  // "set aside", never "quarantine" (FD-31).
  setAside: {
    retentionNote:
      "Set-aside items stay where they are until you empty the Set Aside folder yourself. The app never deletes anything there.",
  },

  // The ruleset editor (F-906, v0.4.0 Phase 6, AC-32/AC-33): "how your
  // shelves get organized", hosted inside Settings. Deliberately never says
  // "ruleset" - plain-language register per design-system Section 6.1 and the
  // vocabulary-discipline standing rule. Naming-style/policy copy paraphrases
  // the F-401/F-402 spec behaviors in family-facing words.
  rulesetEditor: {
    loading: "Loading how your shelves are organized...",
    heading: "How your shelves get organized",
    intro: "Choose a naming style and a few safety options. Changing something here shows you what it would affect before you save it.",
    presetHeading: "Naming style",
    presets: {
      "abs-author-first": {
        label: "Author first",
        description: "Groups everything under the author, with the series and book number next.",
      },
      "title-first": {
        label: "Title first",
        description: "One flat folder per book, named for its title and author.",
      },
      "hybrid-genre": {
        label: "Genre shelves",
        description: "Adds a genre shelf on top of the author-first style, when a genre is known.",
      },
    } as const,
    policiesHeading: "Safety and tidiness options",
    oneBookPerFolder: {
      label: "Split folders that hold more than one book",
      help: "Turns a folder holding several complete books into one folder per book.",
    },
    packShell: {
      label: "The leftover collection folder, once its books are unpacked",
      help: "After every book inside moves out safely, this decides what happens to the folder they came from.",
      quarantine: "Set it aside",
      leaveInPlace: "Leave it where it is",
    },
    sidecars: {
      label: "Ebook and cover files that travel with a book",
      keepWithBook: "Keep with the book",
      quarantine: "Set aside instead",
    },
    preferredFormat: {
      label: "When a book has both an M4B and an MP3 copy, keep",
      m4b: "The M4B copy",
      mp3: "The MP3 copy",
    },
    emptyFolderRemoval: {
      label: "Sweep out folders a tidy-up leaves empty",
    },
    stripNoise: {
      label: "Clean up leftover labels in folder names",
      help: "Removes ripper tags, bitrates, and file sizes some download tools leave behind in folder names.",
    },
    liveCountsHeading: "What this would change",
    liveCountsNoScan: "Scan your library first to see what these choices would change.",
    liveCountsLoading: "Recalculating...",
    saveAction: "Save",
    saveSaving: "Saving...",
    saveSaved: "Saved. The tidy-up plan will use this the next time it's built.",
    unsavedNote: "You have changes that aren't saved yet.",
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
    // Scan Stop control (F-104, AC-36): a real, honest cooperative-cancel
    // affordance on every scan/re-scan progress screen. Never "Skip ahead"
    // (FD-02) - that prototype shortcut is demo-only and does not ship.
    stopScan: "Stop",
    stopped: "Stopped. Nothing was changed - a scan only reads.",
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

  // Apply / activity surface (F-904, v0.5.0 Phase 8, design-system Section 4.16
  // and 5.1). One screen, one job, scrolling sentences, Stop and Pause.
  // Plain-language register throughout (design-system Section 6): never
  // "operation", "ops", "journal", "manifest", "rollback", "verification",
  // "dedupe", "quarantine", "dashboard", or "cancel token" on any user-facing line.
  // FD-10 canon string: pulled from library.reassurance at render time (AC-30).
  apply: {
    // Heading while running (dry-run mode adds the badge below separately).
    heading: "Tidying up your books",
    // Sub-line shown while a REAL walk is running (it moves books for real).
    subline: "Moving books to their new shelves. This may take a little while.",
    // Sub-line shown while a REHEARSAL (dry-run) is running: a rehearsal moves
    // nothing, so it never claims to (the surface must never present a rehearsal
    // as real moves - Critical 1).
    rehearsalSubline:
      "Checking each book against its new shelf. This may take a little while.",
    // Badge shown for a rehearsal (dry-run) apply - NOT a real move.
    rehearsalBadge: "Rehearsal",
    // Pause / resume labels (AC-28): the label toggles depending on state.
    pauseAction: "Pause between books",
    resumeAction: "Resume",
    stopAction: "Stop",
    // The calm "I'm finished with this screen" action shared by every terminal
    // state (stopped / completed / rehearsal complete / failed), FD-23: one key,
    // one word, never inlined at a call site.
    doneAction: "Done",
    // Accessible name for the scrolling sentence feed region (FD-23).
    activityLabel: "Activity",
    // Acknowledge action for the blocked-further-groups state (F-604, AC-29).
    acknowledgeAction: "Got it",
    // Progress line: "3 of 47 books"
    progressLine: (done: number, total: number) =>
      `${done.toLocaleString("en-US")} of ${total.toLocaleString("en-US")} books`,
    // Paused state: shown instead of the sub-line when the job is paused.
    pausedHeading: "Paused between books",
    pausedBody:
      "The tidy-up is waiting at a safe stopping point. Your books are exactly where they were when you paused.",
    // Stopped state: cooperative Stop, not an error (AC-26).
    stoppedHeading: "Stopped between books",
    // Real-apply stopped body: some books really did move to their new places.
    stoppedBody:
      "The tidy-up was stopped at a safe point. The books that have already been moved are in their new places. Nothing was lost or damaged.",
    // Rehearsal stopped body (Critical 1): a rehearsal moved nothing, so it never
    // claims books are "in their new places".
    rehearsalStoppedBody:
      "The rehearsal was stopped at a safe point. No books were moved - this was only a rehearsal. Nothing was lost or damaged.",
    // Completed state (no block).
    completedHeading: "Tidy-up complete",
    completedBody: "All the included books have been tidied up.",
    // Completed with undo file saved note.
    undoSaved: "Your undo file has been saved. You can put everything back from the History screen.",
    // Completed-but-blocked state (F-604, AC-20, AC-29).
    blockedHeading: "Tidy-up complete - needs a look",
    blockedBody:
      "The books have been moved, but the after-the-fact check found something that needs your attention before the next tidy-up can run. Everything is safe.",
    // How many differences the after-the-fact check found (IMPORTANT 5): the
    // blocked state names the specifics instead of an unexplained "Got it".
    blockedCountLine: (count: number) =>
      count === 1
        ? "The check found 1 thing to look at before the next tidy-up."
        : `The check found ${count.toLocaleString("en-US")} things to look at before the next tidy-up.`,
    // The technical pointer to the saved check report (FD-13): named behind
    // "Show file details", never on the family-facing line.
    blockedReportPointer:
      'The full check details are saved as "after-the-fact-check.md" in your Reports folder.',
    // Failed state (FD-04, AC-29): plain language, what happened / what is safe / what to do next.
    failedHeading: "Something stopped the tidy-up",
    // What is safe (always true for all failure modes - no audiobook is ever deleted mid-op).
    failedSafeNote:
      "Your books are safe. No book was moved only partway - every change either completed or was left as it was.",
    // Sentence templates for the scrolling feed of completed ops on a REAL apply.
    // Called with the op's `label` (the book or folder name, no path).
    opMovedSentence: (label: string) => `Moved ${label} to its new shelf.`,
    opSetAsideSentence: (label: string) => `Set aside ${label}.`,
    opRemovedEmpty: "Removed an empty folder.",
    opCreatedFolder: "Created a new folder.",
    // Feed sentences for a REHEARSAL (dry-run): nothing actually moved, so every
    // sentence says what was CHECKED, never what was moved (Critical 1).
    rehearsalOpMovedSentence: (label: string) =>
      `Checked ${label} - ready for its new shelf.`,
    rehearsalOpSetAsideSentence: (label: string) =>
      `Checked ${label} - ready to set aside.`,
    rehearsalOpRemovedEmpty: "Checked an empty folder - ready to remove.",
    rehearsalOpCreatedFolder: "Checked the spot for a new folder.",
    // Dry-run framing (never presented as real moves).
    rehearsalCompletedHeading: "Rehearsal complete",
    rehearsalCompletedBody:
      "This was a rehearsal - no books were actually moved. Everything looks good. Run it for real from the tidy-up screen whenever you're ready.",
  },

  // Error / empty / loading states (F-908, v0.4.0 Phase 7, design-system
  // Section 5). The per-AppError family sentences and next steps live in the
  // centralized error-copy module (src/lib/errorCopy.ts, still one module per
  // FD-23); these are the shared chrome strings the state COMPONENTS render
  // around that copy. Plain-language register throughout (Section 6): never a
  // machine code or raw OS error on a family-facing line.
  states: {
    // Generic family-safe fallback wording, mirrored by GENERIC_ERROR_COPY.
    genericHeading: "Something went wrong.",
    // The disclosure that holds the technical detail (FD-13): the one place a
    // raw error string / path is allowed, for tier 1. Same label the review
    // surface uses so "Show file details" means one thing everywhere.
    showFileDetails: "Show file details",
    tryAgain: "Try again",
    // Startup boot (AppRoot), while the settings load resolves.
    booting: "Getting your library ready...",
    // Settings could not be read/saved at startup (AppRoot). Reassurance that
    // the audiobooks are safe; the technical cause sits in the disclosure.
    settingsUnavailableHeading: "Your settings could not be opened",
    // Persisted library root is gone at scan time (F-909 re-pick, AC-31): a
    // calm surface whose one action re-picks the folder through the OS picker.
    rootMissingRepick: "Choose your library folder again",
    // Plan-building loading state (AC-26, design-system Section 5.3): DISTINCT
    // from the scan screen, with a real Stop. Stopping abandons the wait and
    // returns you; building a plan moves nothing, so this is honest about what
    // Stop does (it never claims to cancel work mid-file - there is no file
    // work to cancel while planning).
    buildingThePlan: {
      heading: "Building the tidy-up plan",
      subline: "Working out the safest set of changes.",
      stop: "Stop",
    },
    planBuildStopped: {
      heading: "Building the plan was stopped.",
      body: "Nothing was changed - building a plan only reads your library. You can build it again whenever you like.",
      action: "Build the plan again",
    },
  },

  // History (v0.6.0): the record of past tidy-ups and the way back from each.
  //
  // Copy register notes. "Put these books back" rather than "roll back" or
  // "prepare the inverse plan" (design-system Section 6; the vocabulary rule
  // forbids the engineering terms on any surface a user reads). Practice runs
  // are named "practice run", never "dry run", matching the rehearsal wording
  // the tidy-up surface already uses.
  history: {
    heading: "History",
    lede: "Every tidy-up you have run, and how to put things back.",
    // Honest pre-first-run state: no tidy-up has ever been run.
    emptyHeading: "No tidy-ups yet",
    emptyBody:
      "Once you tidy up your library, every run shows up here - with a way to put things back.",
    // Row labels.
    practiceRun: "Practice run",
    realRun: "Tidy-up",
    // A practice run changed nothing, so the count would be misleading.
    practiceRunNote: "A practice run. Nothing was moved.",
    changesMade: (n: number) => (n === 1 ? "1 book moved" : `${n} books moved`),
    noChangesMade: "Nothing was moved.",
    // The four undo offers, matching the engine's UndoOffer arms.
    putEverythingBack: "Put these books back",
    putRecentChangesBack: "Put the changes back",
    nothingToPutBack: "Nothing to put back.",
    needsALook: "This one needs a look before anything can be put back.",
    needsALookDetail:
      "The app could not tell for certain what happened to one of your books, so it will not move anything automatically. Open the tidy-up to see the details.",
    // What pressing an undo does: it builds a plan you review first, exactly
    // like a forward tidy-up. Nothing moves on the strength of this button.
    undoIsReviewedFirst: "You will see exactly what moves back before anything happens.",
    preparing: "Getting the undo ready...",
    // States of a past run, in plain language.
    state: {
      done: "Finished",
      failed: "Stopped early",
      stopped: "Stopped by you",
      running: "Interrupted",
    },
  },
} as const;
