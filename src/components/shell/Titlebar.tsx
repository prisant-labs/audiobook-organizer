import { Library } from "lucide-react";
import { STRINGS } from "@/lib/strings";
import type { Theme } from "@/lib/theme";
import { ThemeToggle } from "./ThemeToggle";
import { WindowControls } from "./WindowControls";

export interface TitlebarProps {
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
}

// Custom titlebar (design-system Section 4.1; `decorations:false` in
// tauri.conf.json). 40px tall: app name at left, an empty drag region
// (`data-tauri-drag-region`) to let the OS move the window from the dead
// space, then the theme control, then the caption buttons.
export function Titlebar({ theme, onThemeChange }: TitlebarProps) {
  return (
    <div className="flex h-10 items-center border-b border-border bg-titlebar pl-3.5">
      <span className="flex items-center gap-2 text-[12.5px] text-ink-2">
        <Library size={15} aria-hidden />
        {STRINGS.appName}
      </span>
      {/* Drag handle: empty on purpose. See Tauri's window-customization
          guide; the attribute only works on a plain element, not one wrapping
          interactive children, so the theme toggle and caption buttons are
          siblings, not descendants, of this div. */}
      <div data-tauri-drag-region className="h-full flex-1" />
      <ThemeToggle theme={theme} onChange={onThemeChange} />
      <WindowControls />
    </div>
  );
}
