import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { useAppSettings } from "../useAppSettings";
import { getSettings, saveSettings, type AppSettings } from "@/lib/settings";

// Mock the settings client (the settings_get/settings_set binding wrapper) so
// the round-trip is exercised without a real Tauri runtime - the "mocked
// binding" pattern the implementation plan's Phase 2 settings round-trip test
// calls for.
vi.mock("@/lib/settings", () => ({
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
}));
const mockedGet = vi.mocked(getSettings);
const mockedSave = vi.mocked(saveSettings);

const DAY: AppSettings = {
  library_root: "E:\\Books",
  set_aside_root: null,
  reports_root: null,
  theme: "day",
  scan_retention_count: 10,
};

beforeEach(() => {
  mockedGet.mockReset();
  mockedSave.mockReset();
  window.localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  window.localStorage.clear();
});

describe("useAppSettings", () => {
  it("loads persisted settings and applies the theme", async () => {
    mockedGet.mockResolvedValue(DAY);

    const { result } = renderHook(() => useAppSettings());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(result.current.settings).toEqual(DAY);
    expect(document.documentElement.dataset.theme).toBe("day");
  });

  it("round-trips an update through settings_set and reflects the stored form", async () => {
    mockedGet.mockResolvedValue(DAY);
    const persisted: AppSettings = { ...DAY, theme: "evening", scan_retention_count: 20 };
    mockedSave.mockResolvedValue(persisted);

    const { result } = renderHook(() => useAppSettings());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    await act(async () => {
      await result.current.update({ ...DAY, theme: "evening", scan_retention_count: 20 });
    });

    expect(mockedSave).toHaveBeenCalledWith(
      expect.objectContaining({ theme: "evening", scan_retention_count: 20 }),
    );
    expect(result.current.settings).toEqual(persisted);
    expect(document.documentElement.dataset.theme).toBe("evening");
  });

  it("migrates a legacy localStorage theme into settings exactly once, then clears it", async () => {
    // A pre-Phase-2 user carrying an "evening" choice in the old shim key, while
    // the persisted settings still read the Day default.
    window.localStorage.setItem("abo.theme", "evening");
    mockedGet.mockResolvedValue(DAY);
    const migrated: AppSettings = { ...DAY, theme: "evening" };
    mockedSave.mockResolvedValue(migrated);

    const { result } = renderHook(() => useAppSettings());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    // The legacy value was written into settings once, and the key now holds
    // the pre-paint hint for the migrated (authoritative) Evening theme; the
    // hint is paint-only and never read as authority.
    expect(mockedSave).toHaveBeenCalledWith(expect.objectContaining({ theme: "evening" }));
    expect(window.localStorage.getItem("abo.theme")).toBe("evening");
    expect(result.current.settings?.theme).toBe("evening");
    expect(document.documentElement.dataset.theme).toBe("evening");
  });

  it("does not migrate when the legacy theme already matches settings (settings wins)", async () => {
    window.localStorage.setItem("abo.theme", "day");
    mockedGet.mockResolvedValue(DAY);

    const { result } = renderHook(() => useAppSettings());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    expect(mockedSave).not.toHaveBeenCalled();
    // The key carries the pre-paint hint for the authoritative theme; settings
    // remains the sole source of truth.
    expect(window.localStorage.getItem("abo.theme")).toBe("day");
  });

  it("reverts optimistic state and rethrows when a persist fails", async () => {
    mockedGet.mockResolvedValue(DAY);
    mockedSave.mockRejectedValue(new Error("settings-failed: database is locked"));

    const { result } = renderHook(() => useAppSettings());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    await expect(
      act(async () => {
        await result.current.update({ ...DAY, theme: "evening" });
      }),
    ).rejects.toThrow(/settings-failed/);

    // Reverted to the last-known-good settings and theme.
    expect(result.current.settings).toEqual(DAY);
    expect(document.documentElement.dataset.theme).toBe("day");
  });

  it("surfaces an error status when the initial load fails", async () => {
    mockedGet.mockRejectedValue(new Error("settings-failed: unreadable"));

    const { result } = renderHook(() => useAppSettings());
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.error).toMatch(/settings-failed/);
    expect(result.current.settings).toBeNull();
  });
});
