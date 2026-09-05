//! TYPE-ONLY-IMPORTS-1: the per-cycle `type_only` verdict attachment for the SQLite cycles routes.
//!
//! Abstraction one-liner — WHAT: a crate-private JSON ADAPTER that stamps each rendered cycle with
//! its runtime-vs-type-only verdict (`type_only: {kind[, reason]}`) by assembling neutral inputs
//! from the canonical cycles JSON and delegating to the SHARED
//! [`repo_graph_agent::classify_cycles_type_only`] — the SAME kernel the `orient` cycle leaf calls
//! (`storage::agent_cycle_labeling::label_module_cycles`), so the two surfaces cannot state a
//! different verdict (route-agreement DoD; ORIENT-CYCLES-DISAGREE-1 "one derivation"). CONCRETE
//! CURRENT USERS: the SQLite cycles serve (`livegraph_feed::serve_cycles_sqlite`) and the forced
//! `--engine sqlite` dispatch arm (`dispatch::handle_cycles`), both via [`attach_type_only_labels`].
//! AXIS: none — a single concrete JSON adapter. REJECTED SIMPLER: keeping the verdict + §5 TS/JS
//! membership derivation here (daemon-local) — unreachable from the `storage` adapter that serves
//! `orient`, forcing a second copy of honesty-critical logic that could drift into a per-surface
//! disagreement (the review-0 finding this build fixes). The derivation now lives ONCE in `agent`.
//!
//! The conjunctive verdict (runtime dominates; a corrupt fact outranks an absent one), the §5 TS/JS
//! membership gate, and the disposition vocabulary all live in [`repo_graph_agent::cycle_type_only`].
//! This module only: extracts each cycle's `(node_id, qualified_name)` members from the JSON, maps
//! the stored [`TypeOnlyDisposition`] into the pure-domain `EdgeTypeOnly`, calls the kernel, and
//! writes the returned verdict back as the additive `type_only` field (only on TS/JS-member cycles).

use repo_graph_storage::queries::{edge_type_only_of, TypeOnlyDisposition};
use serde_json::Value;

