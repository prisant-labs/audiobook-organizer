// Fixtures for the dev-only component gallery.
//
// Lifted from the shapes the existing test suite already builds (src/__tests__/
// a11y.test.tsx and the per-component tests) rather than invented, so the
// gallery renders components against the same data the tests prove them
// against. If a binding type changes, `pnpm typecheck` fails here in the same
// way it fails in the tests, which is the point: the gallery is not allowed to
// drift into showing a shape the app can no longer produce.
//
// Numbers are deliberately un-round (1,022 books, 412 needing work) because
// round numbers hide alignment and tabular-nums problems that real data
// exposes.
import type {
  BookExample,
  GoodNews,
  HistoryEntry,
  LibraryOverview,
  PlanGroupView,
  PlanOpView,
  SeriesCluster,
  DuplicateCopyView,
  DuplicateGroupCard,
} from "@/lib/bindings";
import type { StartupInterruption } from "@/lib/interruption";

export const OVERVIEW: LibraryOverview = {
  scan_id: 9,
  total_books: 1022,
  total_bytes: 297 * 1024 ** 3,
  needs_tidy_books: 412,
  worth_a_look: [
    {
      entry_id: 1,
      title: "Sapiens",
      author: "Y. N. Harari",
      reason: { kind: "warn", text: "loose file" },
    },
    {
      entry_id: 2,
      title: "Dune",
      author: "Frank Herbert",
      reason: { kind: "alert", text: "2 copies" },
    },
  ],
  series: [{ entry_id: 30, name: "The Dresden Files", author: "Jim Butcher", book_count: 20 }],
  good_news: {
    already_tidy_books: 582,
    series_shelved: 34,
    empty_folders: 20,
    duplicate_groups: 403,
    duplicate_bytes: Math.round(10.1 * 1024 ** 3),
  },
  metrics: { per_class: [], problems: [], total_bytes: 297 * 1024 ** 3 },
};

/** A library with nothing to do: the lede and good-news line change register. */
export const OVERVIEW_TIDY: LibraryOverview = {
  ...OVERVIEW,
  needs_tidy_books: 0,
};

export const GOOD_NEWS: GoodNews = OVERVIEW.good_news;

export const SERIES: SeriesCluster = {
  entry_id: 30,
  name: "The Dresden Files",
  author: "Jim Butcher",
  book_count: 20,
};

/** More books than MAX_SPINES_SHOWN (14), so the "not shown" caption renders. */
export const SERIES_LONG: SeriesCluster = {
  entry_id: 31,
  name: "Discworld",
  author: "Terry Pratchett",
  book_count: 41,
};

export const BOOK_WARN: BookExample = {
  entry_id: 1,
  title: "Sapiens",
  author: "Y. N. Harari",
  reason: { kind: "warn", text: "loose file" },
};

export const BOOK_ALERT: BookExample = {
  entry_id: 2,
  title: "Dune",
  author: "Frank Herbert",
  reason: { kind: "alert", text: "2 copies" },
};

/** Titles chosen to show the deterministic tint spread, not to look pretty. */
export const FALLBACK_TITLES: readonly { title: string; author: string | null }[] = [
  { title: "Sapiens", author: "Y. N. Harari" },
  { title: "Dune", author: "Frank Herbert" },
  { title: "The Dresden Files: Storm Front", author: "Jim Butcher" },
  { title: "A Very Long Title That Has To Wrap Onto Several Lines To Fit", author: null },
  { title: "Q", author: "Luther Blissett" },
];

export function group(overrides: Partial<PlanGroupView> = {}): PlanGroupView {
  const merged = {
    group: "loose-books",
    label: "loose books",
    headline: "Give 3 loose books their own folders",
    reason: "These audiobooks are sitting as single files instead of their own folder.",
    op_count: 3,
    byte_size: 3000,
    status: "included" as const,
    warning_count: 0,
    blocked_count: 0,
    ...overrides,
  };
  return { ...merged, actionable_count: overrides.actionable_count ?? merged.op_count };
}

export function op(overrides: Partial<PlanOpView> = {}): PlanOpView {
  return {
    id: 1,
    group: "loose-books",
    kind: "move",
    kind_reason: null,
    source_path: "E:\\Books - Audio\\Sapiens.m4b",
    target_path: "E:\\Books - Audio\\Y. N. Harari\\Sapiens\\Sapiens.m4b",
    rationale: "Move this loose book into its own folder.",
    confidence: "high",
    byte_size: 1000,
    validation: "valid",
    validation_reason: null,
    warning_text: null,
    approval: "pending",
    matched_pattern: null,
    extracted_fields: [],
    stripped_noise: null,
    ...overrides,
  };
}

