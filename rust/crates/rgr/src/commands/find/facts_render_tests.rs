//! Unit tests for the FACTS-tier group envelope + remainder rendering
//! ([`super::render_facts_tier`]). Hit-level rendering (path / next validation) is
//! tested in `fact_hit.rs`; tier ORCHESTRATION (facts above seeds, `--exact`) in the
//! parent `find.rs`. Kept in a separate `#[path]` test file so `facts_render.rs`
//! stays under the 500-line guardrail (review-4 item 1).

use super::render_facts_tier;
use crate::commands::find::test_fixtures::empty_facts;
use serde_json::json;

/// Render only the facts tier of `result` to a fresh String.
fn facts(result: &serde_json::Value) -> String {
    let mut out = String::new();
    render_facts_tier(result, None, &mut out);
    out
}

#[test]
fn symbol_hit_renders_with_class_and_certainty_label() {
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "bnrService", "path": "src/bnr.ts", "key": "k1", "next": "explain k1"}],
        "matched": 1, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(out.contains("[symbol · extracted → rmap explain]"), "{out}");
    assert!(out.contains("bnrService  — src/bnr.ts"), "{out}");
    assert!(out.contains("→ rmap explain k1"), "{out}");
}

#[test]
fn per_class_cap_names_exact_total() {
    // FIND-RANK-1 §2.2: the cap is NAMED and EXACT — `showing 8 of 20 — --full for
    // all`, the real shown/matched numbers, never the former unexplained `(+N more)`.
    let hits: Vec<serde_json::Value> = (0..8)
        .map(|i| json!({"display": format!("s{i}"), "path": format!("f{i}.ts"), "key": format!("k{i}"), "next": format!("explain k{i}")}))
        .collect();
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": hits, "matched": 20, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("showing 8 of 20 — --full for all"),
        "named exact cap: {out}"
    );
}

#[test]
fn floor_remainder_renders_at_least_never_plus() {
    // FIND-RANK-1 §2.2: a saturated fetch window (matched_is_floor) renders the total
    // as the honest lower bound `at least 200`, never the fabricated-exact `+N+`.
    let hits: Vec<serde_json::Value> = (0..8)
        .map(|i| json!({"display": format!("s{i}"), "path": format!("f{i}.ts"), "key": format!("k{i}"), "next": format!("explain k{i}")}))
        .collect();
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": hits, "matched": 200, "matched_is_floor": true
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("showing 8 of at least 200 — --full for all"),
        "floor total named as a lower bound: {out}"
    );
    assert!(!out.contains("+"), "no unexplained +N+ marker: {out}");
}

