//! F-907 cover extraction (v0.4.0 "seeing", D-15/FD-03): read a book's cover
//! art READ-ONLY so the cover-forward home and review surfaces (F-902/F-903) can
//! show real covers, with a designed fallback tile when no art exists.
//!
//! # The read-only invariant (D-09, the safety-critical guarantee of this phase)
//!
//! Nothing in this module ever opens a file write-mode, and nothing under the
//! library root is ever created, modified, renamed, or deleted. Cover reading
//! touches the library in exactly two ways, both read-only:
//!
//!   1. Embedded art: `lofty`'s `Probe::open(path).read()` opens the audio file
//!      READ-ONLY and parses its tag; we pull the first embedded picture's bytes.
//!      No `save`/`write` API on the returned file is ever called (the same
//!      discipline the FD-14 probe in `crate::probe` already follows).
//!   2. Sidecar: `std::fs::read` of a sibling `cover.jpg` / `folder.jpg`, which
//!      opens read-only.
//!
//! The ONLY writes this module performs are to the cover CACHE directory
//! (`app_data/covers`, passed in by the shell), which is the app's own data
//! area, never the user's library. The read-only-over-the-library invariant is
//! proven by `tests/cover_extraction.rs`, which snapshots every path, length, and
//! mtime under a temp library before and after extraction and asserts they are
//! byte-for-byte unchanged.
//!
//! # Serving to the WebView without filesystem capability (FD-29)
//!
//! The bytes are returned to the shell as a [`crate::ipc::CoverImage`] (mime plus
//! base64), which crosses the typed IPC boundary as data. The WebView is NEVER
//! handed a filesystem path, an asset-protocol scope, or the library root: it
//! only ever receives already-read image bytes over the `cover_get` command. The
//! library root therefore never becomes WebView-readable.
//!
//! # Decoding is deferred (OQ-2 / plan T-11)
//!
//! The plan's chosen approach returns the raw picture/sidecar bytes and their
//! mime; it does NOT decode or downscale (no image crate is pulled). The cache
//! stores those raw bytes. `cover_get` is called lazily per visible tile, not
//! inline with the scan, so it never pushes the read-only scan past its gate.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::error::AppError;
use crate::ipc::CoverImage;

/// How many audio files under one book to try for embedded art before giving up
/// and falling back to a sidecar. Audiobooks usually tag art on every file, so
/// the first hit is almost always the first file; the cap bounds the cost when
/// early files happen to be untagged (OQ-2 keeps per-tile work small).
const EMBEDDED_ART_TRY_CAP: usize = 4;

/// How many audio files to gather from under a book folder when resolving the
/// cover source. Bounds the DB walk and the number of files considered.
const AUDIO_GATHER_CAP: usize = 8;

/// Maximum directory depth to descend below a book folder when gathering audio
/// files (covers the common `Disc 1/` / `Disc 2/` layout without an unbounded
/// walk).
const AUDIO_GATHER_DEPTH: usize = 3;

/// Raw cover bytes plus their sniffed mime type. Internal (not the IPC shape);
/// [`to_cover_image`](CoverArt::to_cover_image) turns it into the wire form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverArt {
    /// The raw image bytes, exactly as read (undecoded; plan T-11).
    pub bytes: Vec<u8>,
    /// The sniffed mime type, e.g. `image/jpeg`.
    pub mime: &'static str,
    /// The canonical file extension for `mime` (no dot), used as the cache
    /// filename suffix so the mime is recoverable on a cache hit.
    pub ext: &'static str,
}

impl CoverArt {
    /// The IPC wire form: mime plus base64 of the bytes, which the frontend turns
    /// into a `data:` URL. base64 keeps the payload compact versus a raw byte
    /// array and needs no WebView filesystem access (FD-29).
    pub fn to_cover_image(&self) -> CoverImage {
        CoverImage {
            mime: self.mime.to_string(),
            base64: base64_encode(&self.bytes),
        }
    }
}

