import { useCallback, useState } from "react";
import { navCountsFrom } from "@/hooks/useNavCounts";
import { useHealthMetrics, type UseHealthMetrics } from "@/hooks/useHealthMetrics";
import { DEFAULT_ROUTE, type RouteId } from "@/routes";
import { DEFAULT_THEME, isTheme, type Theme } from "@/lib/theme";
import { pickLibraryFolder } from "@/lib/dialog";
import type { AppSettings } from "@/lib/settings";
import { commands, type AppError } from "@/lib/bindings";
import { appErrorCode, formatAppError } from "@/lib/appError";
import { copyForCode } from "@/lib/errorCopy";
import { ErrorCallout } from "@/components/states/ErrorCallout";
import { InterruptionNotice } from "@/components/states/InterruptionNotice";
import { useStartupInterruption } from "@/hooks/useStartupInterruption";
import { Titlebar } from "./Titlebar";
import { Sidebar } from "./Sidebar";
import { ScreenContainer } from "./ScreenContainer";
import { ComingSoon } from "./ComingSoon";
import { History } from "@/routes/History";
import { Settings } from "@/routes/Settings";
import { Library } from "@/routes/Library";
import { Review } from "@/routes/Review";
import { Apply } from "@/routes/Apply";

export interface AppShellProps {
  // The persisted settings (from useAppSettings, owned by AppRoot). AppShell
  // renders once a library root is configured; it derives the theme from these
  // and hosts the Settings screen.
  settings: AppSettings;
  // Persist a full replacement settings row (optimistic; see useAppSettings).
  onUpdate: (next: AppSettings) => Promise<void>;
}

// The real app shell (T-01, refactored in Phase 2): custom titlebar, sidebar,
// and the single screen container every route renders inside. Theme is derived
// from settings and changed through `onUpdate` (the one source of truth,
// persisted via settings_set; AppShell does not touch `data-theme` itself -
// useAppSettings applies it). The Library route now hosts the real F-902
// library home (T-15..T-18, v0.4.0 Phase 4). The disposable v0.1.0 tracer
// (`src/App.tsx`) is deleted (T-37, G-7, Phase 8); this is now the only shell.
// The Settings route hosts the real F-803/F-909 Settings screen.
// Active job state: non-null while an apply job is running. The shell is the
// single source of truth; when set, the Apply screen replaces the normal route
// content regardless of the current route (the sidebar stays visible for
// wayfinding but navigation is naturally blocked while a job is live).
interface ActiveJob {
  jobId: number;
  mode: "dry-run" | "real";
}

