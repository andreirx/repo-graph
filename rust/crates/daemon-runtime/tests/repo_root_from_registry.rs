//! STATE-ROOT-RELATIVE-REPO-ROOT-1 (RG-REQ-011-L13): a repository's working tree is found from
//! the daemon REGISTRY's absolute `canonical_path`; where a state root lives never changes an
//! answer; an unreachable root is named on every surface.
//!
//! Every test builds a real git checkout, a real registry and a real store under `tempfile` roots
//! (never the operator's state root) and drives the surfaces through the REAL `ServiceDispatcher`.
//! One store is indexed once under `<base>/s`, then COPIED to other state roots whose registry
//! points at the checkout (or at a missing path / a regular file), and the same request sequence is
//! answered from each — the SLICE_DOC §2.3 states:
//!   - another directory depth                 → the same root-dependent results;
//!   - a WRONG stored `repos.root_path` (an EXISTING different directory, `<base>/other`) → the same;
//!   - the registered root missing / a regular file → the §2.2 named outcome on every surface;
//!   - no registry entry for a store-keyed request or pass → `repo root unknown: …`.
//!
//! Serialized: the enrichment test seam (`set_test_registry_builder`) and the maintenance toggles
//! are process-global.

mod common;
use common::{index, init_git, run_git, Quiet};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use enrichment::{
    BatchResolution, EligibleEdge, EnrichmentLanguage, ReceiverTypeResolver, ReceiverTypeResult,
    ResolverError, ResolverProgress, ResolverRegistry,
};
use repo_graph_daemon_runtime::enrich_pass::{
    clear_test_registry_builder, set_test_registry_builder, try_enrich_attempt, EnrichAttempt,
};
use repo_graph_daemon_runtime::registry::RegistryFile;
use repo_graph_daemon_runtime::{DaemonState, RegistryEntry, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{DispatchResult, Dispatcher, Request};
use serde_json::{json, Value};
use tempfile::TempDir;

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

/// One indexed checkout: `<base>/checkout` (git, docs, TypeScript with one receiver call the
/// enrichment pipeline is eligible for), `<base>/other` (an existing different tree with its own
/// docs), and the original store + registry entry under `<base>/s`.
struct Fixture {
    _tmp: TempDir,
    base: PathBuf,
    checkout: PathBuf,
    other: PathBuf,
    entry: RegistryEntry,
}

fn fixture() -> Fixture {
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let checkout = base.join("checkout");
    make_checkout(&checkout);
    let other = base.join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("README.md"), "# a different tree\n").unwrap();
    std::fs::write(other.join("ARCHITECTURE.md"), "# not this repo\n").unwrap();

    let s = base.join("s");
    {
        let (dispatcher, _state) = serve(&s);
        index(&dispatcher, &checkout);
    }
    let file: RegistryFile =
        serde_json::from_str(&std::fs::read_to_string(s.join("registry.json")).unwrap()).unwrap();
    let entry = file.repos.into_iter().next().expect("the indexed entry");
    assert_eq!(entry.canonical_path, checkout);
    Fixture {
        _tmp: tmp,
        base,
        checkout,
        other,
        entry,
    }
}

/// A git checkout with docs and TypeScript holding one receiver call the enrichment pipeline is
/// eligible for (`service.start()`).
fn make_checkout(checkout: &Path) {
    std::fs::create_dir_all(checkout.join("docs")).unwrap();
    init_git(checkout);
    std::fs::write(checkout.join("README.md"), "# checkout\n").unwrap();
    std::fs::write(checkout.join("CONTRIBUTING.md"), "# contributing\n").unwrap();
    std::fs::write(checkout.join("docs/guide.md"), "# guide\n").unwrap();
    std::fs::write(
        checkout.join("helper.ts"),
        "export function helperFunction() {\n    return 1;\n}\n",
    )
    .unwrap();
    std::fs::write(
        checkout.join("main.ts"),
        "import { helperFunction } from './helper';\n\nexport function mainEntry(service: any) {\n    helperFunction();\n    return service.start();\n}\n",
    )
    .unwrap();
    run_git(checkout, &["add", "-A"]);
    run_git(checkout, &["commit", "-m", "c1"]);
}

fn serve(state_root: &Path) -> (ServiceDispatcher, Arc<DaemonState>) {
    let registry = RepoRegistry::with_state_root(state_root).unwrap();
    let state = Arc::new(DaemonState::with_registry(registry));
    (ServiceDispatcher::new(Arc::clone(&state)), state)
}