/// The known image types, as `(magic-prefix, mime, extension)`. Mime is sniffed
/// from the bytes rather than trusted from lofty's reported type or a sidecar's
/// filename, so a mislabeled or corrupt payload is rejected (falls back to the
/// deterministic tile, AC-23) instead of being served with a wrong mime.
///
/// WebP is matched specially (RIFF....WEBP) in [`sniff_mime`]; the others are a
/// plain leading-bytes match.
const IMAGE_MAGICS: &[(&[u8], &'static str, &'static str)] = &[
    (&[0xFF, 0xD8, 0xFF], "image/jpeg", "jpg"),
    (&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A], "image/png", "png"),
    (&[b'G', b'I', b'F', b'8'], "image/gif", "gif"),
    (&[b'B', b'M'], "image/bmp", "bmp"),
];

/// The extensions [`find_cached`] probes for on a cache lookup, paired with the
/// mime they map back to. Kept in step with [`IMAGE_MAGICS`].
const CACHE_EXTS: &[(&str, &'static str)] = &[
    ("jpg", "image/jpeg"),
    ("png", "image/png"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
    ("webp", "image/webp"),
];

/// Identify an image's mime + canonical extension from its leading bytes, or
/// `None` if the bytes are not a recognized image (corrupt/unreadable art ->
/// fallback tile, AC-23).
fn sniff_mime(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    // WebP: "RIFF" <4 bytes size> "WEBP".
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(("image/webp", "webp"));
    }
    for (magic, mime, ext) in IMAGE_MAGICS {
        if bytes.len() >= magic.len() && &bytes[..magic.len()] == *magic {
            return Some((mime, ext));
        }
    }
    None
}

/// Build a [`CoverArt`] from raw bytes, sniffing the mime. Returns `None` for an
/// empty or unrecognized payload.
fn cover_from_bytes(bytes: Vec<u8>) -> Option<CoverArt> {
    let (mime, ext) = sniff_mime(&bytes)?;
    Some(CoverArt { bytes, mime, ext })
}

/// Read the first embedded picture frame of `path`, READ-ONLY, or `None` if the
/// file cannot be parsed, carries no tag, carries no picture, or the picture is
/// not a recognized image.
///
/// Properties are NOT read (`read_properties(false)`): the cover reader needs
/// only the tag's pictures, so it never depends on the audio stream being intact
/// and never parses more of the (multi-hundred-MB) file than the tag header. A
/// file lofty cannot open is a first-class `None`, never an error (an audiobook
/// library legitimately contains untagged or odd files; the shelf must degrade,
/// not break).
fn read_embedded_art(path: &Path) -> Option<CoverArt> {
    use lofty::config::ParseOptions;
    use lofty::file::TaggedFileExt;
    use lofty::probe::Probe;

    // `Probe::open(...).read()` opens the file read-only; no save/write API is
    // ever called on the returned `TaggedFile`.
    let tagged = Probe::open(path)
        .ok()?
        .options(ParseOptions::new().read_properties(false))
        .read()
        .ok()?;

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let picture = tag.pictures().first()?;
    cover_from_bytes(picture.data().to_vec())
}

/// The sidecar filenames checked in a book folder, in priority order. Matched
/// case-insensitively against the real directory entries so `Cover.JPG` /
/// `Folder.jpg` are found. Read-only (`std::fs::read`).
const SIDECAR_NAMES: &[&str] = &["cover.jpg", "cover.png", "folder.jpg", "folder.png"];

/// Read a sibling `cover.jpg` / `cover.png` / `folder.jpg` / `folder.png` from
/// `dir`, READ-ONLY, or `None` if none is present or readable as an image.
///
/// The directory is listed once and matched case-insensitively, so this makes at
/// most one `read_dir` plus one `read` and never writes.
fn read_sidecar(dir: &Path) -> Option<CoverArt> {
    let read_dir = std::fs::read_dir(dir).ok()?;
    // Collect the directory's file names once, lower-cased, so the priority
    // order in SIDECAR_NAMES is honored regardless of on-disk casing.
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    for entry in read_dir.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            candidates.push((name.to_ascii_lowercase(), entry.path()));
        }
    }
    for wanted in SIDECAR_NAMES {
        if let Some((_, path)) = candidates.iter().find(|(lower, _)| lower == wanted) {
            if let Ok(bytes) = std::fs::read(path) {
                if let Some(art) = cover_from_bytes(bytes) {
                    return Some(art);
                }
            }
        }
    }
    None
}

