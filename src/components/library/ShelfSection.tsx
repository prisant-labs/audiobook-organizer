import type { ReactNode } from "react";

export interface ShelfSectionProps {
  heading: string;
  subline: string;
  children: ReactNode;
}

// The shared shelf shell (design-system Section 4.7 `.shelf`/`.shelfhead`/
// `.row`/`.rail`): a heading with a quiet sub-line, a horizontally-scrolling
// row of content, and the bookshelf-edge rail beneath it. Both the "Worth a
// look first" shelf and the series shelf render inside this.
export function ShelfSection({ heading, subline, children }: ShelfSectionProps) {
  return (
    <section className="mt-9 max-w-[1060px]">
      <div className="mb-4 flex items-baseline gap-3">
        <h2 className="font-serif text-[19px] font-medium">{heading}</h2>
        <p className="text-[12.5px] text-ink-3">{subline}</p>
      </div>
      <div className="flex items-end gap-[22px] overflow-x-auto px-1.5 pb-3.5">{children}</div>
      <div className="-mt-2 h-[7px] rounded-sm" style={{ background: "var(--shelf-rail)" }} />
    </section>
  );
}
