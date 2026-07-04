import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri v2 dev convention (docs/internal/architecture.md): a fixed port so
// src-tauri/tauri.conf.json's build.devUrl can point at a stable localhost
// address. v0.1.0 spine, Phase 1 scaffold only; no Tailwind, no shadcn/ui
// yet (deferred to v0.4.0 per this phase's brief).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
