import { describe, expect, it, vi, beforeEach } from "vitest";
import { getLibraryOverview, OverviewError } from "../overview";
import { commands } from "../bindings";
import type { LibraryOverview } from "../bindings";

vi.mock("../bindings", () => ({
  commands: { classifyOverview: vi.fn() },
}));
const mockedClassifyOverview = vi.mocked(commands.classifyOverview);

const OVERVIEW: LibraryOverview = {
  scan_id: 1,
  total_books: 1,
  total_bytes: 1,
  needs_tidy_books: 0,
  worth_a_look: [],
  series: [],
  good_news: {
    already_tidy_books: 1,
    series_shelved: 0,
    empty_folders: 0,
    duplicate_groups: 0,
    duplicate_bytes: 0,
  },
  metrics: { per_class: [], problems: [], total_bytes: 1 },
};

beforeEach(() => {
  mockedClassifyOverview.mockReset();
});

describe("getLibraryOverview", () => {
  it("returns the overview on success", async () => {
    mockedClassifyOverview.mockResolvedValue({ status: "ok", data: OVERVIEW });
    await expect(getLibraryOverview()).resolves.toEqual(OVERVIEW);
  });

  it("returns null when no scan has ever completed (not an error)", async () => {
    mockedClassifyOverview.mockResolvedValue({ status: "ok", data: null });
    await expect(getLibraryOverview()).resolves.toBeNull();
  });

  it("throws an OverviewError on an AppError result", async () => {
    mockedClassifyOverview.mockResolvedValue({
      status: "error",
      error: { "scan-failed": { detail: "database is locked" } },
    });
    await expect(getLibraryOverview()).rejects.toBeInstanceOf(OverviewError);
    await expect(getLibraryOverview()).rejects.toThrow(/scan-failed/);
  });
});
