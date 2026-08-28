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

use repo_graph_storage::queries::CycleResult;
use serde_json::{json, Value};

use super::sqlite_module_cycles_json;

/// The per-cycle intra-SCC edge cap. A cycle with more real import edges than this carries the first
/// `EDGE_CAP` (deterministically sorted) plus an EXPLICIT `edges_truncated: true` marker — never a silent
/// cut (spec §2.1). 200 is generous: a cycle needing more than 200 distinct intra-SCC edges is
/// exceptional. When truncated, the human renderer draws NO walk (a partial edge set could imply a chain
/// the full set does not) and falls back to `members (unordered)` per §2.2; the capped `edges` + marker
/// remain in `--json` for programmatic consumers, and the count line states the full size.
pub(crate) const EDGE_CAP: usize = 200;

/// Attach the REAL intra-SCC directed IMPORTS edges to each canonical SQLite cycle, ADDITIVELY
/// (`edges: [{from_node_id, to_node_id}]` + `edges_truncated: bool`). Post-canonicalization (mutates the
/// JSON [`super::canonical_module_cycles_json`] produced): the member SET / order / `cycle_id` / `length`
/// are UNCHANGED, so the certified byte-identity of those fields holds; only the additive fields appear.
/// The LiveGraph fastpath adapter never calls this, so its output omits the field (an absent optional
/// field is honest — operator ruling A1).
///
/// An edge is attached to a cycle iff BOTH endpoints are members of THAT cycle (`from != to`), keyed by
/// the node `node_id` the canonical nodes carry. `module_edges` is the snapshot's MODULE→MODULE IMPORTS
/// set (`module_import_edges`); a self-loop or a cross-cycle pair is dropped. The renderer draws an arrow
/// ONLY between a pair present here — so no arrow can claim an import that does not exist.
pub(crate) fn attach_intra_cycle_edges(
    cycles_json: &mut [Value],
    module_edges: &[(String, String)],
) {
    use std::collections::BTreeSet;

    // node_id -> the index of the (single) cycle it belongs to. SCCs partition the nodes, so a node_id
    // is in at most one cycle; a node in no rendered cycle is simply absent from the map.
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (i, cycle) in cycles_json.iter().enumerate() {
        if let Some(nodes) = cycle["nodes"].as_array() {
            for n in nodes {
                if let Some(id) = n["node_id"].as_str() {
                    owner.insert(id, i);
                }
            }
        }
    }

    // Bucket each intra-cycle edge (both endpoints owned by the SAME cycle). BTreeSet = dedup + deterministic
    // (from, to) order without a separate sort pass.
    let mut buckets: Vec<BTreeSet<(String, String)>> = vec![BTreeSet::new(); cycles_json.len()];
    for (from, to) in module_edges {
        if from == to {
            continue; // a self-import is not an inter-module cycle edge
        }
        if let (Some(&fi), Some(&ti)) = (owner.get(from.as_str()), owner.get(to.as_str())) {
            if fi == ti {
                buckets[fi].insert((from.clone(), to.clone()));
            }
        }
    }

    for (cycle, bucket) in cycles_json.iter_mut().zip(buckets.into_iter()) {
        let truncated = bucket.len() > EDGE_CAP;
        let edges: Vec<Value> = bucket
            .into_iter()
            .take(EDGE_CAP)
            .map(|(from, to)| json!({ "from_node_id": from, "to_node_id": to }))
            .collect();
        cycle["edges"] = Value::Array(edges);
        cycle["edges_truncated"] = Value::Bool(truncated);
    }
}

/// The SQLite canonical cycles ([`super::sqlite_module_cycles_json`]) WITH the additive real intra-SCC edges
/// attached ([`attach_intra_cycle_edges`]). This is what the SQLite-served `rmap cycles` (default fallback +
/// forced `--engine sqlite`) emits so the renderer can draw a REAL walk.
pub(crate) fn sqlite_module_cycles_json_with_edges(
    cycles: &[CycleResult],
    qualified: &HashMap<String, String>,
    module_edges: &[(String, String)],
) -> Vec<Value> {
    let mut out = sqlite_module_cycles_json(cycles, qualified);
    attach_intra_cycle_edges(&mut out, module_edges);
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
        assert_eq!(out[0]["edges"].as_array().unwrap().len(), EDGE_CAP);
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
        let edges = vec![("u_a".to_string(), "u_b".to_string())];

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
