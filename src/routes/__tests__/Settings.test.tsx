import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Settings } from "../Settings";
import { pickLibraryFolder } from "@/lib/dialog";
import type { AppSettings } from "@/lib/settings";

vi.mock("@/lib/dialog", () => ({ pickLibraryFolder: vi.fn() }));
// The ruleset editor (F-906) has its own dedicated test coverage
// (RulesetEditor.test.tsx); stubbing it here keeps this file focused on
// Settings.tsx's own behavior (folders/theme/retention).
vi.mock("@/components/settings/RulesetEditor", () => ({ RulesetEditor: () => null }));
const mockedPick = vi.mocked(pickLibraryFolder);

const BASE: AppSettings = {
  library_root: "E:\\Books",
  set_aside_root: null,
  reports_root: null,
  theme: "day",
  scan_retention_count: 10,
};

beforeEach(() => {
  mockedPick.mockReset();
});

// F-803 + F-909 re-selection (AC-34/AC-35): the Settings screen shows the
// configured values and persists edits via onUpdate (the settings_set path).
describe("Settings", () => {
  it("shows the configured library folder", () => {
    render(<Settings settings={BASE} onUpdate={vi.fn().mockResolvedValue(undefined)} scanId={null} />);
    expect(screen.getByText("E:\\Books")).toBeInTheDocument();
  });

  it("persists a re-selected library folder through onUpdate", async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn().mockResolvedValue(undefined);
    mockedPick.mockResolvedValue("E:\\New Library");

    render(<Settings settings={BASE} onUpdate={onUpdate} scanId={null} />);
    await user.click(screen.getByRole("button", { name: /change folder/i }));

    await waitFor(() =>
      expect(onUpdate).toHaveBeenCalledWith(
        expect.objectContaining({ library_root: "E:\\New Library" }),
      ),
    );
  });

  it("does nothing when the folder picker is cancelled", async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn().mockResolvedValue(undefined);
    mockedPick.mockResolvedValue(null);

    render(<Settings settings={BASE} onUpdate={onUpdate} scanId={null} />);
    await user.click(screen.getByRole("button", { name: /change folder/i }));

    await waitFor(() => expect(mockedPick).toHaveBeenCalledTimes(1));
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("persists a theme change through onUpdate (theme lives in settings, FD-09)", async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn().mockResolvedValue(undefined);

    render(<Settings settings={BASE} onUpdate={onUpdate} scanId={null} />);
    await user.click(screen.getByRole("button", { name: "Evening" }));

    expect(onUpdate).toHaveBeenCalledWith(expect.objectContaining({ theme: "evening" }));
  });

  it("persists a snapshot-retention change on blur (AC-35)", async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn().mockResolvedValue(undefined);

    render(<Settings settings={BASE} onUpdate={onUpdate} scanId={null} />);
    const input = screen.getByRole("spinbutton", { name: /scans to keep/i });
    await user.clear(input);
    await user.type(input, "25");
    await user.tab(); // blur commits

    await waitFor(() =>
      expect(onUpdate).toHaveBeenCalledWith(
        expect.objectContaining({ scan_retention_count: 25 }),
      ),
    );
  });

  it("does not persist an unchanged retention value", async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn().mockResolvedValue(undefined);

    render(<Settings settings={BASE} onUpdate={onUpdate} scanId={null} />);
    const input = screen.getByRole("spinbutton", { name: /scans to keep/i });
    await user.click(input);
    await user.tab(); // blur without changing

    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("clears an override back to the default location", async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn().mockResolvedValue(undefined);
    const withAside: AppSettings = { ...BASE, set_aside_root: "E:\\Aside" };

    render(<Settings settings={withAside} onUpdate={onUpdate} scanId={null} />);
    // Two "Use default" buttons appear (set-aside + reports); the set-aside one
    // is the first, since reports has no value so no button.
    const useDefault = screen.getAllByRole("button", { name: /use default/i })[0];
    await user.click(useDefault);

    await waitFor(() =>
      expect(onUpdate).toHaveBeenCalledWith(expect.objectContaining({ set_aside_root: null })),
    );
  });
});
