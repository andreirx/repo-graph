//! DAEMON-CONCURRENCY-IMPL-1 (B1) — behavior-2 concurrency tests on the REAL dispatch surface.
//!
//! These complete the §10 validation triad (behaviors 1 & 3 live in
//! `daemon-transport/tests/concurrency.rs`: no-head-of-line-blocking and over-cap `Busy`).
//! Behavior 2 is "writes serialize correctly + readers never observe a partial write," and it is
//! the gap the iteration-0 review flagged: the prior test exercised only the primitive
//! `DatabaseState` write lock, not real daemon `index`/`refresh` requests, and did not prove
//! reader-visible last-good READY through the dispatcher.
//!
//! Both tests here drive `ServiceDispatcher::dispatch` — the SAME trait method the concurrent
//! accept loop calls from its per-connection worker threads — from multiple threads over a shared
//! `Arc<ServiceDispatcher>`. The dispatcher is shareable across threads ONLY because B1 made the
//! state `Send + Sync`; if a `!Sync` field regressed, this file would fail to COMPILE. So it is
//! also a live witness of the Send+Sync property (the type-level pin is
//! `state::tests::daemon_and_repo_state_are_send_sync`).
//!
//! Determinism (§10: "no wall-clock flakiness"): the in-flight write is parked on a `Condvar`
//! rendezvous the test controls, not a timer. The only sleeps anywhere in the suite are the
//! transport tests' connect-retries, which gate nothing.
//!
//! ── Why the reader is NOT blocked while the write is in flight (the W-A + S-A + WAL composition) ──
//!
//! `inflight_*` uses `index` (not `refresh`) as the in-flight write deliberately. `handle_index`
//! takes ONLY the daemon's `DatabaseState` write `Mutex<()>` (serializes write operations); it does
//! NOT take the `RepoCoordinator` refresh guard. So under W-A a concurrent reader's `acquire_read()`
//! is not excluded. The parked writer is blocked in Rust code inside its progress callback — it
//! holds NO SQLite-level lock and has no open write transaction at that point — so a per-operation
//! reader connection (S-A) opens freely under WAL and reads the last-good `status='ready'` snapshot.
//! That is precisely the safety the slice relies on: the daemon write Mutex serializes writers;
//! WAL plus the READY-snapshot filter keep readers on the last-good view; and the new snapshot is
//! invisible until its atomic BUILDING→READY flip (`storage` `create_snapshot` only INSERTs a
//! BUILDING row and never demotes the prior READY).

use std::path::Path;
use std::sync::{Arc, Barrier, Condvar, Mutex};
use std::thread;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Emitters ───────────────────────────────────────────────────────────────

/// A progress emitter that discards events. Used for every request whose timing the test does not
/// need to control (the reader, and the concurrent writers in the serialization test).
struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Shared rendezvous between the parked writer and the test thread.
#[derive(Clone, Default)]
struct ParkHandle {
    inner: Arc<(Mutex<ParkFlags>, Condvar)>,
}

#[derive(Default)]
struct ParkFlags {
    entered: bool,
    released: bool,
}

impl ParkHandle {
    /// Block (no timer) until the writer is provably parked inside its handler.
    fn wait_until_entered(&self) {
        let (lock, cv) = &*self.inner;
        let mut f = lock.lock().unwrap();
        while !f.entered {
            f = cv.wait(f).unwrap();
        }
    }

    /// Let the parked writer proceed to completion.
    fn release(&self) {
        let (lock, cv) = &*self.inner;
        let mut f = lock.lock().unwrap();
        f.released = true;
        cv.notify_all();
    }
}

/// Parks the FIRST time the write pipeline emits progress, then passes through. At the first emit
/// (`"scanning" 0/1`) the handler is already inside `handle_index`'s `acquire_write()` scope but has
/// not yet created or flipped the new snapshot — so the prior READY snapshot is still the last-good.
struct ParkOnceEmitter {
    handle: ParkHandle,
    parked: bool,
}

impl ProgressEmitter for ParkOnceEmitter {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        if !self.parked {
            self.parked = true;
            let (lock, cv) = &*self.handle.inner;
            let mut f = lock.lock().unwrap();
            f.entered = true;
            cv.notify_all();
            while !f.released {
                f = cv.wait(f).unwrap();
            }
        }
        Ok(())
    }
}

// ── Harness ──────────────────────────────────────────────────────────────────

/// Isolated daemon: a temp state root (registry + databases never touch the operator's real state),
/// a `DaemonState`, and a thread-shareable `Arc<ServiceDispatcher>`.
fn isolated() -> (Arc<ServiceDispatcher>, Arc<DaemonState>, TempDir) {
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = Arc::new(ServiceDispatcher::new(Arc::clone(&state)));
    (dispatcher, state, state_root)
}

