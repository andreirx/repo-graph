//! CYCLE-HONESTY-1 (§2.2): the cycle-BODY renderer — a real DFS walk over carried edges, else the
//! `members (unordered)` fallback.
//!
//! Abstraction one-liner — WHAT: the crate-private body-rendering + directed-walk-finding for one cycle.
//! CONCRETE CURRENT USERS: the three [`super::CyclesResponse`] renderers (`render_human`,
//! `render_human_file_import`, `render_human_module_import`) via [`render_cycle_body`]. AXIS: none — one
//! renderer, split from `cycles/mod.rs` only to keep both files under the 500-line guardrail and to isolate
//! the graph-walk algorithm from the response/vocabulary formatting. REJECTED SIMPLER: leaving it inline in
//! `mod.rs` (which pushed that file to 746 lines, over the guardrail).
//!
//! The invariant this module enforces: an arrow (`A -> B`) is drawn ONLY between two members with a
//! VERIFIED carried edge. No carried edges, an incomplete (truncated) set, or no directed walk over the
//! carried subset → `members (unordered)`, never a fabricated ring drawn from the member ORDER.

use std::collections::{HashMap, HashSet};

use super::{Cycle, CycleEdge, CycleNode};

/// Render one cycle's members as INDENTED lines. With the COMPLETE real intra-SCC edge set carried AND a
/// directed walk formed over it, draw that walk (`A -> B -> C -> A`) plus a `(+ N more …)` line for members
/// off the displayed walk; otherwise render `members (unordered): …` with NO arrows. An arrow only ever
/// appears between two members with a verified carried edge — the whole point of this slice.
///
/// A TRUNCATED edge set (`edges_truncated == Some(true)`) is one of the no-arrows fallback cases (operator
/// ruling A1, spec §2.2), alongside the LiveGraph route and older daemon replies: the carried edges are an
/// incomplete subset, so a walk over them could imply a chain the full set does not. It is rendered
/// `members (unordered)`, never a partial walk — checked BEFORE any walk attempt.
pub(super) fn render_cycle_body(cycle: &Cycle) -> String {
    if cycle.nodes.is_empty() {
        return "  (empty cycle)\n".to_string();
    }

    // Truncated edges are incomplete -> no arrows may be drawn from them (§2.2). Fall back to unordered.
    if cycle.edges_truncated == Some(true) {
        return render_unordered(cycle);
    }

    if let Some(edges) = cycle.edges.as_deref() {
        if !edges.is_empty() {
            if let Some(ring) = find_walk(&cycle.nodes, edges) {
                let display: HashMap<&str, &str> = cycle
                    .nodes
                    .iter()
                    .map(|n| (n.node_id.as_str(), n.display()))
                    .collect();
                let chain: Vec<&str> = ring
                    .iter()
                    .map(|id| *display.get(id.as_str()).unwrap_or(&id.as_str()))
                    .collect();
                let mut s = format!("  {} -> {}\n", chain.join(" -> "), chain[0]);
                let on_walk: HashSet<&str> = ring.iter().map(String::as_str).collect();
                let more = cycle
                    .nodes
                    .iter()
                    .filter(|n| !on_walk.contains(n.node_id.as_str()))
                    .count();
                if more > 0 {
                    s.push_str(&format!(
                        "  (+ {more} more member{} in this cycle)\n",
                        if more == 1 { "" } else { "s" }
                    ));
                }
                return s;
            }
            // Edges carried but no directed walk could be formed over the set -> fall through to the honest
            // unordered listing, never a fake ring.
        }
    }
    render_unordered(cycle)
}

/// The no-arrows fallback — `members (unordered): A, B, C`. Members are the canonical, already-unique node
/// set; a long list is capped with an explicit `(+ K more)` so the line stays readable (the count line above
/// already states the full member size; `--json` carries every member).
fn render_unordered(cycle: &Cycle) -> String {
    const SHOWN: usize = 8;
    let names: Vec<&str> = cycle.nodes.iter().map(CycleNode::display).collect();
    if names.len() <= SHOWN {
        format!("  members (unordered): {}\n", names.join(", "))
    } else {
        let more = names.len() - SHOWN;
        format!(
            "  members (unordered): {} (+ {more} more)\n",
            names[..SHOWN].join(", ")
        )
    }
}

