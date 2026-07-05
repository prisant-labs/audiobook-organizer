// Theme system (F-901/F-803, FD-09, AC-2). Canonical identifiers are
// EXACTLY "day" and "evening": the prototypes' "calm"/"evening" naming (and
// the report's separate "prose" register) never appears in shipped code.
// `document.documentElement.dataset.theme` is the single source of truth the
// CSS in src/styles/tokens.css keys off (`:root[data-theme="day"|"evening"]`).
export type Theme = "day" | "evening";

export const DEFAULT_THEME: Theme = "day";

// TODO(v0.4.0 Phase 2, T-06/T-09): AC-2 requires the choice to persist across
// restarts "via settings (F-803)", but the settings_get/settings_set command
// wrappers over the singleton settings row are a Phase 2 deliverable that
// does not exist yet. localStorage is an interim persistence shim so the
// toggle is not a no-op across restarts today; swap getStoredTheme/
// persistTheme below for calls through src/lib/bindings.ts once the settings
// binding lands, and drop the localStorage key entirely (do not keep both).
const STORAGE_KEY = "abo.theme";

function isTheme(value: string | null): value is Theme {
  return value === "day" || value === "evening";
}

export function getStoredTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return isTheme(stored) ? stored : DEFAULT_THEME;
  } catch {
    // localStorage can throw (privacy mode, disabled storage); fall back to
    // the default rather than crash the shell over a persistence nicety.
    return DEFAULT_THEME;
  }
}

export function persistTheme(theme: Theme): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Best-effort only; see getStoredTheme.
  }
}

// Applies the theme to the document root. Safe to call before React mounts
// (main.tsx calls this synchronously pre-render to avoid a flash of the
// wrong theme).
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}
