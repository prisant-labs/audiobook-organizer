import { useState } from "react";
import { Undo2, FlaskConical, AlertCircle, CheckCircle2 } from "lucide-react";
import { commands, type HistoryEntry, type UndoOffer } from "@/lib/bindings";
import { useHistory } from "@/hooks/useHistory";
import { ErrorCallout } from "@/components/states/ErrorCallout";
import { ERROR_COPY } from "@/lib/errorCopy";
import { codeOf, formatAppError } from "@/lib/appError";
import { STRINGS } from "@/lib/strings";

// The History screen (v0.6.0): the record of past tidy-ups and the way back from
// each one.
//
// # Why this screen is part of the interruption-safety milestone
//
// v0.5.0 built the undo engine - a self-contained undo file, an inverse-plan
// builder, a partial rollback from a halted run's journal tail - and shipped it
// unreachable behind a placeholder route. The product's promise ("always give me
// a comprehensible way back") was true of the engine and false of the app. An
// interruption-recovery release that still had no History screen would leave a
// user who was interrupted with a recovered journal and nowhere to act on it.
//
// # Undo is a plan, not a button
//
// Every action here PREPARES an undo and hands the user to the same review
// surface a forward tidy-up uses (D-09: rollback is not a special code path).
// Nothing moves on the strength of a click in this screen. That is why the
// buttons say what they do and the note underneath says nothing happens yet.

const S = STRINGS.history;

export function History({ onOpenPlan }: { onOpenPlan: (planId: number) => void }) {
  const { entries, status, errorCode, errorDetail, reload } = useHistory();
  // The job whose undo is currently being prepared, so only that row's button
  // shows a pending state (a global spinner would hide which run is acting).
  const [preparing, setPreparing] = useState<number | null>(null);
  const [actionError, setActionError] = useState<{ code: string | null; detail: string } | null>(
    null,
  );

  async function prepareUndo(entry: HistoryEntry) {
    setPreparing(entry.jobId);
    setActionError(null);
    try {
      const result =
        entry.undo.kind === "put-everything-back"
          ? await commands.rollbackPrepare(entry.undo.manifest_id)
          : entry.undo.kind === "put-recent-changes-back"
            ? await commands.rollbackPreparePartial(entry.jobId, entry.undo.op_ids)
            : null;

      if (!result) return;
      if (result.status === "ok") {
        onOpenPlan(result.data.plan_id);
      } else {
        setActionError({ code: codeOf(result.error), detail: formatAppError(result.error) });
      }
    } finally {
      setPreparing(null);
    }
  }

  if (status === "loading") {
    return (
      <div className="max-w-[64ch]">
        <Heading />
        <p className="mt-6 text-[14px] text-ink-2">{STRINGS.states.booting}</p>
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

  return (
    <div className="max-w-[64ch]">
      <Heading />

      {actionError && (
        <ErrorCallout
          compact
          copy={
            actionError.code && actionError.code in ERROR_COPY
              ? ERROR_COPY[actionError.code as keyof typeof ERROR_COPY]
              : null
          }
          detail={actionError.detail}
        />
      )}

      {entries.length === 0 ? (
        <div className="mt-8">
          <h2 className="font-serif text-[18px] font-medium">{S.emptyHeading}</h2>
          <p className="mt-2 max-w-[46ch] text-[14px] leading-relaxed text-ink-2">{S.emptyBody}</p>
        </div>
      ) : (
        <ul className="mt-6 flex flex-col gap-3">
          {entries.map((entry) => (
            <HistoryRow
              key={entry.jobId}
              entry={entry}
              preparing={preparing === entry.jobId}
              onUndo={() => void prepareUndo(entry)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function Heading() {
  return (
    <>
      <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em]">{S.heading}</h1>
      <p className="mt-2 max-w-[52ch] text-[14px] leading-relaxed text-ink-2">{S.lede}</p>
    </>
  );
}

function HistoryRow({
  entry,
  preparing,
  onUndo,
}: {
  entry: HistoryEntry;
  preparing: boolean;
  onUndo: () => void;
}) {
  const isPractice = entry.mode === "dry-run";
  const Icon = isPractice ? FlaskConical : entry.undo.kind === "needs-a-look" ? AlertCircle : CheckCircle2;

  return (
    <li className="rounded border border-border-2 bg-surface p-4">
      <div className="flex items-start gap-2.5">
        <Icon
          size={18}
          aria-hidden
          className={`mt-0.5 flex-none ${entry.undo.kind === "needs-a-look" ? "text-warn" : "text-ink-3"}`}
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
            <span className="text-[14px] font-semibold text-ink">
              {isPractice ? S.practiceRun : S.realRun}
            </span>
            <span className="text-[12.5px] text-ink-3">{stateLabel(entry.state)}</span>
            {entry.startedAt && (
              <time className="text-[12.5px] text-ink-3" dateTime={entry.startedAt}>
                {formatWhen(entry.startedAt)}
              </time>
            )}
          </div>

          <p className="mt-1 text-[13.5px] text-ink-2">
            {isPractice
              ? S.practiceRunNote
              : entry.changesMade > 0
                ? S.changesMade(entry.changesMade)
                : S.noChangesMade}
          </p>

          <UndoAction entry={entry} preparing={preparing} onUndo={onUndo} />
        </div>
      </div>
    </li>
  );
}

function UndoAction({
  entry,
  preparing,
  onUndo,
}: {
  entry: HistoryEntry;
  preparing: boolean;
  onUndo: () => void;
}) {
  const offer: UndoOffer = entry.undo;

  if (offer.kind === "practice-run" || offer.kind === "nothing-to-put-back") {
    return null;
  }

  if (offer.kind === "needs-a-look") {
    return (
      <div className="mt-3">
        <p className="text-[13px] font-semibold text-warn">{S.needsALook}</p>
        <p className="mt-1 max-w-[46ch] text-[13px] leading-relaxed text-ink-2">
          {S.needsALookDetail}
        </p>
      </div>
    );
  }

  const label = offer.kind === "put-everything-back" ? S.putEverythingBack : S.putRecentChangesBack;

  return (
    <div className="mt-3">
      <button
        type="button"
        onClick={onUndo}
        disabled={preparing}
        className="inline-flex items-center gap-1.5 rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-60"
      >
        <Undo2 size={15} aria-hidden />
        {preparing ? S.preparing : label}
      </button>
      <p className="mt-1.5 max-w-[46ch] text-[12.5px] leading-relaxed text-ink-3">
        {S.undoIsReviewedFirst}
      </p>
    </div>
  );
}

function stateLabel(state: string): string {
  switch (state) {
    case "done":
      return S.state.done;
    case "failed":
      return S.state.failed;
    case "stopped":
      return S.state.stopped;
    case "running":
      return S.state.running;
    default:
      return state;
  }
}

// A short, local, human date. Deliberately not a relative time ("2 days ago"):
// the record is a durable log, and an absolute date stays true when re-read.
function formatWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
