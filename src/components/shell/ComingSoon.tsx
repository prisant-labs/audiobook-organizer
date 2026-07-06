// Placeholder for routes whose real screen has not landed yet (Tidy-up,
// Duplicates, History, Settings arrive in later v0.4.0 phases: F-903/F-905
// review and duplicates, F-8xx history, F-906/F-803 settings). Keeps every
// sidebar destination a real, non-dead route (design-system Section 4.3:
// "Settings is a real destination, never a dead link") without inventing
// screen content ahead of its phase.
export function ComingSoon({ label }: { label: string }) {
  return (
    <div className="max-w-[52ch]">
      <h1 className="font-serif text-[26px] font-medium tracking-[-0.01em]">{label}</h1>
      <p className="mt-2 text-[14px] leading-relaxed text-ink-2">
        This screen has not been built yet. It lands in a later phase of the v0.4.0 release.
      </p>
    </div>
  );
}
