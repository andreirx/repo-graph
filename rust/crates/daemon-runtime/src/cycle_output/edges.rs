//! CYCLE-HONESTY-1 (§2.1): the additive REAL intra-SCC edge attachment for the SQLite cycles route.
//!
//! Abstraction one-liner — WHAT: a crate-private post-pass that attaches the verified intra-SCC directed
//! IMPORTS edges (`edges` + `edges_truncated`) to the already-canonicalized SQLite cycle JSON, so the
//! renderer can draw a walk over ONLY real arrows. CONCRETE CURRENT USERS: the SQLite cycles serve
//! (`livegraph_feed::serve_cycles_sqlite`) and the forced `--engine sqlite` dispatch arm
//! (`dispatch::handle_cycles`), both via the re-exported [`sqlite_module_cycles_json_with_edges`]. AXIS:
//! none — a single concrete post-pass, not a variation point; split from `cycle_output` only to keep both
//! files under the 500-line guardrail and to isolate the additive edge concern from the certified
//! member-set canonicalization. REJECTED SIMPLER: threading edges through
//! `canonical_module_cycles_json`'s dedup/sort, which would couple edge selection to the certified member
//! transform (a byte-identity risk); this post-pass leaves that transform untouched.

use std::collections::HashMap;

use repo_graph_storage::queries::{CycleResult, TypeOnlyDisposition};
use serde_json::{json, Value};

use super::sqlite_module_cycles_json;

/// Attach the REAL intra-SCC directed IMPORTS edges to each canonical SQLite cycle, ADDITIVELY
/// (`edges: [{from_node_id, to_node_id}]` + `edges_truncated: bool`). Post-canonicalization (mutates the
/// JSON [`super::canonical_module_cycles_json`] produced): the member SET / order / `cycle_id` / `length`
/// are UNCHANGED, so the certified byte-identity of those fields holds; only the additive fields appear.
/// The LiveGraph fastpath adapter never calls this, so its output omits the field (an absent optional
/// field is honest — operator ruling A1).
///
/// COHERENCE-3 (§2.1): the per-cycle edge SELECTION is the SHARED
/// [`repo_graph_agent::intra_cycle_edges`] — the SAME function `orient`'s serving path
/// (`agent_cycle_labeling`) uses to feed its walk — so the two surfaces draw over the IDENTICAL edge
/// set (an edge iff BOTH endpoints are members of THAT cycle and `from != to`, keyed by `node_id`,
/// deduped/sorted, capped at [`repo_graph_agent::CYCLE_EDGE_CAP`]). `module_edges` is the
/// snapshot's MODULE→MODULE IMPORTS
/// set (`module_import_edges`). The renderer draws an arrow ONLY between a pair present here.
pub(crate) fn attach_intra_cycle_edges(
    cycles_json: &mut [Value],
    module_edges: &[(String, String)],
) {
    let all_edges: Vec<(&str, &str)> = module_edges
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    for cycle in cycles_json.iter_mut() {
        let member_ids: Vec<&str> = cycle["nodes"]
            .as_array()
            .map(|nodes| nodes.iter().filter_map(|n| n["node_id"].as_str()).collect())
            .unwrap_or_default();
        let (edges, truncated) = repo_graph_agent::intra_cycle_edges(&member_ids, &all_edges);
        let edges_json: Vec<Value> = edges
            .into_iter()
            .map(|(from, to)| json!({ "from_node_id": from, "to_node_id": to }))
            .collect();
        cycle["edges"] = Value::Array(edges_json);
        cycle["edges_truncated"] = Value::Bool(truncated);
    }
}

/// The SQLite canonical cycles ([`super::sqlite_module_cycles_json`]) WITH the additive real intra-SCC edges
/// attached ([`attach_intra_cycle_edges`]). This is what the SQLite-served `rmap cycles` (default fallback +
/// forced `--engine sqlite`) emits so the renderer can draw a REAL walk.
pub(crate) fn sqlite_module_cycles_json_with_edges(
    cycles: &[CycleResult],
    qualified: &HashMap<String, String>,
    // TYPE-ONLY-IMPORTS-1: module edges now carry the per-edge `is_type_only` disposition (3rd element)
    // for the separate per-cycle type-only labeling pass; the CYCLE-HONESTY intra-SCC edge attachment
    // below consumes only the endpoints, so it is projected to pairs (this pass and its byte-identity
    // contract are UNCHANGED).
    module_edges: &[(String, String, Option<TypeOnlyDisposition>)],
) -> Vec<Value> {
    let mut out = sqlite_module_cycles_json(cycles, qualified);
    let pairs: Vec<(String, String)> = module_edges
        .iter()
        .map(|(from, to, _)| (from.clone(), to.clone()))
        .collect();
    attach_intra_cycle_edges(&mut out, &pairs);
    out
}

