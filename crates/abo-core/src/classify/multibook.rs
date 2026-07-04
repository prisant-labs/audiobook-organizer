//! F-203 (multi-book folder detection): decide whether a folder's DIRECT audio
//! files are several complete books sitting as siblings (Narnia: 7 files/7
//! titles; Harry Potter: 11 files spanning 7 titles plus 2 extras), as opposed
//! to one book's parts (disc/track/chapter splits) or one book plus a bonus.
//!
//! Like the rest of `crate::classify`, this module is pure logic: no I/O, no
//! filesystem, nothing `cfg`-gated (the CFG RULE). Its input is the list of a
//! folder's direct audio files, each carrying its raw name, the title F-303
//! parsed from that name (if any), and its declared size; its output is a
//! [`MultiBookVerdict`].
//!
//! # The heuristic, and the two false-positive guards it must survive
//!
//! A folder is a `multi-book-suspect` when it holds >= 2 DISTINCT complete book
//! titles as sibling audio files. "Distinct" and "complete" are where the two
//! spec false-positive guards (AC-203.3) live:
//!
//! - **Disc / track / chapter parts are not books.** A file whose whole name is
//!   just a part marker (`track01`, `Disc 2`, `Part 1`, `01`) is one book's
//!   part, never a book of its own, so it is dropped before counting. A
//!   disc-split single book (`Verbal Advantage/Disc 1/track01.mp3` ...) thus
//!   contributes ZERO distinct titles and is never flagged (AC-203.3).
//! - **Bonus / sample / interview files are not books.** A file whose name
//!   carries a bonus marker (`Extra - ...`, `Bonus`, `Sample`, `Interview`) is
//!   dropped, so a single book plus a bonus file is one title, not two
//!   (AC-203.3), and Harry Potter's two `Extra - ...` files do not inflate its
//!   count.
//!
//! A real title that merely CARRIES a part suffix (`Goblet of Fire - Part 1`,
//! `- Part 2`) is kept but its suffix is normalized off, so the two Goblet
//! files collapse to one title and Harry Potter counts 7 distinct titles across
//! its 9 non-extra files (AC-203.2), not 9.
//!
//! Zero-byte files (placeholder / stub) are also dropped: a folder of empty
//! placeholder audio is not a multi-book folder just because the placeholders
//! have distinct names.

use std::sync::LazyLock;

use regex::Regex;

/// One direct audio file of a folder, as F-203 sees it: its raw name (with
/// extension), the title F-303 parsed from that name (if the name parsed at
/// all), and its declared size in bytes.
#[derive(Debug, Clone, Copy)]
pub struct BookFile<'a> {
    pub name: &'a str,
    pub title: Option<&'a str>,
    pub size: u64,
}

/// The F-203 outcome for one folder: whether it holds several complete books,
/// and the distinct book titles the heuristic actually counted (sorted, for a
/// deterministic evidence surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiBookVerdict {
    pub is_multi: bool,
    pub distinct_titles: Vec<String>,
}

/// A file whose ENTIRE name is a part marker (`track01`, `Disc 2`, `Part 1`,
/// `01`, `1-02`): one book's part, dropped before counting distinct titles.
/// Anchored to the whole stem so a real title that merely ends in `- Part 1`
/// is NOT caught here (that case is handled by [`normalize_title`]).
static WHOLE_PART_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(?:(?:track|chapter|part|disc|disk|cd|section|file|pt|vol|volume)[\s_\-.]*\d+|\d{1,4}|\d{1,3}[\s_\-.]+\d{1,3})\s*$")
        .expect("valid whole-part regex")
});

/// A bonus / sample / interview marker anywhere in a name: the file is
/// supplementary, not a book, and is dropped before counting.
static BONUS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:extra|bonus|sample|interview|excerpt|preview|teaser|trailer|promo)\b")
        .expect("valid bonus regex")
});

/// A trailing part suffix on an OTHERWISE-real title (`Goblet of Fire - Part
/// 1`, `Deathly Hallows Part 2`, `Title - Disc 3`): stripped so the parts of
/// one title collapse to a single distinct title, without dropping the file.
static PART_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\s\-]+(?:part|pt|disc|disk|cd|vol|volume|chapter)\s*\.?\s*\d+\s*$")
        .expect("valid part-suffix regex")
});

