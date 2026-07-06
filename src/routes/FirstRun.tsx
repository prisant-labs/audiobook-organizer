import { useState } from "react";
import { Library } from "lucide-react";
import { STRINGS } from "@/lib/strings";
import { pickLibraryFolder } from "@/lib/dialog";

export interface FirstRunProps {
  // Called with the chosen absolute path once the user picks a folder. The
  // parent (AppRoot) persists it via settings_set, which flips the app out of
  // first-run. Rejects if the save fails, so this screen can show the error and
  // stay put.
  onChoose: (path: string) => Promise<void>;
}

// First-run / library root selection (F-909, AC-28/AC-29). The ONLY path
// forward is picking a library folder through the OS folder picker
// (tauri-plugin-dialog); no library is assumed. Cancelling the picker keeps the
// user here. Calm, one primary action, plain language (design-system Sections
// 4-5, 6). Theme Day and ruleset abs-author-first are the first-run defaults:
// Day is the settings default a fresh database already carries (FD-09), and
// abs-author-first is the shipped default layout preset (D-02); neither needs a
// choice here, so the single action stays "choose your folder".
export function FirstRun({ onChoose }: FirstRunProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const s = STRINGS.firstRun;

  async function choose() {
    setError(null);
    setBusy(true);
    try {
      const path = await pickLibraryFolder();
      if (path === null) {
        // Cancelled: no library chosen, stay on first-run (AC-28).
        setBusy(false);
        return;
      }
      // On success the parent re-renders to the shell; leaving `busy` true
      // avoids a flash of the enabled button in the gap before that happens.
      await onChoose(path);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto flex min-h-full max-w-[48ch] flex-col items-start justify-center gap-5 px-10 py-16">
      <span
        aria-hidden
        className="flex size-11 items-center justify-center rounded-[10px] bg-surface-2 text-primary"
      >
        <Library size={22} />
      </span>
      <h1 className="font-serif text-[30px] font-medium leading-tight tracking-[-0.01em] text-balance">
        {s.heading}
      </h1>
      <p className="max-w-[46ch] text-[14.5px] leading-relaxed text-ink-2 text-pretty">{s.lede}</p>
      <button
        type="button"
        onClick={choose}
        disabled={busy}
        className="mt-1 rounded bg-primary px-5 py-2.5 text-[13.5px] font-semibold text-primary-ink transition-colors hover:bg-primary-hover disabled:opacity-70"
      >
        {busy ? s.choosing : s.chooseAction}
      </button>
      {error && (
        <p role="alert" className="text-[13px] leading-relaxed text-danger">
          {s.errorPrefix}: {error}
        </p>
      )}
      <p className="mt-3 text-[12px] text-ink-3">{s.reassurance}</p>
    </div>
  );
}
