//! F-907 cover extraction golden + safety tests (v0.4.0 "seeing", Phase 3).
//!
//! Covers the plan's Phase 3 verification bullets:
//!   - extraction golden: a book whose audio carries embedded art returns those
//!     exact bytes with the sniffed mime (AC-21);
//!   - sidecar fallback: a book with no embedded art but a `cover.jpg` returns
//!     the sidecar bytes (AC-21);
//!   - neither: a book with no art and no sidecar returns `None` (the frontend
//!     then renders the deterministic fallback tile, AC-23);
//!   - the READ-ONLY proof (the safety-critical guarantee, D-09): a full
//!     snapshot of every path, length, and mtime under the library is byte-for-
//!     byte identical before and after extraction, and the only writes land in
//!     the separate cover cache directory (AC-21 "no file is written or modified
//!     by cover extraction");
//!   - cache hit: a second call is served from the cache even after the source
//!     files are deleted.
//!
//! The embedded-art fixture is a minimal MP3 built in-test: an ID3v2.3 tag with a
//! single APIC (attached picture) frame and no audio frame. The cover reader uses
//! `read_properties(false)`, so it parses the tag's picture without needing a
//! valid audio stream. Building the fixture in-test keeps it byte-stable and
//! avoids committing a binary blob.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use abo_core::db::open_db;
use abo_core::scan::{get_cover, get_scan_entries, read_cover, run_scan};
use tempfile::TempDir;

/// A recognizable, self-contained JPEG payload: the JPEG SOI + APP0 magic the
/// mime sniffer keys on, some filler, and the EOI marker. The reader returns
/// these bytes verbatim, so the golden asserts on them exactly.
const JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x2A, 0x2A, 0xFF, 0xD9,
];

/// A distinct PNG payload for the sidecar tests (so a test can tell a sidecar hit
/// from an embedded hit by mime).
const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
];

/// Encode `n` as a 4-byte ID3v2 syncsafe integer (7 bits per byte).
fn syncsafe(n: u32) -> [u8; 4] {
    [
        ((n >> 21) & 0x7F) as u8,
        ((n >> 14) & 0x7F) as u8,
        ((n >> 7) & 0x7F) as u8,
        (n & 0x7F) as u8,
    ]
}

/// Build a minimal MP3: an ID3v2.3 tag with one APIC front-cover frame carrying
/// `jpeg`, and no audio frame. lofty identifies it as MPEG from the `.mp3`
/// extension and reads the picture out of the tag.
fn mp3_with_cover(jpeg: &[u8]) -> Vec<u8> {
    // APIC frame body: text-encoding(ISO-8859-1) | "image/jpeg"\0 | pic-type(front
    // cover=0x03) | description\0 | picture bytes.
    let mut body: Vec<u8> = Vec::new();
    body.push(0x00);
    body.extend_from_slice(b"image/jpeg");
    body.push(0x00);
    body.push(0x03);
    body.push(0x00);
    body.extend_from_slice(jpeg);

    // APIC frame: "APIC" | u32-be(body len) | flags(0,0) | body.
    let mut frame: Vec<u8> = Vec::new();
    frame.extend_from_slice(b"APIC");
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&[0x00, 0x00]);
    frame.extend_from_slice(&body);

    // ID3v2.3 header: "ID3" | version(3,0) | flags(0) | syncsafe(tag body len).
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"ID3");
    out.extend_from_slice(&[0x03, 0x00, 0x00]);
    out.extend_from_slice(&syncsafe(frame.len() as u32));
    out.extend_from_slice(&frame);
    out
}

/// A bare MP3 with no ID3 tag at all (no embedded art).
fn mp3_no_tag() -> Vec<u8> {
    b"no id3 tag here, just filler bytes standing in for an untagged file".to_vec()
}

/// Recursively snapshot (relative path -> (len, mtime)) for every FILE under
/// `root`, so a before/after comparison proves nothing under the library was
/// written, modified, or created.
fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, (u64, SystemTime)> {
    let mut acc = BTreeMap::new();
    fn walk(dir: &Path, root: &Path, acc: &mut BTreeMap<PathBuf, (u64, SystemTime)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let md = entry.metadata().unwrap();
            if md.is_dir() {
                walk(&path, root, acc);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                acc.insert(rel, (md.len(), md.modified().unwrap()));
            }
        }
    }
    walk(root, root, &mut acc);
    acc
}

// ---- Pure extraction goldens (read_cover) ----

#[test]
fn embedded_art_returns_the_exact_bytes() {
    let book = TempDir::new().unwrap();
    let audio = book.path().join("chapter1.mp3");
    std::fs::write(&audio, mp3_with_cover(JPEG)).unwrap();

    let art = read_cover(book.path(), &[audio]).expect("embedded art is found");
    assert_eq!(art.mime, "image/jpeg", "mime is sniffed from the picture bytes");
    assert_eq!(art.bytes, JPEG, "the exact embedded picture bytes are returned");
}

#[test]
fn sidecar_is_used_when_no_embedded_art() {
    let book = TempDir::new().unwrap();
    let audio = book.path().join("chapter1.mp3");
    std::fs::write(&audio, mp3_no_tag()).unwrap();
    // A PNG sidecar so the hit is unmistakably the sidecar, not embedded art.
    std::fs::write(book.path().join("cover.jpg"), PNG).unwrap();

    let art = read_cover(book.path(), &[audio]).expect("sidecar is found");
    assert_eq!(art.mime, "image/png", "the sidecar bytes drive the mime");
    assert_eq!(art.bytes, PNG);
}

