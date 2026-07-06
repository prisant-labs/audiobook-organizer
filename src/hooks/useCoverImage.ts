import { useEffect, useRef, useState } from "react";
import { getCover, type CoverImage } from "@/lib/covers";

// One book slot's cover fetch (F-907, T-16, v0.4.0 Phase 4). Each shelf card
// fetches its own cover independently, keyed on `(scanId, entryId)`; a cover
// read failure already degrades to `null` inside `getCover` (never breaks the
// shelf, design-system Section 4.5), so this hook only tracks loading vs
// resolved, never a separate error state.
//
// Callers render one `BookSlot` per book keyed by `entry_id` (`Library.tsx`'s
// `.map`), so a given hook instance's `(scanId, entryId)` is fixed for its
// whole lifetime in practice - a changing key remounts the component instead
// of updating props. The effect below still re-fetches if the identity DOES
// change, but deliberately does not reset to the loading placeholder first
// (that would be a synchronous `setState` in the effect body, which the
// react-hooks/set-state-in-effect rule flags): the previous cover simply stays
// on screen until the new one resolves, which is the same graceful-degrade
// posture `Cover`'s own `onError` fallback already uses.
export function useCoverImage(scanId: number, entryId: number): CoverImage | null | undefined {
  // `undefined` = still loading; `null` = resolved, no cover (fallback tile).
  const [image, setImage] = useState<CoverImage | null | undefined>(undefined);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void getCover(scanId, entryId).then((result) => {
      if (!cancelled && mountedRef.current) setImage(result);
    });
    return () => {
      cancelled = true;
    };
  }, [scanId, entryId]);

  return image;
}
