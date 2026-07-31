//! A real apply that really dies mid-operation, for the F-606 kill-recovery tests.
//!
//! # Why a separate process
//!
//! Every other test of the reconciler constructs the in-doubt journal state by
//! hand: write an `intent` row, do not write a terminal row, then reconcile. That
//! proves the reconciler reasons correctly about a given database state, but it
//! assumes the state a kill actually produces. The assumption is the interesting
//! part, and an in-process test cannot check it:
//!
//! - A panic unwinds, running `Drop` for the pool, the connection, and any
//!   buffered writer. A killed process runs none of that.
//! - A test that returns early leaves the runtime free to flush on its own terms.
//! - `#[should_panic]` still ends with an orderly teardown.
//!
//! This binary is killed with [`std::process::abort`], which on every supported
//! platform terminates immediately: no unwinding, no destructors, no atexit
//! handlers, no userspace flush. Whatever is on disk afterwards is exactly what a
//! real crash leaves, and the integration test then reconciles that.
//!
//! # What it proves about the durability boundary
//!
//! FD-33 fixes the threat model at PROCESS KILL, not power loss: the journal runs
//! WAL with `synchronous = NORMAL`, so a committed `intent` has been handed to the
//! OS and survives the process dying, but is not guaranteed to survive the machine
//! losing power before the WAL frames reach the platter. `abort()` is precisely
//! the first case. If this harness ever stops finding its `intent` row after a
//! kill, the documented boundary has moved and F-606's whole premise is void -
//! which is a thing worth having a test say out loud.
//!
//! # Usage
//!
//! `kill_harness <db-dir> <library-root> <kill-at-op> <before|after>`
//!
//! Dies at the Nth `rename`. `op_move` calls `create_dir_all(parent)` and then
//! `rename`, so counting renames alone (not every mutating call) makes "the Nth
//! rename" mean "the Nth move operation" regardless of how many parents needed
//! creating. `before` aborts with the intent committed and the filesystem
//! untouched (AC-4); `after` aborts with the rename done and the terminal row
//! never written (AC-5).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use abo_core::db::open_db;
use abo_core::db::plans::{NewPlan, NewPlanOp};
use abo_core::db::rulesets::NewRuleset;
use abo_core::exec::lock::acquire_apply_job;
use abo_core::exec::vfs::{Vfs, VfsError, VfsMetadata};
use abo_core::exec::{ApplyMode, Executor, RealFs, SqliteJournal};

const NOW: &str = "2026-07-31T00:00:00Z";

/// Where in the operation the process dies.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Abort BEFORE the rename: intent committed, filesystem untouched (AC-4).
    Before,
    /// Abort AFTER the rename: filesystem changed, terminal row never written (AC-5).
    After,
}

/// A [`RealFs`] that kills the process at the Nth `rename`.
///
/// Only `rename` is counted. Reads pass straight through, and the other mutating
/// methods delegate without incrementing, so the counter indexes MOVE OPERATIONS
/// rather than filesystem calls.
struct KillFs {
    inner: RealFs,
    renames: AtomicUsize,
    kill_at: usize,
    phase: Phase,
}

impl KillFs {
    fn die(&self) -> ! {
        // Give the test a breadcrumb, flushed explicitly: abort() runs no atexit
        // handler, so anything still sitting in a userspace buffer is lost.
        use std::io::Write as _;
        let mut err = std::io::stderr();
        let _ = writeln!(err, "KILLING");
        let _ = err.flush();
        std::process::abort();
    }
}

