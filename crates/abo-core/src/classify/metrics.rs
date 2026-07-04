//! F-202 (library health metrics): aggregate counts and byte totals per folder
//! class and per problem type, over a classified snapshot. These are the
//! numbers the later library home (F-902) and the campaign meter surface, so
//! re-computing them after an apply (later releases) shows them moving toward
//! zero.
//!
//! Like the rest of `crate::classify`, this module is pure logic: no I/O, no
//! filesystem, nothing `cfg`-gated (the CFG RULE).
//!
//! # Every metric declares its unit (FD-08)
//!
//! No bare number is emitted without stating what it counts: per-class metrics
//! carry a folder count (folders) and a byte total (bytes); each problem metric
//! carries an explicit [`MetricUnit`]. The duplicate metric's canonical unit is
//! the GROUP (FD-08): one book, N identical copies counts as one group, and its
//! byte total sizes all copies in candidate groups (candidates only; resolution
//! is v0.6.0).
//!
//! # Byte attribution is single-counting
//!
//! Each file's bytes are attributed to exactly one class: the class of its
//! immediate parent folder. A container (series / pack) therefore contributes
//! zero bytes of its own (its bytes live in its book children), so summing
//! every class's byte total plus the loose-root files equals the library's
//! total bytes with no double counting (asserted against the fixture's declared
//! total, AC-F5 + AC-202.1).
//!
//! # Disc-part folds (F-204 preview)
//!
//! A disc-split single book classifies its disc subfolders as `book` too (there
//! is no disc class until F-204, v0.3.0). To keep the book COUNT honest, a
//! `book` folder whose parent is itself a `book` (only possible for a disc part)
//! is not counted as its own book; its bytes still attribute to `book`. So a
//! disc-split single book counts once, with all its disc bytes under `book`.

use std::collections::HashMap;

use crate::classify::engine::{ClassifyInput, FolderClass, FolderClassification};
use crate::parse::extract::NodeKind;
use crate::parse::strip::{strip, StripOptions};
use crate::scan::typing::FileClass;

/// The quantity a metric counts (FD-08: no bare number without its unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricUnit {
    Folders,
    Files,
    Bytes,
    /// Duplicate GROUPS (one book, N copies = one group; FD-08).
    Groups,
}

impl MetricUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            MetricUnit::Folders => "folders",
            MetricUnit::Files => "files",
            MetricUnit::Bytes => "bytes",
            MetricUnit::Groups => "groups",
        }
    }
}

impl serde::Serialize for MetricUnit {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// A per-class metric: how many folders carry the class, and how many bytes are
/// attributed to it. `folder_count` is in folders; `byte_total` is in bytes
/// (the units are stated by the field names, FD-08).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ClassMetric {
    pub class: FolderClass,
    pub folder_count: u64,
    pub byte_total: u64,
}

/// A per-problem metric, carrying its own explicit unit (FD-08). `count` is in
/// the stated `unit`; `byte_total` is always bytes (0 where a size is not
/// meaningful for the problem, e.g. deep nesting).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProblemMetric {
    pub problem: &'static str,
    pub unit: MetricUnit,
    pub count: u64,
    pub byte_total: u64,
}

/// The full health report: per-class metrics (all nine classes, always present,
/// zeroed where a class is absent), per-problem metrics, and the library total
/// in bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HealthMetrics {
    pub per_class: Vec<ClassMetric>,
    pub problems: Vec<ProblemMetric>,
    pub total_bytes: u64,
}

