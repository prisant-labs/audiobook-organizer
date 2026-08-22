// The open-a-folder affordance (F-610, v0.6.0 P10, AC-47 to AC-50).
//
// What these hold to account is the half that can go wrong in the browser: the
// component must ASK the backend and must SHOW a refusal. AC-48's actual gate is
// backend logic and is tested exhaustively in `abo_core::reveal`, including the
// cases that matter (a sibling folder sharing the root's name prefix, a `..`
// traversal, a path that no longer exists). Re-asserting the rule here would be
// a second implementation of it, which is the thing the design avoids.
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OpenFolder, OpenRootLink } from "../OpenFolder";
import { commands } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";

vi.mock("@/lib/bindings", () => ({
  commands: {
    revealInFolder: vi.fn(),
    revealRoot: vi.fn(),
  },
}));

const mockedReveal = vi.mocked(commands.revealInFolder);
const mockedRoot = vi.mocked(commands.revealRoot);

beforeEach(() => {
  vi.clearAllMocks();
  mockedReveal.mockResolvedValue({ status: "ok", data: null } as never);
  mockedRoot.mockResolvedValue({ status: "ok", data: null } as never);
});

describe("OpenFolder", () => {
  // NOTE for the next reader: `path={"..."}` and not `path="..."`. A JSX
  // attribute string is literal, so `path="E:\x"` would pass TWO backslashes
  // and this test would be asserting against a path no real caller sends.
  it("asks the backend for the exact path it was given", async () => {
    render(<OpenFolder path={"E:\\Books - Audio\\Andy Weir"} />);
    await userEvent.click(screen.getByRole("button"));
    expect(mockedReveal).toHaveBeenCalledWith("E:\\Books - Audio\\Andy Weir");
  });

  // FD-29: the WebView has no fs and no shell capability, and AC-47 keeps the
  // allowlist unchanged. The only correct implementation is to ask.
  it("opens nothing itself", async () => {
    render(<OpenFolder path={"E:\\Books - Audio"} />);
    await userEvent.click(screen.getByRole("button"));
    expect(mockedReveal).toHaveBeenCalledTimes(1);
  });

  it("carries a distinct accessible name when one is given", () => {
    render(<OpenFolder path={"E:\\Books - Audio\\a.m4b"} label="Open the folder this copy is in" />);
    expect(
      screen.getByRole("button", { name: "Open the folder this copy is in" }),
    ).toBeInTheDocument();
  });

  it("falls back to a generic accessible name", () => {
    render(<OpenFolder path={"E:\\Books - Audio"} />);
    expect(screen.getByRole("button", { name: STRINGS.openFolder.open })).toBeInTheDocument();
  });

  // AC-48 requires a refusal rather than silence, and a fire-and-forget click is
  // exactly silence. The likeliest refusal is ordinary: the folder moved since
  // the scan that displayed it.
  it("shows a refusal instead of failing silently", async () => {
    mockedReveal.mockResolvedValue({
      status: "error",
      error: { "reveal-refused": { path: "E:\\Elsewhere" } },
    } as never);

    render(<OpenFolder path={"E:\\Elsewhere"} />);
    await userEvent.click(screen.getByRole("button"));

    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());
    expect(screen.getByRole("status").textContent).toBeTruthy();
  });

  it("clears a previous refusal when tried again", async () => {
    mockedReveal.mockResolvedValueOnce({
      status: "error",
      error: { "reveal-refused": { path: "E:\\Elsewhere" } },
    } as never);

    render(<OpenFolder path={"E:\\Books - Audio"} />);
    const button = screen.getByRole("button");

    await userEvent.click(button);
    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());

    await userEvent.click(button);
    await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
  });
});

describe("OpenRootLink", () => {
  // AC-50. The link sends a ROOT NAME, never a path: the Archive root is usually
  // unset in settings because the plan builder derives it, and reconstructing
  // that derivation here would be a second implementation that drifts.
  it("sends a root name rather than a path", async () => {
    render(<OpenRootLink root="archive" label={STRINGS.openFolder.archive} />);
    await userEvent.click(screen.getByRole("button", { name: STRINGS.openFolder.archive }));
    expect(mockedRoot).toHaveBeenCalledWith("archive");
    expect(mockedReveal).not.toHaveBeenCalled();
  });

  it("sends the library root by name too", async () => {
    render(<OpenRootLink root="library" label={STRINGS.openFolder.library} />);
    await userEvent.click(screen.getByRole("button", { name: STRINGS.openFolder.library }));
    expect(mockedRoot).toHaveBeenCalledWith("library");
  });

  // The Archive folder legitimately does not exist until something is archived,
  // so this refusal is a normal state rather than an edge case.
  it("shows a refusal when the root cannot be opened", async () => {
    mockedRoot.mockResolvedValue({
      status: "error",
      error: { "reveal-refused": { path: "" } },
    } as never);

    render(<OpenRootLink root="archive" label={STRINGS.openFolder.archive} />);
    await userEvent.click(screen.getByRole("button", { name: STRINGS.openFolder.archive }));

    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());
  });
});