/// Attach the per-cycle `type_only` verdict (see module docs). `module_edges` is the snapshot's
/// MODULE→MODULE IMPORTS set as `(from_node_id, to_node_id, disposition)`; `files` is
/// `(path, language_token)` for §5 TS/JS membership; `all_module_dirs` is every module's qualified
/// directory (deepest-ownership resolution). Mutates `cycles_json` in place, ADDITIVELY (only the
/// `type_only` field is added, and only on TS/JS-member cycles — §5, byte-stable otherwise).
pub(crate) fn attach_type_only_labels(
    cycles_json: &mut [Value],
    module_edges: &[(String, String, Option<TypeOnlyDisposition>)],
    files: &[(&str, Option<&str>)],
    all_module_dirs: &[String],
) {
    // Neutral per-cycle members `(node_id, qualified_name)` extracted from the canonical JSON — the
    // node_id maps the edges, the qualified_name gates §5 membership. A node missing either key is
    // dropped from the membership/owner view (it can carry no verdict contribution anyway).
    let members_owned: Vec<Vec<(String, String)>> = cycles_json
        .iter()
        .map(|cycle| {
            cycle["nodes"]
                .as_array()
                .map(|nodes| {
                    nodes
                        .iter()
                        .filter_map(|n| {
                            let id = n["node_id"].as_str()?;
                            let qual = n["qualified_name"].as_str()?;
                            Some((id.to_string(), qual.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    let cycle_members: Vec<Vec<(&str, &str)>> = members_owned
        .iter()
        .map(|c| c.iter().map(|(id, q)| (id.as_str(), q.as_str())).collect())
        .collect();
    let edges_mapped: Vec<(&str, &str, Option<repo_graph_agent::EdgeTypeOnly>)> = module_edges
        .iter()
        .map(|(from, to, disp)| (from.as_str(), to.as_str(), disp.map(edge_type_only_of)))
        .collect();

    let verdicts = repo_graph_agent::classify_cycles_type_only(
        &cycle_members,
        &edges_mapped,
        files,
        all_module_dirs,
    );

    for (cycle, verdict) in cycles_json.iter_mut().zip(verdicts) {
        // §5: a non-TS cycle's verdict is `None` — NO field added (byte-stable).
        if let Some(verdict) = verdict {
            cycle["type_only"] =
                serde_json::to_value(verdict).expect("CycleTypeOnly serializes infallibly");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use TypeOnlyDisposition::*;

    fn cyc(id_qual: &[(&str, &str)]) -> Value {
        json!({
            "nodes": id_qual
                .iter()
                .map(|(id, q)| json!({ "node_id": id, "qualified_name": q }))
                .collect::<Vec<_>>(),
        })
    }

    fn files_ts<'a>(pairs: &[(&'a str, Option<&'a str>)]) -> Vec<(&'a str, Option<&'a str>)> {
        pairs.to_vec()
    }

    fn verdict(cycle: &Value) -> Option<(String, Option<String>)> {
        cycle.get("type_only").map(|v| {
            (
                v["kind"].as_str().unwrap().to_string(),
                v.get("reason").and_then(|r| r.as_str()).map(str::to_string),
            )
        })
    }

    /// The JSON adapter round-trip: a pure-type-only TS cycle is stamped `type_only`.
    #[test]
    fn pure_type_only_cycle_is_labeled() {
        let mut cycles = vec![cyc(&[("a", "pkg/a"), ("b", "pkg/b")])];
        let edges = vec![
            ("a".into(), "b".into(), Some(TypeOnly)),
            ("b".into(), "a".into(), Some(TypeOnly)),
        ];
        let files = files_ts(&[
            ("pkg/a/x.ts", Some("typescript")),
            ("pkg/b/y.ts", Some("tsx")),
        ]);
        let all = vec!["pkg/a".to_string(), "pkg/b".to_string()];
        attach_type_only_labels(&mut cycles, &edges, &files, &all);
        assert_eq!(verdict(&cycles[0]), Some(("type_only".into(), None)));
    }

    /// COHERENCE-2 §2.2 (Option A): a 2-cycle with one `import type` edge BREAKS at runtime —
    /// erasing that edge leaves only the runtime edge b->a (no cycle). (Was `has_runtime_edges`
    /// under the old ALL-predicate; the false negative the slice removes.)
    #[test]
    fn one_type_only_edge_2cycle_breaks_at_runtime() {
        let mut cycles = vec![cyc(&[("a", "pkg/a"), ("b", "pkg/b")])];
        let edges = vec![
            ("a".into(), "b".into(), Some(TypeOnly)),
            ("b".into(), "a".into(), Some(Runtime)),
        ];
        let files = files_ts(&[("pkg/a/x.ts", Some("typescript"))]);
        let all = vec!["pkg/a".to_string(), "pkg/b".to_string()];
        attach_type_only_labels(&mut cycles, &edges, &files, &all);
        assert_eq!(
            verdict(&cycles[0]),
            Some(("breaks_at_runtime".into(), None))
        );
        assert_eq!(cycles[0]["type_only"]["type_only"], serde_json::json!(1));
        assert_eq!(cycles[0]["type_only"]["of"], serde_json::json!(2));
    }

    /// COHERENCE-2 §2.2 (Option A): a MIXED SCC {a,b,c} with a runtime 2-cycle a<->b and a type-only
    /// chord b<->c stays `has_runtime_edges` — erasing the chord leaves a<->b intact. The type-only
    /// count rides as detail (2 of 4). This is the review-0 case the ANY-predicate got wrong.
    #[test]
    fn mixed_scc_with_surviving_runtime_cycle_is_has_runtime_edges() {
        let mut cycles = vec![cyc(&[("a", "pkg/a"), ("b", "pkg/b"), ("c", "pkg/c")])];
        let edges = vec![
            ("a".into(), "b".into(), Some(Runtime)),
            ("b".into(), "a".into(), Some(Runtime)),
            ("b".into(), "c".into(), Some(TypeOnly)),
            ("c".into(), "b".into(), Some(TypeOnly)),
        ];
        let files = files_ts(&[("pkg/a/x.ts", Some("typescript"))]);
        let all = vec![
            "pkg/a".to_string(),
            "pkg/b".to_string(),
            "pkg/c".to_string(),
        ];
        attach_type_only_labels(&mut cycles, &edges, &files, &all);
        assert_eq!(
            verdict(&cycles[0]),
            Some(("has_runtime_edges".into(), None))
        );
        assert_eq!(cycles[0]["type_only"]["type_only"], serde_json::json!(2));
        assert_eq!(cycles[0]["type_only"]["of"], serde_json::json!(4));
    }

    /// Operator ruling 2a: a CORRUPT contributor surfaces its OWN reason (distinct from absent).
    #[test]
    fn unreadable_edge_is_its_own_reason_distinct_from_absent() {
        let mut cycles = vec![cyc(&[("a", "pkg/a"), ("b", "pkg/b")])];
        let edges = vec![
            ("a".into(), "b".into(), Some(TypeOnly)),
            ("b".into(), "a".into(), Some(Unreadable)),
        ];
        let files = files_ts(&[("pkg/a/x.ts", Some("typescript"))]);
        let all = vec!["pkg/a".to_string(), "pkg/b".to_string()];
        attach_type_only_labels(&mut cycles, &edges, &files, &all);
        assert_eq!(
            verdict(&cycles[0]),
            Some(("unknown".into(), Some("type-only fact unreadable".into())))
        );
    }

    /// §5: a cycle with no TS/JS member carries NO field (byte-stable), even all-runtime.
    #[test]
    fn non_ts_cycle_gets_no_field() {
        let mut cycles = vec![cyc(&[("a", "crates/a"), ("b", "crates/b")])];
        let edges = vec![
            ("a".into(), "b".into(), Some(Runtime)),
            ("b".into(), "a".into(), Some(Runtime)),
        ];
        let files = files_ts(&[("crates/a/x.rs", Some("rust"))]);
        let all = vec!["crates/a".to_string(), "crates/b".to_string()];
        attach_type_only_labels(&mut cycles, &edges, &files, &all);
        assert_eq!(verdict(&cycles[0]), None);
    }

    /// A TS cycle with no matching intra-cycle edges ⇒ Unknown "cycle import edges unavailable".
    #[test]
    fn no_intra_edges_is_unknown() {
        let mut cycles = vec![cyc(&[("a", "pkg/a"), ("b", "pkg/b")])];
        let files = files_ts(&[("pkg/a/x.ts", Some("typescript"))]);
        let all = vec!["pkg/a".to_string(), "pkg/b".to_string()];
        attach_type_only_labels(&mut cycles, &[], &files, &all);
        assert_eq!(
            verdict(&cycles[0]),
            Some((
                "unknown".into(),
                Some("cycle import edges unavailable".into())
            ))
        );
    }
}
