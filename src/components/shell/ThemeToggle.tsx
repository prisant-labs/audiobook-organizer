import { cn } from "@/lib/utils";
import type { Theme } from "@/lib/theme";

export interface ThemeToggleProps {
  theme: Theme;
  onChange: (theme: Theme) => void;
}

// The theme segmented control (design-system Section 4.2, 7). Pill of two
// buttons; the active one carries `aria-pressed="true"` (AC-2, the canonical
// theme toggle). `role="group"` plus an accessible label ties the pair
// together for screen readers, matching prototype 04/05/07's `.seg`.
export function ThemeToggle({ theme, onChange }: ThemeToggleProps) {
  return (
    <div
      role="group"
      aria-label="Theme"
      className="flex overflow-hidden rounded-full border border-border-2"
    >
      <ThemeButton label="Day" pressed={theme === "day"} onClick={() => onChange("day")} />
      <ThemeButton
        label="Evening"
        pressed={theme === "evening"}
        onClick={() => onChange("evening")}
      />
    </div>
  );
}

function ThemeButton({
  label,
  pressed,
  onClick,
}: {
  label: string;
  pressed: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={cn(
        "px-3 py-[3px] font-sans text-[11.5px] text-ink-3 transition-colors",
        pressed && "bg-surface-2 font-semibold text-ink",
      )}
    >
      {label}
    </button>
  );
}
