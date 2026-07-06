import { useCallback, useState } from "react";
import { navCountsFrom } from "@/hooks/useNavCounts";
import { useHealthMetrics, type UseHealthMetrics } from "@/hooks/useHealthMetrics";
import { DEFAULT_ROUTE, type RouteId } from "@/routes";
import { DEFAULT_THEME, isTheme, type Theme } from "@/lib/theme";
import { pickLibraryFolder } from "@/lib/dialog";
import type { AppSettings } from "@/lib/settings";
import { Titlebar } from "./Titlebar";
import { Sidebar } from "./Sidebar";
import { ScreenContainer } from "./ScreenContainer";
import { ComingSoon } from "./ComingSoon";
import { Settings } from "@/routes/Settings";
import { Library } from "@/routes/Library";
import { Review } from "@/routes/Review";

export interface AppShellProps {
  // The persisted settings (from useAppSettings, owned by AppRoot). AppShell
  // renders once a library root is configured; it derives the theme from these
  // and hosts the Settings screen.
  settings: AppSettings;
  // Persist a full replacement settings row (optimistic; see useAppSettings).
  onUpdate: (next: AppSettings) => Promise<void>;
}

// The real app shell (T-01, refactored in Phase 2): custom titlebar, sidebar,
// and the single screen container every route renders inside. Theme is derived
// from settings and changed through `onUpdate` (the one source of truth,
// persisted via settings_set; AppShell does not touch `data-theme` itself -
// useAppSettings applies it). The Library route now hosts the real F-902
// library home (T-15..T-18, v0.4.0 Phase 4). The disposable v0.1.0 tracer
// (`src/App.tsx`) is deleted (T-37, G-7, Phase 8); this is now the only shell.
// The Settings route hosts the real F-803/F-909 Settings screen.
export function AppShell({ settings, onUpdate }: AppShellProps) {
  const [route, setRoute] = useState<RouteId>(DEFAULT_ROUTE);
  // ONE `classify_overview` load for the whole shell (T-15): the Sidebar
  // badges and the Library home both derive from this single `health` value,
  // so they can never disagree and both refresh together when a scan
  // completes (`Library` calls `health.reload()`; see useNavCounts.ts).
  const health = useHealthMetrics();
  const counts = navCountsFrom(health.overview);
  const theme: Theme = isTheme(settings.theme) ? settings.theme : DEFAULT_THEME;
  const onThemeChange = (next: Theme) => void onUpdate({ ...settings, theme: next });

  // F-909 re-pick (AC-31): the shell owns the settings + dialog seam, so the
  // root-missing surface (in Library) delegates re-picking here. Opens the OS
  // folder picker and persists the new library root; resolves `true` when a
  // folder was chosen (the frontend never touches the filesystem, FD-29).
  const onRepickRoot = useCallback(async (): Promise<boolean> => {
    const path = await pickLibraryFolder();
    if (path === null) return false;
    await onUpdate({ ...settings, library_root: path });
    return true;
  }, [settings, onUpdate]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Titlebar theme={theme} onThemeChange={onThemeChange} />
      <div className="grid min-h-0 flex-1 grid-cols-[212px_1fr]">
        <Sidebar active={route} onNavigate={setRoute} counts={counts} />
        <ScreenContainer>
          <RouteContent
            route={route}
            settings={settings}
            onUpdate={onUpdate}
            onNavigate={setRoute}
            health={health}
            onRepickRoot={onRepickRoot}
          />
        </ScreenContainer>
      </div>
    </div>
  );
}

function RouteContent({
  route,
  settings,
  onUpdate,
  onNavigate,
  health,
  onRepickRoot,
}: {
  route: RouteId;
  settings: AppSettings;
  onUpdate: (next: AppSettings) => Promise<void>;
  onNavigate: (route: RouteId) => void;
  health: UseHealthMetrics;
  onRepickRoot: () => Promise<boolean>;
}) {
  switch (route) {
    case "library":
      return <Library onNavigate={onNavigate} health={health} onRepickRoot={onRepickRoot} />;
    case "tidy-up":
      return <Review scanId={health.overview?.scan_id ?? null} />;
    case "duplicates":
      return <ComingSoon label="Duplicates" />;
    case "history":
      return <ComingSoon label="History" />;
    case "settings":
      return (
        <Settings
          settings={settings}
          onUpdate={onUpdate}
          scanId={health.overview?.scan_id ?? null}
        />
      );
  }
}
