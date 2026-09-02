//! FIXTURE-POLLUTION-1 §2.2/§2.3 — per-cycle test-composition labeling for the
//! SQLite-served module cycles (the only route that reaches the stored `is_test` fact;
//! the LiveGraph fastpath renders unchanged and states the asymmetry — §2.3).
//!
//! ORIENT-CYCLES-DISAGREE-1: the classification itself is NO LONGER duplicated here — it moved
//! to the shared `repo_graph_agent::cycle_composition` classifier, which the `orient` serving
//! computation (the `storage` adapter's `find_module_cycles`) ALSO calls. One classifier, two
//! serving computations ⇒ the `cycles` headline and the `orient` cycle leaf cannot disagree.
//! This module is now the thin JSON WRAPPER: it extracts each cycle's member qualified paths
//! from the canonical cycles JSON, calls the shared classifier, and writes the daemon's
//! [`TestComposition`] discriminant (+ reason). Only the "node list absent/malformed" case is
//! daemon-framed here (the shared classifier speaks in terms of member lists, not raw JSON).
//!
//! Abstraction record — module: `cycle_output::composition`; concrete current users:
//! `dispatch::handle_cycles` and `livegraph_feed::serve_cycles_sqlite` (both via the
//! [`super::label_test_only_cycles`] re-export); axis: the JSON emission of the shared
//! per-cycle classification onto the SQLite cycles output; rejected simpler alternative:
//! keeping a second copy of the classifier here (the drift risk ORIENT-CYCLES-DISAGREE-1 exists
//! to remove — `orient` and `cycles` would classify from two implementations).

use serde_json::Value;

use crate::test_composition::TestComposition;

/// FIXTURE-POLLUTION-1 §2.2/§2.3 + binding direction rule: attach the additive per-cycle
/// `test_composition` (tri-state) to the SQLite-served cycles JSON (the ONLY route that reaches
/// the stored `is_test` fact; the LiveGraph fastpath renders unchanged and states the asymmetry
/// — §2.3). `files` are the repo's tracked `(path, is_test)` rows.
///
/// The classification is the shared `repo_graph_agent::classify_cycles` (the SAME function the
/// `orient` serving computation uses). A cycle whose `nodes` are absent/malformed is `Unknown`
/// with a daemon-framed reason — NEVER a silent `false`/production default.
pub(crate) fn label_test_only_cycles(cycles_json: &mut [Value], files: &[(&str, bool)]) {
    // Two phases: classify under an IMMUTABLE borrow of the JSON (the member `&str`s borrow it),
    // then write under a MUTABLE borrow. The borrows cannot overlap, so this cannot be one pass.
    let comps = classify_all(cycles_json, files);
    for (cycle, comp) in cycles_json.iter_mut().zip(comps) {
        comp.write_json(cycle);
    }
}

/// Classify every cycle in the canonical JSON via the shared classifier. A cycle whose `nodes`
/// field is absent/malformed is classified `Unknown` here (the shared classifier receives member
/// LISTS, not raw JSON); every other reason (`no members`, `member has no qualified path`,
/// `member owns no tracked file`) comes verbatim from the shared classifier — so `cycles` and
/// `orient` cannot diverge on what a given cycle is.
fn classify_all(cycles_json: &[Value], files: &[(&str, bool)]) -> Vec<TestComposition> {
    // Per-cycle member qualified paths (a member with no `qualified_name` is `None`), plus a
    // parallel daemon-framed override for the absent/malformed-`nodes` case.
    let mut member_lists: Vec<Vec<Option<&str>>> = Vec::with_capacity(cycles_json.len());
    let mut malformed: Vec<Option<&'static str>> = Vec::with_capacity(cycles_json.len());
    for cycle in cycles_json {
        match cycle["nodes"].as_array() {
            None => {
                malformed.push(Some(
                    "cycle node list absent or malformed (cannot evaluate test-composition)",
                ));
                member_lists.push(Vec::new());
            }
            Some(nodes) => {
                malformed.push(None);
                member_lists.push(nodes.iter().map(|n| n["qualified_name"].as_str()).collect());
            }
        }
    }
    let agent_comps = repo_graph_agent::classify_cycles(&member_lists, files);
    agent_comps
        .into_iter()
        .zip(malformed)
        .map(|(agent, override_reason)| match override_reason {
            Some(reason) => TestComposition::Unknown(reason.to_string()),
            None => map_agent_composition(agent),
        })
        .collect()
}