#[test]
fn failed_class_renders_unavailable_with_reason_never_dropped() {
    let mut f = empty_facts();
    f[3] = json!({
        "fact_class": "http-surface", "render_command": "boundaries list", "certainty": "inferred",
        "hits": [], "matched": 0, "matched_is_floor": false,
        "error": "http-surface fact read unavailable: db locked"
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("[http-surface · inferred → rmap boundaries list]  unavailable (http-surface fact read unavailable: db locked)"),
        "failed class surfaced: {out}"
    );
}

#[test]
fn missing_facts_field_is_malformed() {
    let out = facts(&json!({"query": "x"}));
    assert!(
        out.contains("malformed find response: facts missing or not a list"),
        "{out}"
    );
}

#[test]
fn group_missing_certainty_is_malformed_never_untagged() {
    let out = facts(&json!({
        "facts": [{"fact_class": "module", "render_command": "map",
                   "hits": [], "matched": 0, "matched_is_floor": false}]
    }));
    assert!(
        out.contains("malformed fact group: missing fact_class/certainty"),
        "{out}"
    );
}

#[test]
fn boundary_group_renders_per_hit_governance_commands() {
    // review-6 re-home: the boundary declarations group carries NO single
    // render_command; each hit shows its own governance renderer (violations|gate),
    // and the group header omits the `→ rmap <cmd>`.
    let mut f = empty_facts();
    f[6] = json!({
        "fact_class": "boundary", "certainty": "governance",
        "hits": [
            {"display": "boundary declaration · r:src/core:MODULE", "next": "violations"},
            {"display": "requirement declaration · r:requirement:REQ-1:1", "next": "gate"}
        ],
        "matched": 2, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("[boundary · governance]\n"),
        "no single verb in header: {out}"
    );
    assert!(
        out.contains("boundary declaration · r:src/core:MODULE"),
        "{out}"
    );
    assert!(
        out.contains("→ rmap violations\n"),
        "boundary-kind → violations: {out}"
    );
    assert!(
        out.contains("→ rmap gate\n"),
        "requirement-kind → gate: {out}"
    );
}

#[test]
fn boundary_group_with_a_single_render_command_is_malformed() {
    // A per-hit-renderer class that claims ONE class-level render command is malformed
    // action text (the dropped `surfaces list` shape) — surfaced, never rendered.
    let mut f = empty_facts();
    f[6] = json!({
        "fact_class": "boundary", "render_command": "surfaces list", "certainty": "governance",
        "hits": [{"display": "boundary declaration · r:src/core:MODULE", "next": "violations"}],
        "matched": 1, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains(
            "per-hit-renderer class boundary carries a single render_command surfaces list"
        ),
        "{out}"
    );
    assert!(
        !out.contains("→ rmap"),
        "no actionable command for a malformed boundary group: {out}"
    );
}

#[test]
fn group_non_string_error_is_malformed_never_treated_as_live() {
    let mut f = empty_facts();
    f[3] = json!({
        "fact_class": "http-surface", "render_command": "boundaries list", "certainty": "inferred",
        "hits": [], "matched": 0, "matched_is_floor": false,
        "error": {"code": 5}
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("malformed fact group: error present but not a non-empty string"),
        "{out}"
    );
}

#[test]
fn group_mistagged_certainty_is_malformed_never_actionable() {
    // `symbol` mistagged `inferred` (ratified: `extracted`) — arbitrary action text.
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "inferred",
        "hits": [{"display": "x", "key": "k1", "next": "explain k1"}],
        "matched": 1, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("class symbol carries certainty inferred, ratified extracted"),
        "{out}"
    );
    assert!(
        !out.contains("→ rmap explain k1"),
        "no actionable command for a mistagged class: {out}"
    );
}

#[test]
fn group_unknown_class_is_malformed_never_actionable() {
    // A class outside the seven-class taxonomy is arbitrary action text.
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "spooky", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "x", "key": "k1", "next": "explain k1"}],
        "matched": 1, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("malformed fact group: unrecognized fact class: spooky"),
        "{out}"
    );
    assert!(
        !out.contains("→ rmap explain k1"),
        "no actionable command: {out}"
    );
}

#[test]
fn group_matched_below_shown_is_malformed_never_incoherent() {
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "a", "key": "k1", "next": "explain k1"},
                 {"display": "b", "key": "k2", "next": "explain k2"}],
        "matched": 1, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("malformed fact group: matched 1 < shown 2"),
        "{out}"
    );
}

#[test]
fn empty_facts_array_surfaces_every_missing_class() {
    let out = facts(&json!({"facts": []}));
    assert!(
        out.contains("malformed find response: fact group(s) missing for class(es): symbol, file, module, http-surface, dependency, framework, boundary"),
        "{out}"
    );
}

#[test]
fn omitted_class_is_surfaced_as_missing_never_silently_dropped() {
    let mut f = empty_facts();
    f.as_array_mut().unwrap().pop(); // drop the `boundary` group
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("malformed find response: fact group(s) missing for class(es): boundary"),
        "{out}"
    );
}

#[test]
fn duplicate_class_group_is_malformed_never_rendered_twice() {
    let mut f = empty_facts();
    let dup = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "dupSym", "path": "src/dup.ts", "key": "kd", "next": "explain kd"}],
        "matched": 1, "matched_is_floor": false
    });
    f.as_array_mut().unwrap().push(dup);
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("malformed find response: duplicate fact group for class symbol"),
        "{out}"
    );
    assert!(
        !out.contains("dupSym"),
        "duplicate group not rendered: {out}"
    );
}

#[test]
fn large_matched_is_preserved_not_truncated() {
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "s0", "path": "f0.ts", "key": "k0", "next": "explain k0"}],
        "matched": 5_000_000_000u64, "matched_is_floor": false
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("showing 1 of 5000000000 — --full for all"),
        "large matched preserved as the exact named total: {out}"
    );
}

#[test]
fn group_non_bool_matched_is_floor_is_malformed() {
    let mut f = empty_facts();
    f[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [], "matched": 0, "matched_is_floor": "nope"
    });
    let out = facts(&json!({"facts": f}));
    assert!(
        out.contains("malformed fact group: matched_is_floor missing or not a bool"),
        "{out}"
    );
}
