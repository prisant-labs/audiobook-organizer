import type { AppError } from "./bindings";

// AppError crosses IPC as an externally-tagged enum: exactly one machine-code
// key is present (e.g. `{ "settings-failed": { detail } }`), or it is a bare
// string for unit variants (e.g. "nothing-approved"). This renders it as
// "code: detail" (or just the code when there is no readable payload field),
// instead of leaking the raw object shape into the UI. Phase 7 (F-908) maps
// codes to full family-safe surfaces; this is the plain fallback formatter the
// earlier phases share.
export function formatAppError(error: AppError): string {
  if (typeof error === "string") return error;
  const record = error as Record<
    string,
    { detail?: string; path?: string; backup_path?: string } | undefined
  >;
  const code = Object.keys(record).find((key) => record[key] !== undefined);
  if (!code) return "unknown-error";
  const payload = record[code];
  const detail = payload?.detail ?? payload?.path ?? payload?.backup_path;
  return detail ? `${code}: ${detail}` : code;
}
