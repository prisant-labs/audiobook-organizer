import { useState } from "react";
import { useTheme } from "@/hooks/useTheme";
import { useNavCounts } from "@/hooks/useNavCounts";
import { DEFAULT_ROUTE, type RouteId } from "@/routes";
import App from "@/App";
import { Titlebar } from "./Titlebar";
import { Sidebar } from "./Sidebar";
import { ScreenContainer } from "./ScreenContainer";
import { ComingSoon } from "./ComingSoon";

// The real app shell (T-01): custom titlebar, sidebar, and the single screen
// container every route renders inside. Replaces the v0.1.0 tracer as the
// mount point at the top of the tree; the tracer itself (`App`) is kept
// alive as the temporary content behind the "Library" route so the
// scan_start -> job:completed -> scan_entries seam keeps working end to end
// until the real Library home (T-15, Phase 4) replaces it. Deleting the
// tracer file is a later phase's task (implementation-plan.md T-37, G-7),
// not this one.
export function AppShell() {
  const { theme, setTheme } = useTheme();
  const [route, setRoute] = useState<RouteId>(DEFAULT_ROUTE);
  const counts = useNavCounts();

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Titlebar theme={theme} onThemeChange={setTheme} />
      <div className="grid min-h-0 flex-1 grid-cols-[212px_1fr]">
        <Sidebar active={route} onNavigate={setRoute} counts={counts} />
        <ScreenContainer>
          <RouteContent route={route} />
        </ScreenContainer>
      </div>
    </div>
  );
}

function RouteContent({ route }: { route: RouteId }) {
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
      return <ComingSoon label="Settings" />;
  }
}