/// Map the shared classifier's result into the daemon's [`TestComposition`] JSON-emitter type.
/// A straight 1:1 mapping — the reason strings are preserved verbatim (the `orient` and `cycles`
/// surfaces read the SAME reason for the SAME cycle).
fn map_agent_composition(c: repo_graph_agent::CycleTestComposition) -> TestComposition {
    match c {
        repo_graph_agent::CycleTestComposition::TestOnly => TestComposition::TestOnly,
        repo_graph_agent::CycleTestComposition::Production => TestComposition::Production,
        repo_graph_agent::CycleTestComposition::Unknown(reason) => TestComposition::Unknown(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::super::canonical_module_cycles_json;
    use super::*;
    use serde_json::json;

    #[test]
    fn label_cycles_writes_tristate_composition() {
        // A wholly-test cycle ⇒ test_only (demoted); a cycle touching a production module
        // ⇒ production; a cycle with an unclassifiable member ⇒ unknown WITH a reason
        // (never collapsed to production — the binding direction rule). The classification is
        // the shared classifier; this test guards the JSON emission wrapper end-to-end.
        let mut out = canonical_module_cycles_json(&[
            vec![
                super::super::CanonModuleCycleNode {
                    node_id: "u1".into(),
                    name: "a".into(),
                    qualified_name: "tests/fixtures/mono/pkg-a".into(),
                },
                super::super::CanonModuleCycleNode {
                    node_id: "u2".into(),
                    name: "b".into(),
                    qualified_name: "tests/fixtures/mono/pkg-b".into(),
                },
            ],
            vec![
                super::super::CanonModuleCycleNode {
                    node_id: "u3".into(),
                    name: "c".into(),
                    qualified_name: "src/core".into(),
                },
                super::super::CanonModuleCycleNode {
                    node_id: "u4".into(),
                    name: "d".into(),
                    qualified_name: "tests/fixtures/mono/pkg-a".into(),
                },
            ],
            vec![
                super::super::CanonModuleCycleNode {
                    node_id: "u5".into(),
                    name: "e".into(),
                    qualified_name: "tests/fixtures/mono/pkg-a".into(),
                },
                super::super::CanonModuleCycleNode {
                    node_id: "u6".into(),
                    name: "f".into(),
                    qualified_name: "vendor/untracked".into(), // owns no tracked file ⇒ unknown
                },
            ],
        ]);
        let files = [
            ("tests/fixtures/mono/pkg-a/mod.rs", true),
            ("tests/fixtures/mono/pkg-b/mod.rs", true),
            ("src/core/x.rs", false),
        ];
        label_test_only_cycles(&mut out, &files);
        let quals = |cycle: &Value| -> Vec<String> {
            cycle["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["qualified_name"].as_str().unwrap().to_string())
                .collect()
        };
        let by_quals = |pred: &dyn Fn(&[String]) -> bool| {
            out.iter()
                .find(|c| pred(&quals(c)))
                .expect("cycle present")
                .clone()
        };
        let fixture = by_quals(&|q| q.iter().all(|s| s.starts_with("tests/fixtures")));
        let mixed = by_quals(&|q| q.iter().any(|s| s == "src/core"));
        let unknown = by_quals(&|q| q.iter().any(|s| s == "vendor/untracked"));
        assert_eq!(fixture["test_composition"], json!("test_only"));
        assert_eq!(mixed["test_composition"], json!("production"));
        assert_eq!(unknown["test_composition"], json!("unknown"));
        assert!(
            unknown["test_composition_unknown_reason"]
                .as_str()
                .expect("reason present")
                .contains("vendor/untracked"),
            "{unknown}"
        );
    }

    #[test]
    fn label_cycle_with_malformed_nodes_is_unknown_not_production() {
        // A cycle whose `nodes` is absent renders UNKNOWN with a reason — never a silent
        // production default (the review-0 `unwrap_or_default()` collapse this fixes).
        let mut out = vec![json!({ "cycle_id": "cycle-1", "length": 0 })];
        label_test_only_cycles(&mut out, &[("src/a.rs", false)]);
        assert_eq!(out[0]["test_composition"], json!("unknown"));
        assert!(out[0]["test_composition_unknown_reason"]
            .as_str()
            .expect("reason present")
            .contains("malformed"));
    }
}
