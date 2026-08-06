// Byte-total formatting for the library home's prose sentences (FD-08: every
// GB figure states which quantity it refers to; this only formats the NUMBER,
// callers supply the "of what" words). Binary (1024-based) units, matching the
// core's own `plan::report` byte formatter.
const GB = 1024 ** 3;
const MB = 1024 ** 2;
const KB = 1024;

// Whole GB for a library-scale total (design-system 6.2 example: "about 297
// GB"); one decimal below 100 GB, where the extra precision reads as
// informative rather than noisy (e.g. "10.1 GB of duplicate copies"); KB for
// the small totals a handful of short duplicate files can produce (a real
// library's duplicate bytes are usually GB-scale, but the unit ladder stays
// honest and readable at every scale a fixture or a tiny library can hit).
export function formatBytes(bytes: number): string {
  if (bytes >= GB) {
    const gb = bytes / GB;
    return `${gb.toFixed(gb >= 100 ? 0 : 1)} GB`;
  }
  if (bytes >= MB) {
    return `${(bytes / MB).toFixed(0)} MB`;
  }
  if (bytes >= KB) {
    return `${(bytes / KB).toFixed(0)} KB`;
  }
  return `${bytes.toLocaleString()} bytes`;
}

// A past timestamp as a family-readable date. Shared by History (the record of
// past tidy-ups) and the interruption recovery surface's details disclosure, so
// the same run reads the same way on both screens.
//
// Deliberately not a relative time ("2 days ago"): the record is a durable log,
// and an absolute date stays true when re-read.
//
// Returns "" rather than "Invalid Date" for an unparseable value: a bad or
// missing timestamp should degrade to a blank line, never shout at a reader who
// opened a disclosure expecting reassurance.
export function formatWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
