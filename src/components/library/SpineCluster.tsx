import { hashTitle, hashTitleToHue } from "@/lib/coverHash";
import type { SeriesCluster } from "@/lib/bindings";

export interface SpineClusterProps {
  series: SeriesCluster;
}

// The maximum number of stylized spines drawn per cluster (design-system
// Section 4.8): a series with more books than this shows the rest as
// "(M not shown)" in the caption rather than growing the shelf without bound.
const MAX_SPINES_SHOWN = 14;

// A muted, warm-library spine tint range, matching the fallback-tile palette's
// mid-tone register (coverHash.ts) rather than a saturated rainbow.
const SPINE_SATURATION = 38;
const SPINE_LIGHTNESS = 34;

// One series spine cluster (design-system Section 4.8, D-06's deliberate
// exception to "no spine shading"): a row of stylized vertical spines with the
// series name on the center spine, and a plain-language caption below. Every
// visual (height jitter, lean, tint) is DETERMINISTIC from the series name, so
// the same series always looks the same across renders (the same discipline
// AC-23's fallback-tile hash already establishes for covers).
export function SpineCluster({ series }: SpineClusterProps) {
  const shown = Math.min(series.book_count, MAX_SPINES_SHOWN) || 1;
  const notShown = series.book_count - shown;
  const centerIndex = Math.floor(shown / 2);

  return (
    <div className="flex flex-none flex-col gap-2.5">
      <div className="flex h-[118px] items-end gap-[3px] px-0.5">
        {Array.from({ length: shown }, (_, i) => {
          const seed = hashTitle(`${series.name}::${i}`);
          const height = 96 + (seed % 23);
          const lean = i === shown - 1 && hashTitle(series.name) % 2 === 0;
          const hue = hashTitleToHue(`${series.name}::spine::${i}`);
          return (
            <span
              key={i}
              className="relative w-[17px] rounded-[2px_2px_1.5px_1.5px] shadow-[0_1px_3px_rgba(0,0,0,0.3)]"
              style={{
                height,
                background: `hsl(${hue} ${SPINE_SATURATION}% ${SPINE_LIGHTNESS}%)`,
                transform: lean ? "rotate(-7deg) translateY(-2px)" : undefined,
                transformOrigin: lean ? "bottom left" : undefined,
                marginRight: lean ? "5px" : undefined,
              }}
            >
              {i === centerIndex && (
                <i
                  className="absolute inset-0 grid place-items-center overflow-hidden py-1 text-[6.8px] tracking-[0.05em] text-white/85 not-italic"
                  style={{ writingMode: "vertical-rl" }}
                >
                  {series.name}
                </i>
              )}
            </span>
          );
        })}
      </div>
      <p className="pl-0.5 text-left text-[12px] text-ink-2">
        <b className="font-semibold text-ink">{series.name}</b>
        {series.author && <> &middot; {series.author}</>}{" "}
        <span className="text-ink-3">
          &middot; {series.book_count} book{series.book_count === 1 ? "" : "s"}
          {notShown > 0 ? ` (${notShown} not shown)` : ""}
        </span>
      </p>
    </div>
  );
}
