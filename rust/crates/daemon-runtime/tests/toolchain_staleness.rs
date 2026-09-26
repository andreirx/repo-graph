//! TOOLCHAIN-STALENESS-1 (RG-REQ-001-L06, D-STALE-SIGNAL-1, D-TS-RUNTIME-1) — the lazy automatic
//! background FULL re-index on first use, driven through the REAL `ServiceDispatcher::dispatch`.
//!
//! Each fixture is a tiny TS repository indexed by a real dispatched `index` (so its stamp is the
//! running one); a differing, missing, malformed or incomplete stamp is then written into the
//! fixture store with SQL. "First use in a daemon process" is a fresh `DaemonState` over the same
//! isolated state root (the registry is reloaded from disk). Every state root is a tempdir — the
//! operator's registry and daemon are never touched. Background enrich/retention/seed passes are
//! off. The tests serialize on one mutex: the opt-out override and the start hook are
//! process-global test seams.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use repo_graph_daemon_runtime::activity::OpKind;
use repo_graph_daemon_runtime::auto_reindex::{
    clear_served_stamp_read_hook_for_test, clear_start_hook_for_test, mark_stdio_transport,
    set_auto_reindex_for_test, set_served_stamp_read_hook_for_test, set_start_hook_for_test,
};
use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{DispatchResult, Dispatcher, NoOpEmitter, Request};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

static SERIAL: Mutex<()> = Mutex::new(());

const STDIO_REASON: &str =
    "stdio transport \u{2014} a background re-index cannot outlive the request";