impl Vfs for KillFs {
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }
    fn metadata(&self, path: &Path) -> Result<VfsMetadata, VfsError> {
        self.inner.metadata(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
        let n = self.renames.fetch_add(1, Ordering::SeqCst);
        if n == self.kill_at && self.phase == Phase::Before {
            self.die();
        }
        let result = self.inner.rename(from, to);
        if n == self.kill_at && self.phase == Phase::After {
            // The rename has landed on disk. Dying here is the act-then-kill case:
            // the executor never gets to write the `done` row.
            self.die();
        }
        result
    }
    fn copy_file(&self, from: &Path, to: &Path) -> Result<u64, VfsError> {
        self.inner.copy_file(from, to)
    }
    fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
        self.inner.remove_file(path)
    }
    fn remove_dir(&self, path: &Path) -> Result<(), VfsError> {
        self.inner.remove_dir(path)
    }
    fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
        self.inner.create_dir_all(path)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!("usage: kill_harness <db-dir> <library-root> <kill-at-op> <before|after>");
        std::process::exit(2);
    }
    let db_dir = PathBuf::from(&args[1]);
    let lib_root = PathBuf::from(&args[2]);
    let kill_at: usize = args[3].parse().expect("kill-at must be a number");
    let phase = match args[4].as_str() {
        "before" => Phase::Before,
        "after" => Phase::After,
        other => panic!("phase must be before|after, got {other}"),
    };

    let (pool, _) = open_db(&db_dir).await.expect("open_db");

    // A scan and a ruleset, because `plans` holds NOT NULL foreign keys to both.
    let scan_id: i64 = sqlx::query_scalar(
        "INSERT INTO scans (source, root_path, started_at, status) \
         VALUES ('live', ?, ?, 'completed') RETURNING id",
    )
    .bind(lib_root.to_string_lossy().to_string())
    .bind(NOW)
    .fetch_one(&pool)
    .await
    .expect("scan");
    let ruleset_id = abo_core::db::rulesets::insert_ruleset(
        &pool,
        &NewRuleset {
            name: "kill-harness",
            body_json: "{}",
            schema_version: 1,
        },
        NOW,
    )
    .await
    .expect("ruleset");

    // Three same-volume moves. The middle one is the interesting target: killing
    // there leaves a completed op behind it and an untouched op ahead of it, so the
    // test can assert the reconciler repairs exactly one row and nothing else.
    let sources: Vec<PathBuf> = (0..3)
        .map(|i| lib_root.join(format!("book{i}.m4b")))
        .collect();
    let targets: Vec<PathBuf> = (0..3)
        .map(|i| lib_root.join("Shelf").join(format!("book{i}.m4b")))
        .collect();
    let ops: Vec<NewPlanOp> = (0..3)
        .map(|i| NewPlanOp {
            op_group: "loose",
            kind: "move",
            kind_reason: None,
            source_path: sources[i].to_str().unwrap(),
            target_path: targets[i].to_str().unwrap(),
            rationale: "kill-harness op.",
            rule_id: "kill-harness",
            confidence: "high",
            byte_size: 8,
            validation_state: "valid",
            validation_reason: None,
            provenance_json: None,
        })
        .collect();
    let plan_id = abo_core::db::plans::insert_plan(
        &pool,
        &NewPlan {
            scan_id,
            ruleset_id,
            status: "ready",
            stats_json: None,
        },
        &ops,
        NOW,
    )
    .await
    .expect("insert plan");

    // The executor walks APPROVED operations only.
    sqlx::query("UPDATE plan_ops SET approval = 'approved' WHERE plan_id = ?")
        .bind(plan_id)
        .execute(&pool)
        .await
        .expect("approve ops");
    let op_rows = abo_core::db::plans::get_plan_ops(&pool, plan_id)
        .await
        .expect("plan ops");

    // Acquire the single-writer lock exactly as a real apply does, so the row this
    // leaves behind is a genuine stranded `running` apply job, not a fixture.
    let job_id = acquire_apply_job(&pool, ApplyMode::Real, NOW)
        .await
        .expect("acquire apply job");

    {
        use std::io::Write as _;
        let mut out = std::io::stdout();
        let _ = writeln!(out, "SETUP_OK job={job_id} plan={plan_id}");
        let _ = out.flush();
    }

    let vfs = KillFs {
        inner: RealFs::new(),
        renames: AtomicUsize::new(0),
        kill_at,
        phase,
    };
    let executor = Executor::new(vfs, job_id, op_rows);
    let journal = SqliteJournal::new(pool.clone());

    // Expected to die inside this call. Reaching the line after it means the kill
    // never fired, which the test treats as a failure rather than a pass.
    let outcome = executor.run(&journal, NOW).await;
    eprintln!("HARNESS SURVIVED (kill never fired): {outcome:?}");
    std::process::exit(3);
}
