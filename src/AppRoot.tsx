import { useAppSettings } from "@/hooks/useAppSettings";
import { DEFAULT_THEME, isTheme, type Theme } from "@/lib/theme";
import { AppShell } from "@/components/shell/AppShell";
import { Frame } from "@/components/shell/Frame";
import { FirstRun } from "@/routes/FirstRun";

// The application root (v0.4.0 Phase 2). Owns the load of the F-803 settings and
// routes the three top-level states before the shell:
//   - loading  -> a calm boot screen
//   - error    -> a plain settings-unavailable fallback with retry
//   - no root  -> first-run (F-909); the only path forward is picking a folder
//   - ready    -> the full AppShell
// Theme is derived from settings and changed through `update` (persisted via
// settings_set); useAppSettings applies `data-theme`. The pre-shell screens
// still get the titlebar (and its theme toggle + window controls) via `Frame`.
export function AppRoot() {
  const { settings, status, error, reload, update } = useAppSettings();
  const theme: Theme = settings && isTheme(settings.theme) ? settings.theme : DEFAULT_THEME;
  const onThemeChange = (next: Theme) => {
    if (settings) void update({ ...settings, theme: next });
  };

  if (status === "loading") {
    return (
      <Frame theme={theme} onThemeChange={onThemeChange}>
        <BootScreen />
      </Frame>
    );
  }

  if (status === "error" || !settings) {
    return (
      <Frame theme={theme} onThemeChange={onThemeChange}>
        <SettingsUnavailable message={error} onRetry={reload} />
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

function BootScreen() {
  return (
    <div className="mx-auto flex min-h-full max-w-[40ch] items-center justify-center px-10 py-16">
      <p className="text-[13.5px] text-ink-3">Getting your library ready...</p>
    </div>
  );
}

// A minimal, plain-language fallback when the settings row cannot be read at
// startup (a rare storage failure). The full F-908 error surfaces (corrupt-DB
// recovery, permission denied, and the rest) are Phase 7's task; this is the
// honest interim so a settings-read failure never leaves a blank window.
function SettingsUnavailable({ message, onRetry }: { message: string | null; onRetry: () => void }) {
  return (
    <div className="mx-auto flex min-h-full max-w-[46ch] flex-col items-start justify-center gap-4 px-10 py-16">
      <h1 className="font-serif text-[24px] font-medium tracking-[-0.01em]">
        Your settings could not be opened
      </h1>
      <p className="text-[14px] leading-relaxed text-ink-2">
        Your audiobooks are untouched - this is only the app's own notes. Try again, and if it keeps
        happening, restart the app.
      </p>
      {message && <p className="text-[12.5px] leading-relaxed text-danger">{message}</p>}
      <button
        type="button"
        onClick={onRetry}
        className="mt-1 rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
      >
        Try again
      </button>
    </div>
  );
}