/// Strip a recognized audio extension from a file name so a name that never
/// parsed a title (so [`BookFile::title`] is `None`) can still be judged by its
/// stem (a bare `track01.mp3` reads as the part marker `track01`). Deliberately
/// a fixed allowlist, mirroring `crate::parse::strip_known_extension`, so a
/// folder-ish name with an internal `.` is never mis-stemmed.
fn stem(name: &str) -> &str {
    const EXTENSIONS: &[&str] = &[".m4b", ".mp3", ".m4a", ".opus", ".wma", ".flac", ".mp4"];
    let lower = name.to_ascii_lowercase();
    for ext in EXTENSIONS {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// Normalize a title for distinctness: strip a trailing part suffix, collapse
/// internal whitespace, and lowercase. Returns an owned normalized form (empty
/// if nothing survives, which the caller treats as "no title").
fn normalize_title(title: &str) -> String {
    let no_suffix = PART_SUFFIX_RE.replace(title.trim(), "");
    no_suffix
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether `name`'s whole stem is a part marker (drop it) or carries a bonus
/// marker (drop it). Both are "this file is not a book of its own".
fn is_not_a_book(name: &str) -> bool {
    let stem = stem(name);
    WHOLE_PART_RE.is_match(stem) || BONUS_RE.is_match(name)
}

/// Choose the title to count a file by. F-303's parsed title is preferred, but
/// a `Title - Part N` name parses (via pattern 2) into author `Title`, title
/// `Part N`, so a parsed title that is itself a whole part marker (`Part 1`) is
/// discarded in favor of the file's stem (`Harry Potter ... Goblet of Fire -
/// Part 1`), whose part suffix [`normalize_title`] then strips. This is what
/// makes Harry Potter's two Goblet files and two Deathly Hallows files collapse
/// to their real titles rather than to `part 1` / `part 2`.
fn title_for<'a>(f: &BookFile<'a>) -> &'a str {
    match f.title {
        Some(t) if !WHOLE_PART_RE.is_match(t) => t,
        _ => stem(f.name),
    }
}

/// Detect whether a folder's direct audio files are several complete books.
///
/// Files that are parts, bonuses, or zero-byte placeholders are dropped first;
/// each surviving file is counted by [`title_for`] with its part suffix
/// normalized off.
pub fn detect_multibook(files: &[BookFile]) -> MultiBookVerdict {
    use std::collections::BTreeSet;

    let mut distinct: BTreeSet<String> = BTreeSet::new();
    for f in files {
        if f.size == 0 {
            continue;
        }
        if is_not_a_book(f.name) {
            continue;
        }
        let norm = normalize_title(title_for(f));
        if norm.is_empty() {
            continue;
        }
        distinct.insert(norm);
    }

    let distinct_titles: Vec<String> = distinct.into_iter().collect();
    MultiBookVerdict {
        is_multi: distinct_titles.len() >= 2,
        distinct_titles,
    }
}

/// Whether a folder NAME is a nonconforming disc-part folder (`Disc 1`, `Disc
/// 2`, `CD3`, `Disk_04`): a single book's disc, never a book or a container of
/// its own. Used by the engine to fold a disc-split single book (a folder whose
/// child folders are ALL disc parts) into one `book`, not a series/pack
/// container or a multi-book suspect (AC-203.3).
static DISC_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(?:disc|disk|cd)[\s_\-.]*\d+\s*$").expect("valid disc-name regex")
});

