//! The virtual-filesystem seam (F-607, dry-run harness): one trait, two
//! implementations.
//!
//! The whole v0.5.0 executor is generic over [`Vfs`], so the SAME operation code
//! path serves a dry run (against [`MemFs`], an in-memory tree seeded from the
//! plan's snapshot) and a Real apply (against [`RealFs`], the actual disk). That
//! is what makes the dry run a first-class product rather than a separate
//! simulation, and it is what lets every later safety test run against memory
//! without ever touching a real path (AC-1, AC-2, R-1).
//!
//! This file is also the ONE place the real filesystem is touched: [`RealFs`] is
//! the single implementation that calls the standard-library filesystem API, and
//! it routes every path through the [`crate::paths`] seam's
//! [`to_extended_length_prefixed`](crate::paths::to_extended_length_prefixed) so
//! Windows extended-length (`\\?\`) handling lives here, not scattered through the
//! executor (FD-19). The executor module itself makes no direct filesystem call;
//! a unit test greps its source to keep that true (AC-1).
//!
//! Never-overwrite is built into the seam (R-3, AC-7): [`Vfs::rename`] and
//! [`Vfs::copy_file`] refuse a destination that already exists, in BOTH
//! implementations, rather than relying on the caller to check first. The
//! executor's TOCTOU re-checks are the primary gate; this is defense in depth at
//! the lowest level, so no code path can silently clobber a target even by mistake.
//!
//! On [`RealFs`] the refusal is ATOMIC at the primitive, not just a pre-check, so a
//! target that RACES in between the check and the OS call is still never
//! overwritten (D-09: the write path never opens a target with truncate/overwrite
//! semantics):
//! - `rename` on Windows uses `MoveFileExW` WITHOUT `MOVEFILE_REPLACE_EXISTING`,
//!   which fails with `ERROR_ALREADY_EXISTS` (files and directories alike) rather
//!   than replacing - unlike `std::fs::rename`, which replaces on both Windows and
//!   Unix. (On non-Windows the pre-check plus `std::fs::rename` is kept for the
//!   structured error contract, with a documented residual race - this product is
//!   Windows-first; macOS/Linux are compiles-only honesty.)
//! - `copy_file` opens the destination with `create_new(true)` on every platform,
//!   which fails with `AlreadyExists` rather than truncating - unlike
//!   `std::fs::copy`. Because a successful `create_new` means the dest is OURS, a
//!   copy error mid-stream removes our own partial dest before returning, leaving
//!   no stray partial; a foreign file at the target is never touched.
//!
//! The pre-checks are kept on all platforms so the structured [`VfsError`] variants
//! stay uniform with [`MemFs`]; the atomic primitive is what closes the race.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::paths::to_extended_length_prefixed;

/// Size and kind of one filesystem node, the shape [`Vfs::metadata`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfsMetadata {
    /// Size in bytes (0 for a directory).
    pub size: u64,
    /// Whether the node is a directory.
    pub is_dir: bool,
}

/// A failure from a [`Vfs`] operation.
///
/// The structured variants ([`NotFound`](VfsError::NotFound),
/// [`AlreadyExists`](VfsError::AlreadyExists), ...) are what [`MemFs`] produces
/// and what the executor's later phases map onto the FD-19 error taxonomy
/// (`source-vanished`, `target-appeared`, and so on). [`Io`](VfsError::Io) is the
/// [`RealFs`] passthrough for an underlying standard-library failure (an
/// access-denied, a full disk); a later phase inspects its
/// [`std::io::ErrorKind`] to route access-denied to its retry-once-then-halt
/// handling.
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    /// The path does not exist.
    #[error("path not found: {0}")]
    NotFound(PathBuf),
    /// The destination already exists (the never-overwrite guard, R-3).
    #[error("path already exists: {0}")]
    AlreadyExists(PathBuf),
    /// A path expected to be a directory was a file.
    #[error("expected a directory: {0}")]
    NotADirectory(PathBuf),
    /// A path expected to be a file was a directory.
    #[error("expected a file: {0}")]
    NotAFile(PathBuf),
    /// A directory could not be removed because it still has children.
    #[error("directory not empty: {0}")]
    DirectoryNotEmpty(PathBuf),
    /// An underlying standard-library filesystem failure from [`RealFs`].
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

