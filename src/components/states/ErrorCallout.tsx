import { AlertCircle, AlertTriangle } from "lucide-react";
import type { ReactNode } from "react";
import { STRINGS } from "@/lib/strings";
import { GENERIC_ERROR_COPY, type ErrorCopy } from "@/lib/errorCopy";

export interface ErrorCalloutAction {
  label: string;
  onClick: () => void;
}

export interface ErrorCalloutProps {
  /**
   * The mapped family-safe copy for a known AppError code (from
   * `errorCopy.ts`). When omitted, the generic family-safe copy is used, so a
   * caller that only has a message string still renders a calm surface with
   * the raw text safely tucked in the disclosure (never as the sentence).
   */
  copy?: ErrorCopy | null;
  /** Optional heading above the sentence (e.g. the settings-unavailable title). */
  heading?: string;
  /**
   * Technical detail for the "Show file details" disclosure (FD-13): the raw
   * `formatAppError` string / OS error, shown only here for tier 1, never on
   * the family-facing line (AC-24).
   */
  detail?: string | null;
  /** A retry control (design-system Section 5.1 "Try the scan again"). */
  onRetry?: (() => void) | null;
  /** Custom label for the retry control (defaults to "Try again"). */
  retryLabel?: string;
  /**
   * An extra primary action distinct from retry - e.g. the F-909 root re-pick
   * ("Choose your library folder again"). Rendered as the one primary button.
   */
  action?: ErrorCalloutAction | null;
  /** A slot for extra content (rarely needed; used by the root-missing surface). */
  children?: ReactNode;
  /** Inline/compact (sits under existing content) vs full centered surface. */
  compact?: boolean;
}

// The single family-safe error/danger surface (F-908, AC-24). Every AppError
// family renders through this one component with its mapped copy, so no error
// path ever shows a bare OS error: the family-facing lines are the plain
// sentence + next step, and the raw technical string lives behind "Show file
// details". Status is never color-alone (design-system Section 8): the tone is
// carried by an icon AND the text, not hue alone. `--danger` (something broke)
// and `--warn` (a held / needs-a-decision state) are the two registers
// (Section 2.3).
export function ErrorCallout({
  copy,
  heading,
  detail,
  onRetry,
  retryLabel,
  action,
  children,
  compact = false,
}: ErrorCalloutProps) {
  const resolved = copy ?? GENERIC_ERROR_COPY;
  const danger = resolved.tone === "danger";
  const Icon = danger ? AlertTriangle : AlertCircle;
  const toneText = danger ? "text-danger" : "text-warn";

  return (
    <div
      role="alert"
      className={
        compact
          ? "mt-3 max-w-[52ch]"
          : "mx-auto flex min-h-full max-w-[52ch] flex-col items-start justify-center px-2 py-10"
      }
    >
      <div className="flex items-start gap-2.5">
        <Icon size={20} aria-hidden className={`mt-0.5 flex-none ${toneText}`} />
        <div>
          {heading && (
            <h1 className="font-serif text-[22px] font-medium leading-tight tracking-[-0.01em] text-balance">
              {heading}
            </h1>
          )}
          <p className={`text-[15px] font-semibold leading-snug ${toneText} ${heading ? "mt-2" : ""}`}>
            {resolved.sentence}
          </p>
          <p className="mt-1.5 max-w-[46ch] text-[13.5px] leading-relaxed text-ink-2 text-pretty">
            {resolved.nextStep}
          </p>

          {(action || onRetry) && (
            <div className="mt-4 flex flex-wrap items-center gap-2.5">
              {action && (
                <button
                  type="button"
                  onClick={action.onClick}
                  className="rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
                >
                  {action.label}
                </button>
              )}
              {onRetry && (
                <button
                  type="button"
                  onClick={onRetry}
                  className={
                    action
                      ? "rounded border border-border-2 bg-surface px-4 py-2 text-[13px] font-semibold text-ink transition-colors hover:border-ink-3"
                      : "rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
                  }
                >
                  {retryLabel ?? STRINGS.states.tryAgain}
                </button>
              )}
            </div>
          )}

          {children}

          {detail && (
            <details className="mt-4 max-w-[52ch]">
              <summary className="cursor-pointer text-[12.5px] font-semibold text-link">
                {STRINGS.states.showFileDetails}
              </summary>
              <pre className="mt-2 overflow-x-auto rounded bg-surface-2 p-3 font-mono text-[11.5px] leading-relaxed text-ink-2">
                {detail}
              </pre>
            </details>
          )}
        </div>
      </div>
    </div>
  );
}