#[cfg(test)]
mod tests {
    use super::super::{canonical_module_cycles_json, CanonModuleCycleNode};
    use super::*;

    fn node(id: &str, name: &str, qual: &str) -> CanonModuleCycleNode {
        CanonModuleCycleNode {
            node_id: id.to_string(),
            name: name.to_string(),
            qualified_name: qual.to_string(),
        }
    }

    fn quals(cycle: &Value) -> Vec<String> {
        cycle["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["qualified_name"].as_str().unwrap().to_string())
            .collect()
    }

    fn edge_pairs(cycle: &Value) -> Vec<(String, String)> {
        cycle["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["from_node_id"].as_str().unwrap().to_string(),
                    e["to_node_id"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn attach_keeps_only_intra_cycle_edges_drops_self_and_cross() {
        // Two cycles: {a,b} and {m,n}. Real import edges include an intra-cycle pair per cycle, a
        // self-loop (dropped), and a cross-cycle pair a->m (dropped — not one cycle's edge).
        let mut out = canonical_module_cycles_json(&[
            vec![node("a", "a", "a"), node("b", "b", "b")],
            vec![node("m", "m", "m"), node("n", "n", "n")],
        ]);
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
            ("m".to_string(), "n".to_string()),
            ("a".to_string(), "a".to_string()), // self-loop
            ("a".to_string(), "m".to_string()), // cross-cycle
        ];
        attach_intra_cycle_edges(&mut out, &edges);
        // out is sorted: {a,b} cycle first, {m,n} second.
        assert_eq!(
            edge_pairs(&out[0]),
            vec![("a".into(), "b".into()), ("b".into(), "a".into())]
        );
        assert_eq!(edge_pairs(&out[1]), vec![("m".into(), "n".into())]);
        assert_eq!(out[0]["edges_truncated"], Value::Bool(false));
        assert_eq!(out[1]["edges_truncated"], Value::Bool(false));
    }

    #[test]
    fn attach_marks_truncation_explicitly() {
        // A cycle over 3 members with more than EDGE_CAP real directed edges among them.
        let members: Vec<CanonModuleCycleNode> = (0..30)
            .map(|i| node(&format!("u{i}"), &format!("m{i}"), &format!("pkg/m{i:02}")))
            .collect();
        let mut out = canonical_module_cycles_json(std::slice::from_ref(&members));
        // Every ordered distinct pair among the 30 members: 30*29 = 870 > EDGE_CAP (200).
        let mut edges = Vec::new();
        for a in &members {
            for b in &members {
                if a.node_id != b.node_id {
                    edges.push((a.node_id.clone(), b.node_id.clone()));
                }
            }
        }
        attach_intra_cycle_edges(&mut out, &edges);
        assert_eq!(
            out[0]["edges"].as_array().unwrap().len(),
            repo_graph_agent::CYCLE_EDGE_CAP
        );
        assert_eq!(out[0]["edges_truncated"], Value::Bool(true));
    }

    #[test]
    fn sqlite_with_edges_matches_plain_on_member_fields() {
        // The `_with_edges` variant is byte-identical to the plain adapter on the CERTIFIED fields
        // (nodes/qualified_name/cycle_id/length); it only ADDS `edges`/`edges_truncated`.
        use repo_graph_storage::queries::{CycleNode, CycleResult};
        let sqlite = vec![CycleResult {
            cycle_id: "cycle-1".to_string(),
            length: 2,
            nodes: vec![
                CycleNode {
                    node_id: "u_a".to_string(),
                    name: "a".to_string(),
                    file: None,
                },
                CycleNode {
                    node_id: "u_b".to_string(),
                    name: "b".to_string(),
                    file: None,
                },
            ],
        }];
        let mut qmap: HashMap<String, String> = HashMap::new();
        qmap.insert("u_a".to_string(), "pkg/a".to_string());
        qmap.insert("u_b".to_string(), "pkg/b".to_string());
        let edges = vec![(
            "u_a".to_string(),
            "u_b".to_string(),
            Some(TypeOnlyDisposition::TypeOnly),
        )];

        let plain = sqlite_module_cycles_json(&sqlite, &qmap);
        let with = sqlite_module_cycles_json_with_edges(&sqlite, &qmap, &edges);
        assert_eq!(quals(&plain[0]), quals(&with[0]));
        assert_eq!(plain[0]["cycle_id"], with[0]["cycle_id"]);
        assert_eq!(plain[0]["length"], with[0]["length"]);
        // The plain adapter carries NO edges; the with-edges one does.
        assert!(plain[0].get("edges").is_none());
        assert_eq!(edge_pairs(&with[0]), vec![("u_a".into(), "u_b".into())]);
    }
}
