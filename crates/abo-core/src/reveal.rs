//! Open a folder in the OS file manager (`F-610`, v0.6.0 `P10`, `AC-47` to `AC-50`).
//!
//! # Why this is a backend command and not a frontend link
//!
//! `FD-29` grants the WebView no `fs` and no `shell` capability, and `AC-47`
//! requires the capability allowlist to stay unchanged. The frontend therefore
//! cannot open anything itself; it asks, and this module answers. That is the
//! same posture the rest of the app takes: the frontend passes a path as a plain
//! string through typed IPC and the backend owns every actual filesystem access.
//!
//! # The gate is here, not in the caller
//!
//! `AC-48` requires the command to refuse any path that is not inside the
//! library root or the Archive root. That check lives in this module rather than
//! in the Tauri command for the same reason `AC-12`'s duplicate gate was moved
//! into `abo_core`: a rule enforced by whichever caller remembers it is true for
//! exactly as long as every caller remembers it. Without the check this command
//! is a general "open any path on this machine" primitive reachable from the web
//! layer, which is precisely what `FD-29`'s minimal-capability posture exists to
//! prevent. Handing it a path is the untrusted half of the boundary.
//!
//! # It fails CLOSED
//!
//! The gate answers one question: **can this path be PROVEN to sit inside a
//! sanctioned root?** Anything that stops it proving that is a refusal, not a
//! shrug. A path that cannot be canonicalized, because it was moved or removed
//! since the scan that displayed it, is refused for the same reason a path
//! outside the library is: the answer is "I cannot show that this is allowed."
//! One outcome, one error, and `AC-48`'s "refuses rather than silently doing
//! nothing" holds for both.

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Every root a reveal is permitted to land inside.
///
/// The Archive root is included because `FD-42`'s holding area sits OUTSIDE the
/// library root by design (`FD-34`, a sibling folder), so a library-only check
/// would refuse exactly the folder a person most wants to look in after a run.
/// When settings carry no explicit Archive root, the builder's default is used,
/// which is the same path the plan builder would have written to.
fn allowed_roots(library_root: Option<&str>, set_aside_root: Option<&str>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(lib) = library_root.filter(|s| !s.is_empty()) {
        roots.push(PathBuf::from(lib));
        // An explicit override wins; otherwise the builder's own default, so this
        // gate and the executor agree about where the Archive is.
        match set_aside_root.filter(|s| !s.is_empty()) {
            Some(explicit) => roots.push(PathBuf::from(explicit)),
            None => roots.push(PathBuf::from(crate::plan::builder::default_set_aside_root(
                lib,
            ))),
        }
    } else if let Some(explicit) = set_aside_root.filter(|s| !s.is_empty()) {
        // No library configured but an Archive is: still a sanctioned root.
        roots.push(PathBuf::from(explicit));
    }
    roots
}

/// Is `candidate` the same as, or beneath, `root`?
///
/// Both sides must already be canonical. [`Path::starts_with`] is COMPONENT-wise
/// rather than textual, which is the whole reason it is used here: a textual
/// prefix test would accept `E:\Books - Audio Extra` as being inside
/// `E:\Books - Audio`, and that sibling-with-a-shared-prefix case is the classic
/// way a containment check leaks. It is covered by a test below.
fn is_inside(candidate: &Path, root: &Path) -> bool {
    candidate.starts_with(root)
}

/// Resolve `target` and prove it sits inside a sanctioned root (`AC-48`).
///
/// Canonicalization is what makes this safe rather than merely tidy: it resolves
/// `..` segments, follows symlinks and junctions, and yields one comparable form
/// for both sides. Comparing unresolved paths would accept
/// `E:\Books - Audio\..\Windows`, which is not inside the library at all.
///
/// Returns the CANONICAL path, so the caller opens the thing that was checked
/// rather than re-deriving it and opening something else.
pub fn resolve_revealable(
    target: &str,
    library_root: Option<&str>,
    set_aside_root: Option<&str>,
) -> Result<PathBuf, AppError> {
    let refused = || AppError::RevealRefused {
        path: target.to_string(),
    };

    if target.is_empty() {
        return Err(refused());
    }

    // Fail closed: a target that cannot be resolved cannot be proven safe.
    let canonical_target = std::fs::canonicalize(target).map_err(|_| refused())?;

    // A root that cannot be canonicalized does not exist on this machine, so
    // nothing can be inside it. Skipping it is correct rather than lenient; the
    // Archive folder legitimately does not exist until something is archived.
    let roots = allowed_roots(library_root, set_aside_root);
    let permitted = roots
        .iter()
        .filter_map(|r| std::fs::canonicalize(r).ok())
        .any(|root| is_inside(&canonical_target, &root));

    if permitted {
        Ok(canonical_target)
    } else {
        Err(refused())
    }
}

