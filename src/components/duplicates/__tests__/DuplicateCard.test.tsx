import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { DuplicateGroupCard } from "@/lib/bindings";
import { DuplicateCard } from "../DuplicateCard";

// The two paths to a decision (AC-12, AC-13), which is the whole safety story on
// this surface. The BACKEND enforces the gate; what these tests pin is that the
// card offers the right affordance for what is actually known, because offering
// the plain button on unverified copies would put a person one click from
// archiving files nothing has compared.

const COPY = (id: number, keeper: boolean) => ({
  entry_id: id,
  path: `E:\\Books\\${id}\\Dune.m4b`,
  size_bytes: 900,
  check: "not-checked" as const,
  check_label: "not checked yet",
  check_reason: null,
  suggested_keeper: keeper,
});

const GROUP: DuplicateGroupCard = {
  book: "Dune",
  group_key: "Dune.m4b|900",
  method: "exact-basename-size",
  found_by: "same file name and size",
  copies: [COPY(1, true), COPY(2, false)],
  copy_count: 2,
  candidate_bytes_estimate: 1800,
  keeper_reason: "the copies were equivalent, so the first one is kept",
  content_verified: false,
  confirmed_keeper: null,
};

const verified: DuplicateGroupCard = {
  ...GROUP,
  content_verified: true,
  copies: GROUP.copies.map((c) => ({
    ...c,
    check: "checked" as const,
    check_label: "contents checked",
  })),
};

describe("DuplicateCard (F-905, AC-17, AC-12/AC-13)", () => {
  it("names the book, never the detector's join key", () => {
    render(<DuplicateCard group={GROUP} onConfirm={vi.fn()} onClear={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Dune" })).toBeInTheDocument();
    // The key reads like `Dune.m4b|900`. Showing it would promise a book name
    // and deliver a detector artifact, which is the defect the P4 CSV shipped.
    expect(screen.queryByText(/Dune\.m4b\|900/)).not.toBeInTheDocument();
  });

  it("confirms immediately when the copies have been proved identical", async () => {
    const onConfirm = vi.fn();
    render(<DuplicateCard group={verified} onConfirm={onConfirm} onClear={vi.fn()} />);

    await userEvent.click(screen.getAllByRole("button", { name: "Keep this one" })[0]);

    expect(onConfirm).toHaveBeenCalledWith(1, [2], false);
  });

  it("routes an unchecked group through the two-step override instead", async () => {
    const onConfirm = vi.fn();
    render(<DuplicateCard group={GROUP} onConfirm={onConfirm} onClear={vi.fn()} />);

    await userEvent.click(screen.getAllByRole("button", { name: "Keep this one" })[0]);

    // The first press must NOT decide anything. That is the whole of AC-13's
    // "a deliberate two-step affordance, not a default".
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Archive without checking" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Archive without checking" }));
    await userEvent.click(screen.getByRole("button", { name: "Archive anyway" }));

    // And when it finally does, it says plainly that this was an override.
    expect(onConfirm).toHaveBeenCalledWith(1, [2], true);
  });

  it("offers no keep button once a decision has been made, only a way back", async () => {
    const onClear = vi.fn();
    render(
      <DuplicateCard
        group={{ ...verified, confirmed_keeper: 1 }}
        onConfirm={vi.fn()}
        onClear={onClear}
      />,
    );

    expect(screen.queryByRole("button", { name: "Keep this one" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Change my mind" }));
    expect(onClear).toHaveBeenCalled();
  });

  it("shows why a copy could not be read rather than only that it was not checked", () => {
    const unreadable: DuplicateGroupCard = {
      ...GROUP,
      copies: [
        GROUP.copies[0],
        {
          ...GROUP.copies[1],
          check: "could-not-read",
          check_label: "could not be read",
          check_reason: "os error 5: access is denied",
        },
      ],
    };
    render(<DuplicateCard group={unreadable} onConfirm={vi.fn()} onClear={vi.fn()} />);

    expect(screen.getByText(/could not be read: os error 5/)).toBeInTheDocument();
  });
});