/// Serializes the test and restores every process-global seam on drop (even on panic).
struct Serial(#[allow(dead_code)] MutexGuard<'static, ()>);

impl Drop for Serial {
    fn drop(&mut self) {
        set_auto_reindex_for_test(None);
        clear_start_hook_for_test();
        clear_served_stamp_read_hook_for_test();
    }
}

fn serial() -> Serial {
    let guard = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    set_auto_reindex_for_test(None);
    clear_start_hook_for_test();
    clear_served_stamp_read_hook_for_test();
    Serial(guard)
}

fn running() -> String {
    repo_graph_repo_index::toolchain::running_toolchain_json()
}

/// The running stamp with cpp-core's version replaced by `0.0.0`, and the one difference it makes.
fn seeded() -> (String, Value) {
    let run: Value = serde_json::from_str(&running()).unwrap();
    let cpp = run["extractors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
        .find(|e| e.starts_with("cpp-core:"))
        .unwrap()
        .to_string();
    let current = cpp.trim_start_matches("cpp-core:").to_string();
    (
        running().replace(&cpp, "cpp-core:0.0.0"),
        json!([{"component":"cpp-core","snapshot_version":"0.0.0","current_version":current}]),
    )
}

/// The isolated state root and the repositories' directory; both removed on drop.
struct Env {
    root: TempDir,
    _repos: TempDir,
}

impl Env {
    fn root(&self) -> &Path {
        self.root.path()
    }
}

struct Fixture {
    repo_dir: PathBuf,
    canonical: String,
    db_path: String,
    uid: String,
}

/// A fresh daemon process over `root`: its own registry (reloaded from disk) and dispatcher.
fn daemon(root: &Path) -> (Arc<DaemonState>, ServiceDispatcher) {
    let registry = RepoRegistry::with_state_root(root).expect("isolated registry");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(Arc::clone(&state));
    (state, dispatcher)
}

fn run(d: &ServiceDispatcher, method: &str, params: Value) -> DispatchResult {
    let request = Request {
        id: method.to_string(),
        method: method.to_string(),
        params,
    };
    d.dispatch(&request, &mut NoOpEmitter)
}

#[track_caller]
fn ok(result: DispatchResult) -> Value {
    match result {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => {
            panic!("expected success: {} {}", e.error.code, e.error.message)
        }
    }
}

fn write_repo(dir: &Path) {
    std::fs::create_dir_all(dir.join("sub dir")).unwrap();
    std::fs::write(
        dir.join("helper.ts"),
        "export function helper() {\n  return 1;\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.ts"),
        "import { helper } from './helper';\nexport function main() {\n  helper();\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("sub dir/extra.ts"), "export const extra = 2;\n").unwrap();
}

/// Index `names.len()` repositories into one isolated state root with the automatic re-index off
/// (so the indexing process queues nothing), then stamp each with `stamp`.
fn fixtures(names: &[&str], stamp: Option<&str>) -> (Env, Vec<Fixture>) {
    let env = Env {
        root: tempdir().unwrap(),
        _repos: tempdir().unwrap(),
    };
    set_auto_reindex_for_test(Some(false));
    let mut out = Vec::new();
    {
        let (_state, d) = daemon(env.root());
        for name in names {
            let repo_dir = env._repos.path().join(name);
            write_repo(&repo_dir);
            let r = ok(run(
                &d,
                "index",
                json!({ "repo_path": repo_dir.to_string_lossy() }),
            ));
            let field = |k: &str| r[k].as_str().unwrap().to_string();
            out.push(Fixture {
                repo_dir,
                canonical: field("canonical_path"),
                db_path: field("db_path"),
                uid: field("repo_uid"),
            });
        }
    }
    set_auto_reindex_for_test(None);
    for f in &out {
        set_stamp(&f.db_path, stamp);
    }
    (env, out)
}

fn fixture(stamp: Option<&str>) -> (Env, Fixture) {
    let (env, mut v) = fixtures(&["repo"], stamp);
    (env, v.remove(0))
}

fn sql(db_path: &str) -> rusqlite::Connection {
    rusqlite::Connection::open(db_path).unwrap()
}

fn set_stamp(db_path: &str, stamp: Option<&str>) {
    sql(db_path)
        .execute(
            "UPDATE snapshots SET toolchain_json = ?1 WHERE status = 'ready'",
            rusqlite::params![stamp],
        )
        .unwrap();
}

/// `(snapshot_uid, kind, status, toolchain_json)` oldest first.
fn snapshots(db_path: &str) -> Vec<(String, String, String, Option<String>)> {
    let c = sql(db_path);
    let mut stmt = c
        .prepare(
            "SELECT snapshot_uid, kind, status, toolchain_json FROM snapshots ORDER BY created_at",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

fn ready_count(db_path: &str) -> usize {
    snapshots(db_path).iter().filter(|s| s.2 == "ready").count()
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn status(d: &ServiceDispatcher, method: &str, repo: &str) -> Value {
    ok(run(d, method, json!({ "repo": repo })))["value"]["toolchain_staleness"].clone()
}

/// Wait for a completed automatic re-index: two ready full snapshots, the new one stamped with the
/// running toolchain — observed on the store only (no request).
fn wait_reindexed(f: &Fixture) {
    wait_until("the automatic re-index to complete", || {
        ready_count(&f.db_path) == 2
    });
    let s = snapshots(&f.db_path);
    assert!(s.iter().all(|s| s.1 == "full" && s.2 == "ready"), "{s:?}");
    assert_eq!(s[1].3.as_deref(), Some(running().as_str()), "{s:?}");
}

/// A start hook that records each started repository and parks until released.
#[derive(Default)]
struct Park {
    started: Mutex<Vec<PathBuf>>,
    released: Mutex<bool>,
    cv: Condvar,
}

impl Park {
    fn install(parked: bool) -> Arc<Park> {
        let park = Arc::new(Park::default());
        *park.released.lock().unwrap() = !parked;
        let p = Arc::clone(&park);
        set_start_hook_for_test(move |repo| {
            p.started.lock().unwrap().push(repo.to_path_buf());
            let mut released = p.released.lock().unwrap();
            while !*released {
                released = p.cv.wait(released).unwrap();
            }
        });
        park
    }
    fn started(&self) -> Vec<PathBuf> {
        self.started.lock().unwrap().clone()
    }
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.cv.notify_all();
    }
}

fn assert_nothing_starts(f: &Fixture, park: &Park) {
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(park.started(), Vec::<PathBuf>::new());
    assert_eq!(snapshots(&f.db_path).len(), 1);
}

fn reindex_of(v: &Value) -> &str {
    v["reindex"].as_str().unwrap_or("<none>")
}

// ── first use of a differing stamp ───────────────────────────────────────────────────────────────

#[test]
fn stale_stamp_first_request_starts_exactly_one_background_full_index() {
    let _s = serial();
    let (stamp, one) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let park = Park::install(false);
    let (_state, d) = daemon(root.root());
    let st = status(&d, "orient", &f.canonical);
    assert_eq!(st["state"], "stale");
    assert_eq!(st["differences"], one);
    assert!(matches!(reindex_of(&st), "queued" | "running"), "{st}");
    let sentinel = PathBuf::from(format!("{}.rebuilding", f.db_path));
    wait_until("completion", || {
        assert!(
            !sentinel.exists(),
            "the automatic re-index never uses repo rebuild"
        );
        ready_count(&f.db_path) == 2
    });
    wait_reindexed(&f);
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(
        snapshots(&f.db_path).len(),
        2,
        "exactly one background full index"
    );
    assert_eq!(park.started(), vec![PathBuf::from(&f.canonical)]);
}

#[test]
fn previous_snapshot_is_served_with_the_running_line_while_the_reindex_runs() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let old = snapshots(&f.db_path)[0].0.clone();
    let park = Park::install(true);
    let (_state, d) = daemon(root.root());
    status(&d, "orient", &f.canonical);
    wait_until("the job to park at the start hook", || {
        park.started().len() == 1
    });
    let served = ok(run(&d, "orient", json!({ "repo": f.canonical })));
    assert_eq!(
        served["value"]["snapshot"],
        old.as_str(),
        "the previous ready snapshot is served"
    );
    assert_eq!(served["value"]["toolchain_staleness"]["reindex"], "running");
    assert_eq!(status(&d, "check", &f.canonical)["reindex"], "running");
    park.release();
    wait_reindexed(&f);
}

#[test]
fn completed_reindex_serves_the_new_snapshot_and_reports_current_with_no_line() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let (_state, d) = daemon(root.root());
    status(&d, "orient", &f.canonical);
    wait_reindexed(&f);
    let new = snapshots(&f.db_path)[1].0.clone();
    let served = ok(run(&d, "orient", json!({ "repo": f.canonical })));
    assert_eq!(served["value"]["snapshot"], new.as_str());
    assert_eq!(
        served["value"]["toolchain_staleness"],
        json!({"state":"current"})
    );
    assert_eq!(
        status(&d, "check", &f.canonical),
        json!({"state":"current"})
    );
}

#[test]
fn second_request_during_the_run_starts_no_second_index() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let park = Park::install(true);
    let (_state, d) = daemon(root.root());
    status(&d, "orient", &f.canonical);
    wait_until("the job to park", || park.started().len() == 1);
    for method in ["orient", "check", "orient"] {
        assert_eq!(status(&d, method, &f.canonical)["reindex"], "running");
    }
    park.release();
    wait_reindexed(&f);
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(park.started().len(), 1);
    assert_eq!(snapshots(&f.db_path).len(), 2);
}

// ── queued behind writes and another repository's job ────────────────────────────────────────────

fn queued_behind_op(kind: OpKind) {
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let park = Park::install(false);
    let (state, d) = daemon(root.root());
    let op = state.activity().begin(
        kind,
        f.canonical.clone(),
        Some(f.uid.clone()),
        PathBuf::from(&f.db_path),
    );
    assert_eq!(status(&d, "orient", &f.canonical)["reindex"], "queued");
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(park.started().len(), 0, "the job waits for the write");
    assert_eq!(status(&d, "check", &f.canonical)["reindex"], "queued");
    drop(op);
    // No further request: the queued job starts by itself.
    wait_reindexed(&f);
    assert_eq!(park.started().len(), 1);
}

#[test]
fn first_request_during_an_index_is_queued_and_starts_without_another_request() {
    let _s = serial();
    queued_behind_op(OpKind::Index);
}

#[test]
fn first_request_during_a_refresh_is_queued_and_starts_without_another_request() {
    let _s = serial();
    queued_behind_op(OpKind::Refresh);
}

#[test]
fn first_request_during_another_repositorys_reindex_is_queued_and_starts_after_it() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, v) = fixtures(&["a", "b"], Some(&stamp));
    let park = Park::install(true);
    let (_state, d) = daemon(root.root());
    status(&d, "orient", &v[0].canonical);
    wait_until("a's job to park", || park.started().len() == 1);
    assert_eq!(status(&d, "orient", &v[1].canonical)["reindex"], "queued");
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(park.started().len(), 1, "one automatic re-index at a time");
    assert_eq!(status(&d, "check", &v[1].canonical)["reindex"], "queued");
    park.release();
    wait_reindexed(&v[0]);
    wait_reindexed(&v[1]);
    let order: Vec<PathBuf> = v.iter().map(|f| PathBuf::from(&f.canonical)).collect();
    assert_eq!(park.started(), order);
}

