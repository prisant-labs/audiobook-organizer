import { useState } from "react";
import { STRINGS } from "@/lib/strings";

export interface ConfirmInlineProps {
  disabled: boolean;
  /**
   * Called when the user confirms the tidy-up (second "Go ahead" press).
   * When provided, replaces the v0.5.0-not-yet stub with the real apply flow.
   * When omitted, the original stub message is shown (backward-compatible).
   */
  onConfirm?: () => Promise<void>;
}

// The two-step inline confirm strip (F-502/AC-12, design-system Section 4.14):
// the first press swaps the primary button for an inline confirm, never a
// modal (Section 7); the second press calls `onConfirm` when provided (v0.5.0
// apply flow) or shows the honest "not yet" message otherwise (pre-v0.5.0 stub).
export function ConfirmInline({ disabled, onConfirm }: ConfirmInlineProps) {
  const [step, setStep] = useState<"idle" | "confirming" | "not-yet">("idle");

  if (step === "not-yet") {
    return <p className="max-w-[36ch] text-[12px] text-ink-3">{STRINGS.review.confirmNotAvailable}</p>;
  }

  if (step === "confirming") {
    return (
      <div className="flex items-center gap-3">
        <span className="text-[12.5px] text-ink-2">{STRINGS.review.confirmPrompt}</span>
        <button
          type="button"
          onClick={() => {
            if (onConfirm) {
              void onConfirm(); // triggers real apply via parent; shell navigates away
            } else {
              setStep("not-yet"); // pre-v0.5.0 stub: honest "not available yet" message
            }
          }}
          className="rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover"
        >
          {STRINGS.review.confirmGoAhead}
        </button>
        <button
          type="button"
          onClick={() => setStep("idle")}
          className="rounded border border-border-2 bg-surface px-3.5 py-2 text-[13px] font-semibold text-ink transition-colors hover:border-ink-3"
        >
          {STRINGS.review.confirmNotYet}
        </button>
      </div>
    );
  }

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={() => setStep("confirming")}
      className="rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-50"
    >
      {STRINGS.review.tidyUpNow}
    </button>
  );
}
