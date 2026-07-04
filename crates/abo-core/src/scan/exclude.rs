//! Exclude-pattern matching for the scanner (F-101, the minimal ruleset scope
//! for v0.2.0).
//!
//! The scan entry point takes a plain `&[String]` of glob patterns (defaulting
//! empty); this module compiles them once into a [`globset::GlobSet`] and tests
//! each walked entry's path against it. The full ruleset model (excluded roots,
//! toggles, per-rule metadata) is F-801 in v0.3.0; here we implement only the
//! exclude-glob parameter the spec's F-101 section calls for
//! ("excluded roots and glob patterns come from the ruleset").
//!
//! Patterns are matched against the entry path RELATIVE to the scan root,
//! normalized to forward slashes so a pattern is separator-independent (it reads
//! the same whether the scan runs on Windows or Linux). Standard glob syntax
//! applies: `*` matches within a path component, `**` matches across component
//! boundaries, `?` a single character, `[...]` a class. So `**/Bonus` excludes
//! every `Bonus` folder at any depth, while a bare `Bonus` excludes only a
//! top-level one. When an excluded entry is a directory, its whole subtree is
//! pruned (the walk does not descend into it).

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

/// A compiled set of exclude globs. [`ExcludeSet::empty`] matches nothing and is
/// the default; [`ExcludeSet::compile`] builds one from caller-supplied strings.
#[derive(Debug, Clone, Default)]
pub struct ExcludeSet {
    /// `None` when there are no patterns (the common default), so the match hot
    /// path is a single `is_none` check with zero globset work.
    set: Option<GlobSet>,
}

impl ExcludeSet {
    /// An exclude set that matches nothing.
    pub fn empty() -> Self {
        Self { set: None }
    }

    /// Compile `patterns` into a matcher. An empty slice yields [`Self::empty`].
    ///
    /// Returns `Err(detail)` with a human-readable message if any pattern is not
    /// a valid glob; the caller maps that to [`crate::error::AppError::ScanFailed`]
    /// (a bad exclude pattern is a caller/config error, not a per-entry runtime
    /// condition, so it fails the scan before any snapshot work begins).
    pub fn compile(patterns: &[String]) -> Result<Self, String> {
        if patterns.is_empty() {
            return Ok(Self::empty());
        }
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern)
                .map_err(|e| format!("invalid exclude pattern {pattern:?}: {e}"))?;
            builder.add(glob);
        }
        let set = builder
            .build()
            .map_err(|e| format!("could not build the exclude set: {e}"))?;
        Ok(Self { set: Some(set) })
    }

    /// Whether `full_path` (an entry's path during the walk) is excluded, judged
    /// by its path relative to `root`. `root` and `full_path` must be in the same
    /// form (both extended-length during a real walk); a path not under `root`,
    /// or the root itself, is never excluded.
    pub fn is_excluded(&self, root: &Path, full_path: &Path) -> bool {
        let Some(set) = &self.set else {
            return false;
        };
        let Ok(relative) = full_path.strip_prefix(root) else {
            return false;
        };
        // The relative path of the root itself is empty; never exclude the root.
        let normalized = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if normalized.is_empty() {
            return false;
        }
        set.is_match(&normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn empty_set_excludes_nothing() {
        let set = ExcludeSet::empty();
        let root = Path::new("/root");
        assert!(!set.is_excluded(root, &root.join("anything")));
    }

    #[test]
    fn compile_empty_slice_is_empty_set() {
        let set = ExcludeSet::compile(&[]).expect("empty compiles");
        let root = Path::new("/root");
        assert!(!set.is_excluded(root, &root.join("x")));
    }

    #[test]
    fn bad_pattern_is_reported_not_panicked() {
        // An unclosed class is an invalid glob.
        let result = ExcludeSet::compile(&["[".to_string()]);
        assert!(
            result.is_err(),
            "an invalid glob must be an Err, got {result:?}"
        );
    }

    #[test]
    fn top_level_name_matches_only_top_level() {
        let set = ExcludeSet::compile(&["Bonus".to_string()]).expect("compile");
        let root = PathBuf::from("/root");
        assert!(set.is_excluded(&root, &root.join("Bonus")));
        // A nested Bonus is NOT matched by the bare top-level pattern.
        assert!(!set.is_excluded(&root, &root.join("Book").join("Bonus")));
    }

    #[test]
    fn double_star_matches_at_any_depth() {
        let set = ExcludeSet::compile(&["**/Bonus".to_string()]).expect("compile");
        let root = PathBuf::from("/root");
        assert!(set.is_excluded(&root, &root.join("Book").join("Bonus")));
        assert!(set.is_excluded(&root, &root.join("a").join("b").join("Bonus")));
    }

    #[test]
    fn extension_glob_matches() {
        let set = ExcludeSet::compile(&["**/*.tmp".to_string()]).expect("compile");
        let root = PathBuf::from("/root");
        assert!(set.is_excluded(&root, &root.join("x").join("scratch.tmp")));
        assert!(!set.is_excluded(&root, &root.join("x").join("keep.m4b")));
    }

    #[test]
    fn root_itself_is_never_excluded() {
        let set = ExcludeSet::compile(&["**".to_string()]).expect("compile");
        let root = PathBuf::from("/root");
        // `**` would match everything relative, but the root's own relative path
        // is empty and is explicitly never excluded.
        assert!(!set.is_excluded(&root, &root));
        assert!(set.is_excluded(&root, &root.join("child")));
    }

    #[test]
    fn path_outside_root_is_not_excluded() {
        let set = ExcludeSet::compile(&["**".to_string()]).expect("compile");
        let root = PathBuf::from("/root");
        assert!(!set.is_excluded(&root, Path::new("/elsewhere/file")));
    }
}
