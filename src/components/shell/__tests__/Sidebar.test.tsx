import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Sidebar } from "../Sidebar";

// AC-1 / AC-3: the sidebar links all five destinations, the active route
// carries aria-current="page" (and only that one), and the Duplicates badge
// is a GROUP count (never "copies" wording).
describe("Sidebar", () => {
  it("renders all five destinations in order", () => {
    render(<Sidebar active="library" onNavigate={vi.fn()} counts={{}} hasLibrary={false} />);
    const items = screen.getAllByRole("button").map((b) => b.textContent);
    expect(items[0]).toContain("Library");
    expect(items[1]).toContain("Organize");
    expect(items[2]).toContain("Duplicates");
    expect(items[3]).toContain("History");
    expect(items[4]).toContain("Settings");
  });

  it("marks only the active route with aria-current=page", () => {
    render(<Sidebar active="duplicates" onNavigate={vi.fn()} counts={{}} hasLibrary={false} />);

    expect(screen.getByRole("button", { name: /Duplicates/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "Library" })).not.toHaveAttribute("aria-current");
    expect(screen.getByRole("button", { name: "Settings" })).not.toHaveAttribute("aria-current");
  });

  it("calls onNavigate with the clicked route id", async () => {
    const onNavigate = vi.fn();
    const user = userEvent.setup();
    render(<Sidebar active="library" onNavigate={onNavigate} counts={{}} hasLibrary={false} />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith("settings");
  });

  it("shows the Duplicates badge as a group count, not a copies count", () => {
    render(<Sidebar active="library" onNavigate={vi.fn()} counts={{ duplicateGroups: 403 }} hasLibrary={false} />);
    const duplicates = screen.getByRole("button", { name: /Duplicates/ });
    expect(duplicates).toHaveTextContent("403");
    expect(duplicates.textContent).not.toMatch(/copies/i);
  });

  it("omits the badge rather than showing a fabricated 0 when the count is unknown", () => {
    render(<Sidebar active="library" onNavigate={vi.fn()} counts={{}} hasLibrary={false} />);
    const duplicates = screen.getByRole("button", { name: /Duplicates/ });
    expect(duplicates.textContent).toBe("Duplicates");
  });
});