/// Read a book's cover, READ-ONLY: try embedded art on the first
/// [`EMBEDDED_ART_TRY_CAP`] audio files (in the given order), then fall back to a
/// `cover.jpg` / `folder.jpg` sidecar in `book_dir`. `None` when neither exists
/// (the caller renders the deterministic fallback tile).
///
/// This is the pure extraction seam (no database, no cache): the resolver and
/// cache layer in [`get_cover`] call it. Kept pure so the read-only assertion
/// test can drive it directly over a temp library.
pub fn read_cover(book_dir: &Path, audio_files: &[PathBuf]) -> Option<CoverArt> {
    for audio in audio_files.iter().take(EMBEDDED_ART_TRY_CAP) {
        if let Some(art) = read_embedded_art(audio) {
            return Some(art);
        }
    }
    read_sidecar(book_dir)
}

// ---- Cover cache (app_data/covers) ----
//
// The cache lives in the app's own data area, NEVER under the library root, so
// caching never violates the read-only-over-the-library invariant. A cache file
// is named `<key>.<ext>` where `key` is a stable, content-ish digest of the
// entry id, its stored path, and its mtime (plan T-11: "entry id + mtime"), so
// the same book yields the same file and a changed mtime yields a fresh key. The
// extension lets the mime be recovered on a hit without a sidecar record.

/// A stable, content-ish cache key from the entry id, its stored path, and its
/// mtime. FNV-1a over the joined fields, rendered hex - deterministic, no crate,
/// and collision-safe enough for a per-entry cache. A `None` mtime contributes
/// an empty field, so an unstattable entry still gets a stable key.
fn cache_key(entry_id: i64, stored_path: &str, mtime: Option<&str>) -> String {
    let material = format!("{entry_id}|{stored_path}|{}", mtime.unwrap_or(""));
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV offset basis
    for b in material.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3); // FNV prime
    }
    format!("{hash:016x}")
}

/// Look for a cached cover for `key` under `cache_dir`, probing each known image
/// extension. Returns the cached [`CoverArt`] on a hit (mime recovered from the
/// extension), or `None` if nothing is cached.
fn find_cached(cache_dir: &Path, key: &str) -> Option<CoverArt> {
    for (ext, mime) in CACHE_EXTS {
        let path = cache_dir.join(format!("{key}.{ext}"));
        if let Ok(bytes) = std::fs::read(&path) {
            if !bytes.is_empty() {
                return Some(CoverArt {
                    bytes,
                    mime,
                    ext,
                });
            }
        }
    }
    None
}

/// Write `art` to the cover cache as `<key>.<ext>`. Best-effort: a cache-write
/// failure (a full or unwritable app-data disk) is logged and swallowed, since
/// the caller already has the bytes to return and the next call simply re-reads
/// the source. Never touches the library.
fn write_cache(cache_dir: &Path, key: &str, art: &CoverArt) {
    if let Err(e) = std::fs::create_dir_all(cache_dir) {
        tracing::warn!(error = %e, "cover: could not create the cover cache directory");
        return;
    }
    let path = cache_dir.join(format!("{}.{}", key, art.ext));
    if let Err(e) = std::fs::write(&path, &art.bytes) {
        tracing::warn!(error = %e, "cover: could not write the cover cache file");
    }
}

// ---- Entry -> cover-source resolution (reads the snapshot, not the frontend) ----

/// One entry's fields needed to resolve its cover source.
#[derive(sqlx::FromRow)]
struct CoverEntry {
    path: String,
    kind: String,
    file_class: Option<String>,
    mtime: Option<String>,
}

