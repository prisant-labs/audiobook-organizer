import type { ReactNode } from "react";

// The scrollable content region every screen renders inside (design-system
// Section 4, the prototypes' `.main`). This is the single `<main>` landmark
// for the whole app; individual screens render plain content, not their own
// `<main>`.
export function ScreenContainer({ children }: { children: ReactNode }) {
  return <main className="min-w-0 flex-1 overflow-y-auto px-11 pb-10 pt-8">{children}</main>;
}
