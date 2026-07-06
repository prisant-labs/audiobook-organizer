import { useCallback, useEffect, useRef, useState } from "react";
import { getSettings, saveSettings, type AppSettings } from "@/lib/settings";
import {
  applyTheme,
  clearLegacyTheme,
  DEFAULT_THEME,
  isTheme,
  readLegacyTheme,
  type Theme,
  writeThemeHint,
} from "@/lib/theme";

// The single owner of the F-803 settings (T-06/T-09). Loads the persisted
// settings once, migrates a legacy Phase-1 theme into them exactly once, applies
// the theme, and exposes an optimistic `update` that persists via settings_set.
//
// This hook is the single applier of the document theme: on load, on every
// update (optimistically, before the IPC round-trip, so the toggle feels
// instant), and on revert if a persist fails. Components read `settings.theme`
// and call `update` to change it; they never touch `data-theme` themselves.

export type SettingsStatus = "loading" | "ready" | "error";

export interface UseAppSettings {
  /** The persisted settings, or null while loading / on error. */
  settings: AppSettings | null;
  status: SettingsStatus;
  /** A plain-language error message when `status === "error"`. */
  error: string | null;
  /** Re-run the initial load (used by the error-state retry). */
  reload: () => void;
  /**
   * Persist a full replacement settings row. Optimistic: local state and the
   * theme update immediately, then reconcile to the stored (normalized) form;
   * on failure the previous settings are restored and the error rethrown.
   */
  update: (next: AppSettings) => Promise<void>;
}

function themeOf(settings: AppSettings | null): Theme {
  return settings && isTheme(settings.theme) ? settings.theme : DEFAULT_THEME;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

// One-time migration of the Phase-1 localStorage theme into settings (see
// src/lib/theme.ts). If a legacy value exists and differs from the persisted
// theme, write it once and clear the key; if it matches (or the write fails),
// just clear/keep it and let settings win.
async function migrateLegacyTheme(loaded: AppSettings): Promise<AppSettings> {
  const legacy = readLegacyTheme();
  if (!legacy) return loaded;
  if (legacy === loaded.theme) {
    clearLegacyTheme();
    return loaded;
  }
  try {
    const migrated = await saveSettings({ ...loaded, theme: legacy });
    clearLegacyTheme();
    return migrated;
  } catch {
    // Best-effort: keep the loaded settings (settings still wins) and leave the
    // legacy key for a later attempt rather than dropping the user's choice.
    return loaded;
  }
}

export function useAppSettings(): UseAppSettings {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [status, setStatus] = useState<SettingsStatus>("loading");
  const [error, setError] = useState<string | null>(null);
  const [reloadNonce, setReloadNonce] = useState(0);
  const settingsRef = useRef<AppSettings | null>(null);
  const mountedRef = useRef(true);

  const commit = useCallback((next: AppSettings | null) => {
    settingsRef.current = next;
    setSettings(next);
    const theme = themeOf(next);
    applyTheme(theme);
    // Refresh the pre-paint hint so the next cold start paints this theme
    // synchronously; settings remains authoritative (see theme.ts).
    writeThemeHint(theme);
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      // Reset to the loading state inside the async body (not synchronously in
      // the effect) so a reload re-enters loading without a synchronous
      // setState-in-effect. On first mount the initial state is already
      // "loading", so this is a no-op there.
      if (!cancelled && mountedRef.current) {
        setStatus("loading");
        setError(null);
      }
      try {
        const loaded = await getSettings();
        const resolved = await migrateLegacyTheme(loaded);
        if (cancelled || !mountedRef.current) return;
        commit(resolved);
        setStatus("ready");
      } catch (e) {
        if (cancelled || !mountedRef.current) return;
        setError(messageOf(e));
        setStatus("error");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [reloadNonce, commit]);

  const reload = useCallback(() => setReloadNonce((n) => n + 1), []);

  const update = useCallback(
    async (next: AppSettings) => {
      const prev = settingsRef.current;
      commit(next); // optimistic: state + theme flip immediately
      try {
        const persisted = await saveSettings(next);
        if (!mountedRef.current) return;
        commit(persisted); // reconcile to the normalized stored form
      } catch (e) {
        if (mountedRef.current) commit(prev); // revert on failure
        throw e instanceof Error ? e : new Error(messageOf(e));
      }
    },
    [commit],
  );

  return { settings, status, error, reload, update };
}
