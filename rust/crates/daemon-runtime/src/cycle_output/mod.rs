//! CYCLES-OUTPUT-CONTRACT-1: the canonical module-cycle OUTPUT contract (ratified D1=B / D2=B / D3=A).
//!
//! The DEFAULT `rmap cycles` output is migrated to a deterministic, backend-INDEPENDENT form so that the
//! SQLite-served answer and the LiveGraph-derived answer render IDENTICALLY for the same cycle SET. This is
//! the precondition the cycles default fastpath (CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1, BLOCKED) was missing.
//!
//! # Why this layer
//!
//! This is a PRESENTATION/output concern, not a storage concern: `find_cycles` (the low-level SCC query) and
//! its `CycleResult`/`CycleNode` types are UNCHANGED. The qualification + canonicalization is applied here, at
//! the daemon output boundary, by both the SQLite default builder and the LiveGraph builder.
//!
//! # Canonical form (byte-identity BY CONSTRUCTION)
//!
//! A strongly-connected component is a SET, not a ring: `graph-algorithms` documents that an SCC's member
//! order is "stack pop order" — a Tarjan artifact, NOT a meaningful directed traversal. So the canonical form
//! of a cycle is its member SET, sorted and de-duplicated. Two independent determinisms:
//!
//! 1. WITHIN a cycle: nodes sorted + de-duplicated by QUALIFIED module path (lexicographic `String` order).
//! 2. ACROSS cycles: cycles sorted by their qualified-name vector (lexicographic).
//!
//! This is the EXACT basis the module-cycle compare uses to match cycles (`module_cycle_compare`: each cycle a
//! `BTreeSet<String>`, the set a `BTreeSet<Vec<String>>`). Both are plain `String::cmp`. Therefore: when the
//! compare certifies the SETS equal (GREEN), the SQLite-default canonical render and the LiveGraph canonical
//! render are byte-identical — inherently, not by coincidence. A unit test also asserts this equivalence.
//!
//! # JSON contract (D2=B — additive, backward-compatible)
//!
//! Every existing field is preserved (`cycle_id`, `length`, per node `node_id`/`name`/`file`); `qualified_name`
//! is ADDED. `node_id` stays backend-native (SQLite node_uid / LiveGraph module path); the cross-backend stable
//! identity for the human + agents is `qualified_name`.
//!
//! # Module layout
//!
//! Two crate-private siblings hold the additive post-passes, each split out to keep every file under the
//! 500-line guardrail and to separate concerns from the certified member-set canonicalization (here):
//! - [`edges`] — the CYCLE-HONESTY-1 intra-SCC edge attachment (the "cycles draw only real arrows" contract).
//! - [`composition`] — the FIXTURE-POLLUTION-1 per-cycle test-composition labeling.
//!
//! Their cross-module entry points, [`sqlite_module_cycles_json_with_edges`] and [`label_test_only_cycles`],
//! are re-exported so their call sites are unchanged.

use std::collections::{BTreeMap, HashMap};

use repo_graph_storage::queries::CycleResult;
use serde_json::{json, Value};

mod composition;
mod edges;
pub(crate) use composition::label_test_only_cycles;
pub(crate) use edges::sqlite_module_cycles_json_with_edges;

/// A module-cycle node normalized for the canonical output: the backend-native `node_id`, the SHORT display
/// `name` (preserved for back-compat), and the QUALIFIED module path — the deterministic sort key AND the
/// human/agent-facing identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonModuleCycleNode {
    /// Backend-native identifier: the SQLite `node_uid` or the LiveGraph module path. Opaque; preserved.
    pub node_id: String,
    /// SHORT module name (e.g. `src`) — the legacy default `name`. Ambiguous across packages; kept additive.
    pub name: String,
    /// QUALIFIED, repo-relative module path (e.g. `packages/a/src`). The canonical identity + sort key.
    pub qualified_name: String,
}

