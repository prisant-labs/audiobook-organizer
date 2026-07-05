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
} as const;
