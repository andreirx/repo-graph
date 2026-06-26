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
use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType, IdentitySource, ImportEdgeMeta, ImportResolution, IrEdge,
    IrNode, Partition, PartitionId, PartitionIr, PartitionKind, Provenance,
};
use repo_graph_livegraph::LiveGraph;
use repo_graph_storage::types::{GraphEdge, GraphNode, TrackedFile};
use repo_graph_storage::StorageConnection;
use repo_graph_trust_model::LanguageSupport;
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

// ── DAEMON-CANCEL-1: honest in-flight cancellation through the dispatcher seam ──
//
// These are the acceptance tests the slice calls out: prove that a peer who
// disconnects DURING a heavy Rust loop has its query cancelled MID-loop, through
// the real `ServiceDispatcher::dispatch` surface (a "closed transport" = a failing
// emitter), on a LARGE fixture — not a 2-node toy and not a unit helper. The prior
// B2 attempt over-claimed; here the fixture is sized so a non-cancelling run would
// take far more than one checkpoint interval, and the assertion pins that the
// cancel fired DURING the loop (the error message says "during", not "before").

/// Emitter that succeeds for its first `ok_for` emits, then fails every emit after.
/// Models a peer that is alive at the handler boundary (so `pre_work_check` passes)
/// but disconnects DURING the heavy work (so the next IN-LOOP checkpoint emit fails).
/// `ok_for = 1` lets the one boundary heartbeat through, then fails the first in-loop
/// checkpoint — so a `Cancelled` result PROVES in-loop (not pre-work) cancellation.
struct FailAfter {
    ok_for: usize,
    emits: usize,
}
impl FailAfter {
    fn new(ok_for: usize) -> Self {
        FailAfter { ok_for, emits: 0 }
    }
}
impl ProgressEmitter for FailAfter {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        self.emits += 1;
        if self.emits > self.ok_for {
            Err(EmitError::new("simulated peer disconnect"))
        } else {
            Ok(())
        }
    }
}

/// Assert the dispatch was cancelled WHILE the heavy loop was in flight: code
/// `Cancelled` AND the message says "during" (the in-loop layer), not "before" (the
/// cheap handler-boundary `pre_work_check`). This is the honesty the slice demands —
/// the cancel must fire mid-loop, not merely detect a pre-gone peer.
#[track_caller]
fn assert_cancelled_in_flight(result: &DispatchResult, what: &str) {
    match result {
        DispatchResult::Error(e) => {
            assert_eq!(
                e.error.code, "Cancelled",
                "{what}: expected ErrorCode::Cancelled, got code={} msg={}",
                e.error.code, e.error.message
            );
            assert!(
                e.error.message.contains("during"),
                "{what}: cancellation must fire DURING the loop (in-flight), not at the handler \
                 boundary; msg={}",
                e.error.message
            );
        }
        DispatchResult::Success(_) => {
            panic!("{what}: ran to completion instead of cancelling mid-flight")
        }
    }
}

fn module_node(repo_uid: &str, snapshot_uid: &str, i: usize) -> GraphNode {
    GraphNode {
        node_uid: format!("cm{i}"),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        stable_key: format!("{repo_uid}:cmod{i}:MODULE"),
        kind: "MODULE".to_string(),
        subtype: None,
        name: format!("cmod{i}"),
        qualified_name: None,
        file_uid: None,
        parent_node_uid: None,
        location: None,
        signature: None,
        visibility: None,
        doc_comment: None,
        metadata_json: None,
    }
}

fn imports_edge(repo_uid: &str, snapshot_uid: &str, i: usize, dst: usize) -> GraphEdge {
    GraphEdge {
        edge_uid: format!("ce{i}"),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        source_node_uid: format!("cm{i}"),
        target_node_uid: format!("cm{dst}"),
        edge_type: "IMPORTS".to_string(),
        resolution: "static".to_string(),
        extractor: "test".to_string(),
        location: None,
        metadata_json: None,
    }
}