/// The last path segment of a repo-relative module path (`packages/a/src` -> `src`) — the SHORT `name` for a
/// LiveGraph module identity (which is itself the qualified directory path).
pub fn module_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Canonicalize derived module cycles into the deterministic, backend-independent JSON output (D1=B/D2=B).
///
/// For each input cycle: sort + de-duplicate its nodes by `qualified_name` (mirroring the compare's
/// `BTreeSet<String>` cycle basis; on a qualified-name collision the lexicographically-smallest `node_id` is
/// kept, deterministically). Then sort the cycles by their qualified-name vector. Emit the additive JSON:
/// `{cycle_id, length, nodes:[{node_id, name, qualified_name, file:null}]}`.
pub fn canonical_module_cycles_json(cycles: &[Vec<CanonModuleCycleNode>]) -> Vec<Value> {
    // 1. WITHIN each cycle: dedup-by-qualified (BTreeMap = sorted by key) keeping the min node_id.
    let mut canon: Vec<Vec<CanonModuleCycleNode>> = cycles
        .iter()
        .map(|cycle| {
            let mut by_qual: BTreeMap<String, CanonModuleCycleNode> = BTreeMap::new();
            for n in cycle {
                by_qual
                    .entry(n.qualified_name.clone())
                    .and_modify(|existing| {
                        if n.node_id < existing.node_id {
                            *existing = n.clone();
                        }
                    })
                    .or_insert_with(|| n.clone());
            }
            // BTreeMap iterates keys ascending -> nodes sorted by qualified_name, unique.
            by_qual.into_values().collect()
        })
        .collect();

    // 2. ACROSS cycles: sort by the qualified-name vector (lexicographic, mirrors BTreeSet<Vec<String>>).
    canon.sort_by(|a, b| {
        let ka: Vec<&str> = a.iter().map(|n| n.qualified_name.as_str()).collect();
        let kb: Vec<&str> = b.iter().map(|n| n.qualified_name.as_str()).collect();
        ka.cmp(&kb)
    });

    // 3. Emit the additive JSON; cycle_id reassigned by canonical order; length = unique member count.
    canon
        .iter()
        .enumerate()
        .map(|(i, nodes)| {
            let node_values: Vec<Value> = nodes
                .iter()
                .map(|n| {
                    json!({
                        "node_id": n.node_id,
                        "name": n.name,
                        "qualified_name": n.qualified_name,
                        "file": Value::Null,
                    })
                })
                .collect();
            json!({
                "cycle_id": format!("cycle-{}", i + 1),
                "length": node_values.len(),
                "nodes": node_values,
            })
        })
        .collect()
}

/// SQLite-default adapter: map `find_cycles`' raw `CycleResult` (SHORT `name` + `node_uid`) plus the
/// uid -> qualified-path map into the canonical output. `qualified_name` is the qualified module path; it falls
/// back to the short `name` when the map lacks the uid (keeping the render consistent with the compare basis,
/// which reads the same `module_qualified_names`).
pub fn sqlite_module_cycles_json(
    cycles: &[CycleResult],
    qualified: &HashMap<String, String>,
) -> Vec<Value> {
    let normalized: Vec<Vec<CanonModuleCycleNode>> = cycles
        .iter()
        .map(|c| {
            c.nodes
                .iter()
                .map(|n| CanonModuleCycleNode {
                    node_id: n.node_id.clone(),
                    name: n.name.clone(),
                    qualified_name: qualified
                        .get(&n.node_id)
                        .cloned()
                        .unwrap_or_else(|| n.name.clone()),
                })
                .collect()
        })
        .collect();
    canonical_module_cycles_json(&normalized)
}

