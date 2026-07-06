import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useRulesetEditor } from "../useRulesetEditor";
import { getActiveRuleset, previewPlan, saveRuleset } from "@/lib/ruleset";
import type { PlanGroupView, Ruleset, RulesetDetail } from "@/lib/bindings";

// F-906 ruleset editor (T-26/T-27, AC-33): "mocked bindings" pattern one
// layer up (`@/lib/ruleset`, mirroring `usePlanReview.test.tsx`'s mock of
// `@/lib/plan`).
vi.mock("@/lib/ruleset", () => ({
  getActiveRuleset: vi.fn(),
  previewPlan: vi.fn(),
  saveRuleset: vi.fn(),
}));

const mockedGetActive = vi.mocked(getActiveRuleset);
const mockedPreview = vi.mocked(previewPlan);
const mockedSave = vi.mocked(saveRuleset);

const RULESET: Ruleset = {
  schema_version: 1,
  naming: { preset: "abs-author-first", series_index_width: 1 },
  structure: {
    one_book_per_folder: true,
    pack_shell: "quarantine",
    sidecars: "keep-with-book",
    clutter: {
      ebook: "keep",
      cover: "keep",
      nfo: "quarantine",
      sfv: "quarantine",
      playlist: "quarantine",
      weblink: "quarantine",
    },
    preferred_format: "m4b",
    empty_folder_removal: true,
  },
  cleanup: { strip_noise: true },
};

const DETAIL: RulesetDetail = { id: 3, name: "Default", is_active: true, ruleset: RULESET };

function groups(overrides: Partial<PlanGroupView> = {}): PlanGroupView[] {
  return [
    {
      group: "loose-books",
      label: "loose books",
      headline: "h",
      reason: "r",
      op_count: 1,
      actionable_count: 1,
      byte_size: 10,
      status: "included",
      warning_count: 0,
      blocked_count: 0,
      ...overrides,
    },
  ];
}

beforeEach(() => {
  mockedGetActive.mockReset().mockResolvedValue(DETAIL);
  mockedPreview.mockReset().mockResolvedValue(groups());
  mockedSave.mockReset().mockResolvedValue(DETAIL);
});

describe("useRulesetEditor", () => {
  it("loads the active ruleset on mount", async () => {
    const { result } = renderHook(() => useRulesetEditor(null));
    expect(result.current.status).toBe("loading");
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.draft).toEqual(RULESET);
    expect(result.current.dirty).toBe(false);
  });

  it("surfaces an error status when loading the active ruleset fails", async () => {
    mockedGetActive.mockReset().mockRejectedValue(new Error("ruleset-operation-failed: locked"));
    const { result } = renderHook(() => useRulesetEditor(null));
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.error).toMatch(/ruleset-operation-failed/);
  });

  it("never previews without a scan id", async () => {
    const { result } = renderHook(() => useRulesetEditor(null));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.previewStatus).toBe("idle");

    await new Promise((r) => setTimeout(r, 450));
    expect(mockedPreview).not.toHaveBeenCalled();
  });

  it("previews the draft against the scan, debounced", async () => {
    const { result } = renderHook(() => useRulesetEditor(9));
    await waitFor(() => expect(result.current.status).toBe("ready"));

    await waitFor(() => expect(result.current.previewStatus).toBe("ready"), { timeout: 2000 });
    expect(mockedPreview).toHaveBeenCalledTimes(1);
    expect(mockedPreview).toHaveBeenCalledWith(9, RULESET);
    expect(result.current.preview).toEqual(groups());
  });

  it("marks the draft dirty on edit and updates only the touched field", async () => {
    const { result } = renderHook(() => useRulesetEditor(null));
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => {
      result.current.updateDraft((prev) => ({
        ...prev,
        naming: { ...prev.naming, preset: "title-first" },
      }));
    });

    expect(result.current.dirty).toBe(true);
    expect(result.current.draft?.naming.preset).toBe("title-first");
    expect(result.current.draft?.structure).toEqual(RULESET.structure);
  });

  it("debounces rapid edits into exactly one preview call carrying the latest draft", async () => {
    const { result } = renderHook(() => useRulesetEditor(9));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    await waitFor(() => expect(mockedPreview).toHaveBeenCalledTimes(1), { timeout: 2000 });
    mockedPreview.mockClear();

    // Three edits in quick succession (well within the debounce window)
    // must collapse into exactly one preview call, for the LAST draft.
    act(() => {
      result.current.updateDraft((prev) => ({
        ...prev,
        structure: { ...prev.structure, one_book_per_folder: false },
      }));
    });
    act(() => {
      result.current.updateDraft((prev) => ({
        ...prev,
        structure: { ...prev.structure, empty_folder_removal: false },
      }));
    });
    act(() => {
      result.current.updateDraft((prev) => ({ ...prev, cleanup: { strip_noise: false } }));
    });

    await waitFor(() => expect(mockedPreview).toHaveBeenCalledTimes(1), { timeout: 2000 });
    const [, draftUsed] = mockedPreview.mock.calls[0];
    expect(draftUsed.structure.one_book_per_folder).toBe(false);
    expect(draftUsed.structure.empty_folder_removal).toBe(false);
    expect(draftUsed.cleanup.strip_noise).toBe(false);
  });

  it("save() persists the draft as the active ruleset and clears dirty", async () => {
    const { result } = renderHook(() => useRulesetEditor(null));
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => {
      result.current.updateDraft((prev) => ({
        ...prev,
        naming: { ...prev.naming, preset: "hybrid-genre" },
      }));
    });
    expect(result.current.dirty).toBe(true);

    const savedDetail: RulesetDetail = {
      ...DETAIL,
      ruleset: { ...RULESET, naming: { ...RULESET.naming, preset: "hybrid-genre" } },
    };
    mockedSave.mockResolvedValue(savedDetail);

    await act(async () => {
      await result.current.save();
    });

    expect(mockedSave).toHaveBeenCalledWith({
      id: 3,
      name: "Default",
      ruleset: { ...RULESET, naming: { ...RULESET.naming, preset: "hybrid-genre" } },
    });
    expect(result.current.saveStatus).toBe("saved");
    expect(result.current.dirty).toBe(false);
  });

  it("surfaces a save error without losing the draft", async () => {
    const { result } = renderHook(() => useRulesetEditor(null));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    act(() => {
      result.current.updateDraft((prev) => ({
        ...prev,
        naming: { ...prev.naming, preset: "title-first" },
      }));
    });
    mockedSave.mockRejectedValue(new Error("ruleset-invalid: bad width"));

    await act(async () => {
      await result.current.save();
    });

    expect(result.current.saveStatus).toBe("error");
    expect(result.current.saveError).toMatch(/ruleset-invalid/);
    expect(result.current.draft?.naming.preset).toBe("title-first");
  });
});