/// Resolve, read (cache-aware), and return a book's cover as the IPC
/// [`CoverImage`], or `None` when the book has no cover (the frontend then
/// renders the deterministic fallback tile).
///
/// `entry_id` identifies a book within snapshot `scan_id`: either a book FOLDER
/// (kind `dir`) or a loose audio FILE (kind `file`). The source paths are read
/// from the snapshot's own `entries` rows - the frontend never supplies a path,
/// so the WebView can never point cover reading at an unsanctioned location
/// (FD-29). Reading is strictly read-only over the library; the only writes go to
/// `cache_dir` (`app_data/covers`).
///
/// Errors: only a genuine database read failure surfaces as
/// [`AppError::ScanFailed`]; a missing entry, unreadable art, or absent sidecar
/// all resolve to `Ok(None)` so a cover problem degrades the one tile to the
/// fallback rather than breaking the shelf (design-system 4.5).
pub async fn get_cover(
    pool: &SqlitePool,
    scan_id: i64,
    entry_id: i64,
    cache_dir: &Path,
) -> Result<Option<CoverImage>, AppError> {
    // ---- Look up the target entry within this snapshot ----
    let entry: Option<CoverEntry> = sqlx::query_as::<_, CoverEntry>(
        "SELECT path, kind, file_class, mtime FROM entries WHERE scan_id = ? AND id = ?",
    )
    .bind(scan_id)
    .bind(entry_id)
    .fetch_optional(pool)
    .await
    .map_err(cover_db_failed)?;

    let Some(entry) = entry else {
        // Unknown entry id (or wrong snapshot): nothing to show, not an error.
        return Ok(None);
    };

    // ---- Cache first, keyed by (entry id, stored path, mtime) ----
    let key = cache_key(entry_id, &entry.path, entry.mtime.as_deref());
    if let Some(art) = find_cached(cache_dir, &key) {
        return Ok(Some(art.to_cover_image()));
    }

    // ---- Resolve the book directory and its audio files from the snapshot ----
    let (book_dir, audio_files) = match entry.kind.as_str() {
        "dir" => {
            let dir = PathBuf::from(&entry.path);
            let audio = gather_audio_under(pool, scan_id, entry_id).await?;
            (dir, audio)
        }
        _ => {
            // A loose file (typically a loose audio book). Its folder is the
            // parent; the file itself is the sole embedded-art candidate.
            let file_path = PathBuf::from(&entry.path);
            let dir = file_path
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| file_path.clone());
            let audio = if entry.file_class.as_deref() == Some("audio") {
                vec![file_path]
            } else {
                Vec::new()
            };
            (dir, audio)
        }
    };

    // ---- Read (read-only), cache, and return ----
    match read_cover(&book_dir, &audio_files) {
        Some(art) => {
            write_cache(cache_dir, &key, &art);
            Ok(Some(art.to_cover_image()))
        }
        None => Ok(None),
    }
}

/// Gather up to [`AUDIO_GATHER_CAP`] audio files under the book folder
/// `folder_id`, descending at most [`AUDIO_GATHER_DEPTH`] levels (to cover
/// `Disc 1/` layouts) via the indexed `parent_id` edge. Deterministic
/// path-sorted order at each level, so the "first audio file" is stable across
/// runs and hosts.
async fn gather_audio_under(
    pool: &SqlitePool,
    scan_id: i64,
    folder_id: i64,
) -> Result<Vec<PathBuf>, AppError> {
    let mut audio: Vec<PathBuf> = Vec::new();
    // BFS frontier of directory ids to expand, with the depth reached so far.
    let mut frontier: Vec<i64> = vec![folder_id];
    let mut depth = 0usize;

    while !frontier.is_empty() && depth < AUDIO_GATHER_DEPTH && audio.len() < AUDIO_GATHER_CAP {
        let mut next: Vec<i64> = Vec::new();
        for parent_id in frontier {
            // Direct children, path-sorted; collect audio files and queue dirs.
            let rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
                "SELECT id, kind, path, file_class FROM entries \
                 WHERE scan_id = ? AND parent_id = ? ORDER BY path",
            )
            .bind(scan_id)
            .bind(parent_id)
            .fetch_all(pool)
            .await
            .map_err(cover_db_failed)?;

            for (id, kind, path, file_class) in rows {
                if kind == "dir" {
                    next.push(id);
                } else if file_class.as_deref() == Some("audio") {
                    audio.push(PathBuf::from(path));
                    if audio.len() >= AUDIO_GATHER_CAP {
                        return Ok(audio);
                    }
                }
            }
        }
        frontier = next;
        depth += 1;
    }
    Ok(audio)
}

