import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import { BookSlot } from "../BookSlot";
import { getCover } from "@/lib/covers";
import type { BookExample } from "@/lib/bindings";

vi.mock("@/lib/covers", () => ({ getCover: vi.fn() }));
const mockedGetCover = vi.mocked(getCover);

afterEach(cleanup);
beforeEach(() => {
  mockedGetCover.mockReset();
});

const WARN_BOOK: BookExample = {
  entry_id: 42,
  title: "Sapiens",
  author: "Y. N. Harari",
  reason: { kind: "warn", text: "loose file" },
};

const ALERT_BOOK: BookExample = {
  entry_id: 43,
  title: "Starter Villain",
  author: "John Scalzi",
  reason: { kind: "alert", text: "2 copies" },
};

describe("BookSlot", () => {
  it("fetches the cover for its (scanId, entryId) and renders the fallback while no cover exists", async () => {
    mockedGetCover.mockResolvedValue(null);
    render(<BookSlot scanId={7} book={WARN_BOOK} />);

    await waitFor(() => expect(mockedGetCover).toHaveBeenCalledWith(7, 42));
    expect(screen.getByText("Sapiens")).toBeInTheDocument();
  });

  it("renders the warn reason chip with its text", () => {
    mockedGetCover.mockResolvedValue(null);
    render(<BookSlot scanId={7} book={WARN_BOOK} />);
    expect(screen.getByText("loose file")).toBeInTheDocument();
  });

  it("renders the alert reason chip with its text", () => {
    mockedGetCover.mockResolvedValue(null);
    render(<BookSlot scanId={7} book={ALERT_BOOK} />);
    expect(screen.getByText("2 copies")).toBeInTheDocument();
  });
});
