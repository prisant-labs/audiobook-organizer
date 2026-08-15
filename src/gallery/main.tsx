import React from "react";
import ReactDOM from "react-dom/client";
import { Gallery } from "./Gallery";
import type { Theme } from "@/lib/theme";
import "../styles/tokens.css";

// Entry point for the dev-only component gallery (gallery.html).
//
// One document serves two roles, decided by the `theme` query parameter:
//
//   /gallery.html               the chrome: a header plus two iframes, one per theme
//   /gallery.html?theme=day     one themed pane, no chrome, which is what the
//   /gallery.html?theme=evening iframes load and what Playwright screenshots
//
// Splitting these would mean a second entry point and a second copy of this
// bootstrap; the parameter keeps it to one file and gives Playwright a clean
// single-theme URL for free.
//
// This is never bundled. vite.config.ts declares no `build.rollupOptions.input`,
// so Vite builds `index.html` alone and gallery.html exists only under
// `pnpm dev`. Do not add it to the build input.

// A stub for the Tauri IPC bridge, which does not exist in a plain browser.
//
// Without it, every component that reaches the backend while rendering throws
// "Cannot read properties of undefined (reading 'invoke')" from inside an
// effect. An error boundary cannot catch that (boundaries catch render throws,
// not async rejections), so the component renders anyway and the failure shows
// up only as console noise, which then masks real errors in this gallery.
//
// Every command answers `null`, which the bindings map to "no data": for
// `cover_get` that is precisely the no-cover case, so BookSlot renders its real
// fallback-tile path rather than a fake. Answering null rather than fabricating
// payloads is deliberate. A stub that invented a cover would make the gallery
// show a state the app cannot actually produce here, which is the drift this
// whole surface exists to catch. Each call is logged so a component quietly
// depending on real data is visible rather than silent.
declare global {
  interface Window {
    __TAURI_INTERNALS__?: { invoke: (command: string, args?: unknown) => Promise<unknown> };
  }
}

if (!window.__TAURI_INTERNALS__) {
  window.__TAURI_INTERNALS__ = {
    invoke: async (command: string) => {
      console.info(`[gallery] IPC stub answered "${command}" with null`);
      return null;
    },
  };
}

const requested = new URLSearchParams(window.location.search).get("theme");
const paneTheme: Theme | null =
  requested === "day" || requested === "evening" ? requested : null;

// A pane paints its requested theme. The chrome has no theme of its own to
// express, so it takes Day, matching the app's default (FD-09, AC-2).
document.documentElement.dataset.theme = paneTheme ?? "day";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Gallery paneTheme={paneTheme} />
  </React.StrictMode>,
);
