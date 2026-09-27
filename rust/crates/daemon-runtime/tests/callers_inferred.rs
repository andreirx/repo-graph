//! PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L02/L03/L09, RG-REQ-002-L11; D-PRB-RATE-1 = A;
//! D-PRB-SCOPE-1 amendments 3–5) — the WIRE proof through the REAL `ServiceDispatcher`.
//!
//! Each test indexes a small Python repository the test writes into an ISOLATED state root (the
//! operator's registry and daemon are never touched) and asserts the JSON payloads — no human
//! rendering (that is rgr's). The fixture holds exactly the shapes the slice changes:
//!   * an untyped-receiver unique-name call (`r.helper()`, `store.persist()`) — INFERRED;
//!   * a carrier self call that hits its own class (`self.helper()` in `Runner.run`) — certain;
//!   * a carrier self call whose class derives from `list` and whose method name is unique
//!     elsewhere (`self.lengthen()` in `Items.grow`) — a hierarchy miss;
//!   * a module-level function taking `self` that calls `self.<unique>()` — no carrier;
//!   * an untyped-receiver call whose method name has two candidates (`store.values()`);
//!   * a function with zero certain callers and an unresolved same-name call (`A.values`);
//!   * a two-function chain linked only by an inferred call (`use` → `Store.persist`).

use std::path::Path;

use repo_graph_daemon_runtime::ServiceDispatcher;
use repo_graph_daemon_transport::{DispatchResult, Dispatcher, Request};
use serde_json::{json, Value};

mod common;

const APP: &str = "\
class Store:
    def persist(self):
        return 1


class Runner:
    def run(self):
        return self.helper()

    def helper(self):
        return 2


def poke(r):
    return r.helper()


def free(self):
    return self.helper()


class Items(list):
    def grow(self):
        return self.lengthen()


def lengthen():
    return 3


def module_level(self):
    return self.unique_thing()


def unique_thing():
    return 4


class A:
    def values(self):
        return 1


class B:
    def values(self):
        return 2


def use(store):
    unique_thing()
    store.persist()
    return store.values()


def lonely():
    return 5


def call_lonely():
    return lonely()
";

/// Line of the first occurrence of `needle` in [`APP`] (1-based).
fn line_of(needle: &str) -> u64 {
    APP.lines()
        .position(|l| l.contains(needle))
        .map(|i| i as u64 + 1)
        .unwrap_or_else(|| panic!("{needle} not in fixture"))
}

fn setup() -> (ServiceDispatcher, tempfile::TempDir, tempfile::TempDir) {
    let (dispatcher, state_root) = common::isolated();
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("app.py"), APP).unwrap();
    common::index(&dispatcher, repo.path());
    (dispatcher, state_root, repo)
}

fn call(dispatcher: &ServiceDispatcher, method: &str, repo: &Path, extra: Value) -> Value {
    let mut params = json!({ "repo": repo.to_string_lossy() });
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    let mut emitter = common::Quiet;
    match dispatcher.dispatch(
        &Request {
            id: method.to_string(),
            method: method.to_string(),
            params,
        },
        &mut emitter,
    ) {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!("{method} failed {}: {}", e.error.code, e.error.message),
    }
}

fn names(rows: &Value) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r["qualified_name"]
                .as_str()
                .or(r["name"].as_str())
                .unwrap()
                .to_string()
        })
        .collect()
}

/// Every value under `key` anywhere in `v`.
fn walk<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                if k == key {
                    out.push(x);
                }
                walk(x, key, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| walk(x, key, out)),
        _ => {}
    }
}

#[test]
fn callers_default_lists_certain_rows_at_call_sites_and_states_the_remainders() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "callers",
        repo.path(),
        json!({ "symbol": "Runner.helper", "engine": "sqlite" }),
    );
    assert_eq!(v["count"], 1, "the certain caller only");
    assert_eq!(names(&v["callers"]), vec!["Runner.run"]);
    assert_eq!(
        v["callers"][0]["line"],
        json!(line_of("return self.helper()")),
        "anchored at the call site, not the caller's declaration"
    );
    assert_eq!(
        v["inferred"],
        json!({"count": 1, "by_basis": {"receiver_untyped_name_only": 1}})
    );
    let un = &v["unresolved_naming"];
    assert_eq!(un["name"], "helper");
    assert_eq!(un["count"], 1);
    assert_eq!(
        un["by_basis"],
        json!({"self_call_without_class_context": 1})
    );
}

#[test]
fn callers_include_inferred_lists_each_inferred_row_with_its_basis() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "callers",
        repo.path(),
        json!({ "symbol": "Runner.helper", "engine": "sqlite", "include_inferred": true }),
    );
    assert_eq!(v["count"], 2, "certain + inferred");
    assert_eq!(
        names(&v["callers"]),
        vec!["Runner.run", "poke"],
        "certain rows first"
    );
    let inf = &v["callers"][1];
    assert_eq!(inf["resolution"], "inferred");
    assert_eq!(inf["inference_basis"], "receiver_untyped_name_only");
    assert_eq!(inf["inference_extractor"], "python-core:0.2.0");
    assert_eq!(inf["line"], json!(line_of("return r.helper()")));
    assert!(v["callers"][0].get("inference_basis").is_none());
    assert_eq!(v["inferred"]["count"], 1, "the remainder is still stated");
}