/// The directory a file manager should be pointed at, plus whether the original
/// target was a file worth selecting inside it.
fn folder_and_selection(canonical: &Path) -> (PathBuf, Option<PathBuf>) {
    if canonical.is_dir() {
        (canonical.to_path_buf(), None)
    } else {
        match canonical.parent() {
            Some(parent) => (parent.to_path_buf(), Some(canonical.to_path_buf())),
            None => (canonical.to_path_buf(), None),
        }
    }
}

/// Hand `canonical` to the platform file manager.
///
/// Deliberately does NOT inspect the child's exit status. `explorer.exe` is
/// documented-by-folklore to return a non-zero code on perfectly successful
/// opens, so treating its status as truth would report failure for a window the
/// user is looking at. Spawn failure (no such binary) is a real error and is
/// reported; what the file manager does afterwards is not this app's business.
///
/// No shell is involved: [`std::process::Command`] passes arguments to the
/// process directly, so a path containing spaces, quotes or ampersands is one
/// argument and cannot be reinterpreted as syntax.
fn launch(canonical: &Path) -> Result<(), AppError> {
    let (folder, selection) = folder_and_selection(canonical);
    spawn_manager(&folder, selection.as_deref()).map_err(|e| AppError::RevealFailed {
        detail: e.to_string(),
    })
}

/// Windows. `explorer.exe`, with the file selected when there is one.
///
/// Deliberately does NOT inspect the child's exit status: `explorer.exe` is
/// known to return a non-zero code on perfectly successful opens, so treating
/// its status as truth would report failure for a window the user is looking at.
/// A spawn failure is real and is reported; what the file manager does
/// afterwards is not this app's business.
///
/// No shell is involved: [`std::process::Command`] passes arguments to the
/// process directly, so a path containing spaces, quotes or ampersands is one
/// argument and cannot be reinterpreted as syntax.
#[cfg(target_os = "windows")]
fn spawn_manager(folder: &Path, selection: Option<&Path>) -> std::io::Result<()> {
    let mut cmd = std::process::Command::new("explorer.exe");
    match selection {
        // `/select,<path>` is ONE argument, comma included. Splitting it in two
        // makes explorer ignore both and open Documents instead, which reads as
        // the feature being broken rather than as a bug.
        Some(file) => {
            cmd.arg(format!("/select,{}", strip_verbatim(file).display()));
        }
        None => {
            cmd.arg(strip_verbatim(folder));
        }
    }
    cmd.spawn().map(|_| ())
}

/// macOS. Compiles-in-CI honesty rather than a supported target.
#[cfg(target_os = "macos")]
fn spawn_manager(folder: &Path, selection: Option<&Path>) -> std::io::Result<()> {
    let mut cmd = std::process::Command::new("open");
    match selection {
        Some(file) => {
            cmd.arg("-R").arg(file);
        }
        None => {
            cmd.arg(folder);
        }
    }
    cmd.spawn().map(|_| ())
}

/// Everything else. No Linux bundle ships, but `cargo clippy --workspace
/// --all-targets` builds this crate on the Ubuntu runner, so the arm has to
/// exist and has to be real rather than a `todo!()`.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn spawn_manager(folder: &Path, _selection: Option<&Path>) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(folder)
        .spawn()
        .map(|_| ())
}

/// Strip Windows' `\\?\` verbatim prefix, which `canonicalize` adds and
/// `explorer.exe` does not understand: handed a verbatim path, explorer opens
/// the user's Documents folder rather than reporting a problem, which would make
/// this feature look broken in the most confusing possible way.
#[cfg(target_os = "windows")]
fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => p.to_path_buf(),
    }
}

/// One of the two permanent destinations the sidebar links to (`AC-50`).
///
/// A NAME rather than a path, deliberately. The Archive root is usually not set
/// in settings: the plan builder derives it, and it sits outside the library by
/// design (`FD-34`). A frontend quick link that had to construct that path would
/// be a second implementation of a rule the builder already owns, and the two
/// would drift. Naming the root instead means no path crosses the IPC boundary
/// for these links at all, so there is nothing for the gate to second-guess and
/// nothing for the frontend to get wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum RevealRoot {
    /// The configured library root.
    Library,
    /// The Archive root: the settings override if there is one, otherwise the
    /// same default the plan builder would archive into.
    Archive,
}

/// Resolve one of the two well-known roots to a path, or `None` when it is not
/// configured (no library has been chosen yet).
pub fn root_path(
    which: RevealRoot,
    library_root: Option<&str>,
    set_aside_root: Option<&str>,
) -> Option<String> {
    let library = library_root.filter(|s| !s.is_empty());
    match which {
        RevealRoot::Library => library.map(|s| s.to_string()),
        RevealRoot::Archive => match set_aside_root.filter(|s| !s.is_empty()) {
            Some(explicit) => Some(explicit.to_string()),
            None => library.map(crate::plan::builder::default_set_aside_root),
        },
    }
}