/// Compute the health metrics for a classified snapshot. `classifications` must
/// be the output of [`crate::classify::classify`] over the same `entries`.
pub fn health_metrics(
    entries: &[ClassifyInput],
    classifications: &[FolderClassification],
) -> HealthMetrics {
    let index_of: HashMap<usize, usize> =
        entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    let class_by_id: HashMap<usize, FolderClass> =
        classifications.iter().map(|c| (c.id, c.class)).collect();

    // Depth via the parent chain (top-level = 1).
    let parent_pos: Vec<Option<usize>> = entries
        .iter()
        .map(|e| e.parent.and_then(|p| index_of.get(&p).copied()))
        .collect();
    let depth = |mut i: usize| -> usize {
        let mut d = 1;
        let mut steps = 0;
        while let Some(p) = parent_pos[i] {
            if steps > entries.len() {
                break;
            }
            steps += 1;
            d += 1;
            i = p;
        }
        d
    };

    // ---- per-class byte attribution + folder counts ----
    let mut folder_count: HashMap<FolderClass, u64> = HashMap::new();
    let mut byte_total: HashMap<FolderClass, u64> = HashMap::new();

    for c in classifications {
        // A `book` whose parent folder is also a `book` is a disc part: fold it
        // into its parent's single book count (F-204 preview).
        if c.class == FolderClass::Book {
            let pos = index_of[&c.id];
            let parent_is_book = entries[pos]
                .parent
                .and_then(|pid| class_by_id.get(&pid))
                .map(|pc| *pc == FolderClass::Book)
                .unwrap_or(false);
            if parent_is_book {
                continue;
            }
        }
        *folder_count.entry(c.class).or_default() += 1;
    }

    let mut total_bytes = 0u64;
    for e in entries {
        if e.kind != NodeKind::File {
            continue;
        }
        total_bytes += e.size;
        if let Some(pid) = e.parent {
            if let Some(cls) = class_by_id.get(&pid) {
                *byte_total.entry(*cls).or_default() += e.size;
            }
        }
    }

    let per_class: Vec<ClassMetric> = FolderClass::ALL
        .iter()
        .map(|&class| ClassMetric {
            class,
            folder_count: folder_count.get(&class).copied().unwrap_or(0),
            byte_total: byte_total.get(&class).copied().unwrap_or(0),
        })
        .collect();

    // ---- problem metrics ----
    // Loose root books: audio files directly at the library root (no parent).
    let (loose_count, loose_bytes) = entries
        .iter()
        .filter(|e| {
            e.kind == NodeKind::File && e.parent.is_none() && e.file_class == Some(FileClass::Audio)
        })
        .fold((0u64, 0u64), |(n, b), e| (n + 1, b + e.size));

    // Noisy names: folders whose name carries strippable noise (brackets,
    // bitrate, size, rank prefix, year prefix, release group).
    let noisy_count = entries
        .iter()
        .filter(|e| e.kind == NodeKind::Folder && has_noise(&e.name))
        .count() as u64;

    // Deep nesting: folders at depth >= 5.
    let deep_count = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == NodeKind::Folder)
        .filter(|(i, _)| depth(*i) >= 5)
        .count() as u64;

    // Duplicate candidate groups: files sharing (basename, size), >= 2 members
    // (the Codex basename+size method; candidates only, FD-08 GROUP unit).
    let mut groups: HashMap<(String, u64), Vec<u64>> = HashMap::new();
    for e in entries {
        if e.kind == NodeKind::File {
            groups
                .entry((e.name.clone(), e.size))
                .or_default()
                .push(e.size);
        }
    }
    let (dup_groups, dup_bytes) = groups
        .values()
        .filter(|copies| copies.len() >= 2)
        .fold((0u64, 0u64), |(g, b), copies| {
            (g + 1, b + copies.iter().sum::<u64>())
        });

    // Empties: folders classified `empty`.
    let empty_count = *folder_count.get(&FolderClass::Empty).unwrap_or(&0);

    let problems = vec![
        ProblemMetric {
            problem: "loose-root-books",
            unit: MetricUnit::Files,
            count: loose_count,
            byte_total: loose_bytes,
        },
        ProblemMetric {
            problem: "noisy-names",
            unit: MetricUnit::Folders,
            count: noisy_count,
            byte_total: 0,
        },
        ProblemMetric {
            problem: "deep-nesting",
            unit: MetricUnit::Folders,
            count: deep_count,
            byte_total: 0,
        },
        ProblemMetric {
            problem: "duplicate-candidate-groups",
            unit: MetricUnit::Groups,
            count: dup_groups,
            byte_total: dup_bytes,
        },
        ProblemMetric {
            problem: "empty-folders",
            unit: MetricUnit::Folders,
            count: empty_count,
            byte_total: 0,
        },
    ];

    HealthMetrics {
        per_class,
        problems,
        total_bytes,
    }
}

/// All-noise strip options: every trailing/leading noise pass on, so any name
/// that changes is "noisy". `underscores_to_spaces` is off (it is a formatting
/// normalization, not a noise marker, and would flag every underscored name).
fn noise_options() -> StripOptions {
    StripOptions {
        bracket_tags: true,
        release_group_suffix: true,
        bitrate: true,
        size: true,
        rank_prefix: true,
        year_prefix: true,
        underscores_to_spaces: false,
    }
}