/// Snapshot S1: a cross-file import + call so `helperFunction` has a resolvable caller. Mirrors the
/// proven `create_graph_drilldown_test_repo` shape from the rgr dispatch suite.
fn write_s1(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("helper.ts"),
        "export function helperFunction() {\n    console.log('helper');\n}\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("main.ts"),
        "import { helperFunction } from './helper';\n\nexport function mainEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

/// Mutate the working tree to S2: add a second symbol (`secondEntry`). A re-index picks this up.
/// `secondEntry` existing-or-not is the last-good discriminator (a function definition is the most
/// deterministic extraction primitive — it does not depend on call-edge resolution).
fn add_s2(repo_dir: &Path) {
    std::fs::write(
        repo_dir.join("second.ts"),
        "import { helperFunction } from './helper';\n\nexport function secondEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

fn request(id: &str, method: &str, params: Value) -> Request {
    Request {
        id: id.to_string(),
        method: method.to_string(),
        params,
    }
}

/// Dispatch with a quiet emitter (the common case).
fn run(dispatcher: &ServiceDispatcher, id: &str, method: &str, params: Value) -> DispatchResult {
    let mut emitter = Quiet;
    dispatcher.dispatch(&request(id, method, params), &mut emitter)
}

fn callers(dispatcher: &ServiceDispatcher, id: &str, repo: &str, symbol: &str) -> DispatchResult {
    run(
        dispatcher,
        id,
        "callers",
        json!({ "repo": repo, "symbol": symbol }),
    )
}

#[track_caller]
fn expect_success(result: DispatchResult) -> Value {
    match result {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => {
            panic!(
                "expected success, got error {}: {}",
                e.error.code, e.error.message
            )
        }
    }
}

fn is_success(result: &DispatchResult) -> bool {
    matches!(result, DispatchResult::Success(_))
}

/// True iff the response is the "symbol not found" read error — i.e. the symbol is ABSENT from the
/// resolved (last-good READY) snapshot. `handle_callers` maps `SymbolResolveError::NotFound` to an
/// `InvalidRequest` whose message contains "symbol not found".
fn is_symbol_not_found(result: &DispatchResult) -> bool {
    match result {
        DispatchResult::Error(e) => e.error.message.contains("symbol not found"),
        DispatchResult::Success(_) => false,
    }
}

// ── Behavior 2a: concurrent real writes serialize without corruption ─────────

/// Required change (1): two or more concurrent REAL daemon write requests (`refresh`) to the same
/// repo/DB serialize correctly and do not corrupt storage.
///
/// Mechanism under test: every write takes the per-DB `DatabaseState` write `Mutex<()>` first
/// (`handle_refresh` → `acquire_write`), then the `RepoCoordinator` refresh guard, so the four
/// requests are admitted one at a time. Determinism: WITH that serialization every request commits
/// on its own connection and all succeed on every run; WITHOUT it, concurrent writers on one SQLite
/// file would race for the WAL write lock and surface `SQLITE_BUSY`/corruption — which this test
/// would catch as a failed/!Success response or an inconsistent post-state.
#[test]
fn concurrent_refresh_same_repo_serializes_without_corruption() {
    let (dispatcher, _state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    // Establish the repo + a first READY snapshot.
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // Fire N concurrent refreshes that all start together (Barrier), maximizing contention on the
    // write lock. Each is a real `refresh` dispatch on a worker thread sharing the Arc dispatcher.
    const N: usize = 4;
    let barrier = Arc::new(Barrier::new(N));
    let mut handles = Vec::new();
    for i in 0..N {
        let dispatcher = Arc::clone(&dispatcher);
        let barrier = Arc::clone(&barrier);
        let repo = canonical.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            match run(
                &dispatcher,
                &format!("rf-{i}"),
                "refresh",
                json!({ "repo": repo }),
            ) {
                DispatchResult::Success(_) => Ok(()),
                DispatchResult::Error(e) => Err(format!("{}: {}", e.error.code, e.error.message)),
            }
        }));
    }
    for h in handles {
        h.join().unwrap().expect(
            "every concurrent refresh must serialize and succeed (no SQLITE_BUSY/corruption)",
        );
    }

    // Storage is intact and consistent with the committed content: the original symbol resolves, and
    // nothing spurious from a torn write leaked in (S1 has no `secondEntry`).
    assert!(
        is_success(&callers(&dispatcher, "chk-a", &canonical, "helperFunction")),
        "after concurrent refreshes the committed symbol must still resolve (no corruption)"
    );
    assert!(
        is_symbol_not_found(&callers(&dispatcher, "chk-b", &canonical, "secondEntry")),
        "no symbol that was never indexed may appear (no torn/partial write leaked through)"
    );
}

// ── Behavior 2b: in-flight write holds the lock; reader sees last-good READY ──

/// Required changes (1) crisp mutual-exclusion + (2) reader sees last-good READY via the dispatch
/// surface (not SQL).
///
/// Timeline (every transition is gated on the rendezvous, never a timer):
///
/// First, index #1 of S1 produces READY #1; the reader sees `helperFunction` but not `secondEntry`.
/// The tree is then mutated to S2 (which adds `secondEntry`), and a worker dispatches index #2 whose
/// progress callback PARKS at the first emit — the handler is inside `acquire_write()` and has not
/// flipped a new snapshot, so READY #1 is still the last-good.
///
/// While the writer is parked, two facts are asserted. (a) `try_acquire_write()` on the SAME
/// `DatabaseState` returns `None`: a REAL in-flight dispatched write holds the exclusive DB write
/// lock, so a concurrent writer is excluded — serialization proven through a real request, not a
/// synthetic lock. (b) A concurrent `callers` dispatch returns the last-good READY: `helperFunction`
/// resolves and is served from SQLite, while `secondEntry` (the in-flight write's new symbol) is
/// INVISIBLE — the reader never observes the partial write.
///
/// Finally the writer is released and index #2 runs to completion (atomic BUILDING→READY flip); the
/// write lock is free again and the reader now sees `secondEntry`, proving the write committed
/// correctly (and that the prior invisibility was the genuine in-flight state, not a dead end).
#[test]
fn inflight_index_holds_write_lock_and_reader_sees_last_good_ready() {
    let (dispatcher, state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    // Step 1: index #1 → READY #1.
    let indexed = expect_success(run(
        &dispatcher,
        "idx1",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();
    let db_path = indexed["db_path"].as_str().unwrap().to_string();

    // Baseline: last-good has helperFunction, not secondEntry.
    let baseline = expect_success(callers(&dispatcher, "base", &canonical, "helperFunction"));
    assert_eq!(
        baseline["backend_used"].as_str(),
        Some("sqlite"),
        "no LiveGraph is loaded, so reads are served from the SQLite READY snapshot"
    );
    assert!(
        is_symbol_not_found(&callers(&dispatcher, "base-b", &canonical, "secondEntry")),
        "secondEntry must not exist before S2 is indexed"
    );

    // Step 2: mutate the working tree to S2.
    add_s2(&repo_dir);

    // Step 3: re-index on a worker, parked at its first progress emit.
    let park = ParkHandle::default();
    let writer = {
        let dispatcher = Arc::clone(&dispatcher);
        let park = park.clone();
        let repo_path = repo_dir.to_string_lossy().to_string();
        thread::spawn(move || {
            let mut emitter = ParkOnceEmitter {
                handle: park,
                parked: false,
            };
            matches!(
                dispatcher.dispatch(
                    &request("idx2", "index", json!({ "repo_path": repo_path })),
                    &mut emitter,
                ),
                DispatchResult::Success(_)
            )
        })
    };
    park.wait_until_entered(); // index #2 is provably parked inside acquire_write(); READY #1 stands.

    // Step 4(1): the in-flight REAL dispatched write holds the exclusive DB write lock.
    let runtime = state
        .get_or_create_db_runtime(Path::new(&db_path))
        .expect("db runtime for the indexed db path");
    assert!(
        runtime.try_acquire_write().is_none(),
        "an in-flight dispatched write must hold the exclusive DB write lock (serializes writers)"
    );

    // Step 4(2): a concurrent reader sees the last-good READY (#1) via dispatch — NOT the in-flight S2.
    let mid = expect_success(callers(&dispatcher, "mid-a", &canonical, "helperFunction"));
    assert_eq!(
        mid["backend_used"].as_str(),
        Some("sqlite"),
        "the reader is served from the last-good READY SQLite snapshot while the write is in flight"
    );
    assert!(
        is_symbol_not_found(&callers(&dispatcher, "mid-b", &canonical, "secondEntry")),
        "the in-flight write's new symbol must be INVISIBLE to a concurrent reader (last-good READY only)"
    );

    // Step 5: let the write finish.
    park.release();
    assert!(
        writer.join().expect("writer thread"),
        "the re-index must complete successfully after release"
    );

    // Step 6: lock released; the flip is visible; the write committed correctly.
    assert!(
        runtime.try_acquire_write().is_some(),
        "the DB write lock must be free once the write completes"
    );
    assert!(
        is_success(&callers(&dispatcher, "after", &canonical, "secondEntry")),
        "after the atomic flip the newly-indexed symbol is visible (the write committed; no corruption)"
    );
}