#[test]
fn callees_default_lists_certain_rows_and_states_the_remainders() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "callees",
        repo.path(),
        json!({ "symbol": "use", "engine": "sqlite" }),
    );
    assert_eq!(v["count"], 1);
    assert_eq!(names(&v["callees"]), vec!["unique_thing"]);
    assert_eq!(
        v["inferred"],
        json!({"count": 1, "by_basis": {"receiver_untyped_name_only": 1}})
    );
    assert_eq!(v["unresolved_from"]["count"], 1);
    assert!(
        v.get("unresolved_naming").is_none(),
        "callees state what leaves the symbol"
    );
}

#[test]
fn callers_without_inferred_or_unresolved_rows_carry_no_remainder_fields() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "callers",
        repo.path(),
        json!({ "symbol": "lonely", "engine": "sqlite" }),
    );
    assert_eq!(v["count"], 1);
    for key in ["inferred", "unresolved_naming", "unresolved_from"] {
        assert!(v.get(key).is_none(), "{key} absent: nothing to state");
    }
}

#[test]
fn self_call_hierarchy_miss_is_stored_unresolved_with_its_basis_and_pool() {
    let (d, _s, repo) = setup();
    // `Items(list).grow` → `self.lengthen()`: the walk finds no `lengthen` in Items' indexed
    // hierarchy; the module-level `lengthen` is recorded as a candidate, never bound.
    let v = call(
        &d,
        "callers",
        repo.path(),
        json!({ "symbol": "lengthen", "engine": "sqlite" }),
    );
    assert_eq!(v["count"], 0, "never bound by name");
    let un = &v["unresolved_naming"];
    assert_eq!(un["by_basis"], json!({"self_call_hierarchy_miss": 1}));
    let site = &un["sites"][0];
    assert_eq!(site["line"], json!(line_of("return self.lengthen()")));
    assert_eq!(site["target_key"], "self.lengthen");
    assert_eq!(site["candidate_reason"], "self_call_hierarchy_miss");
    assert_eq!(site["candidates"], json!(["lengthen"]));
    assert_eq!(site["candidate_count"], 1);
}

#[test]
fn self_call_outside_a_class_is_stored_unresolved_with_its_basis_and_pool() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "callers",
        repo.path(),
        json!({ "symbol": "unique_thing", "engine": "sqlite" }),
    );
    assert_eq!(
        names(&v["callers"]),
        vec!["use"],
        "the bare call stays certain"
    );
    let un = &v["unresolved_naming"];
    assert_eq!(
        un["by_basis"],
        json!({"self_call_without_class_context": 1})
    );
    assert_eq!(
        un["sites"][0]["candidate_reason"],
        "self_call_without_class_context"
    );
    assert_eq!(un["sites"][0]["candidates"], json!(["unique_thing"]));
}

#[test]
fn callees_unresolved_from_lists_each_site_with_its_recorded_candidates() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "callees",
        repo.path(),
        json!({ "symbol": "use", "engine": "sqlite" }),
    );
    let sites = v["unresolved_from"]["sites"].as_array().unwrap();
    assert_eq!(sites.len(), 1);
    let s = &sites[0];
    assert_eq!(s["target_key"], "store.values");
    assert_eq!(s["line"], json!(line_of("return store.values()")));
    assert_eq!(s["candidate_reason"], "ambiguous_name");
    assert_eq!(s["candidate_count"], 2);
    assert_eq!(s["candidates"], json!(["A.values", "B.values"]));
    // The zero-certain-callers symbol names the same unresolved call (RG-REQ-005-L09).
    let z = call(
        &d,
        "callers",
        repo.path(),
        json!({ "symbol": "A.values", "engine": "sqlite" }),
    );
    assert_eq!(z["count"], 0);
    assert_eq!(z["unresolved_naming"]["count"], 1);
    assert_eq!(z["unresolved_naming"]["name"], "values");
}

#[test]
fn explain_symbol_payload_carries_the_remainders() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "explain",
        repo.path(),
        json!({ "target": "Runner.helper" }),
    );
    let mut inferred = Vec::new();
    walk(&v, "inferred_count", &mut inferred);
    assert!(
        inferred.contains(&&json!(1)),
        "explain carries the inferred count: {inferred:?}"
    );
    let mut naming = Vec::new();
    walk(&v, "unresolved_naming", &mut naming);
    assert!(
        naming
            .iter()
            .any(|n| n["count"] == 1 && n["by_basis"]["self_call_without_class_context"] == 1),
        "{naming:?}"
    );
}

