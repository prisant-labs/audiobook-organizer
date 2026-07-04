// Placeholder shell for the v0.1.0 spine (Phase 1: workspace scaffold).
//
// This entire screen is disposable: Phase 6 replaces it with the
// tracer-slice throwaway JSON-dump UI (scan_start -> job:completed ->
// render entries as pretty-printed JSON), and the real product UI begins at
// v0.4.0 (seeing). No Tauri IPC calls happen yet; the tauri-specta seam
// lands in Phase 5.
function App() {
  return (
    <main>
      <h1>Audiobook Organizer - spine scaffold</h1>
      <p>
        This placeholder screen has no functionality yet. It exists only to
        prove the Vite + React + TypeScript + Tauri v2 build seam compiles
        and boots. The v0.1.0 tracer slice (Phase 6) replaces this with a
        disposable JSON-dump UI; the real product UI begins at v0.4.0.
      </p>
    </main>
  );
}

export default App;
