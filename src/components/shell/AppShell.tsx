import { useState } from "react";
import { useNavCounts } from "@/hooks/useNavCounts";
import { DEFAULT_ROUTE, type RouteId } from "@/routes";
import { DEFAULT_THEME, isTheme, type Theme } from "@/lib/theme";
import type { AppSettings } from "@/lib/settings";
import App from "@/App";
import { Titlebar } from "./Titlebar";
import { Sidebar } from "./Sidebar";
import { ScreenContainer } from "./ScreenContainer";
import { ComingSoon } from "./ComingSoon";
import { Settings } from "@/routes/Settings";

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
// useAppSettings applies it). The Library route still hosts the disposable
// v0.1.0 tracer (`App`); deleting that file is a later phase's task (T-37, G-7).
// The Settings route now hosts the real F-803/F-909 Settings screen.
export function AppShell({ settings, onUpdate }: AppShellProps) {
  const [route, setRoute] = useState<RouteId>(DEFAULT_ROUTE);
  const counts = useNavCounts();
  const theme: Theme = isTheme(settings.theme) ? settings.theme : DEFAULT_THEME;
  const onThemeChange = (next: Theme) => void onUpdate({ ...settings, theme: next });

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Titlebar theme={theme} onThemeChange={onThemeChange} />
      <div className="grid min-h-0 flex-1 grid-cols-[212px_1fr]">
        <Sidebar active={route} onNavigate={setRoute} counts={counts} />
        <ScreenContainer>
          <RouteContent route={route} settings={settings} onUpdate={onUpdate} />
        </ScreenContainer>
      </div>
    </div>
  );
}

function RouteContent({
  route,
  settings,
  onUpdate,
}: {
  route: RouteId;
  settings: AppSettings;
  onUpdate: (next: AppSettings) => Promise<void>;
}) {
  switch (route) {
    case "library":
      // TODO(Phase 4, T-15): replace with the real Library home.
      return <App />;
    case "tidy-up":
      return <ComingSoon label="Tidy-up" />;
    case "duplicates":
      return <ComingSoon label="Duplicates" />;
    case "history":
      return <ComingSoon label="History" />;
    case "settings":
      return <Settings settings={settings} onUpdate={onUpdate} />;
  }
}
