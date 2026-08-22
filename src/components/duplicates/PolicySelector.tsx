import { useId } from "react";
import type { ResolutionPolicy } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";

const S = STRINGS.duplicates;

/** The three F-704 policies, in the order they escalate from "no opinion". */
const CHOICES: ReadonlyArray<{ id: ResolutionPolicy; label: string }> = [
  { id: "flag-only", label: S.policyFlagOnly },
  { id: "keep-larger", label: S.policyKeepLarger },
  { id: "keep-m4b", label: S.policyKeepM4b },
];

export interface PolicySelectorProps {
  value: ResolutionPolicy;
  onChange: (policy: ResolutionPolicy) => void;
}

// The F-704 policy selector (AC-28).
//
// THREE POLICIES, NOT FOUR. `keep-higher-bitrate` was cut as F-1108: file size
// is a free proxy for it that cannot be missing, and it has no defined value for
// a book split across N files. The v0.6.0 plan's step 2 still said four; that
// wording is stale rather than a missing feature.
//
// # It says out loud that it usually changes nothing
//
// This is the honest part and the reason for the note underneath. An exact
// duplicate group is found by matching name AND SIZE, so its copies tie under
// "the biggest copy" by construction; a fingerprint book group ties under the
// m4b rule because AC-51 already requires an agreeing audio count. Where a
// policy genuinely discriminates is title-only groups, which AC-55 never
// auto-resolves anyway. A selector that stayed quiet about that would imply the
// choice did more work than it does, and the first time a person switched it and
// saw nothing move they would reasonably think the app was broken.
//
// # A radio group, not a dropdown
//
// Three options, all of which change what is on screen underneath. A dropdown
// would hide two of the three behind a click and make comparing them a memory
// exercise. The label is "Suggest keeping" rather than "Resolution policy":
// a person is choosing which copy to suggest, not configuring an engine.
export function PolicySelector({ value, onChange }: PolicySelectorProps) {
  // A per-instance group name. Radios sharing a `name` are ONE group even across
  // unrelated components, so a hardcoded name means two selectors on one page
  // silently uncheck each other. Caught by rendering the gallery, which shows two
  // of these side by side and rendered both with nothing selected.
  const group = useId();

  return (
    <fieldset className="rounded border border-border bg-surface p-3">
      <legend className="px-1 text-meta font-semibold text-ink">{S.suggestLabel}</legend>
      <div className="flex flex-wrap gap-x-4 gap-y-2">
        {CHOICES.map((choice) => (
          <label key={choice.id} className="flex items-center gap-2 text-body text-ink">
            <input
              type="radio"
              name={group}
              value={choice.id}
              checked={value === choice.id}
              onChange={() => onChange(choice.id)}
            />
            {choice.label}
          </label>
        ))}
      </div>
      <p className="mt-2 max-w-prose text-meta text-ink-3">{S.policyNote}</p>
    </fieldset>
  );
}
