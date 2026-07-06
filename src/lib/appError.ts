import type { AppError } from "./bindings";

// AppError crosses IPC as an externally-tagged enum: exactly one machine-code
// key is present (e.g. `{ "settings-failed": { detail } }`), or it is a bare
// string for unit variants (e.g. "nothing-approved"). This renders it as
// "code: detail" (or just the code when there is no readable payload field),
// instead of leaking the raw object shape into the UI. Phase 7 (F-908) maps
// codes to full family-safe surfaces (see `errorCopy.ts`); this is the plain
// technical formatter those surfaces tuck inside the "Show file details"
// disclosure, never the family-facing sentence.
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

// ---------------------------------------------------------------------------
// Structured error code (v0.4.0 Phase 7, F-908)
// ---------------------------------------------------------------------------
//
// `AppErrorCode` is the union of every stable kebab-case machine code the
// `AppError` taxonomy can carry, DERIVED structurally from the generated
// `AppError` type rather than hand-listed. This is what makes the F-908 copy
// table exhaustive by construction: `errorCopy.ts` keys a
// `Record<AppErrorCode, ...>`, so adding a new variant to the Rust taxonomy
// (and regenerating `bindings.ts`) grows this union and forces new copy at
// compile time - a future code can never ship without a family-safe surface.
//
// The derivation: `AppError` is a distributive union whose members are either
//   - a bare string (unit variants, e.g. "nothing-approved"), or
//   - an intersection `{ "the-code": <payload> } & { "other"?: never; ... }`.
// For an object member, the ACTIVE key is the one whose value is not `never`
// (every OTHER code is present as an optional `never` guard). `ActiveKey`
// extracts exactly that key; string members pass through unchanged.
type ActiveKey<T> = {
  [K in keyof T]-?: [T[K]] extends [never] ? never : K;
}[keyof T];

export type AppErrorCode<T = AppError> = T extends string ? T : ActiveKey<T>;

// Extract the machine code from a structured `AppError` value at runtime. Real
// `AppError`s always carry exactly one code, so the cast is sound; the `??`
// fallback only guards a malformed shape (never produced by the backend) and
// keeps this total for `copyForCode`'s generic fallback.
export function appErrorCode(error: AppError): AppErrorCode {
  if (typeof error === "string") return error as AppErrorCode;
  const record = error as Record<string, unknown>;
  const code = Object.keys(record).find((key) => record[key] !== undefined);
  return (code ?? "unknown-error") as AppErrorCode;
}

// A throwable that carries the STRUCTURED `AppError` (and its extracted code)
// across the client -> hook -> surface boundary, so an F-908 surface can map
// the code to family-safe copy (`errorCopy.ts`) rather than only showing the
// formatted string. `.message` stays the technical `formatAppError` text so
// existing string-only consumers (and the "Show file details" disclosure)
// keep working unchanged.
export class AppErrorException extends Error {
  readonly appError: AppError;
  readonly code: AppErrorCode;

  constructor(appError: AppError) {
    super(formatAppError(appError));
    this.name = "AppErrorException";
    this.appError = appError;
    this.code = appErrorCode(appError);
  }
}

// The structured code behind an unknown thrown value, or `null` when it is a
// plain `Error` (or anything else) that never carried an `AppError`. Hooks use
// this to key their error surface by code when one is available, and fall back
// to the generic surface (message-only) otherwise.
export function codeOf(error: unknown): AppErrorCode | null {
  return error instanceof AppErrorException ? error.code : null;
}
