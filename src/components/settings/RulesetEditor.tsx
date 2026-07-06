import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { STRINGS } from "@/lib/strings";
import { useRulesetEditor } from "@/hooks/useRulesetEditor";
import { getPresetExamples } from "@/lib/ruleset";
import type { Preset, PresetExampleView, Ruleset } from "@/lib/bindings";

export interface RulesetEditorProps {
  // The most recently completed scan's id (from `classify_overview`, owned by
  // AppShell's single `useHealthMetrics()` call - see Library.tsx/Review.tsx's
  // own comment on why this is shared rather than re-fetched here). `null`
  // before any scan has ever completed: the live counts show an honest
  // "scan first" note rather than previewing against nothing.
  scanId: number | null;
}

const PRESET_ORDER: readonly Preset[] = ["abs-author-first", "title-first", "hybrid-genre"];

// F-906 ruleset editor (T-26/T-27, AC-32/AC-33): the "how your shelves get
// organized" section of Settings. Three parts, top to bottom: the naming-
// style preset picker (with a rendered example path per style), the F-402
// safety/tidiness toggles with their safe defaults already applied, and the
// live re-plan counts for the current draft (debounced, cancellable,
// honest loading/error/no-scan states - `useRulesetEditor`). Saving persists
// the draft as the ACTIVE ruleset; the review screen regenerates from it the
// next time it builds a plan.
export function RulesetEditor({ scanId }: RulesetEditorProps) {
  const s = STRINGS.rulesetEditor;
  const editor = useRulesetEditor(scanId);
  const [examples, setExamples] = useState<PresetExampleView[] | null>(null);

  const width = editor.draft?.naming.series_index_width ?? 1;
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const loaded = await getPresetExamples(width);
      if (!cancelled) setExamples(loaded);
    })();
    return () => {
      cancelled = true;
    };
  }, [width]);

  if (editor.status === "loading") {
    return <p className="text-[13px] text-ink-2">{s.loading}</p>;
  }
  if (editor.status === "error" || !editor.draft) {
    return <p className="text-[13px] text-danger">{editor.error}</p>;
  }

  const { draft, updateDraft } = editor;

  return (
    <section className="rounded-md border border-border bg-surface p-5">
      <h2 className="font-serif text-[16px] font-medium">{s.heading}</h2>
      <p className="mt-1 max-w-[56ch] text-[12.5px] leading-relaxed text-ink-2">{s.intro}</p>

      <h3 className="mt-6 text-[13px] font-semibold text-ink">{s.presetHeading}</h3>
      <div className="mt-2.5 grid gap-2.5 sm:grid-cols-3">
        {PRESET_ORDER.map((preset) => {
          const example = examples?.find((e) => e.preset === preset);
          const copy = s.presets[preset];
          const selected = draft.naming.preset === preset;
          return (
            <button
              key={preset}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() =>
                updateDraft((prev) => ({ ...prev, naming: { ...prev.naming, preset } }))
              }
              className={cn(
                "rounded-md border border-border-2 p-3 text-left transition-colors hover:border-ink-3",
                selected && "border-primary bg-surface-2 ring-1 ring-primary",
              )}
            >
              <div className="text-[12.5px] font-semibold text-ink">{copy.label}</div>
              <p className="mt-1 text-[11.5px] leading-snug text-ink-2">{copy.description}</p>
              <p className="mt-2 break-all font-mono text-[10.5px] text-ink-3">
                {example ? example.example_path : "..."}
              </p>
            </button>
          );
        })}
      </div>

      <h3 className="mt-6 text-[13px] font-semibold text-ink">{s.policiesHeading}</h3>
      <div className="mt-2.5 flex flex-col gap-4">
        <ToggleRow
          label={s.oneBookPerFolder.label}
          help={s.oneBookPerFolder.help}
          checked={draft.structure.one_book_per_folder}
          onChange={(v) =>
            updateDraft((prev) => ({
              ...prev,
              structure: { ...prev.structure, one_book_per_folder: v },
            }))
          }
        />
        <ChoiceRow
          label={s.packShell.label}
          help={s.packShell.help}
          options={[
            { value: "quarantine", label: s.packShell.quarantine },
            { value: "leave-in-place", label: s.packShell.leaveInPlace },
          ]}
          value={draft.structure.pack_shell}
          onChange={(v) =>
            updateDraft((prev) => ({
              ...prev,
              structure: { ...prev.structure, pack_shell: v as Ruleset["structure"]["pack_shell"] },
            }))
          }
        />
        <ChoiceRow
          label={s.sidecars.label}
          options={[
            { value: "keep-with-book", label: s.sidecars.keepWithBook },
            { value: "quarantine", label: s.sidecars.quarantine },
          ]}
          value={draft.structure.sidecars}
          onChange={(v) =>
            updateDraft((prev) => ({
              ...prev,
              structure: { ...prev.structure, sidecars: v as Ruleset["structure"]["sidecars"] },
            }))
          }
        />
        <ChoiceRow
          label={s.preferredFormat.label}
          options={[
            { value: "m4b", label: s.preferredFormat.m4b },
            { value: "mp3", label: s.preferredFormat.mp3 },
          ]}
          value={draft.structure.preferred_format}
          onChange={(v) =>
            updateDraft((prev) => ({
              ...prev,
              structure: {
                ...prev.structure,
                preferred_format: v as Ruleset["structure"]["preferred_format"],
              },
            }))
          }
        />
        <ToggleRow
          label={s.emptyFolderRemoval.label}
          checked={draft.structure.empty_folder_removal}
          onChange={(v) =>
            updateDraft((prev) => ({
              ...prev,
              structure: { ...prev.structure, empty_folder_removal: v },
            }))
          }
        />
        <ToggleRow
          label={s.stripNoise.label}
          help={s.stripNoise.help}
          checked={draft.cleanup.strip_noise}
          onChange={(v) =>
            updateDraft((prev) => ({ ...prev, cleanup: { ...prev.cleanup, strip_noise: v } }))
          }
        />
      </div>

      <h3 className="mt-6 text-[13px] font-semibold text-ink">{s.liveCountsHeading}</h3>
      <LiveCounts editor={editor} />

      <div className="mt-5 flex items-center gap-3">
        <button
          type="button"
          onClick={() => void editor.save()}
          disabled={!editor.dirty || editor.saveStatus === "saving"}
          className="rounded bg-primary px-4 py-2 text-[13px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {editor.saveStatus === "saving" ? s.saveSaving : s.saveAction}
        </button>
        <p role="status" aria-live="polite" className="text-[12.5px] text-ink-3">
          {editor.saveStatus === "saved" && s.saveSaved}
          {editor.saveStatus === "error" && <span className="text-danger">{editor.saveError}</span>}
          {editor.saveStatus === "idle" && editor.dirty && s.unsavedNote}
        </p>
      </div>
    </section>
  );
}