/// Inject a LARGE MODULE import ring (`mod0 -> mod1 -> ... -> mod(n-1) -> mod0`, one
/// SCC of size `n`) directly into the daemon's DB via a separate WAL connection. The
/// daemon's per-operation reader (S-A) sees the committed rows. `n` ≫ the Tarjan
/// checkpoint interval (256) so `find_cycles`' SCC pass runs long enough for an
/// in-loop checkpoint to fire.
fn inject_module_ring(db_path: &str, repo_uid: &str, snapshot_uid: &str, n: usize) {
    let mut conn = StorageConnection::open(db_path).expect("open daemon db for fixture injection");
    let nodes: Vec<GraphNode> = (0..n)
        .map(|i| module_node(repo_uid, snapshot_uid, i))
        .collect();
    conn.insert_nodes(&nodes).expect("insert module ring nodes");
    let edges: Vec<GraphEdge> = (0..n)
        .map(|i| imports_edge(repo_uid, snapshot_uid, i, (i + 1) % n))
        .collect();
    conn.insert_edges(&edges).expect("insert module ring edges");
}

/// Inject one SYMBOL node so `path`'s `resolve_symbol` (which requires `kind='SYMBOL'`)
/// can resolve `from`/`to` to a stable_key. The stable_key matches the LiveGraph node
/// key so the resolved key drives the in-memory BFS.
fn inject_symbol(db_path: &str, repo_uid: &str, snapshot_uid: &str, stable_key: &str, name: &str) {
    let mut conn = StorageConnection::open(db_path).expect("open daemon db for fixture injection");
    conn.insert_nodes(&[GraphNode {
        node_uid: format!("sym-{name}"),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        stable_key: stable_key.to_string(),
        kind: "SYMBOL".to_string(),
        subtype: Some("FUNCTION".to_string()),
        name: name.to_string(),
        qualified_name: None,
        file_uid: None,
        parent_node_uid: None,
        location: None,
        signature: None,
        visibility: None,
        doc_comment: None,
        metadata_json: None,
    }])
    .expect("insert symbol node");
}

/// Build a LARGE LiveGraph: a chain `chain.c0 -> chain.c1 -> ... -> chain.c{n}` of
/// `SyntaxConfirmedCall` edges plus an isolated `chain.sink`. A BFS from `c0` toward
/// the unreachable `sink` visits the whole chain (`n+1` pops) ≫ the BFS checkpoint
/// interval (256), so an in-loop checkpoint fires long before the search exhausts.
fn chain_livegraph(n: usize) -> LiveGraph {
    let part = Partition {
        id: PartitionId::new("chain"),
        kind: PartitionKind::TsPackage,
        root: String::new(),
        indexer: "test".to_string(),
        indexer_version: "0".to_string(),
        build_inputs_hash: "h".to_string(),
        package_name: None,
        declared_dependencies: Default::default(),
        tsconfig_aliases: None,
    };
    let prov = || Provenance {
        indexer: "test".to_string(),
        indexer_version: "0".to_string(),
        scip_symbol_id: None,
        build_inputs_hash: "h".to_string(),
    };
    let mk_node = |k: String| IrNode {
        key: CanonicalKey::from_existing(k.clone()),
        subtype: "FUNCTION".to_string(),
        name: k,
        range: None,
        partition_id: PartitionId::new("chain"),
        identity_source: IdentitySource::AstAdopted,
        provenance: prov(),
        attributes: None,
    };
    let mut nodes: Vec<IrNode> = (0..=n).map(|i| mk_node(format!("chain.c{i}"))).collect();
    nodes.push(mk_node("chain.sink".to_string()));
    let edges: Vec<IrEdge> = (0..n)
        .map(|i| IrEdge {
            src: CanonicalKey::from_existing(format!("chain.c{i}")),
            dst: CanonicalKey::from_existing(format!("chain.c{}", i + 1)),
            edge_type: EdgeType::Calls,
            basis: EdgeBasis::SyntaxConfirmedCall,
            provenance: prov(),
            import: None,
        })
        .collect();
    let ir = PartitionIr {
        partition: part,
        nodes,
        edges,
        import_observations: Vec::new(),
    };
    let mut lg = LiveGraph::new();
    lg.load_partition("chain", ir, LanguageSupport::TypeScriptPrimary);
    lg
}

