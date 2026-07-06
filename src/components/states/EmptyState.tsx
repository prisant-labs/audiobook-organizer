import type { ReactNode } from "react";

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
  /** When disabled, the button stays visible with `reason` beside it (5.2). */
  disabled?: boolean;
  /** The explanatory line shown when the primary action is disabled. */
  reason?: string;
}

export interface EmptyStateProps {
  /** A decorative glyph (design-system Section 5.2: icon-plus-label always). */
  icon?: ReactNode;
  heading: string;
  body?: string;
  /**
   * `good` = a positive, already-done state (already tidy, no duplicates);
   * `neutral` = a gentle "nothing here yet" (empty root, pre-first-scan).
   */
  tone?: "good" | "neutral";
  /** The one calm primary action (design-system Section 7), if any. */
  action?: EmptyStateAction | null;
  /** Extra content below the action (e.g. a live scan-progress area). */
  children?: ReactNode;
}

// The shared empty / edge-state surface (F-908, AC-25, design-system Section
// 5.2): already-tidy, empty library root, pre-first-scan, no duplicates. Warm
// and plain, one calm primary action (or an explicitly disabled one with a
// reason beside it, never hidden - the all-groups-excluded rule). Not an
// error: it uses the neutral/good register, never the `--danger` pair.
export function EmptyState({
  icon,
  heading,
  body,
  tone = "neutral",
  action,
  children,
}: EmptyStateProps) {
  const glyphText = tone === "good" ? "text-good" : "text-primary";

  return (
    <div className="max-w-[52ch]">
      {icon && (
        <span
          aria-hidden
          className={`mb-4 flex size-11 items-center justify-center rounded-[10px] bg-surface-2 ${glyphText}`}
        >
          {icon}
        </span>
      )}
      <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em] text-balance">
        {heading}
      </h1>
      {body && (
        <p className="mt-3 text-[14.5px] leading-[1.55] text-ink-2 text-pretty">{body}</p>
      )}
      {action && (
        <div className="mt-5 flex items-center gap-3">
          <button
            type="button"
            onClick={action.onClick}
            disabled={action.disabled}
            className="rounded bg-primary px-4 py-2.5 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:cursor-not-allowed disabled:opacity-60"
          >
            {action.label}
          </button>
          {action.disabled && action.reason && (
            <p className="text-[12.5px] text-ink-2">{action.reason}</p>
          )}
        </div>
      )}
      {children}
    </div>
  );
}
