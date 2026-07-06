import { open } from "@tauri-apps/plugin-dialog";

// Backend-mediated OS folder picker (F-909, FD-29). The one sanctioned
// non-bindings IPC path in the frontend: it is the typed `tauri-plugin-dialog`
// guest binding (NOT a raw `invoke`, which the no-raw-invoke lint forbids), and
// it returns a path STRING only. The frontend performs no filesystem access
// itself - the returned path is handed to the backend via `settings_set`, which
// owns every subsequent filesystem operation (architecture Section 5).
//
// Wrapped here (rather than calling `open` inline) so both first-run and
// Settings share one call site, and so tests mock a single module seam.
//
// Returns the chosen absolute path, or null if the user cancelled. `open` with
// `directory: true` and `multiple: false` returns a single string or null.
export async function pickLibraryFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}
