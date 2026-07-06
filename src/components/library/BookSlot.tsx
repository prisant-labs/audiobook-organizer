import { Layers, TriangleAlert } from "lucide-react";
import { cn } from "@/lib/utils";
import { Cover } from "@/components/Cover";
import { useCoverImage } from "@/hooks/useCoverImage";
import type { BookExample } from "@/lib/bindings";

export interface BookSlotProps {
  scanId: number;
  book: BookExample;
}

// One book on the "Worth a look first" shelf (design-system Section 4.6
// `.bookslot`): a square cover (sz-lg, T-13's `Cover`/`FallbackTile`) plus one
// plain-language reason chip below it. Icon-plus-label always accompanies the
// chip kind (Section 8: color is never the only signal).
export function BookSlot({ scanId, book }: BookSlotProps) {
  const image = useCoverImage(scanId, book.entry_id);
  const Icon = book.reason.kind === "warn" ? TriangleAlert : Layers;

  return (
    <div className="flex w-[118px] flex-none flex-col items-center gap-2.5">
      <div className="w-[112px]">
        <Cover title={book.title} author={book.author} image={image} />
      </div>
      <span
        className={cn(
          "inline-flex items-center gap-1.5 whitespace-nowrap rounded-full px-2.5 py-1 text-[10.5px]",
          book.reason.kind === "warn" ? "bg-warn-bg text-warn" : "bg-alert-bg text-alert",
        )}
      >
        <Icon aria-hidden="true" className="h-[10px] w-[10px] shrink-0" strokeWidth={2.2} />
        {book.reason.text}
      </span>
    </div>
  );
}