/// The filesystem seam every executor operation goes through (F-607).
///
/// Methods take `&self`: [`MemFs`] carries its own interior mutability so a
/// single shared instance can be read and mutated during a walk, and [`RealFs`]
/// is stateless. The mutating methods ([`rename`](Vfs::rename),
/// [`copy_file`](Vfs::copy_file), the `remove_*` and `create_dir_all`) are the
/// only ways the executor changes anything; there is deliberately no
/// open-for-write or truncate primitive, so "overwrite" is not expressible.
///
/// # Uniform error contract (dry-run == Real)
///
/// The whole point of the seam is that a dry run and a Real apply behave
/// identically, so the STRUCTURED [`VfsError`] variants are GUARANTEED to match
/// across [`MemFs`] and [`RealFs`] for these cases (a later phase maps them onto
/// the FD-19 error taxonomy, e.g. [`VfsError::NotFound`] -> `source-vanished`):
///
/// - a missing `from`/`path` yields [`VfsError::NotFound`]
///   ([`rename`](Vfs::rename), [`copy_file`](Vfs::copy_file),
///   [`metadata`](Vfs::metadata), [`remove_file`](Vfs::remove_file),
///   [`remove_dir`](Vfs::remove_dir));
/// - an existing `to` yields [`VfsError::AlreadyExists`]
///   ([`rename`](Vfs::rename), [`copy_file`](Vfs::copy_file)), never-overwrite;
/// - a wrong-kind target yields [`VfsError::NotAFile`] (a directory where a file
///   was expected: [`copy_file`](Vfs::copy_file), [`remove_file`](Vfs::remove_file))
///   or [`VfsError::NotADirectory`] (a file where a directory was expected:
///   [`remove_dir`](Vfs::remove_dir));
/// - a non-empty directory yields [`VfsError::DirectoryNotEmpty`]
///   ([`remove_dir`](Vfs::remove_dir)).
///
/// [`VfsError::Io`] is [`RealFs`]-only and reserved for a genuine platform failure
/// with no structured counterpart (an access-denied, a full disk); [`MemFs`] never
/// produces it. The both-backends contract test locks the guaranteed cases above.
/// One thing the seam does NOT equalize: a target whose PARENT directory is
/// missing (a `RealFs` rename/copy would fail, `MemFs` would not model it) - the
/// executor always creates parents (mkdir-first) before a move, so that case does
/// not arise in a real walk and is not part of the uniform contract.
pub trait Vfs {
    /// Whether `path` exists.
    fn exists(&self, path: &Path) -> bool;

    /// Whether `path` exists and is a directory.
    fn is_dir(&self, path: &Path) -> bool;

    /// The size and kind of `path`, or [`VfsError::NotFound`] if it is absent.
    fn metadata(&self, path: &Path) -> Result<VfsMetadata, VfsError>;

    /// Move `from` to `to` (metadata-only where the platform allows, R-2).
    /// Refuses if `from` is absent ([`VfsError::NotFound`]) or `to` already
    /// exists ([`VfsError::AlreadyExists`], never-overwrite).
    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError>;

    /// Copy the file `from` to `to`, returning the bytes copied. Refuses if
    /// `from` is absent or not a file, or if `to` already exists (never-
    /// overwrite). The cross-volume `copy + verify + delete` path (R-2) composes
    /// this with [`metadata`](Vfs::metadata) and [`remove_file`](Vfs::remove_file)
    /// in a later phase.
    fn copy_file(&self, from: &Path, to: &Path) -> Result<u64, VfsError>;

    /// Remove the file `path`. Refuses a directory ([`VfsError::NotAFile`]).
    fn remove_file(&self, path: &Path) -> Result<(), VfsError>;

    /// Remove the empty directory `path`. Refuses a file
    /// ([`VfsError::NotADirectory`]) or a non-empty directory
    /// ([`VfsError::DirectoryNotEmpty`]).
    fn remove_dir(&self, path: &Path) -> Result<(), VfsError>;

    /// Create `path` and any missing ancestor directories (idempotent).
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError>;
}

// ---- RealFs: the one implementation that touches the real filesystem --------

/// The production [`Vfs`]: delegates to the standard-library filesystem API,
/// routing every path through [`to_extended_length_prefixed`] so operations open
/// past the legacy 260-char `MAX_PATH` limit on Windows (FD-19). Stateless.
///
/// NOTE (D-10, this release): no `apply_start` in v0.5.0 Phase 1 runs the
/// executor against `RealFs` - a Real apply is refused at the command boundary
/// with `apply-not-supported` while the operation logic is a skeleton. `RealFs`
/// is fully implemented and unit-tested here (in a temp dir) so the seam is real
/// and the later executor phases have a proven backend to switch on.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealFs;

impl RealFs {
    /// A fresh, stateless `RealFs`.
    pub fn new() -> Self {
        RealFs
    }
}

/// Translate a standard-library filesystem error into the seam's structured
/// contract: a not-found becomes [`VfsError::NotFound`] (so it matches [`MemFs`]
/// and a later phase can map it to `source-vanished`), and every other kind is a
/// genuine platform failure carried as [`VfsError::Io`]. `path` is the operand the
/// error concerns, used only to fill the `NotFound` payload.
fn map_io(path: &Path, err: std::io::Error) -> VfsError {
    match err.kind() {
        std::io::ErrorKind::NotFound => VfsError::NotFound(path.to_path_buf()),
        _ => VfsError::Io(err),
    }
}