/// Map a SQLite error on the cover read path to the Scan-family failure code.
/// Cover reading itself never hard-errors; only a database read failure does.
fn cover_db_failed(e: sqlx::Error) -> AppError {
    AppError::ScanFailed {
        detail: format!("could not read cover source from the snapshot: {e}"),
    }
}

// ---- base64 (pure, no dependency) ----

/// Standard base64 alphabet (RFC 4648).
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `bytes` as standard base64 (with `=` padding). Pure and
/// dependency-free: the cover payload crosses IPC as a base64 string the frontend
/// wraps in a `data:` URL, so no image crate and no WebView filesystem access is
/// involved (FD-29).
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four JPEG magic bytes are enough for the sniffer; a real embedded
    /// frame carries the full stream, but the reader only inspects the prefix.
    const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0];
    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn sniff_recognizes_the_image_families() {
        assert_eq!(sniff_mime(JPEG_MAGIC), Some(("image/jpeg", "jpg")));
        assert_eq!(sniff_mime(PNG_MAGIC), Some(("image/png", "png")));
        assert_eq!(sniff_mime(b"GIF89a...."), Some(("image/gif", "gif")));
        assert_eq!(sniff_mime(b"BM....."), Some(("image/bmp", "bmp")));
        let webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
        assert_eq!(sniff_mime(webp), Some(("image/webp", "webp")));
    }

    #[test]
    fn sniff_rejects_non_images() {
        assert_eq!(sniff_mime(b"not an image"), None);
        assert_eq!(sniff_mime(b""), None);
        // Truncated magic must not panic or false-match.
        assert_eq!(sniff_mime(&[0xFF]), None);
    }

    #[test]
    fn base64_matches_known_vectors() {
        // RFC 4648 test vectors.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn cache_key_is_stable_and_mtime_sensitive() {
        let a = cache_key(7, r"E:\Books\A Book", Some("2026-01-01T00:00:00Z"));
        let a_again = cache_key(7, r"E:\Books\A Book", Some("2026-01-01T00:00:00Z"));
        assert_eq!(a, a_again, "same inputs must yield the same key");

        // A changed mtime yields a fresh key (content-ish invalidation).
        let b = cache_key(7, r"E:\Books\A Book", Some("2026-02-02T00:00:00Z"));
        assert_ne!(a, b, "a changed mtime must change the key");

        // A different entry yields a different key.
        let c = cache_key(8, r"E:\Books\A Book", Some("2026-01-01T00:00:00Z"));
        assert_ne!(a, c, "a different entry id must change the key");

        // 16 hex chars (u64), always.
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn read_sidecar_prefers_cover_over_folder_and_reads_only() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        // A JPEG cover.jpg and a PNG folder.png; cover.* wins by priority.
        std::fs::write(dir.path().join("cover.jpg"), JPEG_MAGIC).unwrap();
        std::fs::write(dir.path().join("folder.png"), PNG_MAGIC).unwrap();

        let art = read_sidecar(dir.path()).expect("a sidecar is found");
        assert_eq!(art.mime, "image/jpeg", "cover.jpg takes priority");
        assert_eq!(art.bytes, JPEG_MAGIC);
    }

    #[test]
    fn read_sidecar_is_case_insensitive() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("Folder.JPG"), JPEG_MAGIC).unwrap();
        let art = read_sidecar(dir.path()).expect("case-insensitive match");
        assert_eq!(art.mime, "image/jpeg");
    }

    #[test]
    fn read_sidecar_ignores_a_non_image_sidecar() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("cover.jpg"), b"this is not a jpeg").unwrap();
        assert_eq!(read_sidecar(dir.path()), None, "a corrupt sidecar is no cover");
    }

    #[test]
    fn find_and_write_cache_round_trip() {
        let dir = tempfile::TempDir::new().expect("cache tempdir");
        let cache = dir.path().join("covers");
        let art = CoverArt {
            bytes: JPEG_MAGIC.to_vec(),
            mime: "image/jpeg",
            ext: "jpg",
        };
        assert_eq!(find_cached(&cache, "deadbeef"), None, "empty cache misses");

        write_cache(&cache, "deadbeef", &art);
        let hit = find_cached(&cache, "deadbeef").expect("cache hit after write");
        assert_eq!(hit, art, "the cached art round-trips with its mime");
    }
}