// ── rebuild ──────────────────────────────────────────────────────────────────────────────────────

/// The daemon that saw the request (kept alive: its worker holds only a `Weak`), the fixture, the
/// start hook and the environment.
type DuringRebuild = (Arc<DaemonState>, ServiceDispatcher, Fixture, Arc<Park>, Env);

fn request_during_rebuild(result_stamp: Option<&str>) -> DuringRebuild {
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let park = Park::install(false);
    let sentinel = PathBuf::from(format!("{}.rebuilding", f.db_path));
    std::fs::write(&sentinel, b"rebuilding").unwrap();
    let (state, d) = daemon(root.root());
    match run(&d, "orient", json!({ "repo": f.canonical })) {
        DispatchResult::Error(e) => assert!(
            e.error.message.contains("rebuild interrupted"),
            "the named refusal is kept: {}",
            e.error.message
        ),
        DispatchResult::Success(s) => panic!("served during a rebuild: {}", s.result),
    }
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(
        park.started().len(),
        0,
        "nothing starts while the rebuild runs"
    );
    // The rebuild's result: its stamp, then the sentinel removed. No further request.
    set_stamp(&f.db_path, result_stamp);
    std::fs::remove_file(&sentinel).unwrap();
    (state, d, f, park, root)
}

#[test]
fn request_during_rebuild_keeps_the_named_refusal_and_a_current_result_starts_nothing() {
    let _s = serial();
    let (_state, _d, f, park, _root) = request_during_rebuild(Some(&running()));
    assert_nothing_starts(&f, &park);
}

