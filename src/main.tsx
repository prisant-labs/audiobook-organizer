import React from "react";
import ReactDOM from "react-dom/client";
import { AppShell } from "./components/shell/AppShell";
import { applyTheme, getStoredTheme } from "./lib/theme";
import "./styles/tokens.css";

// Apply the persisted (or default) theme before the first paint, so the
// shell never flashes the wrong theme while React mounts (AC-2).
applyTheme(getStoredTheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppShell />
  </React.StrictMode>,
);
