import { Check } from "lucide-react";
import { formatBytes } from "@/lib/format";
import type { GoodNews } from "@/lib/bindings";

export interface GoodNewsLineProps {
  goodNews: GoodNews;
}

// The home's "good news" line (design-system Section 4, `.goodline`): what is
// ALREADY done, each fact a full sentence-fragment naming its own unit (FD-08)
// rather than a bare number. A fact with a zero count is simply omitted - a
// zero here is not itself news.
export function GoodNewsLine({ goodNews }: GoodNewsLineProps) {
  const facts: string[] = [];
  if (goodNews.already_tidy_books > 0) {
    facts.push(
      `${goodNews.already_tidy_books} book${goodNews.already_tidy_books === 1 ? "" : "s"} already in the right folders`,
    );
  }
  // "kept together", not "shelved together": FD-47 retired the shelf word family
  // for "library". The copy guard's own pattern only matches "shelf" and
  // "shelves", so this verb form was live in shipped copy and swept by nothing.
  if (goodNews.series_shelved > 0) {
    facts.push(
      `${goodNews.series_shelved} series kept together`,
    );
  }
  if (goodNews.empty_folders > 0) {
    facts.push(
      `${goodNews.empty_folders} empty folder${goodNews.empty_folders === 1 ? "" : "s"} ready to sweep`,
    );
  }
  if (goodNews.duplicate_bytes > 0) {
    facts.push(`${formatBytes(goodNews.duplicate_bytes)} of duplicate copies found`);
  }

  if (facts.length === 0) return null;

  return (
    <div className="mt-8 flex max-w-[1060px] flex-wrap gap-x-6 gap-y-2 border-t border-border pt-4 text-[13px] text-ink-2">
      {facts.map((fact) => (
        <span key={fact} className="inline-flex items-center gap-1.5">
          <Check aria-hidden="true" className="h-[13px] w-[13px] text-good" strokeWidth={2.4} />
          {fact}
        </span>
      ))}
    </div>
  );
}
