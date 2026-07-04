//! READ-ONLY embedded-tag reading via `lofty`, for the FD-14 tag-quality probe
//! only (release gate G-08).
//!
//! This is the single place `lofty` is touched in the whole crate, and it only
//! ever READS: [`read_tag_fields`] opens a file through lofty's probe, pulls the
//! primary tag's title / artist / album / composer, and returns them. Nothing
//! here writes, converts, or re-saves a tag - the probe is a measurement
//! instrument (does the folder-first assumption hold?), never a mutation of the
//! library (D-09 read-only invariant; this phase's absolute constraint).
//!
//! `lofty` reads only as much of each file as it needs to parse the tag header
//! (not the whole multi-hundred-MB audiobook), so sampling a few hundred files
//! stays bounded. A file lofty cannot parse, or one carrying no tag at all, is a
//! first-class outcome ([`TagReadOutcome::Unreadable`] / [`TagReadOutcome::NoTags`]),
//! not an error: an audiobook library legitimately contains untagged files, and
//! that fact IS part of what the FD-14 verdict measures.

use std::path::Path;

use lofty::prelude::{Accessor, ItemKey, TaggedFileExt};
use lofty::probe::Probe;

/// The subset of embedded-tag fields the FD-14 probe compares against the
/// folder-first parse: `title`, `author` (the tag's artist frame), `album`
/// (which, for audiobooks, frequently carries the series or a series-plus-index
/// string), and `narrator` (read from the composer frame, the de-facto
/// audiobook narrator slot). Every field is normalized (trimmed; empty becomes
/// [`None`]) so a present-but-blank frame never counts as a usable tag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagFields {
    pub title: Option<String>,
    pub author: Option<String>,
    pub album: Option<String>,
    pub narrator: Option<String>,
}

impl TagFields {
    /// Whether any of the four measured fields carried a usable value.
    pub fn has_any(&self) -> bool {
        self.title.is_some()
            || self.author.is_some()
            || self.album.is_some()
            || self.narrator.is_some()
    }
}

/// The outcome of reading one file's tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagReadOutcome {
    /// lofty opened and parsed the file; carries whatever primary-tag fields
    /// were present (any subset may still be [`None`]).
    Read(TagFields),
    /// lofty opened and parsed the file but it carried no tag block at all.
    NoTags,
    /// lofty could not open or parse the file (unsupported container, truncated,
    /// permission-denied, and so on). Recorded, never fatal.
    Unreadable,
}

/// Read the primary-tag `title` / `artist` / `album` / `composer` from `path`,
/// READ-ONLY. See [`TagReadOutcome`] for the three outcomes.
pub fn read_tag_fields(path: &Path) -> TagReadOutcome {
    // `Probe::open(...).read()` opens the file read-only; we never call any
    // save/write API on the returned `TaggedFile`.
    let tagged = match Probe::open(path).and_then(|probe| probe.read()) {
        Ok(t) => t,
        Err(_) => return TagReadOutcome::Unreadable,
    };

    let tag = match tagged.primary_tag().or_else(|| tagged.first_tag()) {
        Some(t) => t,
        None => return TagReadOutcome::NoTags,
    };

    // Trim, and treat a present-but-blank frame as absent.
    fn clean(s: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
        s.map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
    }

    let narrator = tag
        .get_string(ItemKey::Composer)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    TagReadOutcome::Read(TagFields {
        title: clean(tag.title()),
        author: clean(tag.artist()),
        album: clean(tag.album()),
        narrator,
    })
}