export function AppShell({ settings, onUpdate }: AppShellProps) {
  const [route, setRoute] = useState<RouteId>(DEFAULT_ROUTE);
  const [activeJob, setActiveJob] = useState<ActiveJob | null>(null);
  // A run that never started (apply_start failed): there is no job to show, so
  // this holds the family-safe error surface instead of failing silently to the
  // console (P8 minor). Cleared by the retry action, which returns to the review.
  const [startError, setStartError] = useState<AppError | null>(null);
  // An undo plan prepared from History (v0.6.0). Reviewing it uses the SAME
  // surface a forward run uses (D-09), so this is just a plan id the Organize
  // route opens instead of generating one from the current scan.
  //
  // It is scoped to ONE visit. `usePlanReview` prefers this id over the scan on
  // every run, including a regenerate, so leaving it set would strand the user on
  // the undo: navigating away and back to Organize would reopen the undo plan, and
  // "build the plan again" would rebuild the undo rather than a forward plan.
  // `navigate` below is therefore the ONLY way to change route from the chrome,
  // and it always clears the selection; `openUndoPlan` is the one path that sets
  // it, immediately before routing.
  const [openPlanId, setOpenPlanId] = useState<number | null>(null);

  // Ordinary navigation: always drops any prepared undo, so Organize means the
  // forward plan unless the user just asked for an undo.
  const navigate = useCallback((next: RouteId) => {
    setOpenPlanId(null);
    setRoute(next);
  }, []);

  // The one path that opens a prepared undo on the review surface.
  const openUndoPlan = useCallback((planId: number) => {
    setOpenPlanId(planId);
    setRoute("organize");
  }, []);
  // ONE `classify_overview` load for the whole shell (T-15): the Sidebar
  // badges and the Library home both derive from this single `health` value,
  // so they can never disagree and both refresh together when a scan
  // completes (`Library` calls `health.reload()`; see useNavCounts.ts).
  const health = useHealthMetrics();
  const counts = navCountsFrom(health.overview);

  // A run a previous session was killed in the middle of (v0.6.0 P1c,
  // F-606). It takes the screen area ahead of everything else, but the sidebar
  // stays live and navigation stays open: the dangerous action is starting a
  // NEW run, not using the app, and blocking navigation would be a
  // procedural gate that stops nothing an IPC caller can reach. The gate that
  // matters belongs in the engine, beside ensure_forward_tidying_allowed, and
  // is recorded in STATUS.md as a precondition for enabling real changes.
  // See docs/internal/releases/v0.6.0-hardening/design-p1c-interruption-surface.md.
  const interruption = useStartupInterruption();
  const [preparingUndo, setPreparingUndo] = useState(false);
  const theme: Theme = isTheme(settings.theme) ? settings.theme : DEFAULT_THEME;
  const onThemeChange = (next: Theme) => void onUpdate({ ...settings, theme: next });

  // F-909 re-pick (AC-31): the shell owns the settings + dialog seam, so the
  // root-missing surface (in Library) delegates re-picking here. Opens the OS
  // folder picker and persists the new library root; resolves `true` when a
  // folder was chosen (the frontend never touches the filesystem, FD-29).
  const onRepickRoot = useCallback(async (): Promise<boolean> => {
    const path = await pickLibraryFolder();
    if (path === null) return false;
    await onUpdate({ ...settings, library_root: path });
    return true;
  }, [settings, onUpdate]);

  // F-904 apply start (P8, AC-27): called when the user confirms the changes
  // from ReviewFooter. Uses a dry-run apply for safety in P8; the mode can be
  // promoted to "real" in a later release once the UI exposes a mode selector.
  // NEVER targets E:\Books - Audio or any real library in CI/tests; the plan
  // fixture path is the only real-world path a dry-run touches, and it uses MemFs.
  const onStartApply = useCallback(async (planId: number) => {
    setStartError(null);
    const result = await commands.applyStart(planId, "dry-run");
    if (result.status === "error") {
      // apply_start failing means the job never started, so there is no job to
      // navigate to. Surface the family-safe error state (P8 minor) rather than a
      // console-only log; the user can dismiss it and try again from the review.
      setStartError(result.error);
      return;
    }
    setActiveJob({ jobId: result.data.job_id, mode: "dry-run" });
  }, []);

  // Carrying on is a fresh scan and a fresh plan, not a replay of the
  // interrupted job (FD-39): books already moved produce no operation the
  // second time, so the next plan covers exactly the work that remains. That
  // makes this plain navigation to where the Organize action already lives.
  // Deliberately does not start a scan: work should not begin off the back of
  // a recovery screen.
  const onInterruptionGoToLibrary = useCallback(() => {
    interruption.dismiss();
    navigate("library");
  }, [interruption, navigate]);

  const onInterruptionOpenHistory = useCallback(() => {
    interruption.dismiss();
    navigate("history");
  }, [interruption, navigate]);

  // The same two-step History uses (D-09): prepare the inverse plan, then hand
  // the user to the review surface a forward run uses. Nothing moves on
  // this click. The op ids come from the engine's UndoOffer, never recomputed
  // here. A failed prepare leaves the notice up rather than dismissing it,
  // because dismissing would strand the user with nothing undone and no
  // surface left to act from.
  const onInterruptionUndo = useCallback(async () => {
    const offer = interruption.entry?.undo;
    if (offer?.kind !== "put-recent-changes-back") return;
    const jobId = interruption.entry!.jobId;
    setPreparingUndo(true);
    try {
      const result = await commands.rollbackPreparePartial(jobId, offer.op_ids);
      if (result.status === "ok") {
        interruption.dismiss();
        openUndoPlan(result.data.plan_id);
      }
    } finally {
      setPreparingUndo(false);
    }
  }, [interruption, openUndoPlan]);

  // When the user finishes with the Apply screen (Done/acknowledge/stopped),
  // return to the Organize route so they can see the plan or start again. Goes
  // through `navigate` so a just-run undo plan is cleared: after running one, the
  // review surface should show a fresh forward plan, not the undo again.
  const onApplyDone = useCallback(() => {
    setActiveJob(null);
    navigate("organize");
    health.reload();
  }, [health, navigate]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Titlebar theme={theme} onThemeChange={onThemeChange} />
      <div className="grid min-h-0 flex-1 grid-cols-[212px_1fr]">
        <Sidebar active={route} onNavigate={navigate} counts={counts} />
        <ScreenContainer>
          {interruption.interruption ? (
            <InterruptionNotice
              interruption={interruption.interruption}
              entry={interruption.entry}
              preparing={preparingUndo}
              onGoToLibrary={onInterruptionGoToLibrary}
              onUndo={() => void onInterruptionUndo()}
              onOpenHistory={onInterruptionOpenHistory}
            />
          ) : activeJob ? (
            <Apply jobId={activeJob.jobId} mode={activeJob.mode} onDone={onApplyDone} />
          ) : startError ? (
            <ErrorCallout
              copy={copyForCode(appErrorCode(startError))}
              detail={formatAppError(startError)}
              onRetry={() => setStartError(null)}
            />
          ) : (
            <RouteContent
              route={route}
              settings={settings}
              onUpdate={onUpdate}
              onNavigate={navigate}
              health={health}
              onRepickRoot={onRepickRoot}
              onStartApply={onStartApply}
              openPlanId={openPlanId}
              onOpenPlan={openUndoPlan}
            />
          )}
        </ScreenContainer>
      </div>
    </div>
  );
}

function RouteContent({
  route,
  settings,
  onUpdate,
  onNavigate,
  health,
  onRepickRoot,
  onStartApply,
  openPlanId,
  onOpenPlan,
}: {
  route: RouteId;
  settings: AppSettings;
  onUpdate: (next: AppSettings) => Promise<void>;
  onNavigate: (route: RouteId) => void;
  health: UseHealthMetrics;
  onRepickRoot: () => Promise<boolean>;
  onStartApply: (planId: number) => Promise<void>;
  openPlanId: number | null;
  onOpenPlan: (planId: number) => void;
}) {
  switch (route) {
    case "library":
      return <Library onNavigate={onNavigate} health={health} onRepickRoot={onRepickRoot} />;
    case "organize":
      return (
        <Review
          scanId={health.overview?.scan_id ?? null}
          onStartApply={onStartApply}
          openPlanId={openPlanId}
        />
      );
    case "duplicates":
      return <ComingSoon label="Duplicates" />;
    case "history":
      return <History onOpenPlan={onOpenPlan} />;
    case "settings":
      return (
        <Settings
          settings={settings}
          onUpdate={onUpdate}
          scanId={health.overview?.scan_id ?? null}
        />
      );
  }
}