/// Whether `name` carries strippable noise (its stripped form differs).
fn has_noise(name: &str) -> bool {
    strip(name, noise_options()).text.trim() != name.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;

    fn folder(id: usize, parent: Option<usize>, name: &str) -> ClassifyInput {
        ClassifyInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::Folder,
            file_class: None,
            size: 0,
        }
    }

    fn audio(id: usize, parent: Option<usize>, name: &str, size: u64) -> ClassifyInput {
        ClassifyInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::File,
            file_class: Some(FileClass::Audio),
            size,
        }
    }

    fn problem<'a>(m: &'a HealthMetrics, p: &str) -> &'a ProblemMetric {
        m.problems
            .iter()
            .find(|x| x.problem == p)
            .expect("problem present")
    }

    /// AC-202.3: an already-tidy library (author-first, clean names, no dups,
    /// no deep nesting, no loose root, no empties) yields zeroed problem
    /// metrics with no error.
    #[test]
    fn already_tidy_yields_zero_problems() {
        // Author/Series/Book layout, clean names, distinct files, shallow.
        let entries = vec![
            folder(0, None, "Andy Weir - Project Hail Mary"),
            audio(1, Some(0), "Project Hail Mary.m4b", 175_000),
            folder(2, None, "James Clear - Atomic Habits"),
            audio(3, Some(2), "Atomic Habits.m4b", 140_000),
        ];
        let cs = classify(&entries);
        let m = health_metrics(&entries, &cs);
        for p in &m.problems {
            assert_eq!(
                p.count, 0,
                "problem {} must be zero on a tidy library",
                p.problem
            );
        }
        assert_eq!(m.total_bytes, 315_000);
    }

    /// FD-08: every problem metric declares a unit, and byte totals are only
    /// nonzero where a size is meaningful.
    #[test]
    fn every_metric_declares_a_unit() {
        let entries = vec![folder(0, None, "Some Author - Some Book")];
        let cs = classify(&entries);
        let m = health_metrics(&entries, &cs);
        assert_eq!(problem(&m, "loose-root-books").unit, MetricUnit::Files);
        assert_eq!(problem(&m, "noisy-names").unit, MetricUnit::Folders);
        assert_eq!(problem(&m, "deep-nesting").unit, MetricUnit::Folders);
        assert_eq!(
            problem(&m, "duplicate-candidate-groups").unit,
            MetricUnit::Groups
        );
        assert_eq!(problem(&m, "empty-folders").unit, MetricUnit::Folders);
    }

    /// Loose root books are counted in files with their byte total; a bracketed
    /// noisy folder name is counted; a duplicate basename+size pair is one
    /// group.
    #[test]
    fn loose_noisy_and_duplicate_counts() {
        let entries = vec![
            audio(0, None, "Sapiens by Yuval Noah Harari.m4b", 180_000),
            audio(1, None, "Atomic Habits by James Clear.m4b", 140_000),
            folder(2, None, "Cold Days [320k 18;48;03 2.52GB]"),
            audio(3, Some(2), "Cold Days.m4b", 100_000),
            // A duplicate basename+size pair across two folders.
            folder(4, None, "Original"),
            audio(5, Some(4), "Island.m4b", 120_000),
            folder(6, None, "Backup"),
            audio(7, Some(6), "Island.m4b", 120_000),
        ];
        let cs = classify(&entries);
        let m = health_metrics(&entries, &cs);
        assert_eq!(problem(&m, "loose-root-books").count, 2);
        assert_eq!(problem(&m, "loose-root-books").byte_total, 320_000);
        assert_eq!(problem(&m, "noisy-names").count, 1); // the [320k ...] folder
        assert_eq!(problem(&m, "duplicate-candidate-groups").count, 1);
        assert_eq!(
            problem(&m, "duplicate-candidate-groups").byte_total,
            240_000
        );
    }

    /// Byte attribution single-counts: the sum over classes plus loose-root
    /// bytes equals the library total.
    #[test]
    fn per_class_bytes_plus_loose_equals_total() {
        let entries = vec![
            audio(0, None, "Loose Book by An Author.m4b", 50_000),
            folder(1, None, "Genre - SciFI"),
            folder(2, Some(1), "Andy Weir - Project Hail Mary"),
            audio(3, Some(2), "Project Hail Mary.m4b", 175_000),
        ];
        let cs = classify(&entries);
        let m = health_metrics(&entries, &cs);
        let class_bytes: u64 = m.per_class.iter().map(|c| c.byte_total).sum();
        let loose = problem(&m, "loose-root-books").byte_total;
        assert_eq!(class_bytes + loose, m.total_bytes);
        assert_eq!(m.total_bytes, 225_000);
    }
}