function ToggleRow({
  label,
  help,
  checked,
  onChange,
}: {
  label: string;
  help?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div>
        <p className="text-[12.5px] font-medium text-ink">{label}</p>
        {help && <p className="mt-0.5 max-w-[48ch] text-[11.5px] leading-relaxed text-ink-2">{help}</p>}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative h-[21px] w-[38px] flex-none rounded-full border border-border-2 bg-surface-2 transition-colors",
          checked && "border-primary bg-primary",
        )}
      >
        <span
          className={cn(
            "absolute top-0.5 left-0.5 h-[15px] w-[15px] rounded-full bg-ink-3 transition-transform",
            checked && "translate-x-[17px] bg-white",
          )}
        />
      </button>
    </div>
  );
}

function ChoiceRow({
  label,
  help,
  options,
  value,
  onChange,
}: {
  label: string;
  help?: string;
  options: readonly { value: string; label: string }[];
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div>
        <p className="text-[12.5px] font-medium text-ink">{label}</p>
        {help && <p className="mt-0.5 max-w-[48ch] text-[11.5px] leading-relaxed text-ink-2">{help}</p>}
      </div>
      <div role="group" aria-label={label} className="flex flex-none overflow-hidden rounded-full border border-border-2">
        {options.map((opt) => (
          <button
            key={opt.value}
            type="button"
            aria-pressed={value === opt.value}
            onClick={() => onChange(opt.value)}
            className={cn(
              "whitespace-nowrap px-3 py-[3px] text-[11.5px] text-ink-3 transition-colors",
              value === opt.value && "bg-surface-2 font-semibold text-ink",
            )}
          >
            {opt.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function LiveCounts({ editor }: { editor: ReturnType<typeof useRulesetEditor> }) {
  const s = STRINGS.rulesetEditor;

  if (editor.previewStatus === "idle" && editor.preview === null) {
    return <p className="mt-2 text-[12.5px] text-ink-2">{s.liveCountsNoScan}</p>;
  }
  if (editor.previewStatus === "error") {
    return <p className="mt-2 text-[12.5px] text-danger">{editor.previewError}</p>;
  }

  return (
    <div className="relative mt-2.5">
      {editor.previewStatus === "loading" && (
        <p aria-live="polite" className="mb-1.5 text-[11.5px] text-ink-3">
          {s.liveCountsLoading}
        </p>
      )}
      {editor.preview && (
        <ul className={cn("flex flex-col gap-1", editor.previewStatus === "loading" && "opacity-60")}>
          {editor.preview.map((group) => (
            <li
              key={group.group}
              className="flex items-center justify-between text-[12px] tabular-nums text-ink-2"
            >
              <span>{group.label}</span>
              <span className="font-medium text-ink">
                {group.op_count.toLocaleString("en-US")}
                {group.blocked_count > 0 && (
                  <span className="ml-1.5 text-warn">({group.blocked_count} held)</span>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