/// Find a directed cycle (a closed walk) using ONLY the carried real edges. Every SCC contains one, but the
/// carried set may be capped, so a walk is not guaranteed over the subset — returns `None` then (the caller
/// renders unordered). Iterative DFS with an on-path stack; adjacency is sorted for deterministic output.
/// Returns the ring's member `node_id`s in walk order (the closing edge goes from the last back to the
/// first).
fn find_walk(nodes: &[CycleNode], edges: &[CycleEdge]) -> Option<Vec<String>> {
    let member_ids: HashSet<&str> = nodes.iter().map(|n| n.node_id.as_str()).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in edges {
        // Defensive: only traverse edges whose BOTH endpoints are members of this cycle.
        if member_ids.contains(e.from_node_id.as_str())
            && member_ids.contains(e.to_node_id.as_str())
        {
            adj.entry(&e.from_node_id).or_default().push(&e.to_node_id);
        }
    }
    for targets in adj.values_mut() {
        targets.sort_unstable();
    }

    // Deterministic start order: iterate members in their canonical (given) order.
    let mut visited: HashSet<&str> = HashSet::new();
    for start in nodes.iter().map(|n| n.node_id.as_str()) {
        if visited.contains(start) {
            continue;
        }
        if let Some(ring) = dfs_find_cycle(start, &adj, &mut visited) {
            return Some(ring);
        }
    }
    None
}

