//! FIXTURE-POLLUTION-1 §2.2/§2.3 — per-cycle test-composition labeling for the
//! SQLite-served module cycles (the only route that reaches the stored `is_test` fact;
//! the LiveGraph fastpath renders unchanged and states the asymmetry — §2.3).
//!
//! Split out of the sibling [`super`] canonicalization module so the certified member-set
//! canonicalization (there) and this additive classification post-pass (here) each stay
//! well under the 500-line structural guardrail.
//!
//! Abstraction record — module: `cycle_output::composition`; concrete current users:
//! `dispatch::handle_cycles` and `livegraph_feed::serve_cycles_sqlite` (both via the
//! [`super::label_test_only_cycles`] re-export); axis: the per-cycle test-composition
//! classification (conservative module aggregation) kept OFF the canonicalization file to
//! hold the guardrail; rejected simpler alternative: inlining it in `cycle_output/mod.rs`
//! (pushed that file to 546 lines, over the guardrail — the review-1 finding).

use std::collections::HashMap;

use serde_json::Value;

use crate::test_composition::TestComposition;

/// The test-composition of a single MODULE, under the conservative aggregation
/// (CONTRADICTION-SWEEP-1 §2.3): the module owns the tracked files under its directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleComposition {
    /// Owns ≥1 file and EVERY owned file carries the stored `is_test` fact.
    TestOnly,
    /// Owns ≥1 production (non-test) file.
    Production,
    /// Owns NO tracked file — no reachable `is_test` evidence for this module.
    Unknown,
}

/// FIXTURE-POLLUTION-1 §2.1/§2.3 + binding direction rule: classify a MODULE (whose
/// canonical path is the directory `qualified`) from the stored `is_test` fact ONLY —
/// never the path string (`tests/` as text is not evidence; the fact is). Ownership is by
/// directory containment (the module IS the directory `packages/a/src`). ANY production
/// owned file ⇒ `Production`; ≥1 owned file all-test ⇒ `TestOnly`; NO owned file ⇒
/// `Unknown` (distinct from production — the review-0 collapse this fixes).
fn classify_module(qualified: &str, files: &[(&str, bool)]) -> ModuleComposition {
    let prefix = format!("{qualified}/");
    let mut owned_any = false;
    for (path, is_test) in files {
        let owned = *path == qualified || path.starts_with(&prefix);
        if owned {
            owned_any = true;
            if !*is_test {
                return ModuleComposition::Production; // a production owned file ⇒ production
            }
        }
    }
    if owned_any {
        ModuleComposition::TestOnly
    } else {
        ModuleComposition::Unknown
    }
}

/// FIXTURE-POLLUTION-1 §2.2/§2.3 + binding direction rule: attach the additive per-cycle
/// `test_composition` (tri-state) to the SQLite-served cycles JSON (the ONLY route that
/// reaches the stored `is_test` fact; the LiveGraph fastpath renders unchanged and states
/// the asymmetry — §2.3). `files` are the repo's tracked `(path, is_test)` rows; module
/// classification is memoized per distinct member (cost O(distinct members × files)).
///
/// Conservative aggregation over a cycle's members:
/// - ANY member is `Production` ⇒ the cycle is `Production` (a real cycle);
/// - else ANY member is `Unknown` (owns no tracked file, or a malformed node with no
///   `qualified_name`) ⇒ the cycle is `Unknown` WITH a reason — it is NOT demoted (we
///   cannot prove it is wholly test-only);
/// - else (every member `TestOnly`) ⇒ `TestOnly` (demoted).
///
/// A cycle whose `nodes` are absent/malformed is `Unknown` with a reason — NEVER a silent
/// `false`/production default (the review-0 `unwrap_or_default()` collapse this fixes).
pub(crate) fn label_test_only_cycles(cycles_json: &mut [Value], files: &[(&str, bool)]) {
    let mut memo: HashMap<String, ModuleComposition> = HashMap::new();
    for cycle in cycles_json.iter_mut() {
        classify_cycle(cycle, files, &mut memo).write_json(cycle);
    }
}

/// Classify one cycle's test-composition from its member modules (see
/// [`label_test_only_cycles`] for the aggregation rule). Kept separate so the borrow of
/// `cycle` ends before [`TestComposition::write_json`] mutates it.
fn classify_cycle(
    cycle: &Value,
    files: &[(&str, bool)],
    memo: &mut HashMap<String, ModuleComposition>,
) -> TestComposition {
    let Some(nodes) = cycle["nodes"].as_array() else {
        return TestComposition::Unknown(
            "cycle node list absent or malformed (cannot evaluate test-composition)".to_string(),
        );
    };
    if nodes.is_empty() {
        return TestComposition::Unknown("cycle has no members".to_string());
    }

    let mut any_production = false;
    let mut unknown_reason: Option<String> = None;
    for n in nodes {
        match n["qualified_name"].as_str() {
            None => {
                // A node with no qualified module path — unclassifiable member.
                unknown_reason.get_or_insert_with(|| {
                    "a cycle member has no qualified module path".to_string()
                });
            }
            Some(q) => match *memo
                .entry(q.to_string())
                .or_insert_with(|| classify_module(q, files))
            {
                ModuleComposition::Production => any_production = true,
                ModuleComposition::Unknown => {
                    unknown_reason.get_or_insert_with(|| {
                        format!("member module `{q}` owns no tracked file (is_test unknown)")
                    });
                }
                ModuleComposition::TestOnly => {}
            },
        }
    }

    if any_production {
        TestComposition::Production
    } else if let Some(reason) = unknown_reason {
        TestComposition::Unknown(reason)
    } else {
        TestComposition::TestOnly
    }
}

#[cfg(test)]
mod tests {
    use super::super::canonical_module_cycles_json;
    use super::*;
    use serde_json::json;

    #[test]
    fn classify_module_conservative_aggregation() {
        // §2.3 conservative rule: any production owned file ⇒ Production; wholly test-
        // owned ⇒ TestOnly; no owned file ⇒ Unknown (distinct from Production).
        let files = [
            ("tests/fixtures/mono/pkg/a.rs", true),
            ("tests/fixtures/mono/pkg/b.rs", true),
            ("src/core/x.rs", false),
            ("src/core/y_test.rs", true), // a lone test file in a production module
        ];
        // Wholly test-owned directory ⇒ TestOnly.
        assert_eq!(
            classify_module("tests/fixtures/mono/pkg", &files),
            ModuleComposition::TestOnly
        );
        // Mixed module (a production file present) ⇒ Production.
        assert_eq!(
            classify_module("src/core", &files),
            ModuleComposition::Production
        );
        // Owns no tracked file ⇒ Unknown (NOT production, NOT test — never demoted).
        assert_eq!(
            classify_module("vendor/lib", &files),
            ModuleComposition::Unknown
        );
    }

    #[test]
    fn label_cycles_writes_tristate_composition() {
        // A wholly-test cycle ⇒ test_only (demoted); a cycle touching a production module
        // ⇒ production; a cycle with an unclassifiable member ⇒ unknown WITH a reason
        // (never collapsed to production — the binding direction rule).
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