/// Open one of the two well-known roots (`AC-50`).
///
/// Still goes through the same gate as any other reveal. That is not ceremony:
/// it means an Archive folder that has never been created yet is refused with
/// the ordinary message rather than handed to the OS as a path that is not
/// there.
pub fn reveal_root(
    which: RevealRoot,
    library_root: Option<&str>,
    set_aside_root: Option<&str>,
) -> Result<(), AppError> {
    let target = root_path(which, library_root, set_aside_root).ok_or(AppError::RevealRefused {
        path: String::new(),
    })?;
    reveal_in_file_manager(&target, library_root, set_aside_root)
}

/// Open the OS file manager at `target` (`AC-47`), refusing anything outside the
/// sanctioned roots (`AC-48`).
///
/// A file target opens its containing folder with the file selected, because
/// `AC-49` puts this affordance wherever a path is displayed and many of those
/// paths are files: a duplicate copy is a file, and opening its folder without
/// pointing at it leaves the person to find it again by eye.
pub fn reveal_in_file_manager(
    target: &str,
    library_root: Option<&str>,
    set_aside_root: Option<&str>,
) -> Result<(), AppError> {
    let canonical = resolve_revealable(target, library_root, set_aside_root)?;
    launch(&canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate is tested, the spawn is not. Every test here drives
    /// [`resolve_revealable`], which is the half that decides whether a path may
    /// be opened at all; `launch` hands a proven-safe path to the OS and has no
    /// branch worth asserting that does not require a desktop session.
    use std::fs;
    use tempfile::TempDir;

    struct Fixture {
        _tmp: TempDir,
        library: PathBuf,
        archive: PathBuf,
        outside: PathBuf,
    }

    /// A library, a sibling Archive (FD-34 places it OUTSIDE the library), and an
    /// unrelated folder, all real on disk because canonicalization is the thing
    /// under test and it needs real directories.
    fn fixture() -> Fixture {
        let tmp = TempDir::new().expect("tempdir");
        let base = tmp.path();
        let library = base.join("Books - Audio");
        let archive = base.join("Audiobook Archive");
        let outside = base.join("Secrets");
        for d in [&library, &archive, &outside] {
            fs::create_dir_all(d).expect("create fixture dir");
        }
        fs::create_dir_all(library.join("Andy Weir")).expect("create book dir");
        fs::write(library.join("Andy Weir").join("book.m4b"), b"x").expect("write book");
        Fixture {
            _tmp: tmp,
            library,
            archive,
            outside,
        }
    }

    fn resolve(f: &Fixture, target: &Path) -> Result<PathBuf, AppError> {
        resolve_revealable(
            &target.to_string_lossy(),
            Some(&f.library.to_string_lossy()),
            Some(&f.archive.to_string_lossy()),
        )
    }

    #[test]
    fn the_library_root_itself_is_allowed() {
        let f = fixture();
        assert!(resolve(&f, &f.library).is_ok());
    }

    #[test]
    fn a_folder_inside_the_library_is_allowed() {
        let f = fixture();
        assert!(resolve(&f, &f.library.join("Andy Weir")).is_ok());
    }

    #[test]
    fn a_file_inside_the_library_is_allowed() {
        let f = fixture();
        assert!(resolve(&f, &f.library.join("Andy Weir").join("book.m4b")).is_ok());
    }

    /// FD-34 puts the Archive outside the library, so a library-only check would
    /// refuse the folder a person most wants to look in after a run.
    #[test]
    fn the_archive_root_is_allowed_even_though_it_is_outside_the_library() {
        let f = fixture();
        assert!(resolve(&f, &f.archive).is_ok());
    }

    #[test]
    fn an_unrelated_folder_is_refused() {
        let f = fixture();
        assert!(matches!(
            resolve(&f, &f.outside),
            Err(AppError::RevealRefused { .. })
        ));
    }

    /// THE CLASSIC LEAK. A textual prefix test accepts this; a component-wise one
    /// does not. `Books - Audio Extra` shares every character of the library
    /// root's name and is a different folder entirely.
    #[test]
    fn a_sibling_sharing_the_roots_name_prefix_is_refused() {
        let f = fixture();
        let sneaky = f.library.with_file_name("Books - Audio Extra");
        fs::create_dir_all(&sneaky).expect("create sibling");
        assert!(
            matches!(resolve(&f, &sneaky), Err(AppError::RevealRefused { .. })),
            "a textual starts_with would have accepted this"
        );
    }

    /// Canonicalization is what defeats this: the unresolved string begins with
    /// the library root, and the folder it names does not sit inside it.
    #[test]
    fn a_parent_traversal_out_of_the_library_is_refused() {
        let f = fixture();
        let escape = f.library.join("..").join("Secrets");
        assert!(
            matches!(resolve(&f, &escape), Err(AppError::RevealRefused { .. })),
            "`..` must be resolved before the containment test, not after"
        );
    }

    /// Fails CLOSED. Paths on this surface come from a scan, and the library can
    /// change under it, so this is a live case rather than a hypothetical.
    #[test]
    fn a_path_that_no_longer_exists_is_refused() {
        let f = fixture();
        assert!(matches!(
            resolve(&f, &f.library.join("gone")),
            Err(AppError::RevealRefused { .. })
        ));
    }

    #[test]
    fn an_empty_target_is_refused() {
        let f = fixture();
        assert!(matches!(
            resolve_revealable("", Some(&f.library.to_string_lossy()), None),
            Err(AppError::RevealRefused { .. })
        ));
    }

    /// With nothing configured there is no sanctioned root, so there is nothing
    /// to be inside of. The command must not degrade into "open anything".
    #[test]
    fn with_no_roots_configured_everything_is_refused() {
        let f = fixture();
        assert!(matches!(
            resolve_revealable(&f.library.to_string_lossy(), None, None),
            Err(AppError::RevealRefused { .. })
        ));
    }

    /// The Archive root defaults to the builder's own answer when settings carry
    /// no override, so this gate and the executor agree about where it is.
    #[test]
    fn the_default_archive_root_is_allowed_without_an_explicit_setting() {
        let f = fixture();
        let derived = crate::plan::builder::default_set_aside_root(&f.library.to_string_lossy());
        assert_eq!(
            PathBuf::from(&derived),
            f.archive,
            "fixture precondition: the sibling Archive IS the builder's default"
        );
        assert!(resolve_revealable(
            &f.archive.to_string_lossy(),
            Some(&f.library.to_string_lossy()),
            None,
        )
        .is_ok());
    }

    /// A resolved path is returned so the caller opens what was checked. Handing
    /// back the raw input would let a `..` segment survive into the spawn.
    #[test]
    fn the_canonical_path_is_returned_not_the_input() {
        let f = fixture();
        let indirect = f.library.join("Andy Weir").join("..").join("Andy Weir");
        let resolved = resolve(&f, &indirect).expect("inside the library");
        assert!(
            !resolved.to_string_lossy().contains(".."),
            "the returned path still carries a traversal segment: {}",
            resolved.display()
        );
    }

    #[test]
    fn a_file_target_points_the_manager_at_its_folder_and_selects_it() {
        let f = fixture();
        let file = f.library.join("Andy Weir").join("book.m4b");
        let canonical = resolve(&f, &file).expect("inside the library");
        let (folder, selection) = folder_and_selection(&canonical);
        assert!(folder.is_dir(), "a file target opens its containing folder");
        assert_eq!(selection.as_deref(), Some(canonical.as_path()));
    }

    #[test]
    fn the_archive_root_name_resolves_to_the_builders_default_when_unset() {
        let f = fixture();
        let lib = f.library.to_string_lossy().to_string();
        assert_eq!(
            root_path(RevealRoot::Archive, Some(&lib), None).map(PathBuf::from),
            Some(f.archive.clone()),
            "an unset Archive root must resolve the same way the plan builder does"
        );
    }

    #[test]
    fn an_explicit_archive_root_setting_wins_over_the_default() {
        let f = fixture();
        let lib = f.library.to_string_lossy().to_string();
        let override_root = f.outside.to_string_lossy().to_string();
        assert_eq!(
            root_path(RevealRoot::Archive, Some(&lib), Some(&override_root)),
            Some(override_root.clone())
        );
    }

    #[test]
    fn the_library_root_name_resolves_to_the_configured_root() {
        let f = fixture();
        let lib = f.library.to_string_lossy().to_string();
        assert_eq!(root_path(RevealRoot::Library, Some(&lib), None), Some(lib));
    }

    /// Before first-run there is no library, so neither link has a destination.
    /// `None` rather than a guess is what lets the sidebar hide them.
    #[test]
    fn neither_root_resolves_before_a_library_is_configured() {
        assert_eq!(root_path(RevealRoot::Library, None, None), None);
        assert_eq!(root_path(RevealRoot::Archive, None, None), None);
    }

    #[test]
    fn a_folder_target_is_opened_with_nothing_selected() {
        let f = fixture();
        let canonical = resolve(&f, &f.library).expect("the library root");
        let (folder, selection) = folder_and_selection(&canonical);
        assert_eq!(folder, canonical);
        assert!(selection.is_none());
    }
}
