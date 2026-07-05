import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

// The standard shadcn/ui class-merging helper (D-01 common-stack posture).
// Not exercised by any Phase 1 component yet (they compose plain token-driven
// classes), but seeded now so later phases that pull in shadcn primitives via
// the `shadcn` CLI (components.json) have it in place.
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
