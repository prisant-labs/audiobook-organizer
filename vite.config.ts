import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri v2 dev convention (docs/internal/architecture.md): a fixed port so
// src-tauri/tauri.conf.json's build.devUrl can point at a stable localhost
// address. Tailwind v4 (CSS-first config, no tailwind.config.js) and the
// shadcn/ui "@/*" alias land in v0.4.0 Phase 1 (design-system.md; D-01
// common-stack posture, versions pinned to match repo-sync-tool).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
});