/// Iterative DFS from `start` following `adj`; returns the first directed cycle found (the slice of the
/// current path from the revisited node onward). Nodes fully explored without closing a cycle are marked
/// `visited` so a later start does not re-walk them.
fn dfs_find_cycle<'a>(
    start: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
) -> Option<Vec<String>> {
    // Stack frames: (node, index of the next neighbour to try).
    let mut path: Vec<&'a str> = vec![start];
    let mut on_path: HashSet<&'a str> = HashSet::from([start]);
    let mut cursor: Vec<usize> = vec![0];

    while let Some(&node) = path.last() {
        let i = *cursor.last().unwrap();
        let neighbours = adj.get(node).map(Vec::as_slice).unwrap_or(&[]);
        if i < neighbours.len() {
            *cursor.last_mut().unwrap() += 1;
            let next = neighbours[i];
            if on_path.contains(next) {
                // Back-edge -> a cycle. Return the path slice from `next` to the current node.
                let from = path.iter().position(|&x| x == next).unwrap();
                return Some(path[from..].iter().map(|s| s.to_string()).collect());
            }
            if !visited.contains(next) {
                path.push(next);
                on_path.insert(next);
                cursor.push(0);
            }
        } else {
            // Exhausted `node`'s neighbours without closing a cycle through it.
            visited.insert(node);
            on_path.remove(node);
            path.pop();
            cursor.pop();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A MODULE cycle node; `qualified_name` defaults to `None` (exercises the `name` fallback).
    fn cnode(node_id: &str, name: &str) -> CycleNode {
        CycleNode {
            node_id: node_id.to_string(),
            name: name.to_string(),
            qualified_name: None,
            file: None,
        }
    }

    /// A cycle with NO carried edges (the LiveGraph route + older daemon reply) -> unordered render.
    fn cyc(nodes: Vec<CycleNode>) -> Cycle {
        Cycle {
            nodes,
            edges: None,
            edges_truncated: None,
        }
    }

    /// A cycle carrying REAL edges (the SQLite route) -> real-walk render.
    fn cyc_e(nodes: Vec<CycleNode>, edges: Vec<(&str, &str)>) -> Cycle {
        Cycle {
            nodes,
            edges: Some(
                edges
                    .into_iter()
                    .map(|(f, t)| CycleEdge {
                        from_node_id: f.to_string(),
                        to_node_id: t.to_string(),
                    })
                    .collect(),
            ),
            edges_truncated: None,
        }
    }

    #[test]
    fn walk_follows_real_edges_not_member_order() {
        // The audit's core defect: the old renderer drew a ring from the (sorted) member ORDER. Here the
        // real edges form a->c->b->a; the member order is a,b,c. An order-derived ring would be
        // `a -> b -> c -> a` (FABRICATED). The real walk is `a -> c -> b -> a`.
        let body = render_cycle_body(&cyc_e(
            vec![cnode("a", "a"), cnode("b", "b"), cnode("c", "c")],
            vec![("a", "c"), ("c", "b"), ("b", "a")],
        ));
        assert!(
            body.contains("a -> c -> b -> a"),
            "real walk over edges: {body}"
        );
        assert!(
            !body.contains("a -> b -> c -> a"),
            "the fabricated order-ring must NOT appear: {body}"
        );
    }

    #[test]
    fn no_edges_renders_unordered_with_no_arrows() {
        let body = render_cycle_body(&cyc(vec![
            cnode("n1", "src/a"),
            cnode("n2", "src/b"),
            cnode("n3", "src/c"),
        ]));
        assert!(
            body.contains("members (unordered): src/a, src/b, src/c"),
            "unordered listing: {body}"
        );
        assert!(
            !body.contains(" -> "),
            "NO arrows without carried edges: {body}"
        );
    }

    #[test]
    fn empty_edges_renders_unordered() {
        let body = render_cycle_body(&cyc_e(vec![cnode("a", "a"), cnode("b", "b")], vec![]));
        assert!(body.contains("members (unordered): a, b"), "{body}");
        assert!(!body.contains(" -> "), "{body}");
    }

    #[test]
    fn offwalk_members_reported_as_plus_n_more() {
        // The walk covers {a,b}; c,d are members off the displayed walk.
        let body = render_cycle_body(&cyc_e(
            vec![
                cnode("a", "a"),
                cnode("b", "b"),
                cnode("c", "c"),
                cnode("d", "d"),
            ],
            vec![("a", "b"), ("b", "a")],
        ));
        assert!(body.contains("a -> b -> a"), "{body}");
        assert!(
            body.contains("(+ 2 more members in this cycle)"),
            "off-walk members reported: {body}"
        );
    }

    #[test]
    fn truncated_edges_render_unordered_no_arrows() {
        // CYCLE-HONESTY-1 §2.2 (operator ruling A1): a truncated (incomplete) edge set is a no-arrows
        // fallback case. Even though every carried edge here is real (a<->b), the daemon flagged the set as
        // capped, so the renderer must NOT draw a walk over the subset — it renders `members (unordered)`.
        let mut cycle = cyc_e(
            vec![cnode("a", "a"), cnode("b", "b")],
            vec![("a", "b"), ("b", "a")],
        );
        cycle.edges_truncated = Some(true);
        let body = render_cycle_body(&cycle);
        assert!(
            body.contains("members (unordered): a, b"),
            "truncated edges -> unordered listing: {body}"
        );
        assert!(
            !body.contains(" -> "),
            "truncated edges must draw NO arrows: {body}"
        );
    }

    #[test]
    fn walk_prefers_qualified_name() {
        // With qualified_name present, the walk shows the QUALIFIED path, not the collision-prone short name.
        let body = render_cycle_body(&cyc_e(
            vec![
                CycleNode {
                    node_id: "u_a".to_string(),
                    name: "src".to_string(),
                    qualified_name: Some("packages/a/src".to_string()),
                    file: None,
                },
                CycleNode {
                    node_id: "u_b".to_string(),
                    name: "src".to_string(),
                    qualified_name: Some("packages/b/src".to_string()),
                    file: None,
                },
            ],
            vec![("u_a", "u_b"), ("u_b", "u_a")],
        ));
        assert!(
            body.contains("packages/a/src -> packages/b/src -> packages/a/src"),
            "qualified path shown: {body}"
        );
        assert!(
            !body.contains("src -> src"),
            "short name must not render: {body}"
        );
    }
}