/// Whether `name` is a disc-part folder name (see [`DISC_NAME_RE`]).
pub fn is_disc_folder_name(name: &str) -> bool {
    DISC_NAME_RE.is_match(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bf<'a>(name: &'a str, title: Option<&'a str>, size: u64) -> BookFile<'a> {
        BookFile { name, title, size }
    }

    /// AC-203.1: seven distinct real titles (Narnia) flag as multi-book.
    #[test]
    fn narnia_seven_distinct_titles_is_multi() {
        let files = [
            bf(
                "The Lion, the Witch and the Wardrobe.m4b",
                Some("The Lion, the Witch and the Wardrobe"),
                90_000,
            ),
            bf("Prince Caspian.m4b", Some("Prince Caspian"), 75_000),
            bf(
                "The Voyage of the Dawn Treader.m4b",
                Some("The Voyage of the Dawn Treader"),
                80_000,
            ),
            bf("The Silver Chair.m4b", Some("The Silver Chair"), 78_000),
            bf(
                "The Horse and His Boy.m4b",
                Some("The Horse and His Boy"),
                70_000,
            ),
            bf(
                "The Magician's Nephew.m4b",
                Some("The Magician's Nephew"),
                72_000,
            ),
            bf("The Last Battle.m4b", Some("The Last Battle"), 76_000),
        ];
        let v = detect_multibook(&files);
        assert!(v.is_multi);
        assert_eq!(v.distinct_titles.len(), 7);
    }

    /// AC-203.2: eleven Harry Potter files spanning seven titles plus two
    /// extras collapse to seven distinct titles and flag as multi-book, not a
    /// single book.
    #[test]
    fn harry_potter_eleven_files_seven_titles_is_multi() {
        // Titles reflect F-303's real parse: a `Title - Part N` name splits
        // (pattern 2) into author `Title`, title `Part N`, so the Goblet /
        // Deathly Hallows files arrive with a parsed title of `Part 1` / `Part
        // 2`; the stem fallback must recover the real title so the four part
        // files collapse to two real titles, not to `part 1` / `part 2`.
        let files = [
            bf(
                "Harry Potter and the Philosopher's Stone.mp3",
                Some("Harry Potter and the Philosopher's Stone"),
                100_000,
            ),
            bf(
                "Harry Potter and the Chamber of Secrets.mp3",
                Some("Harry Potter and the Chamber of Secrets"),
                105_000,
            ),
            bf(
                "Harry Potter and the Prisoner of Azkaban.mp3",
                Some("Harry Potter and the Prisoner of Azkaban"),
                110_000,
            ),
            bf(
                "Harry Potter and the Goblet of Fire - Part 1.mp3",
                Some("Part 1"),
                130_000,
            ),
            bf(
                "Harry Potter and the Goblet of Fire - Part 2.mp3",
                Some("Part 2"),
                130_000,
            ),
            bf(
                "Harry Potter and the Order of the Phoenix.mp3",
                Some("Harry Potter and the Order of the Phoenix"),
                150_000,
            ),
            bf(
                "Harry Potter and the Half-Blood Prince.mp3",
                Some("Harry Potter and the Half-Blood Prince"),
                140_000,
            ),
            bf(
                "Harry Potter and the Deathly Hallows - Part 1.mp3",
                Some("Part 1"),
                120_000,
            ),
            bf(
                "Harry Potter and the Deathly Hallows - Part 2.mp3",
                Some("Part 2"),
                120_000,
            ),
            bf("Extra - JK Rowling Interview.mp3", None, 20_000),
            bf("Extra - Pottermore Bonus Chapter.mp3", None, 15_000),
        ];
        let v = detect_multibook(&files);
        assert!(v.is_multi, "HP must be multi-book");
        assert_eq!(
            v.distinct_titles.len(),
            7,
            "seven distinct titles after collapsing Part N and dropping extras: {:?}",
            v.distinct_titles
        );
        assert!(
            !v.distinct_titles
                .iter()
                .any(|t| t == "part 1" || t == "part 2"),
            "part markers must not survive as titles: {:?}",
            v.distinct_titles
        );
        assert!(
            v.distinct_titles
                .contains(&"harry potter and the goblet of fire".to_string()),
            "the real Goblet title must be recovered: {:?}",
            v.distinct_titles
        );
    }

    /// AC-203.3 (disc guard): a disc's track files are all parts, so a disc
    /// folder is one book (zero distinct titles), never multi-book.
    #[test]
    fn disc_of_track_files_is_not_multi() {
        let files = [
            bf("track01.mp3", Some("track01"), 45_000),
            bf("track02.mp3", Some("track02"), 45_000),
            bf("track03.mp3", Some("track03"), 45_000),
        ];
        let v = detect_multibook(&files);
        assert!(!v.is_multi, "track files are parts, not distinct books");
        assert!(v.distinct_titles.is_empty());
    }

    /// AC-203.3 (bonus guard): one real book plus a bonus file is one title,
    /// not two.
    #[test]
    fn single_book_plus_bonus_is_not_multi() {
        let files = [
            bf("The Hobbit.m4b", Some("The Hobbit"), 200_000),
            bf(
                "Bonus - Author Interview.mp3",
                Some("Author Interview"),
                10_000,
            ),
        ];
        let v = detect_multibook(&files);
        assert!(!v.is_multi, "one book plus a bonus is a single book");
        assert_eq!(v.distinct_titles, vec!["the hobbit".to_string()]);
    }

    /// Zero-byte placeholders never make a folder multi-book, even with
    /// distinct names.
    #[test]
    fn zero_byte_placeholders_are_not_multi() {
        let files = [
            bf("Empty Track.mp3", Some("Empty Track"), 0),
            bf("placeholder.m4b", Some("placeholder"), 0),
        ];
        let v = detect_multibook(&files);
        assert!(!v.is_multi);
        assert!(v.distinct_titles.is_empty());
    }

    /// The Frank Herbert-Dune content case (note-2 resolution): two distinct
    /// real titles sitting directly inside a pattern-9-NAMED folder make it a
    /// multi-book suspect by content, not a container.
    #[test]
    fn two_distinct_titles_inside_is_multi() {
        let files = [
            bf("Book 1 - Dune.m4b", Some("Dune"), 200_000),
            bf("Book 2 - Dune Messiah.m4b", Some("Dune Messiah"), 130_000),
        ];
        let v = detect_multibook(&files);
        assert!(v.is_multi);
        assert_eq!(
            v.distinct_titles,
            vec!["dune".to_string(), "dune messiah".to_string()]
        );
    }

    /// A single title split across explicit parts collapses to one title.
    #[test]
    fn single_title_in_parts_collapses() {
        let files = [
            bf(
                "The Stand - Part 1.m4b",
                Some("The Stand - Part 1"),
                100_000,
            ),
            bf(
                "The Stand - Part 2.m4b",
                Some("The Stand - Part 2"),
                100_000,
            ),
        ];
        let v = detect_multibook(&files);
        assert!(!v.is_multi, "one title in two parts is a single book");
        assert_eq!(v.distinct_titles, vec!["the stand".to_string()]);
    }

    #[test]
    fn disc_folder_name_detection() {
        for name in ["Disc 1", "Disc 2", "CD3", "Disk_04", "cd 12", "DISC-3"] {
            assert!(
                is_disc_folder_name(name),
                "{name} should be a disc folder name"
            );
        }
        for name in ["Chronicles of Narnia", "Harry Potter", "Disc Golf History"] {
            assert!(
                !is_disc_folder_name(name),
                "{name} should NOT be a disc folder name"
            );
        }
    }
}
