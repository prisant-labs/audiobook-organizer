import { cn } from "@/lib/utils";
import { fallbackTileStyle } from "@/lib/coverHash";

export interface FallbackTileProps {
  // The book title, shown in serif and hashed to the tile's tint (AC-23).
  title: string;
  // The author, shown as micro-caps at the bottom (design-system 4.4/4.5).
  author?: string | null;
  className?: string;
}

// The no-cover fallback tile (F-907, FD-03, design-system Section 4.5): a square
// 1:1 tile whose background is a deterministic tint of the title, the title set
// in serif, and the author in micro-caps. No pattern glyph (design-system 4.5).
// The SAME title always yields the SAME tint (AC-23), so a shelf of fallback
// tiles stays stable and still reads as a warm bookshelf.
export function FallbackTile({ title, author, className }: FallbackTileProps) {
  const { background, color } = fallbackTileStyle(title);
  return (
    <div
      // aspect-square = 1:1 (AC-22); covers are never portrait, never spine-shaded.
      className={cn(
        "flex aspect-square w-full flex-col justify-end overflow-hidden rounded-[5px] p-2",
        className,
      )}
      style={{ background, color }}
      role="img"
      aria-label={author ? `${title} by ${author}` : title}
    >
      <span
        className="font-serif text-[13px] leading-tight font-semibold [text-wrap:balance]"
        // Deterministic tint can be either mood; the ink from fallbackTileStyle
        // keeps the title legible on it. Clamp long titles so the tile stays square.
        style={{
          display: "-webkit-box",
          WebkitLineClamp: 3,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
        }}
      >
        {title}
      </span>
      {author ? (
        <span className="mt-1 truncate font-sans text-[9px] tracking-[0.04em] uppercase opacity-85">
          {author}
        </span>
      ) : null}
    </div>
  );
}