/// Copy the fixture's store into `<state_root>/databases/` and write a registry whose ONE entry
/// registers it at `registered_root`. `stored_root` (if any) overwrites the copy's
/// `repos.root_path` with the store-directory-relative path to that directory. Returns the copy.
fn place(
    fx: &Fixture,
    state_root: &Path,
    registered_root: &Path,
    stored_root: Option<&Path>,
) -> PathBuf {
    let db_dir = state_root.join("databases");
    std::fs::create_dir_all(&db_dir).unwrap();
    let db = db_dir.join(fx.entry.db_path.file_name().unwrap());
    for ext in ["", "-wal", "-shm"] {
        let src = PathBuf::from(format!("{}{ext}", fx.entry.db_path.display()));
        if src.exists() {
            std::fs::copy(&src, format!("{}{ext}", db.display())).unwrap();
        }
    }
    if let Some(target) = stored_root {
        let depth = db_dir.strip_prefix(&fx.base).unwrap().components().count();
        let rel = format!(
            "{}{}",
            "../".repeat(depth),
            target.strip_prefix(&fx.base).unwrap().display()
        );
        assert!(db_dir.join(&rel).is_dir(), "the wrong stored root exists");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute("UPDATE repos SET root_path = ?1", [&rel])
            .unwrap();
    }
    let mut entry = fx.entry.clone();
    entry.canonical_path = registered_root.to_path_buf();
    entry.db_path = db.clone();
    let file = RegistryFile {
        version: 1,
        repos: vec![entry],
    };
    std::fs::write(
        state_root.join("registry.json"),
        serde_json::to_string_pretty(&file).unwrap(),
    )
    .unwrap();
    db
}

fn call(d: &ServiceDispatcher, method: &str, params: Value) -> Result<Value, (String, String)> {
    let request = Request {
        id: method.to_string(),
        method: method.to_string(),
        params,
    };
    match d.dispatch(&request, &mut Quiet) {
        DispatchResult::Success(s) => Ok(s.result),
        DispatchResult::Error(e) => Err((e.error.code, e.error.message)),
    }
}

fn ok(d: &ServiceDispatcher, method: &str, params: Value) -> Value {
    call(d, method, params).unwrap_or_else(|(c, m)| panic!("{method} failed {c}: {m}"))
}

