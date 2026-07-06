import type { ReactNode } from "react";
import type { Theme } from "@/lib/theme";
import { Titlebar } from "./Titlebar";

export interface FrameProps {
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
  children: ReactNode;
}

// A minimal window frame for the pre-shell screens (first-run, boot, error):
// the custom titlebar (window controls + drag region + theme toggle, required
// because `decorations:false`) over a single scrollable `<main>` landmark. The
// full AppShell has its own titlebar-plus-sidebar layout; only one of AppShell
// / Frame renders at a time, so there is never a duplicate `<main>` or titlebar.
export function Frame({ theme, onThemeChange, children }: FrameProps) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <Titlebar theme={theme} onThemeChange={onThemeChange} />
      <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
    </div>
  );
}