/// Move `from` to `to` WITHOUT replacing an existing target, at the primitive.
///
/// Windows: `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`, so an existing
/// target (file or directory) makes the move fail with `ERROR_ALREADY_EXISTS`
/// rather than being overwritten - the atomic never-overwrite the seam promises
/// (D-09). Both paths are already `\\?\`-prefixed by the caller.
#[cfg(windows)]
fn real_rename_no_replace(from: &Path, to: &Path) -> Result<(), VfsError> {
    move_file_no_replace(
        &to_extended_length_prefixed(from),
        &to_extended_length_prefixed(to),
    )
    .map_err(|e| map_move_error(from, to, e))
}

/// Non-Windows counterpart (compiles-only honesty): pre-check plus
/// `std::fs::rename`. RESIDUAL RACE: `std::fs::rename` REPLACES an existing target
/// on Unix, so a target that appears between the caller's pre-check and this call
/// would be replaced. This product is Windows-first; the Windows path closes the
/// window with `MoveFileExW`'s no-replace semantics, and macOS/Linux carry no
/// behavioral apply claim in v0.5.0. The pre-check is still the structured-contract
/// gate on every platform.
#[cfg(not(windows))]
fn real_rename_no_replace(from: &Path, to: &Path) -> Result<(), VfsError> {
    std::fs::rename(
        to_extended_length_prefixed(from),
        to_extended_length_prefixed(to),
    )
    .map_err(|e| map_io(from, e))
}

/// Call `MoveFileExW(from, to, 0)`: move with NO `MOVEFILE_REPLACE_EXISTING` (never
/// clobber) and NO `MOVEFILE_COPY_ALLOWED` (same-volume only; the executor routes
/// cross-volume to copy+verify+delete before this is ever reached). Returns the OS
/// error on failure. Follows the crate's existing raw-FFI convention (see
/// [`crate::paths::available_bytes`]'s `GetDiskFreeSpaceExW`), so it adds NO
/// windows-sys/winapi dependency: `kernel32` is already linked by the standard
/// library on the Windows target.
#[cfg(windows)]
fn move_file_no_replace(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> i32;
    }

    let wide_from: Vec<u16> = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let wide_to: Vec<u16> = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: both buffers are valid NUL-terminated UTF-16 that outlive the call;
    // MoveFileExW reads only the two string pointers and the flags word.
    let ok = unsafe { MoveFileExW(wide_from.as_ptr(), wide_to.as_ptr(), 0) };
    if ok != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Map a `MoveFileExW` OS error onto the structured contract: a present target
/// (`ERROR_ALREADY_EXISTS`) is `AlreadyExists` (never-overwrite), a missing
/// source/path is `NotFound`, everything else is the platform passthrough.
#[cfg(windows)]
fn map_move_error(from: &Path, to: &Path, err: std::io::Error) -> VfsError {
    // Win32 error codes (winerror.h); hardcoded like the crate's other raw-FFI
    // sites so no windows-sys dependency is added.
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PATH_NOT_FOUND: i32 = 3;
    const ERROR_ALREADY_EXISTS: i32 = 183;
    match err.raw_os_error() {
        Some(ERROR_ALREADY_EXISTS) => VfsError::AlreadyExists(to.to_path_buf()),
        Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND) => {
            VfsError::NotFound(from.to_path_buf())
        }
        _ => map_io(from, err),
    }
}

impl Vfs for RealFs {
    fn exists(&self, path: &Path) -> bool {
        std::fs::metadata(to_extended_length_prefixed(path)).is_ok()
    }

    fn is_dir(&self, path: &Path) -> bool {
        std::fs::metadata(to_extended_length_prefixed(path))
            .map(|m| m.is_dir())
            .unwrap_or(false)
    }

