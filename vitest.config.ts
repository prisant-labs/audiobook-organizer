import { defineConfig, mergeConfig } from "vite";
import viteConfig from "./vite.config";

// Separate from vite.config.ts (rather than a `test` block bolted onto it) so
// the app's own Vite config stays free of a test-only dependency graph; this
// file merges it in for Vitest's benefit only. jsdom environment + Testing
// Library matchers per test-strategy.md's Frontend layer (Vitest component
// tests), landing with this phase (v0.4.0 Phase 1).
export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      setupFiles: ["./src/test/setup.ts"],
      css: false,
    },
  }),
);