/// The first value under `key` anywhere in `v` (depth-first), `Null` when absent.
fn find(v: &Value, key: &str) -> Value {
    match v {
        Value::Object(m) => m
            .get(key)
            .cloned()
            .or_else(|| m.values().map(|x| find(x, key)).find(|x| !x.is_null()))
            .unwrap_or(Value::Null),
        Value::Array(a) => a
            .iter()
            .map(|x| find(x, key))
            .find(|x| !x.is_null())
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// The check condition object whose `code` is `code` (wherever the reducer listed it).
fn condition(v: &Value, code: &str) -> Value {
    match v {
        Value::Object(m) if m.get("code") == Some(&json!(code)) && m.contains_key("status") => {
            v.clone()
        }
        Value::Object(m) => m
            .values()
            .map(|x| condition(x, code))
            .find(|x| !x.is_null())
            .unwrap_or(Value::Null),
        Value::Array(a) => a
            .iter()
            .map(|x| condition(x, code))
            .find(|x| !x.is_null())
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// A hermetic TypeScript resolver that records the repo root the pipeline hands it.
struct RootCapture(Arc<Mutex<Option<PathBuf>>>);
impl ReceiverTypeResolver for RootCapture {
    fn language(&self) -> EnrichmentLanguage {
        EnrichmentLanguage::TypeScript
    }
    fn resolve_batch(
        &self,
        repo_root: &Path,
        edges: &[EligibleEdge],
        _progress: Option<&dyn ResolverProgress>,
        _cancel: Option<&dyn Fn() -> bool>,
    ) -> BatchResolution {
        *self.0.lock().unwrap() = Some(repo_root.to_path_buf());
        BatchResolution::from_results(
            edges
                .iter()
                .map(|e| ReceiverTypeResult::failed(e.edge_uid.clone(), "hermetic"))
                .collect(),
        )
    }
    fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
        Ok(())
    }
    fn shutdown(&mut self) {}
}

/// Install the capturing resolver for both the explicit `enrich` handler and the auto pass.
fn capture_enrichment_root() -> Arc<Mutex<Option<PathBuf>>> {
    let seen = Arc::new(Mutex::new(None));
    let s = Arc::clone(&seen);
    set_test_registry_builder(move |_langs: &[EnrichmentLanguage]| {
        let mut registry = ResolverRegistry::new();
        registry.register(Box::new(RootCapture(Arc::clone(&s))));
        registry
    });
    seen
}

fn auto_enrich(state: &Arc<DaemonState>, db: &Path, repo_uid: &str) -> EnrichAttempt {
    let gen = state.enrich_coord().bump_generation(repo_uid);
    try_enrich_attempt(state, &db.canonicalize().unwrap(), repo_uid, "display", gen)
}

fn describe(attempt: &EnrichAttempt) -> String {
    match attempt {
        EnrichAttempt::Ran(_) => "Ran".to_string(),
        EnrichAttempt::Yielded(r) => format!("Yielded({r})"),
        EnrichAttempt::Superseded => "Superseded".to_string(),
        EnrichAttempt::Failed(r) => format!("Failed({r})"),
    }
}

fn coverage_report(fx: &Fixture) -> PathBuf {
    let report = fx.base.join("coverage-final.json");
    let file = fx.checkout.join("helper.ts").display().to_string();
    let body = json!({ file.clone(): { "path": file, "s": { "0": 1, "1": 0 }, "f": {}, "b": {} } });
    std::fs::write(&report, body.to_string()).unwrap();
    report
}

/// The root-dependent result of every L13 surface, in one fixed request order (reads, then the
/// writes that change the store: coverage import, enrichment, refresh).
fn answers(fx: &Fixture, state_root: &Path, db: &Path) -> BTreeMap<&'static str, Value> {
    let (d, state) = serve(state_root);
    let repo = json!({ "repo": fx.checkout.display().to_string() });
    let uid = fx.entry.repo_uid.clone();
    let repo_state = state.load_repo(db, &uid).unwrap();
    let snapshot = fx.entry.last_snapshot_uid.clone().unwrap();
    let mut out = BTreeMap::new();

    // orient, fingerprint-less route (no resident LiveGraph).
    assert!(
        repo_graph_daemon_runtime::orient_serve::orient_serve_witness(&repo_state, &snapshot)
            .fingerprint
            .is_none()
    );
    let o = ok(&d, "orient", repo.clone());
    out.insert("orient.index_drift", find(&o, "index_drift"));
    out.insert("orient.orientation_docs", find(&o, "orientation_docs"));
    out.insert("orient.documentation", find(&o, "documentation"));
    out.insert(
        "check.index_drift",
        condition(&ok(&d, "check", repo.clone()), "INDEX_DRIFT"),
    );
    let mut explain = repo.clone();
    explain["target"] = json!("helper.ts");
    out.insert(
        "explain.index_drift",
        find(&ok(&d, "explain", explain), "index_drift"),
    );
    out.insert(
        "modules_list.orientation_docs",
        find(&ok(&d, "modules_list", repo.clone()), "orientation_docs"),
    );
    out.insert("docs_list", ok(&d, "docs_list", repo.clone()));
    out.insert("docs_extract", ok(&d, "docs_extract", repo.clone()));
    out.insert("churn", ok(&d, "churn", repo.clone()));
    out.insert("hotspots", ok(&d, "hotspots", repo.clone()));
    out.insert("risk", ok(&d, "risk", repo.clone()));
    let mut map = repo.clone();
    map["path"] = json!("");
    out.insert("map", ok(&d, "map", map));

    // orient, fingerprint route: a resident (empty) LiveGraph whose cycle-values cert is GREEN
    // against the store's (empty) cycle set gives the request a fingerprint.
    *repo_state.livegraph.write() = Some(repo_graph_livegraph::LiveGraph::new());
    assert!(
        repo_graph_daemon_runtime::orient_serve::orient_serve_witness(&repo_state, &snapshot)
            .fingerprint
            .is_some()
    );
    let o = ok(&d, "orient", repo.clone());
    out.insert("orient_fingerprint.index_drift", find(&o, "index_drift"));
    out.insert(
        "orient_fingerprint.orientation_docs",
        find(&o, "orientation_docs"),
    );
    out.insert(
        "orient_fingerprint.documentation",
        find(&o, "documentation"),
    );

    let mut coverage = repo.clone();
    coverage["report_path"] = json!(coverage_report(fx).display().to_string());
    out.insert("coverage", ok(&d, "coverage", coverage));

    // The auto-enrich pass (store-keyed) first, then the explicit `enrich` (forced: the pass
    // already recorded its attempt on the one eligible edge).
    let seen = capture_enrichment_root();
    let attempt = auto_enrich(&state, db, &uid);
    assert!(
        matches!(attempt, EnrichAttempt::Ran(_)),
        "{}",
        describe(&attempt)
    );
    out.insert("auto_enrich.root", json!(seen.lock().unwrap().take()));
    let mut enrich = repo.clone();
    enrich["languages"] = json!(["typescript"]);
    enrich["force"] = json!(true);
    let e = ok(&d, "enrich", enrich);
    out.insert("enrich.eligible_count", e["eligible_count"].clone());
    out.insert("enrich.root", json!(seen.lock().unwrap().take()));
    clear_test_registry_builder();

    let r = ok(&d, "refresh", repo.clone());
    assert!(r["snapshot_uid"].is_string());
    // The re-stamp is recomputed from the registry root (relative to this store's directory).
    let stored: String = rusqlite::Connection::open(db)
        .unwrap()
        .query_row("SELECT root_path FROM repos", [], |row| row.get(0))
        .unwrap();
    out.insert(
        "refresh.restamped_root",
        json!(db.parent().unwrap().join(stored).canonicalize().unwrap()),
    );
    out
}

fn assert_alike(
    a: &BTreeMap<&'static str, Value>,
    b: &BTreeMap<&'static str, Value>,
    fx: &Fixture,
) {
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (k, v) in a {
        assert_eq!(v, &b[k], "surface {k} answers differently");
    }
    // The shared answer is the checkout's, not an absence and not the other tree's.
    let root = json!(fx.checkout);
    assert_eq!(a["docs_list"]["repo_path"], root);
    assert_eq!(a["enrich.root"], root);
    assert_eq!(a["auto_enrich.root"], root);
    assert_eq!(a["refresh.restamped_root"], root);
    assert_eq!(a["enrich.eligible_count"], json!(1));
    let docs = a["docs_list"].to_string();
    assert!(
        docs.contains("CONTRIBUTING.md") && !docs.contains("ARCHITECTURE.md"),
        "{docs}"
    );
    for k in ["orient.documentation", "orient_fingerprint.documentation"] {
        assert!(a[k].to_string().contains("README.md"), "{k}: {}", a[k]);
    }
    for k in [
        "orient.orientation_docs",
        "orient_fingerprint.orientation_docs",
        "modules_list.orientation_docs",
    ] {
        assert!(
            a[k]["paths"].to_string().contains("README.md"),
            "{k}: {}",
            a[k]
        );
    }
    assert_eq!(a["orient.index_drift"]["state"], json!("clean"));
    assert_eq!(a["coverage"]["imported_count"], json!(1));
    assert_eq!(
        a["map"]["repo_root"],
        json!(fx.checkout.display().to_string())
    );
    assert!(a["churn"]["count"].as_u64().unwrap() >= 1, "{}", a["churn"]);
}

#[test]
fn same_store_at_two_state_root_depths_answers_alike() {
    let _g = serial();
    let fx = fixture();
    let deep = fx.base.join("d/a/b/c");
    let deep_db = place(&fx, &deep, &fx.checkout, None);
    let shallow = answers(&fx, &fx.base.join("s"), &fx.entry.db_path);
    let deeper = answers(&fx, &deep, &deep_db);
    assert_alike(&shallow, &deeper, &fx);
}

#[test]
fn a_wrong_stored_root_path_changes_no_answer() {
    let _g = serial();
    let fx = fixture();
    let wrong = fx.base.join("w");
    let wrong_db = place(&fx, &wrong, &fx.checkout, Some(&fx.other));
    let right = answers(&fx, &fx.base.join("s"), &fx.entry.db_path);
    let with_wrong_field = answers(&fx, &wrong, &wrong_db);
    assert_alike(&right, &with_wrong_field, &fx);
}

/// The repository a test asks about through a dispatcher: its store, uid and READY snapshot.
struct Asked<'a> {
    d: &'a ServiceDispatcher,
    state: &'a Arc<DaemonState>,
    db: &'a Path,
    repo_uid: &'a str,
    snapshot_uid: &'a str,
}

/// Every L13 surface answers the §2.2 named outcome for an unreachable registered root; orient on
/// BOTH routes (without, then with a LiveGraph fingerprint).
fn assert_named_everywhere(fx: &Fixture, asked: &Asked<'_>, root: &Path) {
    let (d, state) = (asked.d, asked.state);
    let (db, repo_uid) = (asked.db, asked.repo_uid);
    // The remedy's path goes through `shell_quote`, which leaves a path of these bytes unquoted.
    assert!(
        root.to_str()
            .unwrap()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/')),
        "the fixture path needs no shell quoting: {}",
        root.display()
    );
    // The registry resolves a missing registered path by exact match (FORGET-REPO-1), and a file
    // path canonicalizes to itself — so the request names the registered root as the CLI would.
    let repo = json!({ "repo": root.display().to_string() });
    let short = format!("repo root not found: {}", root.display());
    let long = format!(
        "{short} (missing or not a directory) — if the repository still exists, run rmap index \
         inside its directory; or remove this registration: rmap repo remove {}",
        root.display()
    );
    let with = |k: &str, v: Value| {
        let mut p = repo.clone();
        p[k] = v;
        p
    };
    let requests: Vec<(&str, Value)> = vec![
        ("docs_list", repo.clone()),
        ("docs_extract", repo.clone()),
        ("churn", repo.clone()),
        ("hotspots", repo.clone()),
        ("risk", repo.clone()),
        (
            "coverage",
            with(
                "report_path",
                json!(coverage_report(fx).display().to_string()),
            ),
        ),
        ("map", with("path", json!(""))),
        ("refresh", repo.clone()),
        ("enrich", with("languages", json!(["typescript"]))),
    ];
    for (method, params) in requests {
        let (code, message) = call(d, method, params).expect_err(method);
        assert_eq!(code, "InvalidRequest", "{method}");
        assert_eq!(message, long, "{method}");
        assert!(!message.contains("git"), "{method}: no git failure text");
    }
    assert!(
        !root.join("MAP.md").exists(),
        "map planned nothing under the root"
    );

    // orient, both routes. The bare route first (no resident LiveGraph), then the fingerprint
    // route (a resident, empty LiveGraph whose cycle-values cert is GREEN against the store's empty
    // cycle set) — each answer names the root.
    let repo_state = state.load_repo(db, repo_uid).unwrap();
    let mut orients = Vec::new();
    for with_fingerprint in [false, true] {
        *repo_state.livegraph.write() = if with_fingerprint {
            Some(repo_graph_livegraph::LiveGraph::new())
        } else {
            None
        };
        assert_eq!(
            repo_graph_daemon_runtime::orient_serve::orient_serve_witness(
                &repo_state,
                asked.snapshot_uid
            )
            .fingerprint
            .is_some(),
            with_fingerprint,
            "orient route"
        );
        let o = ok(d, "orient", repo.clone());
        let drift = find(&o, "index_drift");
        assert_eq!(
            drift["state"],
            json!("unknown"),
            "orient drift (fingerprint {with_fingerprint})"
        );
        assert_eq!(
            drift["reason"],
            json!(short),
            "orient drift (fingerprint {with_fingerprint})"
        );
        assert_eq!(
            find(&o, "orientation_docs"),
            json!({ "unavailable": short }),
            "orient orientation docs (fingerprint {with_fingerprint})"
        );
        assert!(
            find(&o, "documentation").is_null(),
            "no Docs line on an unreachable root (fingerprint {with_fingerprint})"
        );
        orients.push(o);
    }
    *repo_state.livegraph.write() = None;
    let c = ok(d, "check", repo.clone());
    let drift_condition = condition(&c, "INDEX_DRIFT");
    assert_eq!(drift_condition["status"], json!("incomplete"), "check: {c}");
    assert!(
        drift_condition["summary"]
            .as_str()
            .unwrap()
            .contains(&format!("({short})")),
        "check names the root: {drift_condition}"
    );
    let mut explain = repo.clone();
    explain["target"] = json!("helper.ts");
    let x = ok(d, "explain", explain);
    assert_eq!(
        find(&x, "index_drift")["reason"],
        json!(short),
        "explain drift"
    );
    let m = ok(d, "modules_list", repo.clone());
    assert_eq!(
        find(&m, "orientation_docs"),
        json!({ "unavailable": short })
    );
    for v in orients.iter().chain([&c, &x, &m]) {
        let text = v.to_string();
        assert!(
            !text.contains("No README") && !text.contains("git could not be probed"),
            "{text}"
        );
    }

    match auto_enrich(state, db, repo_uid) {
        EnrichAttempt::Failed(reason) => assert_eq!(reason, short),
        other => panic!(
            "auto-enrich must fail naming the root: {}",
            describe(&other)
        ),
    }

    // The seed pass (store-keyed, spawned detached) skips naming the root — checked before the
    // corpus read and the model load, so no model is needed. Enabled only for this window.
    repo_graph_daemon_runtime::oplog::enable_oplog_capture_for_test();
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(true);
    repo_graph_daemon_runtime::seed_pass::spawn_auto_seed(
        Arc::clone(state),
        db.canonicalize().unwrap(),
        repo_uid.to_string(),
        "display".to_string(),
    );
    let expected = format!("skipped: {short}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let seen = loop {
        let hit = repo_graph_daemon_runtime::oplog::oplog_lines_for_test()
            .into_iter()
            .any(|l| l.contains(repo_uid) && l.contains(&expected));
        if hit || std::time::Instant::now() > deadline {
            break hit;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    assert!(seen, "the seed pass logs `{expected}`");
}

#[test]
fn missing_registered_root_is_named_on_every_surface() {
    let _g = serial();
    let fx = fixture();
    let gone = fx.base.join("gone");
    let root = fx.base.join("m");
    let db = place(&fx, &root, &gone, None);
    let (d, state) = serve(&root);
    let asked = Asked {
        d: &d,
        state: &state,
        db: &db,
        repo_uid: &fx.entry.repo_uid,
        snapshot_uid: fx.entry.last_snapshot_uid.as_deref().unwrap(),
    };
    assert_named_everywhere(&fx, &asked, &gone);
    assert!(!gone.exists(), "nothing was created at the missing root");
}

#[test]
fn registered_root_that_is_a_file_is_named_on_every_surface() {
    let _g = serial();
    let fx = fixture();
    let file = fx.base.join("a-file");
    std::fs::write(&file, "not a directory").unwrap();
    let root = fx.base.join("f");
    let db = place(&fx, &root, &file, None);
    let (d, state) = serve(&root);
    let asked = Asked {
        d: &d,
        state: &state,
        db: &db,
        repo_uid: &fx.entry.repo_uid,
        snapshot_uid: fx.entry.last_snapshot_uid.as_deref().unwrap(),
    };
    assert_named_everywhere(&fx, &asked, &file);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "not a directory");
}

#[test]
fn docs_repo_path_is_the_registry_canonical_path() {
    let _g = serial();
    let fx = fixture();
    let deep = fx.base.join("d/a/b/c");
    place(&fx, &deep, &fx.checkout, None);
    let (d, _state) = serve(&deep);
    let repo = json!({ "repo": fx.checkout.display().to_string() });
    let expected = json!(fx.checkout.display().to_string());
    assert_eq!(ok(&d, "docs_list", repo.clone())["repo_path"], expected);
    assert_eq!(ok(&d, "docs_extract", repo)["repo_path"], expected);
}

#[test]
fn store_keyed_enrich_without_a_registry_entry_is_named() {
    let _g = serial();
    let fx = fixture();
    // A state root whose registry has NO entry for the store; the repo is loaded by store path.
    let root = fx.base.join("u");
    let db = place(&fx, &root, &fx.checkout, None);
    std::fs::remove_file(root.join("registry.json")).unwrap();
    let (d, state) = serve(&root);
    let db = db.canonicalize().unwrap();
    state.load_repo(&db, &fx.entry.repo_uid).unwrap();
    let short = format!(
        "repo root unknown: no registry entry for {} ({})",
        db.display(),
        fx.entry.repo_uid
    );
    let (code, message) = call(
        &d,
        "enrich",
        json!({ "db_path": db.display().to_string(), "repo_uid": fx.entry.repo_uid, "languages": ["typescript"] }),
    )
    .expect_err("legacy enrich without a registry entry");
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("InvalidRequest", short.as_str())
    );
    match auto_enrich(&state, &db, &fx.entry.repo_uid) {
        EnrichAttempt::Failed(reason) => assert_eq!(reason, short),
        other => panic!(
            "the auto pass must fail naming the missing entry: {}",
            describe(&other)
        ),
    }
}

#[test]
fn refresh_on_a_relocated_state_root_succeeds() {
    let _g = serial();
    let fx = fixture();
    let deep = fx.base.join("d/a/b/c");
    let db = place(&fx, &deep, &fx.checkout, None);
    let (d, _state) = serve(&deep);
    let repo = json!({ "repo": fx.checkout.display().to_string() });
    let refreshed = ok(&d, "refresh", repo.clone());
    assert!(refreshed["snapshot_uid"].is_string(), "{refreshed}");
    let stored: String = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row("SELECT root_path FROM repos", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        db.parent().unwrap().join(&stored).canonicalize().unwrap(),
        fx.checkout,
        "refresh re-stamped root_path relative to the relocated store, from the registry root"
    );
    let docs = ok(&d, "docs_list", repo);
    assert!(docs.to_string().contains("README.md"), "{docs}");
}

#[test]
fn registered_root_removed_after_registration_is_named() {
    let _g = serial();
    let fx = fixture();
    // Register an EXISTING directory through a real `index`, prove the daemon reads it, then
    // remove the directory under the running daemon and ask every surface again.
    let later = fx.base.join("later");
    make_checkout(&later);
    let root = fx.base.join("r");
    let (d, state) = serve(&root);
    index(&d, &later);
    let file: RegistryFile =
        serde_json::from_str(&std::fs::read_to_string(root.join("registry.json")).unwrap())
            .unwrap();
    let entry = file.repos.into_iter().next().expect("the registered entry");
    assert_eq!(entry.canonical_path, later);
    let repo = json!({ "repo": later.display().to_string() });
    let before = ok(&d, "docs_list", repo);
    assert_eq!(before["repo_path"], json!(later.display().to_string()));
    assert!(before.to_string().contains("README.md"), "{before}");

    std::fs::remove_dir_all(&later).unwrap();
    let asked = Asked {
        d: &d,
        state: &state,
        db: &entry.db_path,
        repo_uid: &entry.repo_uid,
        snapshot_uid: entry.last_snapshot_uid.as_deref().unwrap(),
    };
    assert_named_everywhere(&fx, &asked, &later);
    assert!(!later.exists(), "nothing was created at the removed root");
}

#[test]
fn refresh_hands_the_registry_root_to_enrich_and_seed() {
    let _g = serial();
    let fx = fixture();
    let deep = fx.base.join("d/a/b/c");
    place(&fx, &deep, &fx.checkout, None);
    let (d, _state) = serve(&deep);

    // The passes refresh starts, ENABLED: enrich observed through its resolver seam, seed through
    // the seed call site's root hook, which ends the seed pass right after its lookup — before any
    // model load, so nothing is fetched (D-SRR-SEED-OBSERVABLE-1 with Correction 1).
    let enrich_seen = capture_enrichment_root();
    repo_graph_daemon_runtime::repo_root::set_test_seed_root_observer(true);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(true);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(true);

    let refreshed = call(
        &d,
        "refresh",
        json!({ "repo": fx.checkout.display().to_string() }),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let (enrich_root, seed_roots) = loop {
        let enrich_root = enrich_seen.lock().unwrap().clone();
        let seed_roots: Vec<PathBuf> =
            repo_graph_daemon_runtime::repo_root::test_seed_roots_observed()
                .into_iter()
                .filter(|(uid, _)| uid == &fx.entry.repo_uid)
                .map(|(_, root)| root)
                .collect();
        if (enrich_root.is_some() && !seed_roots.is_empty()) || std::time::Instant::now() > deadline
        {
            break (enrich_root, seed_roots);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };

    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    repo_graph_daemon_runtime::repo_root::clear_test_seed_root_observer();
    clear_test_registry_builder();

    let refreshed = refreshed.unwrap_or_else(|(c, m)| panic!("refresh failed {c}: {m}"));
    assert!(refreshed["snapshot_uid"].is_string(), "{refreshed}");
    assert_eq!(
        enrich_root.as_deref(),
        Some(fx.checkout.as_path()),
        "the auto-enrich pass received the registry root"
    );
    assert_eq!(
        seed_roots,
        vec![fx.checkout.clone()],
        "the seed pass received the registry root (a seed-attributed record is required)"
    );
}
