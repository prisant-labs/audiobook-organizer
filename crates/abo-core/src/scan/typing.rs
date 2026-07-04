//! File typing (F-103): a pure, case-insensitive extension-to-class table.
//!
//! Every file is classified by its extension alone into one of the classes in
//! [`FileClass`]. This is deliberately a table lookup with no I/O and no content
//! inspection: v0.1.0 owns only the extension-to-class map. Folder-level routing
//! (the Zig Ziglar / radio-play cases) is a v0.2.0 classification concern (F-201).
//!
//! FD-17 conservative default: `.mp4` (and the other video containers) type as
//! [`FileClass::Video`], never `audio`, because the extension alone cannot tell
//! audio-in-mp4 from video-in-mp4; container inspection is deferred to v0.2.0.
//!
//! Directories are not typed here - the walker records `file_class = NULL` for
//! them (see [`crate::scan::walk`]).

use std::path::Path;

/// The F-103 file classes. `as_str` is the stable string persisted in
/// `entries.file_class` and sent across IPC; the strings are kebab-case where
/// multi-word (`release-info`) to match the error-code convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    /// Audiobook/music audio: m4b, mp3, m4a, opus, wma, flac.
    Audio,
    /// E-books: epub, pdf, mobi, azw3, lit, pdb, docx.
    Ebook,
    /// Images (covers, art): jpg, jpeg, png, gif, webp, bmp.
    Image,
    /// Playlists: m3u, m3u8, cue.
    Playlist,
    /// Release/metadata sidecars: nfo, sfv, txt.
    ReleaseInfo,
    /// Web shortcuts: url, html, htm.
    Weblink,
    /// Comics: cbr, cbz.
    Comic,
    /// Video containers (FD-17): mp4, mkv, avi, mov, wmv, m4v.
    Video,
    /// Everything else, including files with no extension.
    Other,
}

impl FileClass {
    /// Stable machine string persisted in `entries.file_class` and sent on IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            FileClass::Audio => "audio",
            FileClass::Ebook => "ebook",
            FileClass::Image => "image",
            FileClass::Playlist => "playlist",
            FileClass::ReleaseInfo => "release-info",
            FileClass::Weblink => "weblink",
            FileClass::Comic => "comic",
            FileClass::Video => "video",
            FileClass::Other => "other",
        }
    }
}

/// Classify a file by its path's extension (case-insensitive).
///
/// A path with no extension (e.g. `readme`) or an unknown extension (e.g.
/// `data.xyz`) maps to [`FileClass::Other`]. This is the single entry point the
/// walker calls for every file.
pub fn classify_path(path: &Path) -> FileClass {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => classify_ext(&ext.to_ascii_lowercase()),
        None => FileClass::Other,
    }
}

/// Classify an already-lowercased extension (no leading dot). Split out so the
/// table is testable directly and the case-folding happens in exactly one place.
fn classify_ext(ext_lower: &str) -> FileClass {
    match ext_lower {
        "m4b" | "mp3" | "m4a" | "opus" | "wma" | "flac" => FileClass::Audio,
        "epub" | "pdf" | "mobi" | "azw3" | "lit" | "pdb" | "docx" => FileClass::Ebook,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" => FileClass::Image,
        "m3u" | "m3u8" | "cue" => FileClass::Playlist,
        "nfo" | "sfv" | "txt" => FileClass::ReleaseInfo,
        "url" | "html" | "htm" => FileClass::Weblink,
        "cbr" | "cbz" => FileClass::Comic,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "m4v" => FileClass::Video,
        _ => FileClass::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-12: each catalogued extension maps to its class; one example per class
    /// plus an unknown extension and a no-extension name mapping to `other`, and
    /// the FD-17 `.mp4 -> video` conservative default. Table-driven so adding a
    /// class or extension is a one-line change with an explicit expectation.
    #[test]
    fn file_typing_table() {
        let cases: &[(&str, FileClass)] = &[
            // one representative per class
            ("audiobook.m4b", FileClass::Audio),
            ("part1.mp3", FileClass::Audio),
            ("novel.epub", FileClass::Ebook),
            ("cover.jpg", FileClass::Image),
            ("book.m3u", FileClass::Playlist),
            ("tracks.m3u8", FileClass::Playlist),
            ("metadata.nfo", FileClass::ReleaseInfo),
            ("notes.txt", FileClass::ReleaseInfo),
            ("buy.url", FileClass::Weblink),
            ("index.html", FileClass::Weblink),
            ("issue.cbz", FileClass::Comic),
            ("issue.cbr", FileClass::Comic),
            // FD-17: video containers, including the deliberate .mp4 -> video
            ("interview.mp4", FileClass::Video),
            ("movie.mkv", FileClass::Video),
            ("clip.m4v", FileClass::Video),
            // fall-through: unknown extension and no extension both -> other
            ("data.xyz", FileClass::Other),
            ("readme", FileClass::Other),
        ];
        for (name, expected) in cases {
            assert_eq!(
                classify_path(Path::new(name)),
                *expected,
                "typing {name} should be {expected:?}"
            );
        }
    }

    #[test]
    fn typing_is_case_insensitive() {
        // Extensions differ only by case must classify identically (AC-12 says
        // the table is case-insensitive).
        assert_eq!(classify_path(Path::new("BOOK.MP4")), FileClass::Video);
        assert_eq!(classify_path(Path::new("Cover.JPG")), FileClass::Image);
        assert_eq!(classify_path(Path::new("track.FLAC")), FileClass::Audio);
    }

    #[test]
    fn class_strings_are_stable_kebab() {
        // The persisted strings are contract; keep them stable and kebab-case.
        let all = [
            FileClass::Audio,
            FileClass::Ebook,
            FileClass::Image,
            FileClass::Playlist,
            FileClass::ReleaseInfo,
            FileClass::Weblink,
            FileClass::Comic,
            FileClass::Video,
            FileClass::Other,
        ];
        for c in all {
            let s = c.as_str();
            assert!(!s.is_empty(), "class string must be non-empty");
            assert!(
                s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "class string must be kebab-case: {s}"
            );
        }
        assert_eq!(FileClass::ReleaseInfo.as_str(), "release-info");
    }
}