#[test]
fn no_art_and_no_sidecar_returns_none() {
    let book = TempDir::new().unwrap();
    let audio = book.path().join("chapter1.mp3");
    std::fs::write(&audio, mp3_no_tag()).unwrap();

    assert_eq!(
        read_cover(book.path(), &[audio]),
        None,
        "with neither embedded art nor a sidecar, there is no cover (fallback tile)"
    );
}

#[test]
fn embedded_art_wins_over_a_sidecar() {
    // When both exist, embedded art is preferred (it is the book's own art).
    let book = TempDir::new().unwrap();
    let audio = book.path().join("chapter1.mp3");
    std::fs::write(&audio, mp3_with_cover(JPEG)).unwrap();
    std::fs::write(book.path().join("cover.jpg"), PNG).unwrap();

    let art = read_cover(book.path(), &[audio]).expect("a cover is found");
    assert_eq!(art.bytes, JPEG, "embedded art takes priority over the sidecar");
}

// ---- The read-only proof + cache, over a real snapshot (get_cover) ----

/// Build a temp library with one book folder (embedded-art audio + a sidecar),
/// scan it into a DB, and return everything the get_cover tests need.
async fn scanned_library() -> (TempDir, TempDir, sqlx::SqlitePool, i64, i64) {
    let lib = TempDir::new().unwrap();
    let book = lib.path().join("Some Author - A Book");
    std::fs::create_dir(&book).unwrap();
    std::fs::write(book.join("chapter1.mp3"), mp3_with_cover(JPEG)).unwrap();
    std::fs::write(book.join("cover.jpg"), PNG).unwrap();

    let db = TempDir::new().unwrap();
    let (pool, _) = open_db(db.path()).await.expect("open_db");
    let summary = run_scan(&pool, lib.path()).await.expect("scan");
    let scan_id = summary.scan_id;

    // Find the book-folder entry id from the snapshot.
    let entries = get_scan_entries(&pool, scan_id).await.expect("entries");
    let folder_id = entries
        .iter()
        .find(|e| e.kind == "dir" && e.name == "Some Author - A Book")
        .map(|e| e.id)
        .expect("the book folder is in the snapshot");

    (lib, db, pool, scan_id, folder_id)
}

#[tokio::test]
async fn get_cover_reads_only_and_returns_embedded_art() {
    let (lib, _db, pool, scan_id, folder_id) = scanned_library().await;

    // A cover cache OUTSIDE the library (as in production: app_data/covers).
    let cache_home = TempDir::new().unwrap();
    let cache_dir = cache_home.path().join("covers");

    let before = snapshot_tree(lib.path());

    let img = get_cover(&pool, scan_id, folder_id, &cache_dir)
        .await
        .expect("get_cover ok")
        .expect("the book has a cover");
    assert_eq!(img.mime, "image/jpeg", "embedded art wins over the sidecar");
    assert_eq!(
        img.base64,
        abo_core::scan::cover::base64_encode(JPEG),
        "the base64 payload is the embedded picture"
    );

    // THE read-only guarantee: nothing under the library changed (D-09, AC-21).
    let after = snapshot_tree(lib.path());
    assert_eq!(
        before, after,
        "cover extraction must not create, modify, or touch any file under the library"
    );

    // The write went to the cache, which is outside the library.
    assert!(
        cache_dir.exists(),
        "the cover cache directory was created for the extracted thumbnail"
    );
    assert!(
        cache_dir.read_dir().unwrap().next().is_some(),
        "a cache file was written"
    );
    assert!(
        !cache_dir.starts_with(lib.path()),
        "the cache must live outside the library root"
    );
}

#[tokio::test]
async fn get_cover_serves_from_cache_after_source_is_gone() {
    let (lib, _db, pool, scan_id, folder_id) = scanned_library().await;
    let cache_home = TempDir::new().unwrap();
    let cache_dir = cache_home.path().join("covers");

    // First call populates the cache.
    let first = get_cover(&pool, scan_id, folder_id, &cache_dir)
        .await
        .expect("ok")
        .expect("cover");

    // Delete BOTH sources; only the cache can answer now.
    let book = lib.path().join("Some Author - A Book");
    std::fs::remove_file(book.join("chapter1.mp3")).unwrap();
    std::fs::remove_file(book.join("cover.jpg")).unwrap();

    let second = get_cover(&pool, scan_id, folder_id, &cache_dir)
        .await
        .expect("ok")
        .expect("cover still served from cache");

    assert_eq!(
        first.base64, second.base64,
        "the second call is served from the cache (sources are gone)"
    );
    assert_eq!(first.mime, second.mime);
}

#[tokio::test]
async fn get_cover_unknown_entry_is_none_not_error() {
    let (_lib, _db, pool, scan_id, _folder_id) = scanned_library().await;
    let cache_home = TempDir::new().unwrap();
    let cache_dir = cache_home.path().join("covers");

    let img = get_cover(&pool, scan_id, 9_999_999, &cache_dir)
        .await
        .expect("an unknown entry id is not an error");
    assert!(img.is_none(), "an unknown entry has no cover");
}