    fn metadata(&self, path: &Path) -> Result<VfsMetadata, VfsError> {
        let m =
            std::fs::metadata(to_extended_length_prefixed(path)).map_err(|e| map_io(path, e))?;
        Ok(VfsMetadata {
            size: m.len(),
            is_dir: m.is_dir(),
        })
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        // Uniform contract (matching MemFs): missing source -> NotFound, present
        // target -> AlreadyExists. The pre-checks give the structured contract; the
        // ATOMIC no-clobber primitive below is what actually closes the race (a
        // target that appears after this check is still never overwritten).
        if !self.exists(from) {
            return Err(VfsError::NotFound(from.to_path_buf()));
        }
        if self.exists(to) {
            return Err(VfsError::AlreadyExists(to.to_path_buf()));
        }
        // No-replace move at the primitive (Windows: MoveFileExW without
        // MOVEFILE_REPLACE_EXISTING; non-Windows: std::fs::rename, documented race).
        // A residual NotFound (source vanished after the check) still maps to
        // NotFound, and a raced-in target maps to AlreadyExists, keeping the
        // contract uniform even under a race.
        real_rename_no_replace(from, to)
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<u64, VfsError> {
        use std::io::Write;

        // Uniform contract (matching MemFs): missing source -> NotFound, a source
        // that is a directory -> NotAFile.
        match std::fs::metadata(to_extended_length_prefixed(from)) {
            Ok(m) if m.is_dir() => return Err(VfsError::NotAFile(from.to_path_buf())),
            Ok(_) => {}
            Err(e) => return Err(map_io(from, e)),
        }
        let mut src =
            std::fs::File::open(to_extended_length_prefixed(from)).map_err(|e| map_io(from, e))?;

        // ATOMIC no-clobber: create_new(true) fails with AlreadyExists if the target
        // exists, so a raced-in target is NEVER truncated (unlike std::fs::copy,
        // R-3). A successful open means the dest is OURS, so a later failure can
        // safely remove it.
        let mut dst = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to_extended_length_prefixed(to))
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(VfsError::AlreadyExists(to.to_path_buf()));
            }
            Err(e) => return Err(map_io(to, e)),
        };

        // Copy the bytes, then flush + fsync so the executor's post-copy verify reads
        // durable data. On ANY failure remove our own partial dest before returning,
        // so no stray partial is left behind (the dest is ours - create_new made it).
        let result = std::io::copy(&mut src, &mut dst).and_then(|n| {
            dst.flush()?;
            dst.sync_all()?;
            Ok(n)
        });
        match result {
            Ok(copied) => Ok(copied),
            Err(e) => {
                drop(dst);
                let _ = std::fs::remove_file(to_extended_length_prefixed(to));
                Err(map_io(to, e))
            }
        }
    }

    fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
        // Uniform contract (matching MemFs): missing -> NotFound, a directory ->
        // NotAFile.
        match std::fs::metadata(to_extended_length_prefixed(path)) {
            Ok(m) if m.is_dir() => return Err(VfsError::NotAFile(path.to_path_buf())),
            Ok(_) => {}
            Err(e) => return Err(map_io(path, e)),
        }
        std::fs::remove_file(to_extended_length_prefixed(path)).map_err(|e| map_io(path, e))?;
        Ok(())
    }

    fn remove_dir(&self, path: &Path) -> Result<(), VfsError> {
        // Uniform contract (matching MemFs): missing -> NotFound, a file ->
        // NotADirectory, a non-empty directory -> DirectoryNotEmpty. The last is
        // pre-checked by reading the directory because the matching io ErrorKind is
        // not stable, keeping the variant deterministic across platforms.
        let prefixed = to_extended_length_prefixed(path);
        match std::fs::metadata(&prefixed) {
            Ok(m) if !m.is_dir() => return Err(VfsError::NotADirectory(path.to_path_buf())),
            Ok(_) => {}
            Err(e) => return Err(map_io(path, e)),
        }
        if std::fs::read_dir(&prefixed)
            .map_err(|e| map_io(path, e))?
            .next()
            .is_some()
        {
            return Err(VfsError::DirectoryNotEmpty(path.to_path_buf()));
        }
        std::fs::remove_dir(&prefixed).map_err(|e| map_io(path, e))?;
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        std::fs::create_dir_all(to_extended_length_prefixed(path)).map_err(|e| map_io(path, e))?;
        Ok(())
    }
}

// ---- MemFs: the in-memory tree a dry run walks ------------------------------

/// One node used to seed a [`MemFs`] from a persisted snapshot: the stored path,
/// its size in bytes, and whether it is a directory. The command layer maps each
/// [`crate::ipc::EntryRow`] of the plan's scan to one of these (path + size +
/// kind), so a dry run walks a memory tree identical to the snapshot the plan was
/// built over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedEntry {
    /// The stored, human-readable path (no `\\?\` prefix).
    pub path: String,
    /// Size in bytes (0 for a directory).
    pub size: u64,
    /// Whether the node is a directory.
    pub is_dir: bool,
}

/// One in-memory node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemNode {
    is_dir: bool,
    size: u64,
}

/// An in-memory [`Vfs`] (F-607, dry-run harness).
///
/// A `MemFs` is disk-inert BY CONSTRUCTION: it holds only a normalized-path
/// `HashMap`, and none of its methods call the filesystem, so no key can ever
/// resolve to a real path no matter what paths it was seeded with (the load-
/// bearing property behind AC-2). Keys are normalized for NTFS-style matching
/// (case-insensitive, separator-agnostic, `\\?\`-free) via [`normalize_key`], so
/// a case-different or backslash/forward-slash spelling of the same path resolves
/// to the same node.
#[derive(Debug, Default)]
pub struct MemFs {
    nodes: Mutex<HashMap<String, MemNode>>,
}

impl MemFs {
    /// An empty in-memory filesystem.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed an in-memory filesystem from a snapshot's entries (path + size +
    /// is_dir). Later duplicate keys overwrite earlier ones (a snapshot never
    /// carries duplicates, so this is only a defined tie-break).
    pub fn from_seed(entries: &[SeedEntry]) -> Self {
        let mut nodes = HashMap::with_capacity(entries.len());
        for e in entries {
            nodes.insert(
                normalize_key(Path::new(&e.path)),
                MemNode {
                    is_dir: e.is_dir,
                    size: e.size,
                },
            );
        }
        Self {
            nodes: Mutex::new(nodes),
        }
    }

