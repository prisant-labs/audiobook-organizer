import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

// v0.1.0 spine, Phase 5. A lean flat config: the typescript-eslint recommended
// baseline plus the no-raw-invoke rule (FD-29). src-tauri (Rust) and build
// outputs are ignored; the frontend is the only lint surface.
export default tseslint.config(
  {
    ignores: ["dist", "src-tauri", "node_modules"],
  },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    rules: {
      // no-raw-invoke (FD-29 typed-IPC-only): the frontend MUST call the
      // tauri-specta generated bindings in src/lib/bindings.ts, never `invoke`
      // directly. Forbid importing `invoke` from @tauri-apps/api/core (and the
      // umbrella @tauri-apps/api) anywhere; the one sanctioned import lives in
      // bindings.ts, which is exempted below. This is the lint half of the
      // release-plan v0.4.0 "no raw invoke" gate, wired now so it can never
      // regress once the tracer UI (P6) and real UI (v0.4.0) build on it.
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "@tauri-apps/api/core",
              importNames: ["invoke"],
              message:
                "Do not call invoke directly. Use the generated typed bindings in src/lib/bindings.ts (FD-29).",
            },
            {
              name: "@tauri-apps/api",
              importNames: ["invoke"],
              message:
                "Do not call invoke directly. Use the generated typed bindings in src/lib/bindings.ts (FD-29).",
            },
          ],
        },
      ],
    },
  },
  {
    // The generated bindings file is the ONE sanctioned wrapper around `invoke`;
    // it legitimately imports it from @tauri-apps/api/core. Exempt it from the
    // raw-invoke ban explicitly, so the rule holds even if the file's leading
    // eslint-disable header is ever dropped.
    files: ["src/lib/bindings.ts"],
    rules: {
      "no-restricted-imports": "off",
    },
  },
);
