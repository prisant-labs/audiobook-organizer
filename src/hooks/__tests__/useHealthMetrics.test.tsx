import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useHealthMetrics } from "../useHealthMetrics";
import { getLibraryOverview } from "@/lib/overview";
import type { LibraryOverview } from "@/lib/bindings";

// "Mocked bindings" pattern (test-strategy Frontend layer): mock the
// lib/overview client, not the raw generated binding, matching
// useAppSettings.test.tsx / Settings.test.tsx.
vi.mock("@/lib/overview", () => ({ getLibraryOverview: vi.fn() }));
const mockedGet = vi.mocked(getLibraryOverview);

const OVERVIEW: LibraryOverview = {
  scan_id: 1,
  total_books: 4,
  total_bytes: 400_000,
  needs_tidy_books: 1,
  worth_a_look: [],
  series: [],
  good_news: {
    already_tidy_books: 3,
    series_shelved: 0,
    empty_folders: 0,
    duplicate_groups: 0,
    duplicate_bytes: 0,
  },
  metrics: { per_class: [], problems: [], total_bytes: 400_000 },
};

beforeEach(() => {
  mockedGet.mockReset();
});

describe("useHealthMetrics", () => {
  it("loads the overview and reports ready", async () => {
    mockedGet.mockResolvedValue(OVERVIEW);

    const { result } = renderHook(() => useHealthMetrics());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.overview).toEqual(OVERVIEW);
    expect(result.current.error).toBeNull();
  });

  it("reports null overview (not an error) when no scan has ever completed", async () => {
    mockedGet.mockResolvedValue(null);

    const { result } = renderHook(() => useHealthMetrics());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.overview).toBeNull();
  });

  it("surfaces an error status when the load fails", async () => {
    mockedGet.mockRejectedValue(new Error("overview-failed: database is locked"));

    const { result } = renderHook(() => useHealthMetrics());
    await waitFor(() => expect(result.current.status).toBe("error"));

    expect(result.current.error).toMatch(/overview-failed/);
    expect(result.current.overview).toBeNull();
  });

  it("reload() re-fetches", async () => {
    mockedGet.mockResolvedValueOnce(null).mockResolvedValueOnce(OVERVIEW);

    const { result } = renderHook(() => useHealthMetrics());
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.overview).toBeNull();

    act(() => result.current.reload());
    await waitFor(() => expect(result.current.overview).toEqual(OVERVIEW));
    expect(mockedGet).toHaveBeenCalledTimes(2);
  });
});
