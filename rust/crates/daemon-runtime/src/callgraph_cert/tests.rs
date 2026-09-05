//! COHERENCE-LEAF-SERVE-IMPL-1: callgraph no-loss cert tests.
//!
//! Two layers: (1) PURE multiset-equality unit tests over `AgentCallerRow`/`AgentCalleeRow` (the
//! order-insensitive, multiplicity-preserving compare orient's `group_by_module`+count consumption
//! demands); (2) the cert BUILD over the faithful fixture — GREEN proves the LiveGraph caller/callee
//! rows are field-exact equal to the SQLite rows (the byte/value PARITY proof, V1), RED proves a
//! dropped CALLS edge is caught (the no-loss gate actually gates).

use super::*;
use repo_graph_agent::{AgentCalleeRow, AgentCallerRow};

fn caller(stable_key: &str, name: &str, module: Option<&str>) -> AgentCallerRow {
    AgentCallerRow {
        stable_key: stable_key.into(),
        name: name.into(),
        file: Some("src/a.ts".into()),
        line: None,
        module_path: module.map(str::to_string),
        module_stable_key: module.map(|m| format!("repo:{m}:MODULE")),
    }
}
fn callee(stable_key: &str, name: &str) -> AgentCalleeRow {
    AgentCalleeRow {
        stable_key: stable_key.into(),
        name: name.into(),
        file: None,
        line: None,
        module_path: None,
        module_stable_key: None,
    }
}

// ── PURE multiset-equality unit tests ───────────────────────────────────────────────────────────

#[test]
fn callers_multiset_eq_is_order_insensitive() {
    let a = vec![
        caller("k1", "f", Some("src")),
        caller("k2", "g", Some("lib")),
    ];
    let b = vec![
        caller("k2", "g", Some("lib")),
        caller("k1", "f", Some("src")),
    ];
    assert!(
        callers_multiset_eq(&a, &b),
        "same rows, different order -> equal"
    );
}

#[test]
fn callers_multiset_eq_preserves_multiplicity() {
    // A repeated CALLS edge (caller k1 twice) must NOT collapse: count is part of the no-loss proof.
    let a = vec![
        caller("k1", "f", Some("src")),
        caller("k1", "f", Some("src")),
    ];
    let b = vec![caller("k1", "f", Some("src"))];
    assert!(
        !callers_multiset_eq(&a, &b),
        "multiplicity 2 != 1 -> a set would wrongly pass; the multiset must catch it"
    );
}

#[test]
fn callers_multiset_eq_catches_field_divergence() {
    // Same key, divergent module enrichment -> NOT equal (the full-row compare, not just keys).
    let a = vec![caller("k1", "f", Some("src"))];
    let b = vec![caller("k1", "f", Some("lib"))];
    assert!(!callers_multiset_eq(&a, &b));
}

#[test]
fn callees_multiset_eq_basic() {
    let a = vec![callee("k1", "f"), callee("k2", "g")];
    let b = vec![callee("k2", "g"), callee("k1", "f")];
    assert!(callees_multiset_eq(&a, &b));
    assert!(!callees_multiset_eq(&a, &[callee("k1", "f")]));
}

// ── Cert BUILD over the faithful fixture (V1 PARITY by construction) ─────────────────────────────

#[test]
fn callgraph_cert_green_on_faithful_mirror() {
    // The LiveGraph caller/callee rows equal the SQLite rows field-exact (incl. the FILE->OWNS->MODULE
    // enrichment) over the corpus -> GREEN. A GREEN field-exact cert IS the byte/value parity proof.
    let f = test_fixture::build_fixture(false);
    assert!(
        callgraph_is_green(&f.state, &f.snapshot_uid),
        "callgraph cert GREEN when the LiveGraph rows equal the SQLite rows"
    );
    // The cert is cached at the live fingerprint (a second call reuses it, no rebuild needed).
    assert!(callgraph_is_green(&f.state, &f.snapshot_uid));
    let cached = f.state.callgraph_cert.read();
    assert_eq!(cached.as_ref().unwrap().verdict, "GREEN");
}

#[test]
fn callgraph_cert_red_when_sqlite_drops_a_calls_edge() {
    // SQLite omits the caller -> callee CALLS edge, so SQLite `find_symbol_callers(callee)` is empty
    // while the LiveGraph has the caller -> the multiset compare diverges -> RED -> SQLite fallback.
    let f = test_fixture::build_fixture(true);
    assert!(
        !callgraph_is_green(&f.state, &f.snapshot_uid),
        "callgraph cert RED when SQLite is missing a CALLS edge the LiveGraph has"
    );
    let cached = f.state.callgraph_cert.read();
    assert_eq!(cached.as_ref().unwrap().verdict, "RED");
}

#[test]
fn callgraph_cert_red_without_livegraph() {
    // No resident LiveGraph -> the producer cannot corroborate -> never GREEN (safe SQLite default).
    let f = test_fixture::build_fixture(false);
    *f.state.livegraph.write() = None;
    assert!(!callgraph_is_green(&f.state, &f.snapshot_uid));
}

#[test]
fn lg_caller_rows_match_sqlite_rows_field_exact() {
    // Direct row-level parity: the LiveGraph caller rows for `callee` equal the SQLite rows (the same
    // multiset the decorator serves). Proves the enrichment (name/file/module) is reproduced no-loss.
    let f = test_fixture::build_fixture(false);
    let guard = f.state.livegraph.read();
    let lg = guard.as_ref().unwrap();
    let lg_rows = lg_caller_rows(lg, &test_fixture::callee_key()).expect("Exact caller rows");
    let sq_rows = repo_graph_agent::AgentStorageRead::find_symbol_callers(
        &f.state.storage().unwrap(),
        &f.snapshot_uid,
        &test_fixture::callee_key(),
    )
    .expect("sqlite callers");
    assert_eq!(lg_rows.len(), 1, "exactly one caller of calleeFn");
    assert!(
        callers_multiset_eq(&lg_rows, &sq_rows),
        "LiveGraph caller rows field-exact == SQLite rows: lg={lg_rows:?} sq={sq_rows:?}"
    );
    // The enrichment is the FILE->OWNS->MODULE join, reproduced from the key's path segment.
    assert_eq!(lg_rows[0].module_path.as_deref(), Some("src"));
    assert_eq!(
        lg_rows[0].module_stable_key.as_deref(),
        Some(format!("{}:src:MODULE", test_fixture::REPO).as_str())
    );
}
