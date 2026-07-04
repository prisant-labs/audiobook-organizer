//! Proves the `fixtures` feature-unification story works from an
//! **integration** test (a separate compilation unit from the library's own
//! `cfg(test)` unit tests), which is how Phases 2-5 will actually consume
//! `GeneratedLibrary` (matching the existing `tests/scan.rs`, `tests/db.rs`
//! convention in this crate). If the self dev-dependency in `Cargo.toml`
//! ever stops unifying the `fixtures` feature in for `cargo test -p
//! abo-core`, this file fails to compile, which is the loudest possible
//! signal.

use abo_core::fixtures::{generate, standard_library_manifest, FixtureFamily};
use tempfile::TempDir;

#[test]
fn generated_library_is_usable_from_an_integration_test() {
    let manifest = standard_library_manifest();
    let dir = TempDir::new().expect("tempdir");
    let lib = generate(&manifest, dir.path()).expect("generate");

    assert!(!lib.entries.is_empty());
    assert!(lib.family_recorded(FixtureFamily::MultiBookNarnia));
    assert!(lib.family_recorded(FixtureFamily::MultiBookHarryPotter));
}
