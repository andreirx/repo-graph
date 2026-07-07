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
use repo_graph_storage::types::{GraphEdge, GraphNode, MeasurementInput, TrackedFile};
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
    // SNAPSHOT-RETENTION-1: these concurrency/cancellation proofs open RAW SQLite connections to the
    // daemon DB (fixture injection, snapshot-status reads) that DELIBERATELY bypass the repo
    // coordinator. PRODUCTION reads take the coordinator read-lock, so the pass's VACUUM excludes them
    // honestly — proven in `retention_pass`'s reader-vs-VACUUM tests
    // (`vacuum_defers_to_an_active_reader_then_runs_when_idle`,
    // `reader_arriving_during_vacuum_window_blocks_then_reads_correct_data`). A raw connection has no
    // such guard and would race the VACUUM's exclusive lock, so disable the background actor here: this
    // binary tests dispatch concurrency, not retention, and its raw readers are a harness artifact.
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    // ENRICH-LIFECYCLE-1: auto-enrichment is the SECOND background write-lock + activity actor spawned
    // on index/refresh completion (same class as auto-retention above). Its write-lock hold races the
    // raw SQLite readers this binary injects — an uncoordinated `StorageConnection::open` landing in the
    // pass's write window returns a raw `database is locked`. This binary tests dispatch concurrency,
    // not enrichment (enrichment's contention is proven in `enrich_lifecycle`), so disable it too.
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
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

/// DEFAULT `cycles` (no `--engine`), FIRST-CALL CERT BUILD (review iteration 1's gap; W-B-EPOCH-IMPL-2B
/// relocated WHERE the build runs): the default route now captures the request epoch via the BUILD-THEN-PEEK
/// `cycles_cert_eligibility` BEFORE serving — its WARM step runs `build_and_store_cycles_cert_cancellable`
/// (threaded with the SAME `finding_cycles` checkpoint), whose SHARED compare data runs a SQLite Tarjan
/// (`find_cycles_cancellable`) over the module graph. Iteration 1 found THIS cert-build Tarjan was the LAST
/// uncheckpointed loop on the default route (a disconnect during it ran the SCC pass to completion); the
/// build-then-peek capture keeps it cancellable — a disconnect mid-cert-build returns `Err(Cancelled)` from
/// the eligibility capture, mapped to `ErrorCode::Cancelled`. This proves it still cancels MID-cert-build
/// after the relocation.
///
/// The fixture sizes the cert build so the cancel deterministically lands in its SQLite SCC pass: a TINY
/// resident LiveGraph ring (2 modules ⇒ the compare's LiveGraph SCC runs fewer than the 256-step checkpoint
/// interval ⇒ emits ZERO heartbeats) and a LARGE SQLite module ring (1000 modules ⇒ the compare's
/// `find_cycles_cancellable` Tarjan emits many). `FailAfter(1)` therefore lets the first cert-build heartbeat
/// pass and the second fail inside the SQLite SCC pass. The cert is reset to missing before the cancel call
/// so the eligibility WARM takes the (re)build path (not a cached-cert peek).
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

// ── DAEMON-CANCEL-3: orient / explain in-flight cancellation ─────────────────
//
// These prove the THIRD/final B2 piece: the orient and explain handlers cancel
// WHILE their demonstrated heavy work is in flight — the module-cycle Tarjan and
// the complexity FETCH_ALL materialization — through the REAL `ServiceDispatcher::
// dispatch` surface with a closed transport (a failing emitter). Same honest
// large-fixture shape as the cycles/path/stats tests above: the fixture is sized so
// a non-cancelling run would complete far more than one checkpoint interval, and
// `assert_cancelled_in_flight` pins that the cancel fired DURING the loop.

