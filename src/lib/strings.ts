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
} as const;
