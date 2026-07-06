import React from "react";
import ReactDOM from "react-dom/client";
import { AppRoot } from "./AppRoot";
import { applyTheme, bootTheme } from "./lib/theme";
import "./styles/tokens.css";

// Apply a synchronous pre-paint theme hint before the first paint, so the shell
// does not flash the wrong theme while React mounts and the async settings load
// resolves (AC-2). This is a paint hint only (the legacy value if a migration is
// pending, else Day); useAppSettings then reconciles to the persisted setting.
applyTheme(bootTheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppRoot />
  </React.StrictMode>,
);