#[test]
fn request_during_rebuild_with_a_still_stale_result_starts_the_reindex_after_it() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (_state, _d, f, park, _root) = request_during_rebuild(Some(&stamp));
    wait_reindexed(&f);
    assert_eq!(park.started().len(), 1);
}

#[test]
fn rebuild_of_a_stale_store_is_not_bounced_by_the_check_its_own_load_queues() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let park = Park::install(false);
    let (_state, d) = daemon(root.root());
    // The rebuild is this process's first use: its own load queues the stamp check.
    let r = ok(run(
        &d,
        "repo_rebuild",
        json!({ "repo": f.canonical, "confirm": true }),
    ));
    assert_eq!(r["repo_uid"], f.uid.as_str(), "{r}");
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(
        park.started().len(),
        0,
        "the rebuilt store is current: no automatic index"
    );
    let s = snapshots(&f.db_path);
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0].3.as_deref(), Some(running().as_str()));
    assert_eq!(
        status(&d, "check", &f.canonical),
        json!({"state":"current"})
    );
}

// ── opt-out, failure, unreadable stamps ──────────────────────────────────────────────────────────

#[test]
fn auto_reindex_off_starts_nothing_and_reports_disabled_with_the_rebuild_remedy() {
    let _s = serial();
    let (stamp, one) = seeded();
    let (root, f) = fixture(Some(&stamp));
    set_auto_reindex_for_test(Some(false));
    let park = Park::install(false);
    let (_state, d) = daemon(root.root());
    let expected = json!({"state":"stale","differences":one,"reindex":"disabled",
        "reindex_disabled_reason":"RMAP_AUTO_REINDEX=off"});
    assert_eq!(status(&d, "orient", &f.canonical), expected);
    assert_eq!(status(&d, "check", &f.canonical), expected);
    assert_nothing_starts(&f, &park);
}

