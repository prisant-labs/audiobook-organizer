import { useCallback, useEffect, useRef, useState } from "react";
import {
  getActiveRuleset,
  previewPlan,
  saveRuleset,
  type Ruleset,
} from "@/lib/ruleset";
import type { PlanGroupView } from "@/lib/bindings";

// F-906 ruleset editor (v0.4.0 Phase 6, T-26/T-27): owns the Settings screen's
// "how your shelves get organized" section. Loads the ACTIVE ruleset once,
// tracks an in-memory draft the preset picker and policy toggles edit, and
// re-plans a live preview against that draft whenever it changes - debounced
// and cancellable, exactly like the P5 `usePlanReview` re-entrancy guard
// pattern, but simpler: because the preview effect keys off `draft` itself
// (rather than a manually-bumped nonce), React's own cleanup-before-next-run
// ordering already supersedes a stale in-flight request every time the draft
// changes again, so no separate `inFlightRef` is needed here.
//
// Saving (AC-32/AC-33) always makes the draft the ACTIVE ruleset; the next
// time `usePlanReview` calls `plan_generate` (Review.tsx, on mount or its own
// `regenerate()`), it reads whatever is active, which is how "a saved change
// persists the active ruleset; the review screen regenerates from it" is
// satisfied without any direct coupling between this hook and Review.tsx.
// This does NOT push a live update into an already-open Tidy-up screen - a
// known, documented follow-up (cross-screen invalidation is out of this
// phase's scope).

const PREVIEW_DEBOUNCE_MS = 400;

export type EditorStatus = "loading" | "ready" | "error";
export type PreviewStatus = "idle" | "loading" | "ready" | "error";
export type SaveStatus = "idle" | "saving" | "saved" | "error";

export interface UseRulesetEditor {
  status: EditorStatus;
  error: string | null;
  /** The loaded ruleset's editable draft; `null` until the initial load resolves. */
  draft: Ruleset | null;
  /** Replace the draft with the result of `updater(current)`. A no-op before load. */
  updateDraft: (updater: (prev: Ruleset) => Ruleset) => void;
  /** Whether the draft differs from the last saved/loaded ruleset. */
  dirty: boolean;
  previewStatus: PreviewStatus;
  previewError: string | null;
  /** The seven per-group projected counts for the current draft, or `null` (no scan, or not previewed yet). */
  preview: PlanGroupView[] | null;
  saveStatus: SaveStatus;
  saveError: string | null;
  save: () => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useRulesetEditor(scanId: number | null): UseRulesetEditor {
  const [status, setStatus] = useState<EditorStatus>("loading");
  const [error, setError] = useState<string | null>(null);
  const [rulesetId, setRulesetId] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [saved, setSaved] = useState<Ruleset | null>(null);
  const [draft, setDraft] = useState<Ruleset | null>(null);

  const [rawPreviewStatus, setRawPreviewStatus] = useState<PreviewStatus>("idle");
  const [rawPreviewError, setRawPreviewError] = useState<string | null>(null);
  const [rawPreview, setRawPreview] = useState<PlanGroupView[] | null>(null);

  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [saveError, setSaveError] = useState<string | null>(null);

  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Initial load: the active ruleset, once.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      setStatus("loading");
      setError(null);
      try {
        const detail = await getActiveRuleset();
        if (cancelled || !mountedRef.current) return;
        setRulesetId(detail.id);
        setName(detail.name);
        setSaved(detail.ruleset);
        setDraft(detail.ruleset);
        setStatus("ready");
      } catch (e) {
        if (cancelled || !mountedRef.current) return;
        setError(messageOf(e));
        setStatus("error");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const updateDraft = useCallback((updater: (prev: Ruleset) => Ruleset) => {
    setDraft((prev) => (prev ? updater(prev) : prev));
    // A fresh edit supersedes any pending save confirmation/error.
    setSaveStatus("idle");
    setSaveError(null);
  }, []);

  // Live re-plan preview (AC-33): debounced and cancellable. `rawPreviewStatus`/
  // `rawPreview` track only the async operation itself; the "no scan yet"
  // idle state is a DERIVED value below rather than something the effect sets,
  // so the effect's early-return branch (no scanId or no draft) never calls
  // setState synchronously from within an effect body.
  useEffect(() => {
    if (scanId == null || draft == null) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      setRawPreviewStatus("loading");
      setRawPreviewError(null);
      void (async () => {
        try {
          const groups = await previewPlan(scanId, draft);
          if (cancelled || !mountedRef.current) return;
          setRawPreview(groups);
          setRawPreviewStatus("ready");
        } catch (e) {
          if (cancelled || !mountedRef.current) return;
          setRawPreviewError(messageOf(e));
          setRawPreviewStatus("error");
        }
      })();
    }, PREVIEW_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [scanId, draft]);

  const canPreview = scanId != null && draft != null;
  const previewStatus: PreviewStatus = canPreview ? rawPreviewStatus : "idle";
  const previewError = canPreview ? rawPreviewError : null;
  const preview = canPreview ? rawPreview : null;

  const dirty = saved != null && draft != null && JSON.stringify(saved) !== JSON.stringify(draft);

  const save = useCallback(async () => {
    if (!draft) return;
    setSaveStatus("saving");
    setSaveError(null);
    try {
      const detail = await saveRuleset({ id: rulesetId, name, ruleset: draft });
      if (!mountedRef.current) return;
      setRulesetId(detail.id);
      setSaved(detail.ruleset);
      setDraft(detail.ruleset);
      setSaveStatus("saved");
    } catch (e) {
      if (!mountedRef.current) return;
      setSaveError(messageOf(e));
      setSaveStatus("error");
    }
  }, [draft, rulesetId, name]);

  return {
    status,
    error,
    draft,
    updateDraft,
    dirty,
    previewStatus,
    previewError,
    preview,
    saveStatus,
    saveError,
    save,
  };
}
