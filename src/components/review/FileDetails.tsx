import type { PlanOpView } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";
import { OpenFolder } from "@/components/OpenFolder";

export interface FileDetailsProps {
  op: PlanOpView;
}

// The extended "Show file details" disclosure (F-504, design-system Section
// 4.12, FD-13): the ONE sanctioned place raw source/target paths appear on a
// primary review surface, plus the matched pattern (id + human name) and the
// per-field confidence breakdown (AC-18). Closed by default; low-confidence
// fields are shown, never hidden (AC-20).
export function FileDetails({ op }: FileDetailsProps) {
  return (
    <details className="mt-2 text-[11.5px]">
      <summary className="inline-flex cursor-pointer list-none items-center gap-1 text-link">
        {STRINGS.review.fileDetails}
      </summary>
      <div className="mt-1.5 space-y-2 rounded-md border border-border bg-surface-2 p-3">
        {/* AC-49: the affordance sits WHEREVER a path is displayed, and this
            disclosure is the one sanctioned place a raw path appears on the
            review surface (FD-13). Source and target get their own control,
            because before a run only the source exists on disk and after one
            only the target does; a single button would be wrong half the time. */}
        <div className="flex items-start gap-2">
        <pre className="min-w-0 flex-1 overflow-x-auto whitespace-pre-wrap break-all font-mono text-[10.5px] leading-relaxed text-ink-2">
          {`${op.source_path}${op.target_path ? `\n  → ${op.target_path}` : ""}`}
        </pre>
          <span className="flex flex-none flex-col gap-1 pt-0.5">
            <OpenFolder path={op.source_path} label={STRINGS.openFolder.openSource} />
            {op.target_path && (
              <OpenFolder path={op.target_path} label={STRINGS.openFolder.openTarget} />
            )}
          </span>
        </div>
        {op.matched_pattern && (
          <p className="text-ink-3">
            Matched pattern: <span className="text-ink-2">{op.matched_pattern.name}</span>{" "}
            <span className="tabular-nums">{`(${op.matched_pattern.id})`}</span>
          </p>
        )}
        {op.extracted_fields.length > 0 && (
          <ul className="grid grid-cols-2 gap-x-4 gap-y-1 text-ink-3">
            {op.extracted_fields.map((field) => (
              <li key={field.field}>
                <span className="capitalize">{field.field.replace("-", " ")}</span>:{" "}
                <span className="text-ink-2">{field.value}</span>{" "}
                <span>{`(${field.confidence})`}</span>
              </li>
            ))}
          </ul>
        )}
        {op.stripped_noise && (
          <p className="text-ink-3">
            Stripped from the name: <span className="text-ink-2">{op.stripped_noise}</span>
          </p>
        )}
        {(op.matched_pattern || op.extracted_fields.length > 0) && (
          <p className="text-[10.5px] italic text-ink-3">{STRINGS.review.ownNameCaveat}</p>
        )}
      </div>
    </details>
  );
}