/** The seven campaign groups the review surface actually shows. */
export const GROUPS: readonly PlanGroupView[] = [
  group({
    group: "staging",
    label: "staging",
    headline: "Move 12 books out of your downloads folder",
    op_count: 12,
    byte_size: 4 * 1024 ** 3,
  }),
  group({ group: "loose-books", label: "loose books" }),
  group({
    group: "messy-names",
    label: "messy names",
    headline: "Rename 47 messy folders",
    reason: "These folder names carry ripper tags and release noise.",
    op_count: 47,
    byte_size: 18 * 1024 ** 3,
    warning_count: 3,
  }),
  group({
    group: "box-sets",
    label: "box sets",
    headline: "Split 4 box sets into their books",
    op_count: 4,
    byte_size: 9 * 1024 ** 3,
    status: "left-out",
  }),
  group({
    group: "bundles",
    label: "bundles",
    headline: "Unpack 2 bundles",
    op_count: 2,
    byte_size: 2 * 1024 ** 3,
    blocked_count: 2,
    actionable_count: 0,
  }),
  group({
    group: "copies",
    label: "duplicates",
    headline: "Archive 403 duplicate copies",
    reason: "These look like duplicates of books you already have.",
    op_count: 403,
    byte_size: Math.round(10.1 * 1024 ** 3),
    warning_count: 12,
  }),
  group({
    group: "empty-folders",
    label: "empty folders",
    headline: "Sweep out 20 empty folders",
    op_count: 20,
    byte_size: 0,
  }),
];

export const HISTORY_ROW: HistoryEntry = {
  jobId: 14,
  mode: "real",
  state: "failed",
  startedAt: "2026-08-04T00:18:15Z",
  finishedAt: null,
  changesMade: 142,
  undo: { kind: "put-recent-changes-back", op_ids: [140, 141, 142] },
};

const INTERRUPTION_BASE = {
  job_id: 14,
  mode: "real" as const,
  interrupted: true,
  outcome: "completed" as const,
  in_doubt_op_id: 142,
  resume_offered: true,
  done_count: 142,
};

export const INTERRUPTION_PRACTICE: StartupInterruption = {
  ...INTERRUPTION_BASE,
  mode: "dry-run",
  resume_offered: false,
};
export const INTERRUPTION_DECISIVE: StartupInterruption = INTERRUPTION_BASE;
export const INTERRUPTION_AMBIGUOUS: StartupInterruption = {
  ...INTERRUPTION_BASE,
  resume_offered: false,
};

// One duplicate group (F-905, P5). Three states of the same book, because the
// three are what the card actually has to look right in: nobody has checked the
// copies, they have been checked and match, and a decision has been made.
const COPY_A: DuplicateCopyView = {
  entry_id: 41,
  path: "E:\\Books - Audio\\Frank Herbert\\Dune\\Dune.m4b",
  size_bytes: 903_000_000,
  check: "not-checked",
  check_label: "not checked yet",
  check_reason: null,
  suggested_keeper: true,
};
const COPY_B: DuplicateCopyView = {
  entry_id: 42,
  path: "E:\\Books - Audio\\_incoming\\Dune\\Dune.m4b",
  size_bytes: 903_000_000,
  check: "not-checked",
  check_label: "not checked yet",
  check_reason: null,
  suggested_keeper: false,
};

export const DUPES_UNCHECKED: DuplicateGroupCard = {
  book: "Dune",
  group_key: "Dune.m4b|903000000",
  method: "exact-basename-size",
  found_by: "same file name and size",
  copies: [COPY_A, COPY_B],
  copy_count: 2,
  candidate_bytes_estimate: 1_806_000_000,
  keeper_reason: "the copies were equivalent, so the first one is kept",
  content_verified: false,
  confirmed_keeper: null,
};

export const DUPES_CHECKED: DuplicateGroupCard = {
  ...DUPES_UNCHECKED,
  copies: [
    { ...COPY_A, check: "checked", check_label: "contents checked" },
    { ...COPY_B, check: "checked", check_label: "contents checked" },
  ],
  content_verified: true,
};

export const DUPES_DECIDED: DuplicateGroupCard = {
  ...DUPES_CHECKED,
  confirmed_keeper: 41,
};

/** A copy the app tried to read and could not: the state AC-12 cares most about. */
export const DUPES_UNREADABLE: DuplicateGroupCard = {
  ...DUPES_UNCHECKED,
  copies: [
    { ...COPY_A, check: "checked", check_label: "contents checked" },
    {
      ...COPY_B,
      check: "could-not-read",
      check_label: "could not be read",
      check_reason: "os error 5: access is denied",
    },
  ],
};

/** No-op handlers. The gallery renders appearance and states, not behaviour. */
export const noop = () => {};
export const noopAsync = async () => {};
