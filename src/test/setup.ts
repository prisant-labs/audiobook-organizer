// Vitest global setup: extends `expect` with Testing Library's DOM matchers
// (toBeInTheDocument, etc.) for every test file, per test-strategy.md.
import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Testing Library's own auto-cleanup only self-registers when it can see a
// global `afterEach` (Jest-style globals); this project imports `afterEach`
// explicitly per test file instead of enabling Vitest's `globals: true`, so
// it never fires on its own. Without this, DOM from one test leaks into the
// next within the same file/describe block. Wire it explicitly here once.
afterEach(() => {
  cleanup();
});