#[test]
fn failed_reindex_reports_failed_with_its_reason_and_is_not_retried_in_the_process() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    std::fs::remove_dir_all(&f.repo_dir).unwrap(); // the index will fail: no checkout
    let park = Park::install(false);
    let (_state, d) = daemon(root.root());
    status(&d, "check", &f.canonical);
    let mut st = Value::Null;
    wait_until("the failure", || {
        st = status(&d, "check", &f.canonical);
        st["reindex"] == "failed"
    });
    assert!(
        st["reindex_failure"]
            .as_str()
            .unwrap()
            .contains("does not exist"),
        "{st}"
    );
    assert_eq!(st["state"], "stale");
    // Unload, reload, further requests: never retried in this process.
    ok(run(
        &d,
        "unload_repo",
        json!({ "db_path": f.db_path, "repo_uid": f.uid }),
    ));
    for _ in 0..3 {
        assert_eq!(status(&d, "check", &f.canonical)["reindex"], "failed");
    }
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(park.started().len(), 1, "one attempt only");
    assert_eq!(
        snapshots(&f.db_path)
            .iter()
            .filter(|s| s.2 == "ready")
            .count(),
        1
    );
}

#[test]
fn terminal_stamp_read_failure_reports_failed_with_its_reason_and_starts_nothing() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let park = Park::install(false);
    let (state, d) = daemon(root.root());
    // The job waits behind a write while the store becomes unreadable (a non-lock fault).
    let op = state.activity().begin(
        OpKind::Index,
        f.canonical.clone(),
        Some(f.uid.clone()),
        PathBuf::from(&f.db_path),
    );
    assert_eq!(status(&d, "orient", &f.canonical)["reindex"], "queued");
    let away = format!("{}.away", f.db_path);
    std::fs::rename(&f.db_path, &away).unwrap();
    drop(op);
    std::thread::sleep(Duration::from_millis(3500));
    std::fs::rename(&away, &f.db_path).unwrap();
    // Readable again: the job still reports its terminal failure, never "queued" forever.
    let st = status(&d, "check", &f.canonical);
    assert_eq!(st["state"], "stale", "{st}");
    assert_eq!(st["reindex"], "failed", "{st}");
    let failure = st["reindex_failure"].as_str().expect("reindex_failure");
    assert!(
        failure.starts_with("could not read this index's toolchain stamp: "),
        "{st}"
    );
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(
        park.started().len(),
        0,
        "a terminal read fault is not retried"
    );
    assert_eq!(snapshots(&f.db_path).len(), 1);
}

