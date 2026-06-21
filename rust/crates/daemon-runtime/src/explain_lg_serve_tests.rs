//! EXPLAIN-LIVEGRAPH-IMPL: pure-helper unit tests for `explain_lg_serve` (split out to respect the 500-line
//! structural guardrail — the module file keeps the serving logic only). The end-to-end LiveGraph-served
//! value proofs live in `explain_coherence_tests.rs` (through `build_explain_envelope`).

use super::{cycle_involves, explain_items_cap, rebuild_identity_rows, truncate};
use std::collections::BTreeMap;

#[test]
fn items_cap_matches_agent_budget() {
    assert_eq!(explain_items_cap(false), 15, "Medium/Small floor = 15");
    assert_eq!(explain_items_cap(true), 50, "Large = 50");
}

#[test]
fn truncate_flags_match_agent_contract() {
    let mut within = vec![1, 2, 3];
    assert_eq!(truncate(&mut within, 15), (None, None));
    assert_eq!(within.len(), 3);

    let mut over: Vec<u32> = (0..20).collect();
    let (trunc, omitted) = truncate(&mut over, 15);
    assert_eq!(trunc, Some(true));
    assert_eq!(omitted, Some(5));
    assert_eq!(over.len(), 15);
}

#[test]
fn cycle_involves_symbol_focus_is_exact_membership() {
    let members = vec!["src/a".to_string(), "src/b".to_string()];
    assert!(cycle_involves(&members, "src/a", false));
    assert!(
        !cycle_involves(&members, "src", false),
        "no prefix match in symbol focus"
    );
    assert!(!cycle_involves(&members, "src/c", false));
}

#[test]
fn cycle_involves_path_focus_matches_prefix() {
    let members = vec!["src/core/auth".to_string(), "src/util".to_string()];
    assert!(
        cycle_involves(&members, "src/core/auth", true),
        "exact member"
    );
    assert!(
        cycle_involves(&members, "src/core", true),
        "prefix `src/core/`"
    );
    assert!(cycle_involves(&members, "src", true), "prefix `src/`");
    assert!(!cycle_involves(&members, "lib", true));
    // A prefix must be a path segment boundary: `src/cor` does not match `src/core/auth`.
    assert!(!cycle_involves(&members, "src/cor", true));
}

// ── rebuild_identity_rows: the callgraph value-rebuild join (LG names + SQLite module/order) ──

fn row(key: &str, name: &str, module: Option<&str>) -> (String, String, Option<String>) {
    (
        key.to_string(),
        name.to_string(),
        module.map(str::to_string),
    )
}

#[test]
fn rebuild_swaps_in_live_name_keeps_sqlite_module_and_order() {
    // Live names: `a` has a current-state IR name; `b` has none (non-resident owner -> SQLite name kept).
    let live: BTreeMap<String, Option<String>> = BTreeMap::from([
        ("a".to_string(), Some("liveA".to_string())),
        ("b".to_string(), None),
    ]);
    let sqlite = vec![row("a", "staleA", Some("modA")), row("b", "sqlB", None)];
    let out = rebuild_identity_rows(&live, 2, &sqlite).expect("sets match -> rebuilt");
    // SQL order preserved; `a` gets the LIVE name; `b` falls back to the SQLite name; modules SQLite.
    assert_eq!(out[0], row("a", "liveA", Some("modA")));
    assert_eq!(out[1], row("b", "sqlB", None));
}

#[test]
fn rebuild_diverges_on_count_mismatch() {
    let live: BTreeMap<String, Option<String>> =
        BTreeMap::from([("a".to_string(), Some("liveA".to_string()))]);
    // The SQLite full count (2) != the live identity-set size (1) -> divergence (None).
    assert!(rebuild_identity_rows(&live, 2, &[row("a", "x", None)]).is_none());
}

#[test]
fn rebuild_diverges_when_rendered_key_absent_from_live_set() {
    let live: BTreeMap<String, Option<String>> =
        BTreeMap::from([("a".to_string(), Some("liveA".to_string()))]);
    // A rendered key (`z`) absent from the live set -> divergence (no false LiveGraph value).
    assert!(rebuild_identity_rows(&live, 1, &[row("z", "x", None)]).is_none());
}
