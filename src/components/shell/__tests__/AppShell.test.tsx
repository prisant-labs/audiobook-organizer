import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppShell } from "../AppShell";
import type { AppSettings } from "@/lib/settings";

// The tracer (mounted behind the "Library" route, see AppShell.tsx) calls
// through src/lib/bindings.ts, which needs a real Tauri webview
// (`window.__TAURI_INTERNALS__`) to resolve. Mocked here so the shell can be
// rendered in jsdom without an unhandled rejection; this is the "mocked
// binding" pattern the implementation plan uses throughout. vi.mock calls are
// hoisted above imports by Vitest's transform.
vi.mock("../../../lib/bindings", () => ({
  commands: {
    dbStatus: vi.fn().mockResolvedValue({ recovered: false, backup_path: null }),
    scanStart: vi.fn(),
    scanEntries: vi.fn(),
  },
  events: {
    jobCompleted: { listen: vi.fn().mockResolvedValue(() => {}) },
    jobFailed: { listen: vi.fn().mockResolvedValue(() => {}) },
  },
}));

const SETTINGS: AppSettings = {
  library_root: "E:\\Books",
  set_aside_root: null,
  reports_root: null,
  theme: "day",
  scan_retention_count: 10,
};

function renderShell(onUpdate = vi.fn().mockResolvedValue(undefined)) {
  render(<AppShell settings={SETTINGS} onUpdate={onUpdate} />);
  return onUpdate;
}

// AC-1: the shell renders the titlebar, sidebar, and screen container together;
// the active nav item carries aria-current and only one at a time. Theme changes
// are delegated to the parent via onUpdate (useAppSettings applies data-theme;
// AppShell itself does not, so the assertion is on the callback).
describe("AppShell", () => {
  it("renders the titlebar, sidebar, and a screen container", () => {
    renderShell();
    expect(screen.getByRole("group", { name: "Theme" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(document.querySelector("main")).not.toBeNull();
  });

  it("defaults to the Library route as the active item", () => {
    renderShell();
    expect(screen.getByRole("button", { name: "Library" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("moves aria-current when a different nav item is clicked, never leaving two active", async () => {
    const user = userEvent.setup();
    renderShell();

    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "Library" })).not.toHaveAttribute("aria-current");
    expect(
      screen.getAllByRole("button").filter((b) => b.getAttribute("aria-current")),
    ).toHaveLength(1);
  });

  it("toggling the theme control persists the flipped theme through onUpdate", async () => {
    const user = userEvent.setup();
    const onUpdate = renderShell();

    await user.click(screen.getByRole("button", { name: "Evening" }));
    expect(onUpdate).toHaveBeenCalledWith(expect.objectContaining({ theme: "evening" }));
  });

  it("routes the Settings nav item to the real Settings screen", async () => {
    const user = userEvent.setup();
    renderShell();

    await user.click(screen.getByRole("button", { name: "Settings" }));
    // The Settings screen shows the configured library folder (F-803/F-909).
    expect(screen.getByText("E:\\Books")).toBeInTheDocument();
  });
});
