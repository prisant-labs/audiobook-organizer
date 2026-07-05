import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppShell } from "../AppShell";

// The tracer (mounted behind the "Library" route, see AppShell.tsx) calls
// through src/lib/bindings.ts, which needs a real Tauri webview
// (`window.__TAURI_INTERNALS__`) to resolve. Mocked here so the shell can be
// rendered in jsdom without an unhandled rejection; this is the "mocked
// binding" pattern test-strategy.md/the implementation plan use throughout
// (e.g. Phase 2's settings round-trip test). vi.mock calls are hoisted above
// imports by Vitest's transform, so this takes effect before AppShell (and
// the App tracer it mounts) import the real module.
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

// AC-1/AC-2 integration: the shell renders the titlebar, sidebar, and the
// screen container together; the theme toggle flips `data-theme` on the
// document root to exactly "day" or "evening", and switching the active nav
// item moves `aria-current` rather than duplicating it.
describe("AppShell", () => {
  it("renders the titlebar, sidebar, and a screen container", () => {
    render(<AppShell />);
    expect(screen.getByRole("group", { name: "Theme" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(document.querySelector("main")).not.toBeNull();
  });

  it("defaults to the Library route as the active item", () => {
    render(<AppShell />);
    expect(screen.getByRole("button", { name: "Library" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("moves aria-current when a different nav item is clicked, never leaving two active", async () => {
    const user = userEvent.setup();
    render(<AppShell />);

    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "Library" })).not.toHaveAttribute("aria-current");
    expect(screen.getAllByRole("button").filter((b) => b.getAttribute("aria-current"))).toHaveLength(
      1,
    );
  });

  it("toggling the theme control sets data-theme to exactly day or evening", async () => {
    const user = userEvent.setup();
    render(<AppShell />);

    await user.click(screen.getByRole("button", { name: "Evening" }));
    expect(document.documentElement.dataset.theme).toBe("evening");

    await user.click(screen.getByRole("button", { name: "Day" }));
    expect(document.documentElement.dataset.theme).toBe("day");
  });
});