/// LiveGraph adapter: each member IS the qualified dirname module identity, so `node_id == qualified_name` and
/// the SHORT `name` is its basename. Produces the SAME canonical JSON as [`sqlite_module_cycles_json`] for the
/// same qualified member sets.
pub fn livegraph_module_cycles_json(cycles: &[Vec<String>]) -> Vec<Value> {
    let normalized: Vec<Vec<CanonModuleCycleNode>> = cycles
        .iter()
        .map(|members| {
            members
                .iter()
                .map(|m| CanonModuleCycleNode {
                    node_id: m.clone(),
                    name: module_basename(m).to_string(),
                    qualified_name: m.clone(),
                })
                .collect()
        })
        .collect();
    canonical_module_cycles_json(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

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

    #[test]
    fn module_basename_extracts_last_segment() {
        assert_eq!(module_basename("packages/a/src"), "src");
        assert_eq!(module_basename("src"), "src");
        assert_eq!(module_basename(""), "");
    }

    #[test]
    fn members_sorted_within_cycle() {
        let out = canonical_module_cycles_json(&[vec![
            node("u3", "z", "packages/c/z"),
            node("u1", "a", "packages/a/a"),
            node("u2", "m", "packages/b/m"),
        ]]);
        assert_eq!(
            quals(&out[0]),
            vec!["packages/a/a", "packages/b/m", "packages/c/z"]
        );
    }

    #[test]
    fn cycles_sorted_by_member_tuple() {
        let out = canonical_module_cycles_json(&[
            vec![node("u9", "y", "z/y"), node("u8", "x", "z/x")],
            vec![node("u1", "b", "a/b"), node("u0", "a", "a/a")],
        ]);
        // The "a/*" cycle sorts before the "z/*" cycle; cycle_id reassigned by canonical order.
        assert_eq!(out[0]["cycle_id"], "cycle-1");
        assert_eq!(quals(&out[0]), vec!["a/a", "a/b"]);
        assert_eq!(quals(&out[1]), vec!["z/x", "z/y"]);
    }

    #[test]
    fn dedup_collapses_same_qualified_keeps_min_node_id() {
        // Two uids collide on the same qualified path -> one node, the smaller node_id kept.
        let out = canonical_module_cycles_json(&[vec![
            node("u_z", "src", "packages/a/src"),
            node("u_a", "src", "packages/a/src"),
            node("u_b", "lib", "packages/a/lib"),
        ]]);
        let nodes = out[0]["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2, "collision collapsed to a unique member set");
        assert_eq!(out[0]["length"], 2);
        // The "packages/a/lib" sorts first; "packages/a/src" kept node_id = min("u_a","u_z") = "u_a".
        assert_eq!(nodes[1]["qualified_name"], "packages/a/src");
        assert_eq!(nodes[1]["node_id"], "u_a");
    }

    #[test]
    fn json_preserves_additive_fields() {
        let out = canonical_module_cycles_json(&[vec![
            node("u1", "a", "packages/a"),
            node("u2", "b", "packages/b"),
        ]]);
        let c = &out[0];
        assert_eq!(c["cycle_id"], "cycle-1");
        assert_eq!(c["length"], 2);
        let n = &c["nodes"][0];
        assert_eq!(n["node_id"], "u1");
        assert_eq!(n["name"], "a");
        assert_eq!(n["qualified_name"], "packages/a");
        assert!(n["file"].is_null());
    }

    #[test]
    fn sqlite_and_livegraph_adapters_agree_on_same_member_sets() {
        use repo_graph_storage::queries::{CycleNode, CycleResult};
        // The SAME two qualified cycles, expressed both ways.
        let lg = vec![
            vec!["packages/b/x".to_string(), "packages/a/y".to_string()],
            vec!["pkg/m".to_string(), "pkg/n".to_string()],
        ];
        let sqlite = vec![
            CycleResult {
                cycle_id: "cycle-2".to_string(),
                length: 2,
                nodes: vec![
                    CycleNode {
                        node_id: "uid::pkg/m".to_string(),
                        name: "m".to_string(),
                        file: None,
                    },
                    CycleNode {
                        node_id: "uid::pkg/n".to_string(),
                        name: "n".to_string(),
                        file: None,
                    },
                ],
            },
            CycleResult {
                cycle_id: "cycle-1".to_string(),
                length: 2,
                nodes: vec![
                    CycleNode {
                        node_id: "uid::packages/a/y".to_string(),
                        name: "y".to_string(),
                        file: None,
                    },
                    CycleNode {
                        node_id: "uid::packages/b/x".to_string(),
                        name: "x".to_string(),
                        file: None,
                    },
                ],
            },
        ];
        let mut qmap: HashMap<String, String> = HashMap::new();
        qmap.insert("uid::pkg/m".to_string(), "pkg/m".to_string());
        qmap.insert("uid::pkg/n".to_string(), "pkg/n".to_string());
        qmap.insert("uid::packages/a/y".to_string(), "packages/a/y".to_string());
        qmap.insert("uid::packages/b/x".to_string(), "packages/b/x".to_string());

        let lg_json = livegraph_module_cycles_json(&lg);
        let sq_json = sqlite_module_cycles_json(&sqlite, &qmap);

        // The HUMAN-visible identity (qualified_name) + order + count are identical across backends.
        assert_eq!(lg_json.len(), sq_json.len());
        for (l, s) in lg_json.iter().zip(sq_json.iter()) {
            assert_eq!(quals(l), quals(s));
            assert_eq!(l["cycle_id"], s["cycle_id"]);
            assert_eq!(l["length"], s["length"]);
        }
        // And the canonical order is deterministic: "packages/*" cycle before "pkg/*".
        assert_eq!(quals(&sq_json[0]), vec!["packages/a/y", "packages/b/x"]);
        assert_eq!(quals(&sq_json[1]), vec!["pkg/m", "pkg/n"]);
    }

    #[test]
    fn cycle_order_matches_compare_canonical_set_basis() {
        // Guard the byte-identity invariant: our cross-cycle order equals the compare's BTreeSet<Vec<String>>
        // basis (both plain lexicographic). If a future edit changes the comparator, this fails.
        let cycles = vec![
            vec!["z/b".to_string(), "z/a".to_string()],
            vec!["a/d".to_string(), "a/c".to_string()],
            vec!["m/y".to_string(), "m/x".to_string()],
        ];
        let expected: Vec<Vec<String>> = cycles
            .iter()
            .map(|c| {
                c.iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            })
            .collect::<BTreeSet<Vec<String>>>()
            .into_iter()
            .collect();
        let out = livegraph_module_cycles_json(&cycles);
        let got: Vec<Vec<String>> = out.iter().map(quals).collect();
        assert_eq!(got, expected);
    }
}
