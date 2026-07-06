// Theme system (F-901/F-803, FD-09, AC-2). Canonical identifiers are
// EXACTLY "day" and "evening": the prototypes' "calm"/"evening" naming (and
// the report's separate "prose" register) never appears in shipped code.
// `document.documentElement.dataset.theme` is the single source of truth the
// CSS in src/styles/tokens.css keys off (`:root[data-theme="day"|"evening"]`).
export type Theme = "day" | "evening";

export const DEFAULT_THEME: Theme = "day";

export function isTheme(value: string | null | undefined): value is Theme {
  return value === "day" || value === "evening";
}

// Applies the theme to the document root. Safe to call before React mounts
// (main.tsx calls this synchronously pre-paint to avoid a flash of the wrong
// theme).
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

// ---- Legacy localStorage shim migration (v0.4.0 Phase 2) ----
//
// Phase 1 persisted the theme in localStorage as an interim shim, because the
// settings_get/settings_set IPC did not exist yet. Phase 2 lands that IPC and
// moves theme persistence into the singleton settings row (F-803). Per AC-2 the
// choice now persists "via settings (F-803)", so the settings row is the single
// source of truth going forward.
//
// The one remaining role of localStorage is a ONE-TIME migration: on the first
// launch after this upgrade, a user may still carry a Phase-1 theme choice in
// localStorage. `useAppSettings` reads it once (`readLegacyTheme`), writes it
// into settings if it differs, and then removes it (`clearLegacyTheme`); after
// every settings load or update it rewrites the key as a pre-paint hint
// (`writeThemeHint`) so cold starts paint the persisted theme without a flash.
// Settings is always the authority; the hint is only ever read pre-paint, so from
// then on settings wins and the key is gone. There is no dual-write: nothing in
// the app writes this key anymore.
const LEGACY_STORAGE_KEY = "abo.theme";

// Read the legacy Phase-1 theme, or null if absent/unreadable/invalid. Used
// only for the one-time migration into settings.
export function readLegacyTheme(): Theme | null {
  try {
    const stored = window.localStorage.getItem(LEGACY_STORAGE_KEY);
    return isTheme(stored) ? stored : null;
  } catch {
    // localStorage can throw (privacy mode, disabled storage); treat as absent.
    return null;
  }
}

// Remove the legacy key after migrating (or when it is stale), so settings is
// the sole source of truth and the shim leaves no residue.
export function clearLegacyTheme(): void {
  try {
    window.localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    // Best-effort only; see readLegacyTheme.
  }
}

// Mirror the authoritative settings theme into the pre-paint hint key so the
// NEXT cold start paints the right theme synchronously. This is a paint hint
// only: settings remains the sole authority, and a stale or missing hint is
// always reconciled by the async settings load.
export function writeThemeHint(theme: Theme): void {
  try {
    window.localStorage.setItem(LEGACY_STORAGE_KEY, theme);
  } catch {
    // Best-effort only; see readLegacyTheme.
  }
}

// The synchronous pre-paint theme hint (main.tsx), applied before the async
// settings load resolves so the shell does not flash the wrong theme. It uses a
// pending legacy value if one exists (the migrating user), else the Day default;
// the async settings load then reconciles to the authoritative persisted value.
// This is a paint hint only, never an authoritative read.
export function bootTheme(): Theme {
  return readLegacyTheme() ?? DEFAULT_THEME;
}
