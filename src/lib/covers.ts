import { commands, type CoverImage } from "./bindings";
import { formatAppError } from "./appError";

// Frontend client for F-907 cover retrieval (v0.4.0 Phase 3). Wraps the generated
// `cover_get` binding. The bytes arrive as a base64 `CoverImage` over typed IPC;
// the frontend never reads the filesystem and never receives a path (FD-29). The
// library root never becomes WebView-readable.

export type { CoverImage };

// Compose the `<img src>` data URL from a CoverImage. The WebView renders
// already-read bytes; there is no filesystem or asset-protocol access involved.
export function coverDataUrl(image: CoverImage): string {
  return `data:${image.mime};base64,${image.base64}`;
}

// Fetch one book's cover for a snapshot entry, or `null` when the book has no
// cover. A cover problem must never break the shelf (design-system 4.5), so a
// backend error degrades to `null` (the caller renders the fallback tile) rather
// than throwing; the formatted code is logged for diagnosis only.
export async function getCover(scanId: number, entryId: number): Promise<CoverImage | null> {
  const result = await commands.coverGet(scanId, entryId);
  if (result.status === "error") {
    console.warn(`cover_get failed: ${formatAppError(result.error)}`);
    return null;
  }
  return result.data;
}
