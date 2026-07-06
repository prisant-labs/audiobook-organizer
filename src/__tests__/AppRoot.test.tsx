import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppRoot } from "@/AppRoot";
import { getSettings, saveSettings, type AppSettings } from "@/lib/settings";
import { pickLibraryFolder } from "@/lib/dialog";

vi.mock("@/lib/settings", () => ({ getSettings: vi.fn(), saveSettings: vi.fn() }));
vi.mock("@/lib/dialog", () => ({ pickLibraryFolder: vi.fn() }));
// The tracer behind the Library route needs the IPC bindings mocked to render
// in jsdom (see AppShell.test).
vi.mock("@/lib/bindings", () => ({
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

const mockedGet = vi.mocked(getSettings);
const mockedSave = vi.mocked(saveSettings);
const mockedPick = vi.mocked(pickLibraryFolder);

const NO_LIBRARY: AppSettings = {
  library_root: null,
  set_aside_root: null,
  reports_root: null,
  theme: "day",
  scan_retention_count: 10,
};
const WITH_LIBRARY: AppSettings = { ...NO_LIBRARY, library_root: "E:\\Books" };

beforeEach(() => {
  mockedGet.mockReset();
  mockedSave.mockReset();
  mockedPick.mockReset();
  window.localStorage.clear();
});

// F-909 AC-28/AC-31: with no persisted root the app opens on first-run; with a
// persisted root it opens on the shell (the backend re-allowed the root at
// startup). This is the routing that the whole security posture rides on.
describe("AppRoot first-run gating", () => {
  it("shows first-run when no library root is configured", async () => {
    mockedGet.mockResolvedValue(NO_LIBRARY);

    render(<AppRoot />);

    expect(
      await screen.findByRole("button", { name: /choose your library folder/i }),
    ).toBeInTheDocument();
    // The shell's nav is NOT present on first-run.
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();
  });

  it("shows the shell when a library root is already configured", async () => {
    mockedGet.mockResolvedValue(WITH_LIBRARY);

    render(<AppRoot />);

    expect(await screen.findByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /choose your library folder/i })).toBeNull();
  });

  it("advances from first-run to the shell once a folder is picked and persisted", async () => {
    const user = userEvent.setup();
    mockedGet.mockResolvedValue(NO_LIBRARY);
    mockedPick.mockResolvedValue("E:\\Books");
    mockedSave.mockResolvedValue(WITH_LIBRARY);

    render(<AppRoot />);
    const choose = await screen.findByRole("button", { name: /choose your library folder/i });
    await user.click(choose);

    // The picked path was persisted via settings_set with library_root set...
    await waitFor(() =>
      expect(mockedSave).toHaveBeenCalledWith(
        expect.objectContaining({ library_root: "E:\\Books" }),
      ),
    );
    // ...and the app advanced to the shell.
    expect(await screen.findByRole("navigation", { name: "Main" })).toBeInTheDocument();
  });
});
