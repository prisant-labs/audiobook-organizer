// A calm loading skeleton for the library home (design-system Section 5.3
// loading states; T-15/T-16). Shown only for the brief window before the
// FIRST `classify_overview` read resolves - not a stand-in for "no scan yet"
// (that is the honest empty state, a distinct condition). Respects
// `prefers-reduced-motion` via the global collapse rule in tokens.css (the
// `animate-pulse` transition duration is zeroed there, not re-implemented
// here).
export function LibrarySkeleton() {
  return (
    <div aria-hidden="true" className="max-w-[1060px] animate-pulse">
      <div className="h-[30px] w-[220px] rounded bg-surface-2" />
      <div className="mt-3 h-[14px] w-[70%] rounded bg-surface-2" />
      <div className="mt-2 h-[14px] w-[50%] rounded bg-surface-2" />
      <div className="mt-9 h-[19px] w-[180px] rounded bg-surface-2" />
      <div className="mt-4 flex gap-[22px]">
        {Array.from({ length: 6 }, (_, i) => (
          <div key={i} className="h-[112px] w-[112px] flex-none rounded-[6px] bg-surface-2" />
        ))}
      </div>
    </div>
  );
}
