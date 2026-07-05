import { useCallback, useEffect, useState } from "react";
import { applyTheme, getStoredTheme, persistTheme, type Theme } from "@/lib/theme";

// React binding over src/lib/theme.ts. Reapplies the `data-theme` attribute
// whenever the theme changes (main.tsx already applies it once before the
// first render to avoid a flash; this keeps it in sync after that).
export function useTheme(): { theme: Theme; setTheme: (theme: Theme) => void } {
  const [theme, setThemeState] = useState<Theme>(() => getStoredTheme());

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const setTheme = useCallback((next: Theme) => {
    persistTheme(next);
    setThemeState(next);
  }, []);

  return { theme, setTheme };
}
