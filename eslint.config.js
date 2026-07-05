import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";

// v0.1.0 spine, Phase 5, extended in v0.4.0 Phase 1 with the react-hooks and
// react-refresh presets (D-01 common-stack posture, matching repo-sync-tool's
// eslint.config.js) now that the tree has real components with hooks. A lean
// flat config: the typescript-eslint recommended baseline plus the
// no-raw-invoke rule (FD-29). src-tauri (Rust) and build outputs are ignored;
// the frontend is the only lint surface.
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
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      ...reactHooks.configs["recommended-latest"].rules,
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
      // no-raw-invoke, dynamic-import half. `no-restricted-imports` only sees
      // STATIC `import ... from` declarations; a dynamic `import("@tauri-apps/api/core")`
      // is an ImportExpression it never inspects, so it would slip the gate and
      // hand back a raw `invoke`. Forbid that expression form too, for both the
      // /core subpath and the @tauri-apps/api umbrella. bindings.ts (the one
      // sanctioned wrapper) is exempted from both rules below.
      "no-restricted-syntax": [
        "error",
        {
          selector: "ImportExpression[source.value='@tauri-apps/api/core']",
          message:
            "Do not dynamically import @tauri-apps/api/core for invoke. Use the generated typed bindings in src/lib/bindings.ts (FD-29).",
        },
        {
          selector: "ImportExpression[source.value='@tauri-apps/api']",
          message:
            "Do not dynamically import @tauri-apps/api for invoke. Use the generated typed bindings in src/lib/bindings.ts (FD-29).",
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
      "no-restricted-syntax": "off",
    },
  },
  reactRefresh.configs.vite,
);
