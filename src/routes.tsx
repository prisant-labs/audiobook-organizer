// Route registry for the app shell (T-01). This is deliberately NOT a URL
// router: the sibling common-stack project (repo-sync-tool, D-01) renders its
// shell the same way, a single-window desktop app with no address bar or
// deep-linking need, so a `useState<RouteId>` switch in AppShell plus this
// registry is the whole "router" this phase calls for. `aria-current="page"`
// (AC-1) does not require History-API navigation, only a stable notion of
// "the active item."
export type RouteId = "library" | "tidy-up" | "duplicates" | "history" | "settings";

export interface RouteDef {
  id: RouteId;
  label: string;
}

// Order and labels match AC-1 / design-system Section 4.3 exactly: Library,
// Tidy-up, Duplicates, History, Settings.
export const ROUTES: readonly RouteDef[] = [
  { id: "library", label: "Library" },
  { id: "tidy-up", label: "Tidy-up" },
  { id: "duplicates", label: "Duplicates" },
  { id: "history", label: "History" },
  { id: "settings", label: "Settings" },
];

export const DEFAULT_ROUTE: RouteId = "library";