/// cycles: a LARGE module-import ring makes the Tarjan SCC traversal run long; a peer
/// that disconnects DURING it is cancelled mid-traversal (not after completing the
/// full SCC pass), proven through the real dispatcher with a closed transport.
#[test]
fn dispatched_cycles_cancels_mid_tarjan_when_peer_disconnects() {
    let (dispatcher, _state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // 1000-module ring ⇒ the SCC pass runs ~3000 DFS steps, ≫ the 256 checkpoint
    // interval, so the first in-loop checkpoint fires far before completion.
    inject_module_ring(&db_path, &repo_uid, &snapshot_uid, 1000);

    // `--engine sqlite` routes to `find_cycles_cancellable` (the genuinely-deep Rust
    // loop). FailAfter(1): the boundary heartbeat passes, the first in-loop checkpoint
    // fails ⇒ Cancelled DURING the traversal.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request(
            "cyc",
            "cycles",
            json!({ "repo": canonical, "engine": "sqlite" }),
        ),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "cycles");
}

/// path: a LARGE resident LiveGraph chain makes the BFS run long; a peer that
/// disconnects DURING the search is cancelled mid-BFS, proven through the real
/// dispatcher with a closed transport.
#[test]
fn dispatched_path_cancels_mid_bfs_when_peer_disconnects() {
    let (dispatcher, state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // from/to must resolve in SQLite (kind='SYMBOL'); their stable_keys are the
    // LiveGraph node keys the BFS traverses.
    inject_symbol(&db_path, &repo_uid, &snapshot_uid, "chain.c0", "c0");
    inject_symbol(&db_path, &repo_uid, &snapshot_uid, "chain.sink", "sink");

    // Cache the RepoState (get-or-insert) and inject the large chain into the SAME
    // Arc the dispatcher will resolve, so `path --engine livegraph` traverses it.
    let repo_state = state
        .load_repo(Path::new(&db_path), &repo_uid)
        .expect("load (cache) the repo state");
    *repo_state.livegraph.write() = Some(chain_livegraph(5000));

    // FailAfter(1): boundary heartbeat passes, the first in-loop BFS checkpoint fails
    // ⇒ Cancelled DURING the search (the BFS would otherwise visit 5001 nodes).
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request(
            "pth",
            "path",
            json!({
                "repo": canonical,
                "from": "chain.c0",
                "to": "chain.sink",
                "engine": "livegraph",
            }),
        ),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "path");
}

// ── DAEMON-CANCEL-1 revision: the routes the iteration-0 review flagged ──────────
//
// Review finding #1: the DEFAULT `rmap cycles` (no `--engine`) runs through
// `cycles_auto_response`, whose Tarjan loops (the LiveGraph `module_import_cycles`
// and the SQLite `find_cycles` fallback) were NOT checkpointed — so the prior
// `--engine sqlite`-only test did not prove the default path cancels mid-loop.
// Review finding #2: `path --engine compare` ran the SAME LiveGraph BFS without the
// checkpoint. These tests drive the real dispatcher with a closed transport and
// assert in-flight cancellation on a LARGE fixture for BOTH default cycles sub-paths
// and path-compare, plus live-peer transparency (identical success when connected).

/// Build a LARGE resident LiveGraph whose MODULE-import graph is an `n`-module ring:
/// `mod0/index.ts -> mod1/index.ts -> ... -> mod(n-1)/index.ts -> mod0/index.ts`, each
/// file in its OWN directory so `module(file) = dirname` yields `n` distinct modules in
/// one SCC. The DEFAULT-route LiveGraph Tarjan (`module_import_cycles_cancellable`, the
/// line the review flagged) runs ~3·n DFS steps over this — ≫ the 256 checkpoint
/// interval — so an in-loop checkpoint fires mid-traversal. Mirrors `chain_livegraph`
/// but with FILE nodes + `AstImport` edges (the module-cycle input).
fn module_ring_livegraph(n: usize) -> LiveGraph {
    let part = Partition {
        id: PartitionId::new("ring"),
        kind: PartitionKind::TsPackage,
        root: String::new(),
        indexer: "test".to_string(),
        indexer_version: "0".to_string(),
        build_inputs_hash: "h".to_string(),
        package_name: None,
        declared_dependencies: Default::default(),
        tsconfig_aliases: None,
    };
    let prov = || Provenance {
        indexer: "test".to_string(),
        indexer_version: "0".to_string(),
        scip_symbol_id: None,
        build_inputs_hash: "h".to_string(),
    };
    let file_key = |i: usize| format!("repo:mod{i}/index.ts:FILE");
    let nodes: Vec<IrNode> = (0..n)
        .map(|i| IrNode {
            key: CanonicalKey::from_existing(file_key(i)),
            subtype: "FILE".to_string(),
            name: file_key(i),
            range: None,
            partition_id: PartitionId::new("ring"),
            identity_source: IdentitySource::AstFileScope,
            provenance: prov(),
            attributes: None,
        })
        .collect();
    let edges: Vec<IrEdge> = (0..n)
        .map(|i| IrEdge {
            src: CanonicalKey::from_existing(file_key(i)),
            dst: CanonicalKey::from_existing(file_key((i + 1) % n)),
            edge_type: EdgeType::Imports,
            basis: EdgeBasis::AstImport,
            provenance: prov(),
            import: Some(ImportEdgeMeta {
                raw_specifier: "./x".to_string(),
                resolved_path: "x".to_string(),
                resolution: ImportResolution::StaticResolved,
            }),
        })
        .collect();
    let ir = PartitionIr {
        partition: part,
        nodes,
        edges,
        import_observations: Vec::new(),
    };
    let mut lg = LiveGraph::new();
    lg.load_partition("ring", ir, LanguageSupport::TypeScriptPrimary);
    lg
}

