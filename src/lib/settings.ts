import { commands, type AppSettings } from "./bindings";
import { formatAppError } from "./appError";

// Frontend client for the F-803 singleton settings (T-06). Wraps the generated
// settings_get / settings_set bindings and unwraps their `Result` shape into a
// value-or-throw contract the hooks can consume. All filesystem-adjacent state
// (library root, set-aside root, reports override, theme, retention) lives in
// the backend behind this typed IPC; the frontend never reads or writes the
// filesystem or the settings row directly (FD-29, AC-30).

export type { AppSettings };

// A settings IPC failure surfaced as a throwable Error carrying the formatted
// AppError message, so callers can show a plain-language line and (Phase 7)
// map the code to a family-safe surface.
export class SettingsError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SettingsError";
  }
}

// Read the persisted settings (AC-34). The shell calls this once at startup to
// route first-run (a null `library_root`) and to seed the theme (FD-09).
export async function getSettings(): Promise<AppSettings> {
  const result = await commands.settingsGet();
  if (result.status === "error") {
    throw new SettingsError(formatAppError(result.error));
  }
  return result.data;
}

// Persist the whole settings row and return the normalized stored form (AC-34).
export async function saveSettings(next: AppSettings): Promise<AppSettings> {
  const result = await commands.settingsSet(next);
  if (result.status === "error") {
    throw new SettingsError(formatAppError(result.error));
  }
  return result.data;
}
