import { useState } from "react";
import { AlertTriangle } from "lucide-react";
import { STRINGS } from "@/lib/strings";

const S = STRINGS.duplicatesOverride;

export interface UnverifiedArchiveConfirmProps {
  /** Called on the SECOND press, once the warning has been read and accepted. */
  onConfirm: () => void;
  /** Renders the opener unavailable rather than pressable-but-inert. */
  disabled?: boolean;
}

// The F-702 override (v0.6.0 hardening, AC-13): how a person overrules the
// AC-12 gate and archives duplicate copies that were never compared.
//
// NOT WIRED YET, AND THAT IS THE HONEST STATE. There is nothing to override
// today: `dedupe-quarantine` emits no plan operations, the auto-resolve gate
// (`group_may_auto_resolve`) has no production callers, and the duplicates
// surface is P5. Resolution arrives at P3 (AC-23..AC-27) and the surface that
// hosts this at P5 (AC-30), and the flag this control would set belongs on P3's
// resolve request, which does not exist. Adding that flag now would mean
// shipping a parameter nothing reads. So this is the affordance and its copy,
// which IS what AC-13 specifies, rendered in the gallery and left for its
// caller.
//
// WHY INLINE RATHER THAN A MODAL. Spec AC-12 says the override is confirmed
// through a "warning dialog". Design-system Section 7 says confirming is "a
// two-step inline confirm strip ... NEVER a modal dialog", and Section 4.14
// calls inline the canonical confirm pattern. AC-13 itself resolves the
// tension: it asks for consistency with the design system, so the design
// system's mechanism wins and "dialog" reads as "warning confirm". Inline also
// keeps the group the user is deciding about visible while they decide, which a
// modal would cover up.
//
// WHY DANGER TOKENS AND AN ICON. The FD-09 pair (`--danger` / `--danger-bg` /
// `--danger-ink`) is the register for "this one is different", and Section 8
// forbids carrying status by colour alone, so the warning is in the sentence
// and the icon as well as the hue.
//
// The first press deliberately does NOT act. That is the whole of AC-13's "a
// deliberate two-step affordance (not a default)", and it is why the opener is
// a quiet outline button rather than a primary or a filled danger one: reaching
// for it should feel like opening a question, not answering it.
export function UnverifiedArchiveConfirm({
  onConfirm,
  disabled = false,
}: UnverifiedArchiveConfirmProps) {
  const [step, setStep] = useState<"idle" | "confirming">("idle");

  if (step === "confirming") {
    return (
      <div role="alert" className="rounded border border-danger bg-danger-bg p-3">
        <div className="flex items-start gap-2">
          <AlertTriangle size={18} aria-hidden className="mt-0.5 flex-none text-danger" />
          <div>
            <p className="max-w-md text-sm font-semibold leading-snug text-danger">{S.warning}</p>
            <p className="mt-1 max-w-md text-sm leading-relaxed text-ink-2">{S.reassurance}</p>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => {
                  // Close first, then act: a second press on a control that is
                  // already gone cannot archive the same copies twice.
                  setStep("idle");
                  onConfirm();
                }}
                className="rounded bg-danger px-4 py-2 text-sm font-semibold text-danger-ink transition-opacity hover:opacity-90"
              >
                {S.goAhead}
              </button>
              <button
                type="button"
                onClick={() => setStep("idle")}
                className="rounded border border-border-2 bg-surface px-4 py-2 text-sm font-semibold text-ink transition-colors hover:border-ink-3"
              >
                {S.cancel}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={() => setStep("confirming")}
      className="rounded border border-border-2 bg-surface px-4 py-2 text-sm font-semibold text-ink transition-colors hover:border-ink-3 disabled:opacity-50"
    >
      {S.start}
    </button>
  );
}
