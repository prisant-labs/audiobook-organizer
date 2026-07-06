export interface LoadingSkeletonProps {
  /**
   * An accessible label for what is loading (announced to assistive tech and
   * shown as a quiet line). The shimmer bars are decorative (`aria-hidden`).
   */
  label: string;
  /** How many placeholder bars to render (default 3). */
  rows?: number;
  /** Center the block vertically in its container (boot / full-screen loads). */
  centered?: boolean;
}

// The shared loading placeholder (F-908, design-system Section 5.3 loading
// register). Calm shimmer bars plus a plain-language label, so a load reads as
// "getting ready", never a blank window. The bars honor prefers-reduced-motion
// via the global rule in tokens.css (animations collapse to ~instant, AC-4).
// Distinct from the plan-building state (BuildingThePlan): that one carries a
// Stop control; this is a passive placeholder for reads with no cancel.
export function LoadingSkeleton({ label, rows = 3, centered = false }: LoadingSkeletonProps) {
  return (
    <div
      className={
        centered
          ? "mx-auto flex min-h-full max-w-[52ch] flex-col justify-center px-2 py-10"
          : "max-w-[52ch]"
      }
      role="status"
      aria-live="polite"
    >
      <div aria-hidden className="space-y-3">
        {Array.from({ length: rows }).map((_, i) => (
          <div
            key={i}
            className="h-3.5 animate-pulse rounded bg-surface-2"
            style={{ width: `${88 - i * 14}%` }}
          />
        ))}
      </div>
      <p className="mt-4 text-[13px] text-ink-3">{label}</p>
    </div>
  );
}
