import { STRINGS } from "@/lib/strings";
import { ERROR_COPY } from "@/lib/errorCopy";
import { formatBytes } from "@/lib/format";
import { useDuplicates } from "@/hooks/useDuplicates";
import { DuplicateCard } from "@/components/duplicates/DuplicateCard";
import { PolicySelector } from "@/components/duplicates/PolicySelector";
import { EmptyState } from "@/components/states/EmptyState";
import { ErrorCallout } from "@/components/states/ErrorCallout";
import { ScanProgress } from "@/components/ScanProgress";

const S = STRINGS.duplicates;

export interface DuplicatesProps {
  scanId: number | null;
}

// The duplicates surface (F-905, v0.6.0 P5, AC-28 to AC-31).
//
// This is the screen P2's hash engine, P3's resolution policies and P4's review
// model were all built for and none of them could reach. Everything it renders
// comes from `dupes_review`, and everything it does goes back through commands
// that enforce their own rules: the AC-12 gate is the backend's, so this screen
// cannot accidentally archive an unverified group by forgetting a check.
//
// # Why checking is a button rather than something that just happens
//
// AC-10 forbids a hash-everything path, and FD-49 measured why it would hurt:
// the hashing code runs at 2,765 MB/s while the library's drive delivers 42 to
// 80, so on a real library this is minutes of waiting on the disk. Opening the
// screen is free and honest ("not checked yet"); reading the bytes is a thing a
// person asks for, and can stop.
export function Duplicates({ scanId }: DuplicatesProps) {
  const {
    review,
    policy,
    setPolicy,
    status,
    errorCode,
    errorDetail,
    progress,
    actionError,
    savedTo,
    reload,
    check,
    stopCheck,
    confirm,
    clearConfirmation,
    save,
  } = useDuplicates(scanId);

  if (status === "loading") {
    return (
      <div className="max-w-prose">
        <Heading />
        <p className="mt-6 text-body text-ink-2">{S.loading}</p>
      </div>
    );
  }

  if (status === "error") {
    return (
      <ErrorCallout
        copy={errorCode ? ERROR_COPY[errorCode] : null}
        heading={S.heading}
        detail={errorDetail}
        onRetry={reload}
      />
    );
  }

  if (scanId === null) {
    return <EmptyState heading={S.noScanHeading} body={S.noScanBody} tone="neutral" />;
  }

  const groups = review?.groups ?? [];

  return (
    <div>
      <Heading />

      {actionError && (
        <div className="mt-4">
          <ErrorCallout
            compact
            copy={
              actionError.code && actionError.code in ERROR_COPY
                ? ERROR_COPY[actionError.code as keyof typeof ERROR_COPY]
                : null
            }
            detail={actionError.detail}
          />
        </div>
      )}

      {/* AC-31's distinct "checking copies" state: a job with its own progress,
          not a generic spinner, because the wait is long enough that a person
          needs to see it moving and be able to stop it. */}
      {progress && (
        <div className="mt-6 max-w-prose">
          <ScanProgress
            done={progress.total === null ? undefined : progress.done}
            total={progress.total ?? undefined}
            onStop={stopCheck}
          />
          <p className="mt-2 text-meta text-ink-3">{S.checkingNote}</p>
        </div>
      )}

      {groups.length === 0 ? (
        <div className="mt-8">
          <EmptyState heading={S.emptyHeading} body={S.emptyBody} tone="good" />
        </div>
      ) : (
        <>
          <div className="mt-6 flex flex-wrap items-center gap-3">
            <p className="text-body tabular-nums text-ink-2">
              {S.groupCount(review?.group_count ?? 0)}
              {" · "}
              {S.copyCount(review?.copy_count ?? 0)}
              {" · "}
              {S.estimate(formatBytes(review?.candidate_bytes_estimate ?? 0))}
            </p>
            {!progress && (
              <button
                type="button"
                onClick={() => void check()}
                className="rounded border border-border-2 px-3 py-1.5 text-body font-semibold text-ink hover:bg-surface-2"
              >
                {S.check}
              </button>
            )}
            <button
              type="button"
              onClick={() => void save()}
              className="rounded border border-border-2 px-3 py-1.5 text-body font-semibold text-ink hover:bg-surface-2"
            >
              {S.save}
            </button>
            {savedTo && <span className="text-meta break-all text-ink-3">{S.saved(savedTo)}</span>}
          </div>

          <div className="mt-4">
            <PolicySelector value={policy} onChange={setPolicy} />
          </div>

          <ul className="mt-6 flex flex-col gap-3">
            {groups.map((group) => (
              <DuplicateCard
                key={`${group.method}:${group.group_key}`}
                group={group}
                onConfirm={(keeper, losers, override) =>
                  void confirm(group, keeper, losers, override)
                }
                onClear={() => void clearConfirmation(group)}
              />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function Heading() {
  return (
    <header>
      <h1 className="font-serif text-hero font-medium tracking-display text-ink text-balance">
        {S.heading}
      </h1>
      <p className="mt-2 max-w-prose text-lead leading-relaxed text-ink-2 text-pretty">{S.lede}</p>
    </header>
  );
}
