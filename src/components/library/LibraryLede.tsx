import { formatBytes } from "@/lib/format";
import type { LibraryOverview } from "@/lib/bindings";

export interface LibraryLedeProps {
  overview: LibraryOverview;
}

// The home's lede sentence (F-902, AC-6/AC-7, design-system Section 6.2):
// every count/byte figure is a PROP read from `classify_overview` at render
// time, never a literal - only the surrounding prose is fixed text. Numbers
// are `.toLocaleString()`-formatted with `tabular-nums` so digits do not
// jitter as a re-scan updates them (design-system Section 3.3).
export function LibraryLede({ overview }: LibraryLedeProps) {
  const { total_books, total_bytes, needs_tidy_books } = overview;

  return (
    <p className="mt-2 max-w-[52ch] text-[14.5px] leading-[1.55] text-ink-2 [text-wrap:pretty]">
      <b className="tabular-nums font-semibold text-ink">
        {total_books.toLocaleString()} audiobook{total_books === 1 ? "" : "s"}
      </b>
      , about {formatBytes(total_bytes)}. Most are already in good shape for Audiobookshelf.{" "}
      {needs_tidy_books > 0 ? (
        <>
          <b className="tabular-nums font-semibold text-ink">
            {needs_tidy_books.toLocaleString()}
          </b>{" "}
          could use organizing: loose files, messy folder names, and a few box sets that need
          splitting up.
        </>
      ) : (
        "Your library is already organized - nothing needs doing right now."
      )}
    </p>
  );
}
