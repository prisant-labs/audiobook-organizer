import { useAppSettings } from "@/hooks/useAppSettings";
import { DEFAULT_THEME, isTheme, type Theme } from "@/lib/theme";
import { STRINGS } from "@/lib/strings";
import { copyForCode } from "@/lib/errorCopy";
import { AppShell } from "@/components/shell/AppShell";
import { Frame } from "@/components/shell/Frame";
import { FirstRun } from "@/routes/FirstRun";
import { ErrorCallout } from "@/components/states/ErrorCallout";
import { LoadingSkeleton } from "@/components/states/LoadingSkeleton";

// The application root (v0.4.0 Phase 2, error states hardened in Phase 7). Owns
// the load of the F-803 settings and routes the three top-level states before
// the shell:
//   - loading  -> a calm boot placeholder (LoadingSkeleton)
//   - error    -> the F-908 settings-unavailable surface (ErrorCallout), mapped
//                 from the AppError code when one was carried, generic otherwise
//   - no root  -> first-run (F-909); the only path forward is picking a folder
//   - ready    -> the full AppShell
// Theme is derived from settings and changed through `update` (persisted via
// settings_set); useAppSettings applies `data-theme`. The pre-shell screens
// still get the titlebar (and its theme toggle + window controls) via `Frame`.
export function AppRoot() {
  const { settings, status, error, errorCode, reload, update } = useAppSettings();
  const theme: Theme = settings && isTheme(settings.theme) ? settings.theme : DEFAULT_THEME;
  const onThemeChange = (next: Theme) => {
    if (settings) void update({ ...settings, theme: next });
  };

  if (status === "loading") {
    return (
      <Frame theme={theme} onThemeChange={onThemeChange}>
        <LoadingSkeleton label={STRINGS.states.booting} centered />
      </Frame>
    );
  }

  if (status === "error" || !settings) {
    // A settings read/save failure at startup (F-908, AC-24). The mapped copy
    // (settings-failed / db-migration-failed / ...) is the family-facing line;
    // the raw cause is tucked in the disclosure, never shown bare. Reassurance
    // that the audiobooks are untouched rides in the mapped next step.
    return (
      <Frame theme={theme} onThemeChange={onThemeChange}>
        <ErrorCallout
          heading={STRINGS.states.settingsUnavailableHeading}
          copy={errorCode ? copyForCode(errorCode) : null}
          detail={error}
          onRetry={reload}
        />
      </Frame>
    );
  }

  if (!settings.library_root) {
    return (
      <Frame theme={theme} onThemeChange={onThemeChange}>
        <FirstRun onChoose={(path) => update({ ...settings, library_root: path })} />
      </Frame>
    );
  }

  return <AppShell settings={settings} onUpdate={update} />;
}
