import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RulesetEditor } from "../RulesetEditor";
import { getActiveRuleset, getPresetExamples, previewPlan, saveRuleset } from "@/lib/ruleset";
import type { PlanGroupView, Ruleset, RulesetDetail } from "@/lib/bindings";

// F-906 ruleset editor (T-26/T-27, AC-32/AC-33): mocked at the `@/lib/ruleset`
// client boundary (mirrors Library.test.tsx's "mocked bindings" pattern one
// layer up, since RulesetEditor never imports `@/lib/bindings` directly).
vi.mock("@/lib/ruleset", () => ({
  getActiveRuleset: vi.fn(),
  getPresetExamples: vi.fn(),
  previewPlan: vi.fn(),
  saveRuleset: vi.fn(),
}));

const mockedGetActive = vi.mocked(getActiveRuleset);
const mockedExamples = vi.mocked(getPresetExamples);
const mockedPreview = vi.mocked(previewPlan);
const mockedSave = vi.mocked(saveRuleset);

const DEFAULT_RULESET: Ruleset = {
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

const DETAIL: RulesetDetail = { id: 1, name: "Default", is_active: true, ruleset: DEFAULT_RULESET };

const GROUPS: PlanGroupView[] = [
  {
    group: "loose-books",
    label: "loose books",
    headline: "Give 3 loose books their own folders",
    reason: "why",
    op_count: 3,
    actionable_count: 3,
    byte_size: 100,
    status: "included",
    warning_count: 0,
    blocked_count: 0,
  },
];

afterEach(cleanup);
beforeEach(() => {
  mockedGetActive.mockReset().mockResolvedValue(DETAIL);
  mockedExamples.mockReset().mockResolvedValue([
    { preset: "abs-author-first", example_path: "Author\\Series\\Book 1 - 2010 - Title" },
    { preset: "title-first", example_path: "Title - Author (2010)" },
    { preset: "hybrid-genre", example_path: "Genre\\Author\\Series\\Book 1 - 2010 - Title" },
  ]);
  mockedPreview.mockReset().mockResolvedValue(GROUPS);
  mockedSave.mockReset().mockResolvedValue(DETAIL);
});

describe("RulesetEditor", () => {
  it("loads the active ruleset and shows the current preset selected with its example path", async () => {
    render(<RulesetEditor scanId={5} />);

    const authorFirst = await screen.findByRole("radio", { name: /author first/i });
    expect(authorFirst).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Author\\Series\\Book 1 - 2010 - Title")).toBeInTheDocument();
  });

  it("switching the naming style marks the draft dirty and re-renders the picker", async () => {
    const user = userEvent.setup();
    render(<RulesetEditor scanId={5} />);
    await screen.findByRole("radio", { name: /author first/i });

    await user.click(screen.getByRole("radio", { name: /title first/i }));

    expect(screen.getByRole("radio", { name: /title first/i })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: /author first/i })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/aren't saved yet/i)).toBeInTheDocument();
  });

  it("shows an honest 'scan first' note and never previews when there is no scan", async () => {
    render(<RulesetEditor scanId={null} />);
    await screen.findByRole("radio", { name: /author first/i });

    expect(screen.getByText(/scan your library first/i)).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 450));
    expect(mockedPreview).not.toHaveBeenCalled();
  });

  it("previews the draft (debounced) against the given scan and shows the projected counts", async () => {
    render(<RulesetEditor scanId={5} />);
    await screen.findByRole("radio", { name: /author first/i });

    await waitFor(() => expect(mockedPreview).toHaveBeenCalledWith(5, DEFAULT_RULESET), {
      timeout: 2000,
    });
    expect(await screen.findByText("loose books")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("toggling a policy switch updates the draft the next preview call uses", async () => {
    const user = userEvent.setup();
    render(<RulesetEditor scanId={5} />);
    await screen.findByRole("radio", { name: /author first/i });
    await waitFor(() => expect(mockedPreview).toHaveBeenCalledTimes(1), { timeout: 2000 });

    await user.click(screen.getByRole("switch", { name: /split folders that hold more than one book/i }));

    await waitFor(() => expect(mockedPreview).toHaveBeenCalledTimes(2), { timeout: 2000 });
    const [, secondDraft] = mockedPreview.mock.calls[1];
    expect(secondDraft.structure.one_book_per_folder).toBe(false);
  });

  it("disables Save until the draft is dirty, then saves and shows confirmation", async () => {
    const user = userEvent.setup();
    render(<RulesetEditor scanId={null} />);
    await screen.findByRole("radio", { name: /author first/i });

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    await user.click(screen.getByRole("radio", { name: /title first/i }));
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(mockedSave).toHaveBeenCalledWith({
        id: 1,
        name: "Default",
        ruleset: { ...DEFAULT_RULESET, naming: { ...DEFAULT_RULESET.naming, preset: "title-first" } },
      }),
    );
    expect(await screen.findByText(/saved/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });
});