#[test]
fn path_without_a_certain_route_states_the_inferred_edges_on_route_and_the_search_depth() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "path",
        repo.path(),
        json!({ "from": "use", "to": "Store.persist", "engine": "sqlite" }),
    );
    assert_eq!(v["found"], false, "no route through certain edges");
    assert_eq!(v["inferred_edges_on_route"], 1);
    assert_eq!(v["search_depth"], 8);
}

#[test]
fn path_with_a_certain_route_carries_no_inferred_field() {
    let (d, _s, repo) = setup();
    let v = call(
        &d,
        "path",
        repo.path(),
        json!({ "from": "call_lonely", "to": "lonely", "engine": "sqlite" }),
    );
    assert_eq!(v["found"], true);
    assert!(v.get("inferred_edges_on_route").is_none());
    assert!(v.get("search_depth").is_none());
}

/// The number of inferred CALLS the fixture produces (r.helper, store.persist).
const INFERRED: u64 = 2;

#[test]
fn trust_and_check_state_the_inferred_calls_beside_the_rate() {
    let (d, _s, repo) = setup();
    let t = call(&d, "trust", repo.path(), json!({}));
    let mut found = Vec::new();
    walk(&t, "inferred_calls", &mut found);
    assert!(
        !found.is_empty() && found.iter().all(|x| **x == json!(INFERRED)),
        "{found:?}"
    );
    let c = call(&d, "check", repo.path(), json!({}));
    let text = c.to_string();
    assert!(
        text.contains(&format!(
            "+{INFERRED} inferred calls not counted as resolved"
        )),
        "check's CALL_GRAPH_RELIABILITY summary states the inferred calls"
    );
}

#[test]
fn orient_payload_call_coverage_carries_the_inferred_count() {
    let (d, _s, repo) = setup();
    let o = common::orient(&d, repo.path());
    let mut found = Vec::new();
    walk(&o, "inferred_calls", &mut found);
    assert!(
        !found.is_empty() && found.iter().all(|x| **x == json!(INFERRED)),
        "{found:?}"
    );
}

/// Every `*.db` store under the isolated state root.
fn stores_under(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            stores_under(&p, out);
        } else if p.extension().is_some_and(|e| e == "db") {
            out.push(p);
        }
    }
}

fn refused(dispatcher: &ServiceDispatcher, method: &str, repo: &Path, extra: Value) -> String {
    let mut params = json!({ "repo": repo.to_string_lossy() });
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    let mut emitter = common::Quiet;
    match dispatcher.dispatch(
        &Request {
            id: method.to_string(),
            method: method.to_string(),
            params,
        },
        &mut emitter,
    ) {
        DispatchResult::Success(s) => panic!("{method} answered {}", s.result),
        DispatchResult::Error(e) => e.error.message,
    }
}

/// Review F-2 (RG-REQ-002-L04/L11): a malformed input is refused by name, never read as a
/// default — a non-boolean `include_inferred` is an invalid request (never `false`), and a stored
/// CALLS resolution outside `static | dynamic | inferred` refuses `callers`, `callees`, `explain`,
/// `path` and `reliability`'s live rate (never "0 callers" or a complete-looking rate). `trust`
/// serves the aggregate the indexer persisted from its own typed output and is not re-read here.
#[test]
fn callers_refuse_a_non_boolean_include_inferred_and_an_unreadable_stored_resolution() {
    let (dispatcher, state_root, repo) = setup();
    for bad in [json!("yes"), json!(1), Value::Null] {
        for method in ["callers", "callees"] {
            let symbol = if method == "callers" {
                "Store.persist"
            } else {
                "use"
            };
            let msg = refused(
                &dispatcher,
                method,
                repo.path(),
                json!({ "symbol": symbol, "include_inferred": bad.clone() }),
            );
            assert!(msg.contains("include_inferred"), "{method} {bad}: {msg}");
        }
    }
    // A readable store answers first.
    call(
        &dispatcher,
        "callers",
        repo.path(),
        json!({ "symbol": "Store.persist" }),
    );

    let mut stores = Vec::new();
    stores_under(state_root.path(), &mut stores);
    let mut rewritten = 0;
    for db in &stores {
        let conn = rusqlite::Connection::open(db).unwrap();
        let has_edges: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'edges'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if has_edges == 1 {
            rewritten += conn
                .execute(
                    "UPDATE edges SET resolution = 'resolved' WHERE type = 'CALLS' AND resolution = 'inferred'",
                    [],
                )
                .unwrap();
        }
    }
    assert!(
        rewritten > 0,
        "the fixture holds inferred CALLS rows to rewrite"
    );

    for (method, extra) in [
        ("callers", json!({ "symbol": "Store.persist" })),
        ("callees", json!({ "symbol": "use" })),
        ("explain", json!({ "target": "Store.persist" })),
        (
            "path",
            json!({ "from": "use", "to": "Store.persist", "engine": "sqlite" }),
        ),
        ("reliability", json!({})),
    ] {
        let msg = refused(&dispatcher, method, repo.path(), extra);
        assert!(
            msg.contains("unreadable resolution") && msg.contains("\"resolved\""),
            "{method}: {msg}"
        );
    }
}
