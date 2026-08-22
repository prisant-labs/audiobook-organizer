import { useState } from "react";
import { FolderOpen } from "lucide-react";
import { commands, type RevealRoot } from "@/lib/bindings";
import { codeOf, formatAppError, type AppErrorCode } from "@/lib/appError";
import { cn } from "@/lib/utils";
import { STRINGS } from "@/lib/strings";

const S = STRINGS.openFolder;

export interface OpenFolderProps {
  /** The path to reveal. A file opens its folder with the file selected. */
  path: string;
  /** Accessible name, which must say WHICH folder when several sit together. */
  label?: string;
  /** `icon` for inline use beside a path; `link` for a standalone text link. */
  variant?: "icon" | "link";
  className?: string;
}

// The open-a-folder affordance (F-610, v0.6.0 P10, AC-49).
//
// ONE COMPONENT, USED EVERYWHERE, because AC-49 puts this wherever a path is
// displayed and a second implementation would be a second place to get the
// failure handling wrong. It renders next to a path rather than replacing it:
// the path is the information, this is the action.
//
// # It asks; it never opens anything itself
//
// FD-29 grants the WebView no `fs` and no `shell` capability and AC-47 requires
// the allowlist to stay unchanged, so there is no browser-side way to open a
// folder and this component does not pretend otherwise. It calls
// `reveal_in_folder` and the backend decides, including whether the path is
// allowed at all (AC-48). Nothing here re-derives that rule: a check duplicated
// on the frontend would be a second implementation of the one thing that must
// not be got wrong, and it would drift.
//
// # Failure is shown inline, not swallowed
//
// AC-48 requires a refusal rather than silence, and silence is exactly what a
// fire-and-forget click produces. The most likely refusal is ordinary: the
// folder moved since the scan that displayed it. So the error appears beside the
// control that caused it, in the same plain language the rest of the app uses,
// and clears on the next attempt.
export function OpenFolder({ path, label, variant = "icon", className }: OpenFolderProps) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const accessibleName = label ?? S.open;

  async function open() {
    setError(null);
    setBusy(true);
    const result = await commands.revealInFolder(path);
    setBusy(false);
    if (result.status === "error") {
      const code: AppErrorCode | null = codeOf(result.error);
      setError(formatAppError(result.error) || (code ?? S.failed));
    }
  }

  return (
    <>
      <button
        type="button"
        onClick={() => void open()}
        disabled={busy}
        aria-label={accessibleName}
        title={accessibleName}
        className={cn(
          "inline-flex flex-none items-center gap-1 rounded text-link",
          "hover:underline disabled:opacity-50",
          variant === "icon" ? "p-0.5" : "text-meta",
          className,
        )}
      >
        <FolderOpen size={14} aria-hidden className="flex-none" />
        {variant === "link" && <span>{accessibleName}</span>}
      </button>
      {error && (
        <span role="status" className="text-meta text-danger">
          {error}
        </span>
      )}
    </>
  );
}

export interface OpenRootLinkProps {
  /** Which well-known root, by NAME. No path crosses the boundary here. */
  root: RevealRoot;
  label: string;
}

// AC-50's permanent sidebar quick links.
//
// A separate entry point from `OpenFolder` because it sends a ROOT NAME rather
// than a path. The Archive root is usually unset in settings, since the plan
// builder derives it, so a link that passed a path would have to reconstruct
// that derivation in TypeScript and would drift from the builder the first time
// it changed. Naming the root keeps the rule in one place.
export function OpenRootLink({ root, label }: OpenRootLinkProps) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function open() {
    setError(null);
    setBusy(true);
    const result = await commands.revealRoot(root);
    setBusy(false);
    if (result.status === "error") setError(formatAppError(result.error) || S.failed);
  }

  return (
    <>
      <button
        type="button"
        onClick={() => void open()}
        disabled={busy}
        className="inline-flex items-center gap-1.5 rounded text-left text-meta text-link hover:underline disabled:opacity-50"
      >
        <FolderOpen size={13} aria-hidden className="flex-none" />
        <span>{label}</span>
      </button>
      {error && (
        <span role="status" className="text-meta text-danger">
          {error}
        </span>
      )}
    </>
  );
}
