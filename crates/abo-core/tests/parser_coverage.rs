//! Integration test: the F-301 parser-coverage metric (v0.2.0 implementation
//! plan Phase 4 step 5; AC-301.4), measured over the real P1 fixture library
//! (`crates/abo-core/src/fixtures/manifest.rs`), not a hand-picked subset.
//! Feeds the P7 freeze decision (release gate G-05): if coverage falls below
//! ~90%, the pattern set is frozen and the remainder routes to
//! `manual-review` by design, not as a failure.
//!
//! Run with `cargo test -p abo-core --test parser_coverage -- --nocapture`
//! to see the printed coverage number and the non-parsing name list.

use abo_core::fixtures::{FixtureFamily, FixtureManifest, FixtureNode};
use abo_core::parse::coverage;

/// Every family that exemplifies one of the 9 F-301 patterns (including the
/// deliberate non-clean outlier, which is SUPPOSED to fail the coverage
/// check, per AC-301.3's 237/238 baseline).
const PATTERN_FAMILIES: &[FixtureFamily] = &[
    FixtureFamily::Pattern1TitleByAuthorLooseRoot,
    FixtureFamily::Pattern1NonCleanOutlier,
    FixtureFamily::Pattern2AuthorDashTitle,
    FixtureFamily::Pattern3TitleByAuthorFolder,
    FixtureFamily::Pattern4YearAuthorTitleAward,
    FixtureFamily::Pattern5RankTitleAuthorYear,
    FixtureFamily::Pattern6SeriesIndexTitle,
    FixtureFamily::Pattern7Underscored,
    FixtureFamily::Pattern8BareTitle,
    FixtureFamily::Pattern9IrregularSeriesContainer,
];

fn is_pattern_family(families: &[FixtureFamily]) -> bool {
    families.iter().any(|f| PATTERN_FAMILIES.contains(f))
}

/// Walk the manifest and collect the raw name of every node tagged with one
/// of the 9 pattern families. This is deliberately the manifest's own
/// `name` field (a single folder/file name component), matching what
/// `abo_core::parse::matchers` operates on.
fn collect_pattern_names(manifest: &FixtureManifest) -> Vec<String> {
    fn walk(node: &FixtureNode, out: &mut Vec<String>) {
        match node {
            FixtureNode::Folder(f) => {
                if is_pattern_family(&f.families) {
                    out.push(f.name.clone());
                }
                for child in &f.children {
                    walk(child, out);
                }
            }
            FixtureNode::File(file) => {
                if is_pattern_family(&file.families) {
                    out.push(file.name.clone());
                }
            }
        }
    }
    let mut names = Vec::new();
    for node in &manifest.top_level {
        walk(node, &mut names);
    }
    names
}

#[test]
fn parser_coverage_over_the_fixture_library_meets_the_ac_301_4_threshold() {
    let manifest = abo_core::fixtures::standard_library_manifest();
    let names = collect_pattern_names(&manifest);
    assert!(
        !names.is_empty(),
        "expected at least one Pattern-family fixture name in the manifest"
    );

    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let report = coverage(name_refs);

    println!(
        "F-301 parser coverage over the P1 fixture library: {}/{} names parsed cleanly ({:.1}%)",
        report.clean,
        report.total,
        report.fraction() * 100.0,
    );
    if !report.non_parsing.is_empty() {
        println!("Non-parsing names (fall through to manual-review by design):");
        for name in &report.non_parsing {
            println!("  - {name}");
        }
    }

    // AC-301.4 / release gate G-05: below ~90%, the pattern set is frozen
    // and the remainder routes to manual-review (a recorded decision, not
    // a failure). This assertion is the mechanical trip-wire for that
    // decision gate.
    assert!(
        report.fraction() >= 0.90,
        "parser coverage {:.1}% fell below the ~90% AC-301.4 freeze threshold \
         ({}/{} clean); non-parsing names: {:?}",
        report.fraction() * 100.0,
        report.clean,
        report.total,
        report.non_parsing,
    );
}