/// Copy snapshot `from` into a NEWER ready snapshot `new_uid` stamped `stamp` — the row a
/// completing full index promotes (the newest ready snapshot by `created_at`).
fn promote_copy(db_path: &str, from: &str, new_uid: &str, stamp: &str) {
    let c = sql(db_path);
    let cols: Vec<String> = {
        let mut stmt = c.prepare("PRAGMA table_info(snapshots)").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    let select: Vec<&str> = cols
        .iter()
        .map(|c| match c.as_str() {
            "snapshot_uid" => "?1",
            "created_at" => "?2",
            "toolchain_json" => "?3",
            other => other,
        })
        .collect();
    let text = format!(
        "INSERT INTO snapshots ({}) SELECT {} FROM snapshots WHERE snapshot_uid = ?4",
        cols.join(", "),
        select.join(", ")
    );
    let n = c
        .execute(
            &text,
            rusqlite::params![new_uid, "9999-12-31T23:59:59.999Z", stamp, from],
        )
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn check_status_describes_the_snapshot_check_evaluated_when_a_newer_one_is_promoted_before_the_stamp_read(
) {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&running()));
    set_auto_reindex_for_test(Some(false)); // a (wrong) stale status would start nothing
    let evaluated = snapshots(&f.db_path)[0].0.clone();
    let promoted = Arc::new(Mutex::new(false));
    let (p, db, from) = (Arc::clone(&promoted), f.db_path.clone(), evaluated.clone());
    set_served_stamp_read_hook_for_test(move || {
        let mut done = p.lock().unwrap();
        if !*done {
            *done = true;
            promote_copy(&db, &from, "promoted-newer-snapshot", &stamp);
        }
    });
    let (_state, d) = daemon(root.root());
    let v = ok(run(&d, "check", json!({ "repo": f.canonical })));
    assert!(
        *promoted.lock().unwrap(),
        "a newer ready snapshot was promoted after the check, before its stamp read"
    );
    assert_eq!(v["value"]["snapshot"], evaluated.as_str(), "{v}");
    assert_eq!(
        v["value"]["toolchain_staleness"],
        json!({"state":"current"}),
        "the status describes the snapshot check evaluated, not the promoted one"
    );
}

fn unreadable_stamp_reindexes(stamp: Option<&str>, reason: &str) {
    let (root, f) = fixture(stamp);
    let (_state, d) = daemon(root.root());
    let st = status(&d, "orient", &f.canonical);
    assert_eq!(st["state"], "unknown", "{st}");
    assert_eq!(st["reason"], reason);
    assert!(matches!(reindex_of(&st), "queued" | "running"), "{st}");
    wait_reindexed(&f);
    assert_eq!(
        status(&d, "orient", &f.canonical),
        json!({"state":"current"})
    );
}

#[test]
fn missing_stamp_reports_unknown_and_starts_the_reindex() {
    let _s = serial();
    unreadable_stamp_reindexes(None, "this index has no toolchain stamp");
}

#[test]
fn malformed_stamp_reports_unknown_and_starts_the_reindex() {
    let _s = serial();
    unreadable_stamp_reindexes(
        Some("not json"),
        "this index's toolchain stamp is unreadable",
    );
}

#[test]
fn structurally_incomplete_stamp_reports_unknown_and_starts_the_reindex() {
    let _s = serial();
    unreadable_stamp_reindexes(
        Some(r#"{"extractors":["ts-core:0.2.0"]}"#),
        "this index's toolchain stamp is incomplete (no indexer version)",
    );
}

// ── the JSON shapes and verdict neutrality ───────────────────────────────────────────────────────

#[test]
fn check_json_toolchain_staleness_has_the_current_stale_and_unknown_shapes() {
    let _s = serial();
    let (stamp, one) = seeded();
    let (root, f) = fixture(Some(&running()));
    set_auto_reindex_for_test(Some(false));
    let (_state, d) = daemon(root.root());
    assert_eq!(
        status(&d, "check", &f.canonical),
        json!({"state":"current"})
    );
    set_stamp(&f.db_path, Some(&stamp));
    assert_eq!(
        status(&d, "check", &f.canonical),
        json!({"state":"stale","differences":one,"reindex":"disabled",
            "reindex_disabled_reason":"RMAP_AUTO_REINDEX=off"})
    );
    set_stamp(&f.db_path, None);
    assert_eq!(
        status(&d, "check", &f.canonical),
        json!({"state":"unknown","reason":"this index has no toolchain stamp","reindex":"disabled",
            "reindex_disabled_reason":"RMAP_AUTO_REINDEX=off"})
    );
    // Enabled and parked: `queued`/`running` carry neither `reindex_failure` nor a disabled reason.
    set_auto_reindex_for_test(None);
    let park = Park::install(true);
    set_stamp(&f.db_path, Some(&stamp));
    let st = status(&d, "check", &f.canonical);
    let mut keys: Vec<&String> = st.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["differences", "reindex", "state"], "{st}");
    wait_until("park", || park.started().len() == 1);
    park.release();
    wait_reindexed(&f);
}

