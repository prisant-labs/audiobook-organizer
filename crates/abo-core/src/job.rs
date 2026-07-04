//! The pure job model (F-104): a cooperative cancellation flag plus a progress
//! sink that any long-running core operation threads through.
//!
//! This is the substrate every later long operation (hash, apply, rollback in
//! future releases) reuses, so it is deliberately Tauri-free and network-free:
//! the core only ever sees [`JobContext`]. The `src-tauri` shell adapts it -
//! it installs a progress sink that emits the `job:progress` event, holds the
//! [`CancelFlag`] in managed state so a `scan_cancel` command can flip it, and
//! persists the `jobs`-row lifecycle around the work. None of that leaks in
//! here (AC-3, D-01: abo-core has zero tauri dependency).
//!
//! Cancellation is COOPERATIVE (FD-02, the real Stop control): the running job
//! observes the flag only at a safe boundary (between entries during a scan)
//! and stops there. It is never interrupted mid-entry, so a cancelled scan can
//! never leave a torn snapshot (AC-104.3). What a cancelled operation does with
//! its partial work is the operation's decision; the scanner DISCARDS it (see
//! [`crate::scan::run_scan_with_job`]).

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A shareable cooperative-cancellation flag.
///
/// Cloning shares the same underlying atomic, so one clone handed to the
/// running job and another kept by the caller lets the caller request
/// cancellation from a different task or thread. The flag is one-way: once set
/// it stays set (a job is never "un-cancelled").
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    /// A fresh, not-yet-cancelled flag.
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Request cancellation. Idempotent; safe to call from any thread.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// One progress observation, emitted at a safe boundary during a job.
///
/// `done` is the count of units completed so far (entries recorded, for a
/// scan). `total_estimate` is `None` when the total is not yet known - which it
/// never is during a first walk, because the tree size is unknown until the
/// walk finishes (brief: emit a done-count with an unknown total). Progress is
/// monotonically non-decreasing across the life of one job (AC-104.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressUpdate {
    /// Units of work completed so far (monotonically non-decreasing).
    pub done: u64,
    /// Best-known total, or `None` when indeterminate (always `None` on a first
    /// walk: the tree size is unknown until the walk completes).
    pub total_estimate: Option<u64>,
    /// A short human-readable label for the current step (the current path, for
    /// a scan).
    pub current_label: String,
}

/// The sink the shell installs to receive [`ProgressUpdate`]s. `Send + Sync` so
/// it can be invoked from the spawned job task.
type ProgressSink = Arc<dyn Fn(ProgressUpdate) + Send + Sync>;

/// The Tauri-free job context threaded into a long-running core operation.
///
/// Carries the cancellation flag the operation polls at safe boundaries and an
/// optional progress sink it reports to. Cheap to clone (both fields are
/// `Arc`-backed).
#[derive(Clone)]
pub struct JobContext {
    cancel: CancelFlag,
    progress: Option<ProgressSink>,
}

impl JobContext {
    /// A context that can never be cancelled and drops all progress: the
    /// zero-overhead default for callers (and tests) that just want the plain
    /// operation. [`crate::scan::run_scan`] uses this so its behaviour is
    /// identical to the pre-job-model path.
    pub fn inert() -> Self {
        Self {
            cancel: CancelFlag::new(),
            progress: None,
        }
    }

    /// A context wired to a caller-supplied cancel flag and progress sink (the
    /// shell's production wiring).
    pub fn new(cancel: CancelFlag, progress: ProgressSink) -> Self {
        Self {
            cancel,
            progress: Some(progress),
        }
    }

    /// A context with a cancel flag but no progress sink (useful in tests that
    /// exercise cancellation without observing progress).
    pub fn with_cancel(cancel: CancelFlag) -> Self {
        Self {
            cancel,
            progress: None,
        }
    }

    /// Whether cancellation has been requested. Polled at safe boundaries only.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Report a progress update to the installed sink, if any. A no-op when the
    /// context carries no sink (the inert / cancel-only cases).
    pub fn report(&self, update: ProgressUpdate) {
        if let Some(sink) = &self.progress {
            sink(update);
        }
    }
}

impl Default for JobContext {
    fn default() -> Self {
        Self::inert()
    }
}

impl fmt::Debug for JobContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The progress sink is a boxed closure and cannot be Debug; report only
        // whether one is installed, plus the cancellation state.
        f.debug_struct("JobContext")
            .field("cancelled", &self.cancel.is_cancelled())
            .field("has_progress_sink", &self.progress.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn cancel_flag_starts_unset_and_latches() {
        let flag = CancelFlag::new();
        assert!(!flag.is_cancelled());
        flag.cancel();
        assert!(flag.is_cancelled());
        // Idempotent: a second cancel keeps it set.
        flag.cancel();
        assert!(flag.is_cancelled());
    }

    #[test]
    fn cancel_flag_clone_shares_state() {
        let a = CancelFlag::new();
        let b = a.clone();
        a.cancel();
        assert!(
            b.is_cancelled(),
            "a clone must observe the same cancellation"
        );
    }

    #[test]
    fn inert_context_never_cancels_and_swallows_progress() {
        let ctx = JobContext::inert();
        assert!(!ctx.is_cancelled());
        // Reporting to an inert context is a no-op that must not panic.
        ctx.report(ProgressUpdate {
            done: 1,
            total_estimate: None,
            current_label: "x".to_string(),
        });
    }

    #[test]
    fn context_reports_to_its_sink() {
        let seen: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let ctx = JobContext::new(
            CancelFlag::new(),
            Arc::new(move |u: ProgressUpdate| {
                sink_seen.lock().unwrap().push(u.done);
            }),
        );
        ctx.report(ProgressUpdate {
            done: 1,
            total_estimate: None,
            current_label: "a".to_string(),
        });
        ctx.report(ProgressUpdate {
            done: 2,
            total_estimate: None,
            current_label: "b".to_string(),
        });
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn with_cancel_observes_the_shared_flag() {
        let flag = CancelFlag::new();
        let ctx = JobContext::with_cancel(flag.clone());
        assert!(!ctx.is_cancelled());
        flag.cancel();
        assert!(ctx.is_cancelled());
    }
}
