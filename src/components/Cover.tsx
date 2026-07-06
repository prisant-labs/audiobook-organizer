import { useState } from "react";
import { cn } from "@/lib/utils";
import { type CoverImage, coverDataUrl } from "@/lib/covers";
import { FallbackTile } from "./FallbackTile";

export interface CoverProps {
  // The book title, used for the fallback tile and the image alt text.
  title: string;
  // The author, shown on the fallback tile and in the alt text.
  author?: string | null;
  // The extracted cover art (F-907), or `null`/`undefined` when the book has no
  // cover, in which case the deterministic fallback tile renders (AC-23).
  image?: CoverImage | null;
  className?: string;
}

// One book's cover (F-907, design-system Section 4.4/4.5). Renders the real cover
// art at a square 1:1 aspect ratio (AC-22) when present, and the deterministic
// fallback tile when there is no cover OR the bytes fail to decode (the `onError`
// path, T-13/AC-23). The image bytes arrive as a `data:` URL over typed IPC; the
// WebView never touches the filesystem (FD-29).
export function Cover({ title, author, image, className }: CoverProps) {
  const [decodeFailed, setDecodeFailed] = useState(false);

  if (!image || decodeFailed) {
    return <FallbackTile title={title} author={author} className={className} />;
  }

  return (
    <img
      src={coverDataUrl(image)}
      alt={author ? `${title} by ${author}` : title}
      // aspect-square = 1:1 (AC-22); object-cover keeps non-square source art
      // from distorting within the square frame.
      className={cn("aspect-square w-full rounded-[5px] object-cover", className)}
      style={{ boxShadow: "var(--cover-shadow)" }}
      onError={() => setDecodeFailed(true)}
    />
  );
}