    /// How many nodes the tree currently holds (test/inspection helper).
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the tree holds no nodes.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, MemNode>> {
        self.nodes.lock().expect("MemFs mutex poisoned")
    }
}

impl Vfs for MemFs {
    fn exists(&self, path: &Path) -> bool {
        self.lock().contains_key(&normalize_key(path))
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.lock()
            .get(&normalize_key(path))
            .map(|n| n.is_dir)
            .unwrap_or(false)
    }

    fn metadata(&self, path: &Path) -> Result<VfsMetadata, VfsError> {
        self.lock()
            .get(&normalize_key(path))
            .map(|n| VfsMetadata {
                size: n.size,
                is_dir: n.is_dir,
            })
            .ok_or_else(|| VfsError::NotFound(path.to_path_buf()))
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        let from_key = normalize_key(from);
        let to_key = normalize_key(to);
        let mut nodes = self.lock();
        if !nodes.contains_key(&from_key) {
            return Err(VfsError::NotFound(from.to_path_buf()));
        }
        if nodes.contains_key(&to_key) {
            return Err(VfsError::AlreadyExists(to.to_path_buf()));
        }
        // Move the node AND everything under it (a directory rename carries its
        // whole subtree), re-keying each descendant onto the new prefix.
        let descendant_prefix = format!("{from_key}/");
        let moved: Vec<(String, MemNode)> = nodes
            .iter()
            .filter(|(k, _)| *k == &from_key || k.starts_with(&descendant_prefix))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        for (k, _) in &moved {
            nodes.remove(k);
        }
        for (k, node) in moved {
            let new_key = if k == from_key {
                to_key.clone()
            } else {
                // Splice the tail after `from_key/` onto `to_key`.
                format!("{}/{}", to_key, &k[from_key.len() + 1..])
            };
            nodes.insert(new_key, node);
        }
        Ok(())
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<u64, VfsError> {
        let from_key = normalize_key(from);
        let to_key = normalize_key(to);
        let mut nodes = self.lock();
        let node = *nodes
            .get(&from_key)
            .ok_or_else(|| VfsError::NotFound(from.to_path_buf()))?;
        if node.is_dir {
            return Err(VfsError::NotAFile(from.to_path_buf()));
        }
        if nodes.contains_key(&to_key) {
            return Err(VfsError::AlreadyExists(to.to_path_buf()));
        }
        nodes.insert(
            to_key,
            MemNode {
                is_dir: false,
                size: node.size,
            },
        );
        Ok(node.size)
    }

    fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
        let key = normalize_key(path);
        let mut nodes = self.lock();
        match nodes.get(&key) {
            None => Err(VfsError::NotFound(path.to_path_buf())),
            Some(n) if n.is_dir => Err(VfsError::NotAFile(path.to_path_buf())),
            Some(_) => {
                nodes.remove(&key);
                Ok(())
            }
        }
    }

    fn remove_dir(&self, path: &Path) -> Result<(), VfsError> {
        let key = normalize_key(path);
        let mut nodes = self.lock();
        match nodes.get(&key) {
            None => return Err(VfsError::NotFound(path.to_path_buf())),
            Some(n) if !n.is_dir => return Err(VfsError::NotADirectory(path.to_path_buf())),
            Some(_) => {}
        }
        let child_prefix = format!("{key}/");
        if nodes.keys().any(|k| k.starts_with(&child_prefix)) {
            return Err(VfsError::DirectoryNotEmpty(path.to_path_buf()));
        }
        nodes.remove(&key);
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        let key = normalize_key(path);
        if key.is_empty() {
            return Ok(());
        }
        let mut nodes = self.lock();
        // Every ancestor prefix that ends at a separator, then the full key, each
        // created as a directory if absent. A file sitting where a directory is
        // needed is a hard error.
        let mut prefixes: Vec<String> = Vec::new();
        for (idx, ch) in key.char_indices() {
            if ch == '/' && idx > 0 {
                prefixes.push(key[..idx].to_string());
            }
        }
        prefixes.push(key.clone());
        for prefix in prefixes {
            match nodes.get(&prefix) {
                Some(n) if n.is_dir => {}
                Some(_) => return Err(VfsError::NotADirectory(PathBuf::from(prefix))),
                None => {
                    nodes.insert(
                        prefix,
                        MemNode {
                            is_dir: true,
                            size: 0,
                        },
                    );
                }
            }
        }
        Ok(())
    }
}