#[test]
fn check_verdict_conditions_and_exit_status_are_unchanged_by_toolchain_staleness() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&running()));
    set_auto_reindex_for_test(Some(false));
    let (_state, d) = daemon(root.root());
    let signals = |d: &ServiceDispatcher| {
        let v = ok(run(d, "check", json!({ "repo": f.canonical })));
        (
            v["value"]["signals"].clone(),
            v["value"]["toolchain_staleness"]["state"].clone(),
        )
    };
    let (base, s0) = signals(&d);
    assert_eq!(s0, "current");
    assert!(base.as_array().is_some_and(|a| !a.is_empty()), "{base}");
    for (stamp, state) in [
        (Some(stamp.as_str()), "stale"),
        (None, "unknown"),
        (Some("{}"), "unknown"),
    ] {
        set_stamp(&f.db_path, stamp);
        let (sig, st) = signals(&d);
        assert_eq!(st, state);
        // Same verdict signal, conditions and hence exit status (the code maps 1:1 to the exit).
        assert_eq!(sig, base, "{state}");
    }
}

#[test]
fn current_stamp_starts_nothing_and_orient_renders_no_toolchain_line() {
    let _s = serial();
    let (root, f) = fixture(Some(&running()));
    let park = Park::install(false);
    let (_state, d) = daemon(root.root());
    assert_eq!(
        status(&d, "orient", &f.canonical),
        json!({"state":"current"})
    );
    assert_eq!(
        status(&d, "check", &f.canonical),
        json!({"state":"current"})
    );
    assert_nothing_starts(&f, &park);
}

#[test]
fn stdio_daemon_schedules_nothing_and_reports_disabled() {
    let _s = serial();
    let (stamp, one) = seeded();
    let (root, f) = fixture(Some(&stamp));
    let park = Park::install(false);
    let registry = RepoRegistry::with_state_root(root.root()).unwrap();
    let state = Arc::new(DaemonState::with_registry(registry));
    mark_stdio_transport(&state); // as run_daemon_stdio does, before the dispatcher
    let d = ServiceDispatcher::new(Arc::clone(&state));
    let expected = json!({"state":"stale","differences":one,"reindex":"disabled",
        "reindex_disabled_reason":STDIO_REASON});
    assert_eq!(status(&d, "orient", &f.canonical), expected);
    assert_eq!(status(&d, "check", &f.canonical), expected);
    assert_nothing_starts(&f, &park);
}

#[test]
fn rebuild_remedy_named_from_a_repository_subdirectory_rebuilds_the_repository() {
    let _s = serial();
    let (stamp, _) = seeded();
    let (root, f) = fixture(Some(&stamp));
    set_auto_reindex_for_test(Some(false));
    let (_state, d) = daemon(root.root());
    let sub = Path::new(&f.canonical)
        .join("sub dir")
        .canonicalize()
        .unwrap();
    let sub = sub.to_string_lossy().to_string();
    let st = status(&d, "check", &sub);
    assert_eq!(
        (st["state"].clone(), st["reindex"].clone()),
        (json!("stale"), json!("disabled"))
    );
    // `rmap repo rebuild '<sub>'` — the remedy exactly as printed from the subdirectory.
    let r = ok(run(
        &d,
        "repo_rebuild",
        json!({ "repo": sub, "confirm": true }),
    ));
    assert_eq!(r["repo_uid"], f.uid.as_str(), "{r}");
    let entries = RepoRegistry::with_state_root(root.root()).unwrap();
    let listed: Vec<(String, String)> = entries
        .list()
        .iter()
        .map(|e| (e.repo_uid.clone(), e.db_path.to_string_lossy().to_string()))
        .collect();
    assert_eq!(
        listed,
        vec![(f.uid.clone(), f.db_path.clone())],
        "the same registry entry"
    );
    assert_eq!(status(&d, "check", &sub), json!({"state":"current"}));
}
