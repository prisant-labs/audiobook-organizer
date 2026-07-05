//! F-204 (disc-structure detection): recognize disc-based books and propose
//! renames for nonconforming disc-part folder names.
//!
//! A multi-disc audiobook stores each disc in its own child folder. ABS
//! recognizes a disc child when its name is the conformant `Disc N` / `CD N` /
//! `Disk N` shape (a canonical disc word, a single space, then the disc
//! number). A nonconforming variant (`CD3` with no space, `Disk_04` with an
//! underscore, `Disc1`, `(Disc 01)` embedded in a larger name, and so on) means
//! the same thing but does not read as a disc to the target app, so F-204
//! proposes a rename to the conformant shape.
//!
//! This module is pure string logic: no I/O, no filesystem, nothing
//! `cfg`-gated (the CFG RULE), exactly like the rest of `crate::plan`. It feeds
//! the `normalize-series` internal pass of the plan builder (F-403), which folds
//! into the "messy names" user-facing group for the UI (FD-26). It shares the
//! disc-name recognition with [`crate::classify::multibook::is_disc_folder_name`]
//! (the classifier already marks disc-part folders as such); this module adds the
//! conformant-vs-nonconforming judgement and the rename target.
//!
//! # What is conformant
//!
//! `Disc 1`, `Disc 2`, `CD 3`, `Disk 04` are conformant: a canonical disc word
//! (`Disc` / `CD` / `Disk`), exactly one space, then the digits, with no
//! surrounding text. The number is not re-padded (`Disc 1` stays `Disc 1`, not
//! `Disc 01`): the fixture treats `Disc 1` as already conformant, and re-padding
//! a perfectly readable name would be churn, not a fix. A name that is a disc
//! part but not in that exact shape is nonconforming and
//! [`conformant_disc_rename`] returns its conformant form.

use std::sync::LazyLock;

use regex::Regex;

/// A disc-part folder name, capturing the disc word (group 1) and the disc
/// number (group 2). The separator between them is any run of space / underscore
/// / hyphen / dot (or none), so `CD3`, `Disk_04`, `Disc-1`, and `Disc 1` all
/// match. Anchored to the whole (trimmed) name so a real title that merely
/// contains the word "disc" is never caught. Mirrors
/// [`crate::classify::multibook::is_disc_folder_name`]'s recognizer so the two
/// agree on exactly which folders are disc parts.
static DISC_PART_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(disc|disk|cd)[\s_\-.]*(\d+)\s*$").expect("valid disc-part regex")
});

/// The canonical capitalization for a recognized disc word. `disc`/`Disc` ->
/// `Disc`, `disk` -> `Disk`, `cd`/`CD` -> `CD`. Keeps the disc family the user
/// chose (a `CD` folder becomes `CD 3`, not `Disc 3`): the spec recognizes all
/// three prefixes as conformant, so a rename normalizes the SHAPE, not the word.
fn canonical_word(word: &str) -> &'static str {
    match word.to_ascii_lowercase().as_str() {
        "disk" => "Disk",
        "cd" => "CD",
        _ => "Disc",
    }
}

/// Whether `name` is a disc-part folder name at all (conformant or not). Thin
/// wrapper over the shared recognizer, offered so callers do not need to know
/// the regex.
pub fn is_disc_part_name(name: &str) -> bool {
    DISC_PART_RE.is_match(name.trim())
}

/// The conformant disc-part name for `name` (`Disc 1`, `CD 3`, `Disk 04`), or
/// [`None`] when `name` is not a disc-part name. Always returns the canonical
/// shape, even when `name` is already conformant (in which case it equals
/// `name.trim()`); use [`conformant_disc_rename`] when you only want a rename
/// for names that actually need one.
pub fn conformant_disc_name(name: &str) -> Option<String> {
    let caps = DISC_PART_RE.captures(name.trim())?;
    let word = caps.get(1)?.as_str();
    let num = caps.get(2)?.as_str();
    Some(format!("{} {}", canonical_word(word), num))
}

/// Whether `name` is already in the conformant disc-part shape (so F-204 leaves
/// it untouched). A non-disc name is not conformant (there is nothing to
/// conform).
pub fn is_conformant_disc(name: &str) -> bool {
    match conformant_disc_name(name) {
        Some(canonical) => canonical == name.trim(),
        None => false,
    }
}

/// If `name` is a NONCONFORMING disc-part folder name, the conformant name it
/// should be renamed to; [`None`] when `name` is already conformant or is not a
/// disc-part name at all. This is the F-204 proposal the `normalize-series` pass
/// turns into a rename op (AC-32).
pub fn conformant_disc_rename(name: &str) -> Option<String> {
    let canonical = conformant_disc_name(name)?;
    if canonical == name.trim() {
        None
    } else {
        Some(canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-32: conformant disc names are recognized and produce no rename.
    #[test]
    fn conformant_disc_names_need_no_rename() {
        for name in ["Disc 1", "Disc 2", "CD 3", "Disk 04", "Disc 10"] {
            assert!(is_disc_part_name(name), "{name} is a disc part");
            assert!(is_conformant_disc(name), "{name} is conformant");
            assert_eq!(
                conformant_disc_rename(name),
                None,
                "{name} must not be renamed"
            );
        }
    }

    /// AC-32: nonconforming disc names produce a rename to the conformant shape,
    /// keeping the disc family the user chose (a `CD` stays `CD`, a `Disk` stays
    /// `Disk`).
    #[test]
    fn nonconforming_disc_names_rename_to_the_conformant_shape() {
        let cases: &[(&str, &str)] = &[
            ("CD3", "CD 3"),
            ("Disk_04", "Disk 04"),
            ("Disc1", "Disc 1"),
            ("Disc-2", "Disc 2"),
            ("disc 3", "Disc 3"),
            ("cd 12", "CD 12"),
            ("DISK.5", "Disk 5"),
        ];
        for (input, expected) in cases {
            assert!(is_disc_part_name(input), "{input} is a disc part");
            assert!(!is_conformant_disc(input), "{input} is nonconforming");
            assert_eq!(
                conformant_disc_rename(input).as_deref(),
                Some(*expected),
                "{input} should rename to {expected}"
            );
        }
    }

    /// A name that merely contains the word "disc" (a real title) is not a disc
    /// part and is never renamed.
    #[test]
    fn non_disc_names_are_not_touched() {
        for name in ["Chronicles of Narnia", "Disc Golf History", "The Discovery"] {
            assert!(!is_disc_part_name(name), "{name} is not a disc part");
            assert_eq!(conformant_disc_rename(name), None);
            assert!(!is_conformant_disc(name));
        }
    }
}