/// Normalize a path into a [`MemFs`] key: strip any `\\?\` verbatim prefix, use
/// `/` separators, drop a trailing separator, and lowercase (NTFS is case-
/// insensitive). Reuses the [`crate::paths`] seam's prefix stripper so the two
/// speak one dialect.
fn normalize_key(path: &Path) -> String {
    let stripped = crate::paths::strip_extended_length_prefix(path);
    let mut key = stripped.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    key.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Vec<SeedEntry> {
        vec![
            SeedEntry {
                path: r"E:\lib".to_string(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: r"E:\lib\Book.m4b".to_string(),
                size: 1234,
                is_dir: false,
            },
        ]
    }

    #[test]
    fn memfs_answers_from_its_seed() {
        let fs = MemFs::from_seed(&seed());
        assert!(fs.exists(Path::new(r"E:\lib\Book.m4b")));
        assert!(fs.is_dir(Path::new(r"E:\lib")));
        assert!(!fs.is_dir(Path::new(r"E:\lib\Book.m4b")));
        let md = fs.metadata(Path::new(r"E:\lib\Book.m4b")).unwrap();
        assert_eq!(md.size, 1234);
        assert!(!md.is_dir);
        assert!(matches!(
            fs.metadata(Path::new(r"E:\lib\missing.m4b")),
            Err(VfsError::NotFound(_))
        ));
    }

    #[test]
    fn memfs_normalizes_separators_and_case() {
        let fs = MemFs::from_seed(&seed());
        // A different separator and case spelling resolves to the same node.
        assert!(fs.exists(Path::new("e:/LIB/book.M4B")));
        assert!(fs.is_dir(Path::new("e:/lib/")));
    }

    #[test]
    fn memfs_rename_moves_the_whole_subtree() {
        let fs = MemFs::from_seed(&[
            SeedEntry {
                path: r"E:\lib\Old".to_string(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: r"E:\lib\Old\a.m4b".to_string(),
                size: 10,
                is_dir: false,
            },
            SeedEntry {
                path: r"E:\lib\Old\disc1\b.m4b".to_string(),
                size: 20,
                is_dir: false,
            },
        ]);
        fs.rename(Path::new(r"E:\lib\Old"), Path::new(r"E:\lib\New"))
            .unwrap();
        assert!(!fs.exists(Path::new(r"E:\lib\Old")));
        assert!(!fs.exists(Path::new(r"E:\lib\Old\a.m4b")));
        assert!(fs.is_dir(Path::new(r"E:\lib\New")));
        assert!(fs.exists(Path::new(r"E:\lib\New\a.m4b")));
        assert_eq!(
            fs.metadata(Path::new(r"E:\lib\New\disc1\b.m4b"))
                .unwrap()
                .size,
            20
        );
    }

    #[test]
    fn memfs_rename_refuses_missing_source_and_present_target() {
        let fs = MemFs::from_seed(&seed());
        assert!(matches!(
            fs.rename(Path::new(r"E:\lib\gone"), Path::new(r"E:\lib\x")),
            Err(VfsError::NotFound(_))
        ));
        // Target already exists -> never-overwrite.
        assert!(matches!(
            fs.rename(Path::new(r"E:\lib\Book.m4b"), Path::new(r"E:\lib")),
            Err(VfsError::AlreadyExists(_))
        ));
    }

    #[test]
    fn memfs_copy_file_reports_size_and_refuses_overwrite() {
        let fs = MemFs::from_seed(&seed());
        let n = fs
            .copy_file(Path::new(r"E:\lib\Book.m4b"), Path::new(r"E:\lib\Copy.m4b"))
            .unwrap();
        assert_eq!(n, 1234);
        assert!(fs.exists(Path::new(r"E:\lib\Copy.m4b")));
        // Copying onto an existing target is refused.
        assert!(matches!(
            fs.copy_file(Path::new(r"E:\lib\Book.m4b"), Path::new(r"E:\lib\Copy.m4b")),
            Err(VfsError::AlreadyExists(_))
        ));
        // Copying a directory as a file is refused.
        assert!(matches!(
            fs.copy_file(Path::new(r"E:\lib"), Path::new(r"E:\lib\Whatever")),
            Err(VfsError::NotAFile(_))
        ));
    }

    #[test]
    fn memfs_create_dir_all_then_remove() {
        let fs = MemFs::new();
        fs.create_dir_all(Path::new(r"E:\lib\Author\Title"))
            .unwrap();
        assert!(fs.is_dir(Path::new(r"E:\lib")));
        assert!(fs.is_dir(Path::new(r"E:\lib\Author")));
        assert!(fs.is_dir(Path::new(r"E:\lib\Author\Title")));
        // Idempotent.
        fs.create_dir_all(Path::new(r"E:\lib\Author\Title"))
            .unwrap();
        // A non-empty directory cannot be removed.
        fs.copy_file_seed(r"E:\lib\Author\Title\a.m4b", 5);
        assert!(matches!(
            fs.remove_dir(Path::new(r"E:\lib\Author\Title")),
            Err(VfsError::DirectoryNotEmpty(_))
        ));
        fs.remove_file(Path::new(r"E:\lib\Author\Title\a.m4b"))
            .unwrap();
        fs.remove_dir(Path::new(r"E:\lib\Author\Title")).unwrap();
        assert!(!fs.exists(Path::new(r"E:\lib\Author\Title")));
    }

    #[test]
    fn memfs_remove_file_refuses_a_directory() {
        let fs = MemFs::from_seed(&seed());
        assert!(matches!(
            fs.remove_file(Path::new(r"E:\lib")),
            Err(VfsError::NotAFile(_))
        ));
        assert!(matches!(
            fs.remove_dir(Path::new(r"E:\lib\Book.m4b")),
            Err(VfsError::NotADirectory(_))
        ));
    }

    // Small test helper: insert a file node directly (used to make a directory
    // non-empty without going through copy_file's source requirement).
    impl MemFs {
        fn copy_file_seed(&self, path: &str, size: u64) {
            self.lock().insert(
                normalize_key(Path::new(path)),
                MemNode {
                    is_dir: false,
                    size,
                },
            );
        }
    }

    #[test]
    fn realfs_round_trips_in_a_temp_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fs = RealFs::new();
        let dir = tmp.path().join("a").join("b");
        fs.create_dir_all(&dir).unwrap();
        assert!(fs.is_dir(&dir));

        let file = dir.join("f.txt");
        std::fs::write(&file, b"hello").unwrap();
        assert!(fs.exists(&file));
        let md = fs.metadata(&file).unwrap();
        assert_eq!(md.size, 5);
        assert!(!md.is_dir);

        let moved = dir.join("g.txt");
        fs.rename(&file, &moved).unwrap();
        assert!(!fs.exists(&file));
        assert!(fs.exists(&moved));

        let copy = dir.join("h.txt");
        assert_eq!(fs.copy_file(&moved, &copy).unwrap(), 5);
        assert!(fs.exists(&copy));

        fs.remove_file(&copy).unwrap();
        fs.remove_file(&moved).unwrap();
        fs.remove_dir(&dir).unwrap();
        assert!(!fs.exists(&dir));
    }

    #[test]
    fn realfs_refuses_to_overwrite_an_existing_target() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fs = RealFs::new();
        let a = tmp.path().join("a.txt");
        let b = tmp.path().join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"bb").unwrap();

        assert!(matches!(fs.rename(&a, &b), Err(VfsError::AlreadyExists(_))));
        assert!(fs.exists(&a), "the source must survive a refused rename");
        assert_eq!(std::fs::read(&b).unwrap(), b"bb", "the target is untouched");

        assert!(matches!(
            fs.copy_file(&a, &b),
            Err(VfsError::AlreadyExists(_))
        ));
        assert_eq!(std::fs::read(&b).unwrap(), b"bb", "the target is untouched");
    }

    /// copy_file opens the destination with `create_new(true)`, so a present dest is
    /// refused with AlreadyExists and its bytes are left EXACTLY as they were - never
    /// truncated (unlike `std::fs::copy`). Portable (create_new is atomic no-clobber
    /// on every platform).
    #[test]
    fn realfs_copy_create_new_refuses_and_leaves_dest_unmodified() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fs = RealFs::new();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("dst.bin");
        std::fs::write(&src, b"NEW-SOURCE-BYTES").unwrap();
        std::fs::write(&dst, b"ORIGINAL-DEST").unwrap();

        let err = fs.copy_file(&src, &dst).unwrap_err();
        assert!(
            matches!(err, VfsError::AlreadyExists(_)),
            "create_new must refuse a present dest, not truncate it"
        );
        assert_eq!(
            std::fs::read(&dst).unwrap(),
            b"ORIGINAL-DEST",
            "the dest is byte-identical after a refused copy (never opened for truncate)"
        );
    }

    /// Windows: drive the no-replace rename PRIMITIVE directly (bypassing the Vfs
    /// pre-check) onto a pre-existing target, proving the refusal is atomic at
    /// `MoveFileExW` - not just the pre-check. The target is NEVER replaced and BOTH
    /// files survive intact, which is the race the pre-check alone cannot close.
    #[cfg(windows)]
    #[test]
    fn realfs_no_replace_rename_primitive_refuses_a_present_target() {
        let tmp = tempfile::TempDir::new().unwrap();
        let from = tmp.path().join("from.txt");
        let to = tmp.path().join("to.txt");
        std::fs::write(&from, b"SOURCE").unwrap();
        std::fs::write(&to, b"TARGET").unwrap();

        let err = real_rename_no_replace(&from, &to).unwrap_err();
        assert!(
            matches!(err, VfsError::AlreadyExists(_)),
            "MoveFileExW without MOVEFILE_REPLACE_EXISTING must refuse a present target"
        );
        assert_eq!(
            std::fs::read(&from).unwrap(),
            b"SOURCE",
            "the source survives the refused move"
        );
        assert_eq!(
            std::fs::read(&to).unwrap(),
            b"TARGET",
            "the target is NEVER overwritten"
        );
    }

    #[test]
    fn realfs_metadata_on_a_missing_path_is_not_found() {
        // Uniform contract: a missing path is NotFound (not a raw Io error), so it
        // matches MemFs and a later phase can map it to `source-vanished`.
        let tmp = tempfile::TempDir::new().unwrap();
        let fs = RealFs::new();
        let missing = tmp.path().join("nope.txt");
        assert!(!fs.exists(&missing));
        assert!(matches!(fs.metadata(&missing), Err(VfsError::NotFound(_))));
    }

    /// The short name of a [`VfsError`] variant, for cross-backend comparison.
    fn variant_name(err: &VfsError) -> &'static str {
        match err {
            VfsError::NotFound(_) => "NotFound",
            VfsError::AlreadyExists(_) => "AlreadyExists",
            VfsError::NotADirectory(_) => "NotADirectory",
            VfsError::NotAFile(_) => "NotAFile",
            VfsError::DirectoryNotEmpty(_) => "DirectoryNotEmpty",
            VfsError::Io(_) => "Io",
        }
    }

    /// The seam's whole purpose is dry-run == Real, so both backends MUST return
    /// the same structured [`VfsError`] variant for every guaranteed case (see the
    /// `Vfs` trait's uniform-error-contract doc). Table-driven: one real temp-dir
    /// layout and a `MemFs` seeded to match it, exercised with identical operands.
    #[test]
    fn both_backends_share_the_same_error_contract() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        let dir = base.join("d"); // a directory
        let file = base.join("f.txt"); // a file
        let existing = base.join("g.txt"); // a file used as a present target
        let full_dir = base.join("full"); // a non-empty directory
        let child = full_dir.join("c.txt");
        let missing = base.join("missing"); // absent in both backends
        let fresh = base.join("fresh"); // an absent target (parent `base` exists)

        // RealFs: build the real layout.
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(&file, b"x").unwrap();
        std::fs::write(&existing, b"yy").unwrap();
        std::fs::create_dir(&full_dir).unwrap();
        std::fs::write(&child, b"z").unwrap();
        let real = RealFs::new();

        // MemFs: seed the identical layout.
        let mem = MemFs::from_seed(&[
            SeedEntry {
                path: dir.to_string_lossy().into_owned(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: file.to_string_lossy().into_owned(),
                size: 1,
                is_dir: false,
            },
            SeedEntry {
                path: existing.to_string_lossy().into_owned(),
                size: 2,
                is_dir: false,
            },
            SeedEntry {
                path: full_dir.to_string_lossy().into_owned(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: child.to_string_lossy().into_owned(),
                size: 1,
                is_dir: false,
            },
        ]);

        // Each case: (label, expected variant, op). The op errors without mutating
        // (it fails a pre-check), so the order of cases never disturbs the layout.
        type Case<'a> = (&'a str, &'a str, Box<dyn Fn(&dyn Vfs) -> VfsError + 'a>);
        let cases: Vec<Case> = vec![
            (
                "rename missing source",
                "NotFound",
                Box::new(|fs: &dyn Vfs| fs.rename(&missing, &fresh).unwrap_err()),
            ),
            (
                "copy_file missing source",
                "NotFound",
                Box::new(|fs: &dyn Vfs| fs.copy_file(&missing, &fresh).unwrap_err()),
            ),
            (
                "copy_file on a directory source",
                "NotAFile",
                Box::new(|fs: &dyn Vfs| fs.copy_file(&dir, &fresh).unwrap_err()),
            ),
            (
                "metadata on missing path",
                "NotFound",
                Box::new(|fs: &dyn Vfs| fs.metadata(&missing).unwrap_err()),
            ),
            (
                "rename onto existing target",
                "AlreadyExists",
                Box::new(|fs: &dyn Vfs| fs.rename(&file, &existing).unwrap_err()),
            ),
            (
                "copy_file onto existing target",
                "AlreadyExists",
                Box::new(|fs: &dyn Vfs| fs.copy_file(&file, &existing).unwrap_err()),
            ),
            (
                "remove_file on missing path",
                "NotFound",
                Box::new(|fs: &dyn Vfs| fs.remove_file(&missing).unwrap_err()),
            ),
            (
                "remove_file on a directory",
                "NotAFile",
                Box::new(|fs: &dyn Vfs| fs.remove_file(&dir).unwrap_err()),
            ),
            (
                "remove_dir on a file",
                "NotADirectory",
                Box::new(|fs: &dyn Vfs| fs.remove_dir(&file).unwrap_err()),
            ),
            (
                "remove_dir on a non-empty directory",
                "DirectoryNotEmpty",
                Box::new(|fs: &dyn Vfs| fs.remove_dir(&full_dir).unwrap_err()),
            ),
        ];

        for (label, expected, op) in &cases {
            let mem_variant = variant_name(&op(&mem));
            let real_variant = variant_name(&op(&real));
            assert_eq!(mem_variant, *expected, "{label}: MemFs variant");
            assert_eq!(
                real_variant, *expected,
                "{label}: RealFs variant (must match MemFs)"
            );
        }
    }
}
