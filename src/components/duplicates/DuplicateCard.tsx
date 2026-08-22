import { useState } from "react";
import { Check, CircleAlert, CircleCheck } from "lucide-react";
import type { DuplicateGroupCard } from "@/lib/bindings";
import { STRINGS } from "@/lib/strings";
import { OpenFolder } from "@/components/OpenFolder";
import { formatBytes } from "@/lib/format";
import { UnverifiedArchiveConfirm } from "@/components/review/UnverifiedArchiveConfirm";

const S = STRINGS.duplicates;

export interface DuplicateCardProps {
  group: DuplicateGroupCard;
  /** Keep `keeperEntryId`, archive the rest. `override` is AC-13's escape hatch. */
  onConfirm: (keeperEntryId: number, loserEntryIds: number[], override: boolean) => void;
  /** Put the group back to undecided. */
  onClear: () => void;
}

// One duplicate group: one book, N copies (FD-08, AC-17).
//
// THE GROUP IS THE UNIT, and the card says so before it says anything else. The
// headline is the book; the copies are inside it. A surface that led with copies
// would be counting a different thing from the badge, the report and the Copies
// card, which is exactly what AC-20 forbids.
//
// # Two paths to a decision, and they are not the same path
//
// When the copies have been read and proved identical, keeping one is a plain
// button: the app knows what it is archiving. When they have not, the same
// choice routes through AC-13's two-step override, because the honest state is
// that nothing has confirmed these are the same book. The gate itself is the
// BACKEND's (`confirm_resolution_gated` refuses an unverified confirmation
// without the override flag); this component decides which affordance to show,
// never whether the rule applies.
//
// # Why the paths are shown rather than the group key
//
// `group.group_key` reads like `Dune.m4b|900`. It is a join key, and a card that
// showed it would be lying about what it is. The book name is what a person
// recognises; the paths live in the FD-13 disclosure, which is the one sanctioned
// place a raw path appears on a primary surface.
export function DuplicateCard({ group, onConfirm, onClear }: DuplicateCardProps) {
  // Which copy the user picked while the override strip is open. Local because
  // it is not a decision until the second step confirms it.
  const [pending, setPending] = useState<number | null>(null);

  const confirmed = group.confirmed_keeper !== null;
  const losersFor = (keeper: number) =>
    group.copies.filter((c) => c.entry_id !== keeper).map((c) => c.entry_id);

  function keep(entryId: number) {
    if (group.content_verified) {
      onConfirm(entryId, losersFor(entryId), false);
    } else {
      setPending(entryId);
    }
  }

  return (
    <li className="rounded border border-border bg-surface p-4">
      <h3 className="font-serif text-lead font-medium text-ink">{group.book}</h3>

      <p className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-meta tabular-nums text-ink-3">
        <span>{S.copyCount(group.copy_count)}</span>
        <span aria-hidden>&middot;</span>
        <span>{S.estimate(formatBytes(group.candidate_bytes_estimate))}</span>
        <span aria-hidden>&middot;</span>
        <span>{group.found_by}</span>
      </p>

      {/* Status carried by an icon and a sentence, never by colour alone
          (design-system Section 8). */}
      <p
        className={`mt-2 flex items-center gap-1.5 text-meta ${
          group.content_verified ? "text-good" : "text-ink-3"
        }`}
      >
        {group.content_verified ? (
          <CircleCheck size={14} aria-hidden className="flex-none" />
        ) : (
          <CircleAlert size={14} aria-hidden className="flex-none" />
        )}
        {group.content_verified ? S.verified : S.notVerified}
      </p>

      <ul className="mt-3 flex flex-col gap-2">
        {group.copies.map((copy) => {
          const isKept = group.confirmed_keeper === copy.entry_id;
          return (
            <li
              key={copy.entry_id}
              className={`rounded border p-3 ${
                isKept ? "border-good bg-good-bg" : "border-border bg-bg"
              }`}
            >
              <div className="flex flex-wrap items-baseline justify-between gap-2">
                <span className="text-body break-all text-ink">{shortPath(copy.path)}</span>
                <span className="text-meta tabular-nums text-ink-3">
                  {formatBytes(copy.size_bytes)}
                </span>
              </div>

              <p className="mt-1 text-meta text-ink-3">
                {copy.check_label}
                {copy.check_reason ? `: ${copy.check_reason}` : ""}
                {copy.suggested_keeper && !confirmed ? ` · ${group.keeper_reason ?? ""}` : ""}
              </p>

              {/* FD-13: the one sanctioned place a full path appears. */}
              <details className="mt-1 text-meta">
                <summary className="inline-flex cursor-pointer list-none items-center text-link">
                  {STRINGS.states.showFileDetails}
                </summary>
                {/* AC-49. Per COPY rather than per group: the point of looking
                    is to tell two copies of one book apart, and a single control
                    on the group could only ever open one of them. */}
                <p className="mt-1 flex items-start gap-2 break-all font-mono text-meta text-ink-2">
                  <span className="min-w-0 flex-1">{copy.path}</span>
                  <OpenFolder path={copy.path} label={STRINGS.openFolder.openCopy} />
                </p>
              </details>

              <div className="mt-2">
                {isKept ? (
                  <p className="flex items-center gap-1.5 text-meta font-semibold text-good">
                    <Check size={14} aria-hidden className="flex-none" />
                    {S.kept}
                  </p>
                ) : confirmed ? null : (
                  <button
                    type="button"
                    onClick={() => keep(copy.entry_id)}
                    className="rounded border border-border-2 px-2 py-1 text-meta font-semibold text-ink hover:bg-surface-2"
                  >
                    {S.keepThis}
                  </button>
                )}
              </div>
            </li>
          );
        })}
      </ul>

      {/* AC-13, and only when it is actually needed: a copy has been picked and
          the app cannot vouch for what it would archive. */}
      {pending !== null && !confirmed && !group.content_verified && (
        <div className="mt-3">
          <UnverifiedArchiveConfirm
            onConfirm={() => {
              onConfirm(pending, losersFor(pending), true);
              setPending(null);
            }}
          />
        </div>
      )}

      {confirmed && (
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <p className="text-meta text-ink-2">{S.decided}</p>
          <button
            type="button"
            onClick={onClear}
            className="text-meta font-semibold text-link underline"
          >
            {S.undo}
          </button>
        </div>
      )}
    </li>
  );
}

/**
 * The last few path components, which is the smallest thing that actually tells
 * two copies apart.
 *
 * The file NAME cannot: an exact duplicate group is keyed on basename and size,
 * so every copy in one is called the same thing by construction. Rendering just
 * the name would print the identical line twice and ask a person to choose
 * between them. What differs is the folder above, and often the one above that
 * ("Frank Herbert\Dune" against "_incoming\Dune"), so three components is the
 * shortest form that reliably distinguishes without printing the whole path on
 * a primary surface. The full path stays one click away in the FD-13 disclosure.
 *
 * Caught by rendering the gallery and reading it: the first version showed the
 * basename and produced two rows of "Dune.m4b".
 */
function shortPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  if (parts.length <= 3) return path;
  return `...\\${parts.slice(-3).join("\\")}`;
}