/// DEFAULT `cycles` (no `--engine`), SQLite-fallback Tarjan: with NO resident LiveGraph,
/// `cycles_auto_response` falls to `serve_cycles_sqlite → find_cycles_cancellable`. A
/// large SQLite module ring makes that SCC pass run long; a peer that disconnects DURING
/// it is cancelled mid-traversal — proving review finding #1's SQLite half. Live-peer
/// transparency is asserted first: connected, the same default query returns the cycle.
#[test]
fn dispatched_default_cycles_cancels_via_sqlite_fallback() {
    let (dispatcher, _state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // 1000-module ring ⇒ the SQLite SCC pass runs ~3000 DFS steps ≫ the 256 interval.
    inject_module_ring(&db_path, &repo_uid, &snapshot_uid, 1000);

    // Transparency: a live peer (Quiet) gets the full answer — no LiveGraph resident, so
    // the default route serves the SQLite SCC and finds the 1000-module cycle.
    let live = expect_success(run(
        &dispatcher,
        "cyc-live",
        "cycles",
        json!({ "repo": canonical }),
    ));
    assert_eq!(
        live["count"].as_u64(),
        Some(1),
        "connected: the 1000-module ring is one cycle (cancellation is transparent)"
    );
    assert_eq!(
        live["backend_used"].as_str(),
        Some("sqlite"),
        "no LiveGraph resident ⇒ the default route serves the SQLite SCC"
    );

    // Cancellation: FailAfter(1) — the first in-loop checkpoint passes, the next fails ⇒
    // Cancelled DURING the SCC pass (NOT at the handler boundary, NOT after completing).
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("cyc", "cycles", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "default cycles (sqlite fallback)");
}

/// DEFAULT `cycles` (no `--engine`), LiveGraph module-cycle Tarjan: a large resident
/// module-ring LiveGraph makes `cycles_auto_response`'s `module_import_cycles_cancellable`
/// (line ~2351 — the exact call the review flagged as uncheckpointed) run long; a peer
/// that disconnects DURING it is cancelled mid-traversal. This is review finding #1's
/// LiveGraph half, proven through the real dispatcher.
#[test]
fn dispatched_default_cycles_cancels_via_livegraph_module_tarjan() {
    let (dispatcher, state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // Cache the RepoState and inject a large module-ring LiveGraph into the SAME Arc the
    // dispatcher resolves, so the DEFAULT route runs the LiveGraph module Tarjan over it.
    let repo_state = state
        .load_repo(Path::new(&db_path), &repo_uid)
        .expect("load (cache) the repo state");
    *repo_state.livegraph.write() = Some(module_ring_livegraph(1000));

    // Transparency: a live peer succeeds. (The injected LiveGraph-only ring diverges from
    // the empty SQLite module graph → RED no-loss cert → the honest SQLite answer is
    // served; the point is a successful response when connected, not an error.)
    let live = expect_success(run(
        &dispatcher,
        "cyc-live",
        "cycles",
        json!({ "repo": canonical }),
    ));
    assert!(
        live["count"].is_u64(),
        "connected: the default cycles query returns a count (cancellation is transparent)"
    );

    // Cancellation: FailAfter(1) — `module_import_cycles_cancellable` runs FIRST in the
    // route (computing the precondition + cycles), so the cancel fires in ITS Tarjan,
    // before the cert ladder. Proves the flagged LiveGraph Tarjan now cancels mid-loop.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("cyc", "cycles", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "default cycles (livegraph module tarjan)");
}

/// DEFAULT `cycles` (no `--engine`), FIRST-CALL CERT BUILD (review iteration 1's gap): with a resident
/// LiveGraph whose module-cycle answer is `Exact`, `cycles_auto_response`'s precondition passes, then — on a
/// stale/missing cert — it runs `build_and_store_cycles_cert_cancellable`, whose SHARED compare data runs a
/// SQLite Tarjan (`find_cycles_cancellable`) over the module graph. Iteration 1 found THIS cert-build Tarjan
/// was the LAST uncheckpointed loop on the default route: a disconnect AFTER the precondition but DURING the
/// cert build ran the SCC pass to completion (`build_and_store_cycles_cert` → the non-cancellable
/// `module_cycle_compare_data`). This proves it now cancels MID-cert-build.
///
/// The fixture separates the two phases so the cancel deterministically lands in the cert build, NOT the
/// precondition: a TINY resident LiveGraph ring (2 modules ⇒ the precondition Tarjan runs fewer than the
/// 256-step checkpoint interval ⇒ emits ZERO heartbeats) and a LARGE SQLite module ring (1000 modules ⇒ the
/// cert build's `find_cycles_cancellable` Tarjan emits many). `FailAfter(1)` therefore lets the 0-heartbeat
/// precondition complete, then fails inside the cert-build SCC pass. The cert is reset to missing before the
/// cancel call so the StaleOrMissing cert-BUILD path is taken (not a cached-cert serve).
#[test]
fn dispatched_default_cycles_cancels_during_cert_build() {
    let (dispatcher, state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // LARGE SQLite module ring ⇒ the cert build's `find_cycles_cancellable` SCC pass runs ~3000 DFS steps
    // ≫ the 256 checkpoint interval, so it emits many heartbeats.
    inject_module_ring(&db_path, &repo_uid, &snapshot_uid, 1000);

    // TINY resident LiveGraph module ring (2 modules): resident + Fresh + TS ⇒ the precondition answer is
    // `Exact` (so the cert-build path is reached), AND its Tarjan runs < 256 steps ⇒ ZERO precondition
    // heartbeats. Injected into the SAME Arc the dispatcher resolves.
    let repo_state = state
        .load_repo(Path::new(&db_path), &repo_uid)
        .expect("load (cache) the repo state");
    *repo_state.livegraph.write() = Some(module_ring_livegraph(2));

    // Transparency: connected, the cert-build path runs to COMPLETION. The 2-module LiveGraph ring diverges
    // from the 1000-module SQLite ring ⇒ RED cert ⇒ the honest SQLite answer (the 1000-module cycle) is
    // served. Proves the cert build is transparent when the peer stays connected.
    let live = expect_success(run(
        &dispatcher,
        "cyc-live",
        "cycles",
        json!({ "repo": canonical }),
    ));
    assert_eq!(
        live["count"].as_u64(),
        Some(1),
        "connected: the cert build completes; the divergent (RED) cert serves the SQLite 1000-module cycle"
    );
    assert_eq!(
        live["backend_used"].as_str(),
        Some("sqlite"),
        "the divergent cert falls back to the SQLite answer"
    );

    // The live call STORED a (RED) cert. Reset it so the cancel call re-enters the StaleOrMissing cert-BUILD
    // path — otherwise a cached cert would skip the build and cancel only in the serve_sqlite fallback,
    // which the sibling `..._via_sqlite_fallback` test already covers.
    *repo_state.cycles_cert.write() = None;

    // Cancellation: FailAfter(1) — the 0-heartbeat precondition passes, then the cert build's
    // `find_cycles_cancellable` Tarjan emits its first checkpoint (~256 steps in, OK) and its second (~512
    // steps in) FAILS ⇒ Cancelled DURING the cert-build SCC pass: not at the handler boundary, not in the
    // precondition, not after completing. This is the exact loop review iteration 1 flagged.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("cyc", "cycles", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "default cycles (cert build)");
}

/// `path --engine compare` (review finding #2): the compare mode reads SQLite first, then
/// runs the SAME LiveGraph BFS. A large resident chain makes that BFS run long; a peer
/// that disconnects DURING it is cancelled mid-search. Transparency asserted first: the
/// connected compare returns the SQLite primary + the LiveGraph compare report.
#[test]
fn dispatched_path_compare_cancels_mid_bfs() {
    let (dispatcher, state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    inject_symbol(&db_path, &repo_uid, &snapshot_uid, "chain.c0", "c0");
    inject_symbol(&db_path, &repo_uid, &snapshot_uid, "chain.sink", "sink");
    let repo_state = state
        .load_repo(Path::new(&db_path), &repo_uid)
        .expect("load (cache) the repo state");
    *repo_state.livegraph.write() = Some(chain_livegraph(5000));

    // Transparency: connected, the compare returns SQLite as the primary backend plus the
    // LiveGraph compare report — identical to pre-cancellation behavior.
    let live = expect_success(run(
        &dispatcher,
        "pth-live",
        "path",
        json!({
            "repo": canonical,
            "from": "chain.c0",
            "to": "chain.sink",
            "engine": "compare",
        }),
    ));
    assert_eq!(
        live["backend_used"].as_str(),
        Some("sqlite"),
        "compare's primary answer is SQLite"
    );
    assert!(
        live.get("livegraph_path_compare").is_some(),
        "compare emits the LiveGraph compare report (the BFS ran when connected)"
    );

    // Cancellation: FailAfter(1) — the path handler's boundary heartbeat passes, then the
    // first in-loop checkpoint of the compare's LiveGraph BFS fails ⇒ Cancelled mid-search.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request(
            "pth",
            "path",
            json!({
                "repo": canonical,
                "from": "chain.c0",
                "to": "chain.sink",
                "engine": "compare",
            }),
        ),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "path compare");
}

// ── DAEMON-CANCEL-2: stats SQL cancellation via sqlite3_interrupt ─────────────────
//
// stats' heavy work is a SINGLE opaque SQL aggregation (`compute_module_stats`) the
// worker blocks INSIDE — no Rust frame can poll a cooperative flag (unlike
// cycles/path). So it cancels via `sqlite3_interrupt` driven by CANCEL-1's worker
// supervisor (`run_interruptible` + the `on_disconnect` actuator). This is the
// slice's required acceptance test: the REAL `stats --engine sqlite` handler path, on
// a LARGE fixture, through the dispatcher seam (a closed transport = a failing
// emitter).
//
// Timing note (honest): opaque-SQL cancellation is inherently heartbeat-timed (no
// in-statement Rust checkpoint exists — that is the whole reason it needs the
// interrupt). The fixture is sized so the real aggregation runs FAR longer than one
// supervisor heartbeat interval, so the disconnect lands mid-statement and a
// `Cancelled` result is not a fast-query fluke. The machine-speed-INDEPENDENT proof
// that the interrupt actually ABORTS the in-flight statement (rather than the worker
// completing and being discarded) is storage's deterministic
// `interrupt_handle_aborts_in_flight_compute_module_stats` (progress-handler barrier,
// no wall-clock).

/// Inject a LARGE stats fixture into the daemon's DB via a separate WAL connection
/// (the daemon's per-op reader, S-A, sees the committed rows): `n` directory MODULEs,
/// each OWNing one FILE node and `syms` exported SYMBOLs, plus an IMPORTS ring. The
/// `syms`-per-file symbol scan in `compute_module_stats`' `file_stats` CTE is the slow
/// lever — sized so the aggregation ≫ one heartbeat interval.
fn inject_stats_fixture(db_path: &str, repo_uid: &str, snapshot_uid: &str, n: usize, syms: usize) {
    let mut conn = StorageConnection::open(db_path).expect("open daemon db for stats fixture");

    let files: Vec<TrackedFile> = (0..n)
        .map(|i| TrackedFile {
            file_uid: format!("fu{i}"),
            repo_uid: repo_uid.to_string(),
            path: format!("statsmod{i}/index.ts"),
            language: Some("typescript".to_string()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        })
        .collect();
    conn.upsert_files(&files)
        .expect("insert stats fixture files");

    let mut nodes: Vec<GraphNode> = Vec::with_capacity(n * (2 + syms));
    for i in 0..n {
        // MODULE node (qualified_name drives `stats`' `path`/ORDER BY + the count).
        nodes.push(GraphNode {
            node_uid: format!("sm{i}"),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            stable_key: format!("{repo_uid}:statsmod{i}:MODULE"),
            kind: "MODULE".to_string(),
            subtype: None,
            name: format!("statsmod{i}"),
            qualified_name: Some(format!("statsmod{i}")),
            file_uid: None,
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: None,
        });
        // The OWNS target: a node carrying file_uid (so the module is not excluded).
        nodes.push(GraphNode {
            node_uid: format!("sfn{i}"),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            stable_key: format!("{repo_uid}:statsmod{i}/index.ts:FILE"),
            kind: "FILE".to_string(),
            subtype: None,
            name: format!("statsmod{i}/index.ts"),
            qualified_name: None,
            file_uid: Some(format!("fu{i}")),
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: None,
        });
        for k in 0..syms {
            nodes.push(GraphNode {
                node_uid: format!("ss{i}_{k}"),
                snapshot_uid: snapshot_uid.to_string(),
                repo_uid: repo_uid.to_string(),
                stable_key: format!("{repo_uid}:statsmod{i}/index.ts:sym{k}:SYMBOL"),
                kind: "SYMBOL".to_string(),
                subtype: Some("FUNCTION".to_string()),
                name: format!("sym{k}"),
                qualified_name: None,
                file_uid: Some(format!("fu{i}")),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: Some("export".to_string()),
                doc_comment: None,
                metadata_json: None,
            });
        }
    }
    conn.insert_nodes(&nodes)
        .expect("insert stats fixture nodes");

    let mut edges: Vec<GraphEdge> = Vec::with_capacity(n * 2);
    for i in 0..n {
        edges.push(GraphEdge {
            edge_uid: format!("sowns{i}"),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            source_node_uid: format!("sm{i}"),
            target_node_uid: format!("sfn{i}"),
            edge_type: "OWNS".to_string(),
            resolution: "static".to_string(),
            extractor: "test".to_string(),
            location: None,
            metadata_json: None,
        });
        edges.push(GraphEdge {
            edge_uid: format!("simp{i}"),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            source_node_uid: format!("sm{i}"),
            target_node_uid: format!("sm{}", (i + 1) % n),
            edge_type: "IMPORTS".to_string(),
            resolution: "static".to_string(),
            extractor: "test".to_string(),
            location: None,
            metadata_json: None,
        });
    }
    conn.insert_edges(&edges)
        .expect("insert stats fixture edges");
}

/// stats: the real `--engine sqlite` handler path runs `compute_module_stats` on the
/// worker under the supervisor; a peer that disconnects DURING the aggregation has the
/// in-flight SELECT aborted by `sqlite3_interrupt` ⇒ Cancelled (not run-to-completion).
/// Live-peer transparency is asserted first: connected, the same query returns the full
/// answer.
#[test]
fn dispatched_stats_cancels_via_sqlite_interrupt_when_peer_disconnects() {
    // Opaque-SQL cancellation is heartbeat-timed (no in-statement Rust checkpoint), so
    // probe FAST here: the first heartbeat then fires while a SMALL fixture's query is
    // still running, proving in-flight cancellation without a multi-second giant
    // fixture. SAFE to set process-wide: in this test binary the ONLY `run_interruptible`
    // caller is the stats handler, and ONLY this test exercises it. (The wall-clock-free
    // proof that the interrupt actually aborts the statement is storage's
    // `interrupt_handle_aborts_in_flight_compute_module_stats`.)
    repo_graph_daemon_runtime::cancel::set_heartbeat_interval_ms_for_test(5);

    let (dispatcher, _state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // ~60k exported SYMBOLs (50 modules × 1200): with the shortened heartbeat above the
    // `file_stats` scan + aggregation spans many heartbeat intervals, so the disconnect
    // lands mid-statement — without the multi-second build a 100 ms-cadence margin would
    // require.
    const N_MODULES: usize = 50;
    const SYMS_PER_FILE: usize = 1200;
    inject_stats_fixture(&db_path, &repo_uid, &snapshot_uid, N_MODULES, SYMS_PER_FILE);

    // Transparency: a live peer (Quiet) gets the full answer — the supervisor wrapping
    // is invisible when connected. `>=` because the real S1 index may add its own
    // MODULE rows; the injected modules must all be present.
    let live = expect_success(run(
        &dispatcher,
        "stats-live",
        "stats",
        json!({ "repo": canonical, "engine": "sqlite" }),
    ));
    assert!(
        live["count"].as_u64().unwrap_or(0) >= N_MODULES as u64,
        "connected: `stats --engine sqlite` returns at least the injected modules \
         (cancellation transparent); got count={:?}",
        live["count"]
    );

    // Cancellation: FailAfter(1) — the handler-boundary heartbeat passes, then the
    // supervisor's first heartbeat (fired mid-aggregation) fails ⇒ the interrupt aborts
    // the in-flight SELECT ⇒ Cancelled DURING the aggregation (NOT at the boundary, NOT
    // after completing).
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request(
            "stats",
            "stats",
            json!({ "repo": canonical, "engine": "sqlite" }),
        ),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "stats (sqlite interrupt)");
}

/// DAEMON-CANCEL-2 (review iteration 1): the DEFAULT `rmap stats` — NO `--engine` flag, i.e. engine
/// `auto`, the path agents actually take. With NO resident LiveGraph the auto route falls back to
/// `serve_stats_sqlite` → `compute_module_stats`, through the SAME `cancellable_module_stats` chokepoint
/// the `--engine sqlite` route uses. The iteration-0 build wired ONLY `--engine sqlite`, so default
/// `rmap stats` still ran heavy SQL to completion after a peer disconnect (the BLOCKING GAP the reviewer
/// flagged). This proves the DEFAULT path now aborts the in-flight `SELECT` via `sqlite3_interrupt` ⇒
/// `Cancelled`, not run-to-completion.
#[test]
fn dispatched_default_stats_cancels_via_sqlite_fallback_when_peer_disconnects() {
    // Opaque-SQL cancellation is heartbeat-timed (see the sibling `--engine sqlite` test); probe FAST so
    // a SMALL fixture's query is still running when a heartbeat fires. Process-global, but the ONLY
    // `run_interruptible` caller in this binary is the stats handler and both stats tests set 5 ms.
    repo_graph_daemon_runtime::cancel::set_heartbeat_interval_ms_for_test(5);

    let (dispatcher, _state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // Same large fixture as the sibling test: the aggregation spans many 5 ms heartbeat intervals, so a
    // disconnect lands mid-statement rather than after a fast completion.
    const N_MODULES: usize = 50;
    const SYMS_PER_FILE: usize = 1200;
    inject_stats_fixture(&db_path, &repo_uid, &snapshot_uid, N_MODULES, SYMS_PER_FILE);

    // Transparency + path proof: a live peer (Quiet) gets the full answer on the DEFAULT request (no
    // `engine` param), and `backend_used == "sqlite"` confirms we exercised the AUTO→SQLite fallback —
    // NOT the GREEN LiveGraph fastpath (there is no resident LiveGraph after a bare `index`). If a
    // LiveGraph were unexpectedly resident, this assert fails loudly rather than silently testing the
    // wrong (no-SQL, nothing-to-cancel) path.
    let live = expect_success(run(
        &dispatcher,
        "stats-live",
        "stats",
        json!({ "repo": canonical }),
    ));
    assert_eq!(
        live["backend_used"], "sqlite",
        "no resident LiveGraph ⇒ default `stats` serves the AUTO→SQLite fallback (the path under test); \
         got backend_used={:?}",
        live["backend_used"]
    );
    assert!(
        live["count"].as_u64().unwrap_or(0) >= N_MODULES as u64,
        "connected: default `stats` returns at least the injected modules (cancellation transparent); \
         got count={:?}",
        live["count"]
    );

    // Cancellation on the DEFAULT path (NO `engine` param). The auto route has no handler-boundary
    // pre-check (unlike the `--engine sqlite` route), so the supervisor's heartbeats are the only
    // disconnect probes: `FailAfter(1)` lets heartbeat #1 pass and heartbeat #2 (fired mid-aggregation)
    // fail ⇒ the interrupt aborts the in-flight SELECT ⇒ Cancelled DURING the aggregation.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("stats", "stats", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "default stats (auto -> sqlite fallback interrupt)");
}