/// Inject `n` `cyclomatic_complexity` measurements (all above the orient threshold of
/// 20) into a snapshot. The orient complexity aggregator fetches the FULL set
/// (`FETCH_ALL`) and materializes every row in a Rust loop, checkpointing every 1024
/// rows — so `n` ≫ 1024 makes that materialization the heavy, cancellable path. The
/// `target_stable_key`s need not match any node: the adapter's LEFT JOINs tolerate
/// missing name/file (the unresolved sample is still materialized + threshold-tested).
fn inject_complexity_measurements(db_path: &str, repo_uid: &str, snapshot_uid: &str, n: usize) {
    let mut conn =
        StorageConnection::open(db_path).expect("open daemon db for complexity injection");
    let rows: Vec<MeasurementInput> = (0..n)
        .map(|i| MeasurementInput {
            measurement_uid: format!("cmx{i}"),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            target_stable_key: format!("{repo_uid}:cxsym{i}:SYMBOL"),
            kind: "cyclomatic_complexity".to_string(),
            // Above the DEFAULT_COMPLEXITY_THRESHOLD (20) so every row survives the filter.
            value_json: "{\"value\": 42}".to_string(),
            source: "test".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .collect();
    conn.insert_measurements(&rows)
        .expect("insert complexity measurements");
}

/// A repo whose files live under a `pkg/` subdirectory, so `explain("pkg")` resolves
/// to the PATH pipeline (`explain_path` -> `find_cycles_involving_path` -> the heavy
/// module-cycle Tarjan) rather than the file pipeline (which reaches no Tarjan).
fn write_subdir_repo(repo_dir: &Path) {
    let pkg = repo_dir.join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("helper.ts"),
        "export function helperFunction() {\n    console.log('helper');\n}\n",
    )
    .unwrap();
    std::fs::write(
        pkg.join("main.ts"),
        "import { helperFunction } from './helper';\n\nexport function mainEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

/// orient (repo focus): a LARGE module-import ring makes the module-cycle Tarjan run
/// long; a peer that disconnects DURING it is cancelled mid-traversal, proven through
/// the real dispatcher with a closed transport. This is the orient analogue of
/// `dispatched_cycles_cancels_mid_tarjan_when_peer_disconnects`, exercising
/// `orient_cancellable` -> `find_module_cycles_cancellable` -> `find_cycles_cancellable`.
#[test]
fn dispatched_orient_cancels_mid_cycle_tarjan_when_peer_disconnects() {
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

    // FailAfter(1): the handler-boundary `pre_work_check` heartbeat passes; the first
    // in-loop checkpoint inside the Tarjan fails ⇒ Cancelled DURING the traversal.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("ori", "orient", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "orient (module-cycle Tarjan)");
}

/// orient (repo focus): a LARGE complexity set makes the FETCH_ALL materialization the
/// heavy, cancellable path (the freshly-indexed repo has no module ring, so the cycle
/// Tarjan returns immediately without checkpointing). A peer that disconnects DURING
/// the materialization is cancelled mid-loop, exercising
/// `query_high_complexity_symbols_cancellable`.
#[test]
fn dispatched_orient_cancels_mid_complexity_materialization_when_peer_disconnects() {
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

    // 4000 above-threshold complexity rows ⇒ the materialization loop runs ≫ the 1024
    // checkpoint chunk; with no injected ring, cycles is trivial (≈0 checkpoints), so
    // complexity is the first heavy in-loop checkpoint.
    inject_complexity_measurements(&db_path, &repo_uid, &snapshot_uid, 4000);

    // FailAfter(1): boundary heartbeat passes; the first in-loop complexity checkpoint
    // fails ⇒ Cancelled DURING the materialization.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("oric", "orient", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "orient (complexity materialization)");
}

/// explain (path focus): a LARGE module-import ring makes the path-scoped cycle Tarjan
/// run long; a peer that disconnects DURING it is cancelled mid-traversal. Exercises
/// the explain handler's NEW emitter + `run_explain_cancellable` ->
/// `find_cycles_involving_path_cancellable` -> `find_cycles_cancellable`. The
/// `pkg/`-subdir repo makes `explain("pkg")` route to the PATH pipeline (the file
/// pipeline reaches no Tarjan).
#[test]
fn dispatched_explain_cancels_mid_cycle_tarjan_when_peer_disconnects() {
    let (dispatcher, _state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_subdir_repo(&repo_dir);
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

    inject_module_ring(&db_path, &repo_uid, &snapshot_uid, 1000);

    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request(
            "exp",
            "explain",
            json!({ "repo": canonical, "target": "pkg" }),
        ),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "explain (path-scoped module-cycle Tarjan)");
}

/// Live-peer transparency: with a connected peer (a Quiet emitter that never fails),
/// orient and explain run to completion and return Success EVEN on the large ring
/// fixture — proving the cooperative checkpoint never spuriously cancels and the
/// cancellable path is byte-transparent when the peer stays. (The cancel only fires on
/// emit failure = disconnect.)
#[test]
fn live_peer_orient_and_explain_complete_on_large_fixture() {
    let (dispatcher, _state, _state_root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_subdir_repo(&repo_dir);
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

    // Heavy on BOTH chokepoints, yet a live peer must still get a full answer.
    inject_module_ring(&db_path, &repo_uid, &snapshot_uid, 1000);
    inject_complexity_measurements(&db_path, &repo_uid, &snapshot_uid, 4000);

    // orient (repo focus): full CoherenceEnvelope semantics, not merely Success — the
    // cooperative checkpoint is transparent when the peer stays (no spurious cancel).
    let orient = expect_success(run(
        &dispatcher,
        "ori-live",
        "orient",
        json!({ "repo": canonical }),
    ));
    assert_is_coherence_envelope(&orient, "live-peer orient (large fixture)");
    let orient_b = expect_success(run(
        &dispatcher,
        "ori-live-b",
        "orient",
        json!({ "repo": canonical }),
    ));
    assert_eq!(
        orient, orient_b,
        "live-peer orient must be deterministic + transparent across the checkpoint"
    );
    // explain (path focus): same — full envelope, deterministic, transparent.
    let explain = expect_success(run(
        &dispatcher,
        "exp-live",
        "explain",
        json!({ "repo": canonical, "target": "pkg" }),
    ));
    assert_is_coherence_envelope(&explain, "live-peer explain (large fixture)");
    let explain_b = expect_success(run(
        &dispatcher,
        "exp-live-b",
        "explain",
        json!({ "repo": canonical, "target": "pkg" }),
    ));
    assert_eq!(
        explain, explain_b,
        "live-peer explain must be deterministic + transparent across the checkpoint"
    );
}

// ── DAEMON-CANCEL-3: trust / check in-flight cancellation ────────────────────
//
// trust/check are the WORKER-SUPERVISED half of B2 (like stats): their heavy work is
// opaque SQL (trust `compute_module_stats` + the up-to-100k `query_unresolved_edges`,
// and the gate complexity load) PLUS the pure 100k unresolved-sample loop. The SQL
// can't be checkpointed in-Rust, so it cancels via `sqlite3_interrupt` driven by
// CANCEL-1's `run_interruptible` supervisor; the pure loop cancels via the cooperative
// `CancelFlag`. These tests prove the REAL handlers cancel mid-work through the
// dispatcher with a closed transport. (The deterministic, timing-free proof of the
// sample-loop checkpoint lives in trust's `cancellable_assembly_breaks_the_sample_loop_mid_flight`;
// the SQL-interrupt proof in storage's `interrupt_handle_aborts_in_flight_compute_module_stats`.)

/// Inject `n` unresolved CALLS edges (classification `unknown`, category
/// `CallsObjMethodNeedsTypeInfo` with enrichment metadata — the heaviest per-row trust
/// sample: each row both derives a blast radius AND parses JSON). This makes trust's
/// `query_unresolved_edges` fetch (LEFT JOIN + ORDER BY over the full set, capped at
/// 100k) AND the pure sample loop run FAR longer than one shortened heartbeat interval,
/// so a peer disconnect lands mid-work: the fetch is aborted by `sqlite3_interrupt`, the
/// loop broken by the cooperative `CancelFlag` — the two trust chokepoints CANCEL-3 wires.
fn inject_unresolved_calls(db_path: &str, repo_uid: &str, snapshot_uid: &str, n: usize) {
    use repo_graph_classification::types::{
        UnresolvedEdgeBasisCode, UnresolvedEdgeCategory, UnresolvedEdgeClassification,
    };
    use repo_graph_indexer::storage_port::{PersistedUnresolvedEdge, UnresolvedEdgePort};
    use repo_graph_indexer::types::{EdgeType as IxEdgeType, Resolution as IxResolution};

    let mut conn =
        StorageConnection::open(db_path).expect("open daemon db for unresolved-edge injection");

    // `unresolved_edges.source_node_uid` has a FK to `nodes(node_uid)`. One shared
    // source node satisfies it for every injected edge (the FK needs existence, not
    // uniqueness; the trust query LEFT JOINs it for visibility, identical per row).
    let src = GraphNode {
        node_uid: "usrc0".to_string(),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        stable_key: format!("{repo_uid}:usrc0:SYMBOL"),
        kind: "SYMBOL".to_string(),
        subtype: Some("FUNCTION".to_string()),
        name: "usrc0".to_string(),
        qualified_name: None,
        file_uid: None,
        parent_node_uid: None,
        location: None,
        signature: None,
        visibility: Some("export".to_string()),
        doc_comment: None,
        metadata_json: None,
    };
    conn.insert_nodes(std::slice::from_ref(&src))
        .expect("insert unresolved-edge source node");

    let edges: Vec<PersistedUnresolvedEdge> = (0..n)
        .map(|i| PersistedUnresolvedEdge {
            edge_uid: format!("ue{i}"),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            source_node_uid: "usrc0".to_string(),
            target_key: format!("utgt{i}"),
            edge_type: IxEdgeType::Calls,
            resolution: IxResolution::Inferred,
            extractor: "test".to_string(),
            line_start: None,
            col_start: None,
            line_end: None,
            col_end: None,
            metadata_json: Some(
                r#"{"enrichment":{"receiverType":"Map","typeDisplayName":"Map","isExternalType":true}}"#
                    .to_string(),
            ),
            category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
            classification: UnresolvedEdgeClassification::Unknown,
            classifier_version: 1,
            basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
            observed_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .collect();
    conn.insert_unresolved_edges(&edges)
        .expect("insert unresolved edges");
}

/// Assert a Success body is a `CoherenceEnvelope` — the four contract keys present —
/// so the live-peer transparency checks inspect real response SEMANTICS, not just the
/// Success discriminant.
#[track_caller]
fn assert_is_coherence_envelope(body: &Value, what: &str) {
    for key in ["value", "provenance", "trust", "freshness"] {
        assert!(
            body.get(key).is_some(),
            "{what}: response must be a CoherenceEnvelope (missing `{key}`); got {body:?}"
        );
    }
}

/// trust: a LARGE unknown-CALLS set makes the trust assembly (the
/// `query_unresolved_edges` fetch + the 100k sample loop) run long; a peer that
/// disconnects DURING it is cancelled mid-work — `sqlite3_interrupt` aborts the
/// in-flight `SELECT` and/or the cooperative flag breaks the loop. Proven through the
/// real dispatcher with a closed transport. The trust analogue of the stats interrupt
/// test, exercising `assemble_trust_report_cancellable` on the worker.
#[test]
fn dispatched_trust_cancels_in_flight_when_peer_disconnects() {
    // Opaque-SQL cancellation is heartbeat-timed (no in-statement Rust checkpoint), so
    // probe FAST: the first supervisor heartbeat then fires while the assembly is still
    // running. Process-global, but every `run_interruptible` caller in this binary
    // (stats, trust, check) sets 5 ms.
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

    // 120k unknown CALLS samples ⇒ the fetch (capped at 100k) + the 100k sample loop
    // span many 5 ms intervals, so a disconnect lands mid-work rather than after a fast
    // completion.
    inject_unresolved_calls(&db_path, &repo_uid, &snapshot_uid, 120_000);

    // Transparency first: a live peer (Quiet) gets the full answer — the worker wrapping
    // is invisible when connected.
    let live = run(
        &dispatcher,
        "trust-live",
        "trust",
        json!({ "repo": canonical }),
    );
    assert!(
        is_success(&live),
        "connected: trust on the heavy fixture must return Success, got {live:?}"
    );

    // Cancellation: FailAfter(1) — the handler-boundary `pre_work_check` heartbeat
    // passes, then the supervisor's first heartbeat (fired mid-assembly) fails ⇒ the
    // interrupt aborts the in-flight SELECT / the flag breaks the sample loop ⇒
    // Cancelled DURING the assembly.
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("trust", "trust", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "trust (worker + sqlite interrupt / sample loop)");
}

/// check: INHERITS trust's heavy assembly via `get_trust_summary`, so the same large
/// unknown-CALLS fixture makes `run_check` run long; a peer that disconnects DURING it
/// is cancelled mid-work, exercising the check handler's worker + `run_check_cancellable`
/// → `get_trust_summary_cancellable` path (the SQL interrupt + the cooperative flag).
#[test]
fn dispatched_check_cancels_in_flight_via_inherited_trust_when_peer_disconnects() {
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

    inject_unresolved_calls(&db_path, &repo_uid, &snapshot_uid, 120_000);

    // Transparency first.
    let live = run(
        &dispatcher,
        "check-live",
        "check",
        json!({ "repo": canonical }),
    );
    assert!(
        is_success(&live),
        "connected: check on the heavy fixture must return Success, got {live:?}"
    );

    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request("check", "check", json!({ "repo": canonical })),
        &mut emitter,
    );
    assert_cancelled_in_flight(&result, "check (inherited trust assembly on the worker)");
}

/// Live-peer transparency: with a connected peer, trust and check run to completion and
/// return Success with the FULL `CoherenceEnvelope` semantics — AND two runs are
/// byte-identical (the worker-supervised path is deterministic and transparent, never
/// spuriously cancelling when the peer stays). "Identical response semantics, not just
/// success" — the new worker wrapping changes nothing the peer observes.
#[test]
fn live_peer_trust_and_check_return_identical_results() {
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
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // trust: two live runs, identical + full envelope semantics.
    let trust_a = expect_success(run(
        &dispatcher,
        "t-a",
        "trust",
        json!({ "repo": canonical }),
    ));
    let trust_b = expect_success(run(
        &dispatcher,
        "t-b",
        "trust",
        json!({ "repo": canonical }),
    ));
    assert_is_coherence_envelope(&trust_a, "live-peer trust");
    assert_eq!(
        trust_a, trust_b,
        "live-peer trust must be deterministic + transparent across the worker boundary"
    );

    // check: two live runs, identical + full envelope semantics.
    let check_a = expect_success(run(
        &dispatcher,
        "c-a",
        "check",
        json!({ "repo": canonical }),
    ));
    let check_b = expect_success(run(
        &dispatcher,
        "c-b",
        "check",
        json!({ "repo": canonical }),
    ));
    assert_is_coherence_envelope(&check_a, "live-peer check");
    assert_eq!(
        check_a, check_b,
        "live-peer check must be deterministic + transparent across the worker boundary"
    );
}

// ── W-B-EPOCH-IMPL-3: read-during-refresh through the REAL dispatcher ─────────────
//
// The end-to-end proof of the WB-A flip: a reader request driven through the real
// `ServiceDispatcher::dispatch` is ADMITTED and returns a coherent last-good answer WHILE a
// refresh guard is held on the SAME repo's coordinator. Under W-A the reader's `acquire_read`
// was excluded by `Refreshing`, so this dispatch would block for the whole refresh (here, until
// the test drops the guard) and the `recv_timeout` would fire. Under W-B (this slice) it returns
// immediately while the refresh is STILL held — proving the flip on the real dispatch path + real
// coordinator, not only the state machine. Deterministic: the proof is that the result ARRIVES
// while the refresh is still held; the timeout only guards against a W-A regression (a block).

/// A real `callers` request is admitted + coherent through the dispatcher while a refresh is in
/// flight on the same repo (previously it would block under W-A — `reader_admitted_*` names).
#[test]
fn reader_admitted_through_dispatch_while_refresh_in_flight() {
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
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();

    // The coherent last-good answer, captured with NO refresh in flight (the steady state).
    let baseline = expect_success(callers(&dispatcher, "base", &canonical, "helperFunction"));

    // Resolve the SAME cached RepoState the dispatcher will resolve for the reader request, and
    // hold a refresh guard on its coordinator for the whole reader dispatch — a deterministic
    // stand-in for a long in-flight refresh (no producer needed).
    let repo_state = state
        .load_repo(Path::new(&db_path), &repo_uid)
        .expect("load (cache) the repo state");
    let refresh = repo_state.coordinator.acquire_refresh();
    assert!(
        repo_state.coordinator.state().is_write_active(),
        "a refresh is in flight on the repo coordinator"
    );

    // Drive a real `callers` request on a worker thread; under W-B it is ADMITTED rather than
    // blocked by the in-flight refresh.
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = {
        let dispatcher = Arc::clone(&dispatcher);
        let repo = canonical.clone();
        thread::spawn(move || {
            let _ = tx.send(callers(&dispatcher, "rd", &repo, "helperFunction"));
        })
    };

    // Under W-B the dispatch returns while the refresh is STILL held; under W-A it would block
    // until we drop the guard, so this recv would time out.
    let result = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("W-B: a reader admitted during a refresh must RETURN (W-A would block -> timeout)");

    // The refresh is STILL in flight (not yet dropped) — proving the reader was admitted
    // CONCURRENTLY with it, not after it ended.
    assert!(
        repo_state.coordinator.state().is_write_active(),
        "the refresh is still held; the reader was admitted concurrently with it"
    );

    // ...and the admitted reader served the SAME coherent last-good answer as the no-refresh read.
    let body = expect_success(result);
    assert_eq!(
        body, baseline,
        "the admitted reader serves the coherent last-good answer (identical to a no-refresh read)"
    );

    drop(refresh);
    reader.join().unwrap();
}
