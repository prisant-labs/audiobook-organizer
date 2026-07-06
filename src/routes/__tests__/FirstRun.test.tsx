import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FirstRun } from "../FirstRun";
import { pickLibraryFolder } from "@/lib/dialog";

// The folder picker is the backend-mediated OS dialog (tauri-plugin-dialog);
// mock the seam so the flow is exercised without a real Tauri runtime.
vi.mock("@/lib/dialog", () => ({ pickLibraryFolder: vi.fn() }));
const mockedPick = vi.mocked(pickLibraryFolder);

beforeEach(() => {
  mockedPick.mockReset();
});

// F-909 AC-28/AC-29: the only path forward is picking a library folder through
// the OS picker; cancelling keeps the user on first-run; a chosen folder is
// handed to the parent to persist.
describe("FirstRun", () => {
  it("offers exactly one primary action: choosing the library folder", () => {
    render(<FirstRun onChoose={vi.fn().mockResolvedValue(undefined)} />);
    const button = screen.getByRole("button", { name: /choose your library folder/i });
    expect(button).toBeInTheDocument();
  });

  it("blocks progress when the picker is cancelled (no folder chosen)", async () => {
    const user = userEvent.setup();
    const onChoose = vi.fn().mockResolvedValue(undefined);
    mockedPick.mockResolvedValue(null); // user cancelled the OS dialog

    render(<FirstRun onChoose={onChoose} />);
    await user.click(screen.getByRole("button", { name: /choose your library folder/i }));

    await waitFor(() => expect(mockedPick).toHaveBeenCalledTimes(1));
    expect(onChoose).not.toHaveBeenCalled();
    // Still on first-run: the choose action is available again.
    expect(
      await screen.findByRole("button", { name: /choose your library folder/i }),
    ).toBeEnabled();
  });

  it("hands the chosen folder to onChoose so the parent can persist it", async () => {
    const user = userEvent.setup();
    const onChoose = vi.fn().mockResolvedValue(undefined);
    mockedPick.mockResolvedValue("E:\\Books - Audio");

    render(<FirstRun onChoose={onChoose} />);
    await user.click(screen.getByRole("button", { name: /choose your library folder/i }));

    await waitFor(() => expect(onChoose).toHaveBeenCalledExactlyOnceWith("E:\\Books - Audio"));
  });

  it("shows a plain-language error and stays put when saving the folder fails", async () => {
    const user = userEvent.setup();
    const onChoose = vi.fn().mockRejectedValue(new Error("settings-failed: database is locked"));
    mockedPick.mockResolvedValue("E:\\Books - Audio");

    render(<FirstRun onChoose={onChoose} />);
    await user.click(screen.getByRole("button", { name: /choose your library folder/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/could not be saved/i);
    expect(screen.getByRole("button", { name: /choose your library folder/i })).toBeEnabled();
  });
});
