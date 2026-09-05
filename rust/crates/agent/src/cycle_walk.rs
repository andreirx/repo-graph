//! COHERENCE-3 (§2.1): the ONE cycle-WALK kernel — the shared intra-SCC edge selection and the
//! directed-walk finder, so `orient`'s cycle parenthetical and the `cycles` command render the
//! SAME walk for one snapshot (extends the ORIENT-CYCLES-DISAGREE-1 "one derivation" seam from
//! counts/verdicts to WALKS).
//!
//! # Why this module exists
//!
//! Before this slice two surfaces derived a cycle "walk" independently: `cycles`
//! (`rgr::presentation::cycles::walk`) drew a REAL DFS walk over the daemon's carried intra-SCC
//! edges, while `orient` (`rgr::presentation::orient_guidance::format_cycle_anchor`) FABRICATED a
//! ring by joining the lexically-sorted member basenames with `->` — a walk the import graph does
//! not support (the audit's `apps -> backends -> backends -> … -> apps`: a basename collision and a
//! nonexistent edge). CYCLE-HONESTY-1 fixed `cycles`; `orient` was a second, dishonest derivation.
//!
//! This module hoists BOTH honesty-critical steps into the port-owner `agent` crate (already
//! depended on by `storage` → `orient`, by `daemon-runtime` → `cycles`, and by `rgr` → both
//! renderers), so every surface that draws a walk calls ONE function:
//!   - [`intra_cycle_edges`] — the deterministic real-edge selection (dedup + sort + [`EDGE_CAP`]),
//!     the SAME set the daemon's `attach_intra_cycle_edges` and the storage adapter's cycle
//!     labeling feed to the walk finder.
//!   - [`find_cycle_walk`] — the directed-walk DFS over ONLY those real edges; `None` when no walk
//!     can be formed (no/empty/incomplete edges), which every caller renders as the honest
//!     unordered form.
//!
//! Abstraction record — module: `cycle_walk`; concrete current users: `rgr`'s cycles renderer
//! (`walk::render_cycle_body`), the daemon's intra-SCC edge attachment
//! (`cycle_output::edges::attach_intra_cycle_edges`), and the storage adapter's `orient` cycle
//! labeling (`agent_cycle_labeling::label_module_cycles`, which precomputes `orient`'s walk). Axis:
//! none — a single deterministic graph algorithm shared so the two rendered walks cannot drift;
//! rejected simpler: leaving `find_walk` in `rgr` and the bucketing in `daemon-runtime`, then
//! REIMPLEMENTING both in the `orient` serving path — two copies of the exact logic this slice
//! unifies (the drift COHERENCE-3 removes).
//!
//! # Determinism (load-bearing for the seam)
//!
//! The walk a DFS finds depends on the START order. `cycles` feeds members already sorted by
//! qualified path; `orient`'s serving path feeds them in storage traversal order. So the finder
//! sorts its own start candidates by DISPLAY (then `node_id`) internally — a walk is then a pure
//! function of the cycle's (members, edges), independent of caller order. Both surfaces therefore
//! produce the IDENTICAL walk for one cycle, which is what makes disagreement unrepresentable.

use std::collections::{BTreeSet, HashMap, HashSet};

/// The per-cycle intra-SCC edge cap (CYCLE-HONESTY-1 §2.1). A cycle with more real intra-cycle
/// import edges than this keeps the first [`EDGE_CAP`] (deterministically sorted) and reports
/// truncation — never a silent cut. A truncated set draws NO walk (an incomplete subset could imply
/// a chain the full set does not); the renderer falls back to the unordered form.
pub const EDGE_CAP: usize = 200;

/// Select the REAL intra-cycle directed edges among `member_ids` from the snapshot's module→module
/// IMPORTS `all_edges` (endpoints keyed by the SAME `node_id` the members carry). An edge is kept
/// iff BOTH endpoints are members of THIS cycle and `from != to` (a self-import is not a cycle
/// edge). Deduped and sorted (`BTreeSet`) for determinism, then capped at [`EDGE_CAP`]; the returned
/// `bool` is `true` iff the real set exceeded the cap (the truncation marker). This is the ONE
/// selection both the `cycles` route (`attach_intra_cycle_edges`) and the `orient` route
/// (`label_module_cycles`) use, so the two draw over the SAME edges.
pub fn intra_cycle_edges(
    member_ids: &[&str],
    all_edges: &[(&str, &str)],
) -> (Vec<(String, String)>, bool) {
    let members: HashSet<&str> = member_ids.iter().copied().collect();
    let mut kept: BTreeSet<(String, String)> = BTreeSet::new();
    for (from, to) in all_edges {
        if from == to {
            continue;
        }
        if members.contains(from) && members.contains(to) {
            kept.insert(((*from).to_string(), (*to).to_string()));
        }
    }
    let truncated = kept.len() > EDGE_CAP;
    let edges: Vec<(String, String)> = kept.into_iter().take(EDGE_CAP).collect();
    (edges, truncated)
}

/// Find a directed cycle (a closed walk) over ONLY the carried real `edges`, returning its members'
/// DISPLAY names in walk order (the closing edge runs from the last back to the first). `members`
/// are `(node_id, display)`; `edges` are `(from_node_id, to_node_id)`. `None` when no walk can be
/// formed — no edges, or a set (e.g. a truncated one the caller already emptied) over which no
/// directed cycle closes — which every caller renders as the honest unordered form, NEVER a
/// fabricated ring.
///
/// Start candidates are sorted by `(display, node_id)` so the result is a pure function of the
/// cycle, independent of the caller's member order (see the module determinism note).
pub fn find_cycle_walk(members: &[(&str, &str)], edges: &[(&str, &str)]) -> Option<Vec<String>> {
    let member_ids: HashSet<&str> = members.iter().map(|(id, _)| *id).collect();
    let display: HashMap<&str, &str> = members.iter().copied().collect();

    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in edges {
        // Defensive: only traverse edges whose BOTH endpoints are members of this cycle.
        if member_ids.contains(from) && member_ids.contains(to) {
            adj.entry(from).or_default().push(to);
        }
    }
    for targets in adj.values_mut() {
        targets.sort_unstable();
    }

    // Deterministic start order: members sorted by display (then node_id) — order-independent.
    let mut starts: Vec<(&str, &str)> = members.to_vec();
    starts.sort_unstable_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(b.0)));

    let mut visited: HashSet<&str> = HashSet::new();
    for (start, _) in starts {
        if visited.contains(start) {
            continue;
        }
        if let Some(ring) = dfs_find_cycle(start, &adj, &mut visited) {
            return Some(
                ring.iter()
                    .map(|id| (*display.get(id.as_str()).unwrap_or(&id.as_str())).to_string())
                    .collect(),
            );
        }
    }
    None
}

/// Iterative DFS from `start` following `adj`; returns the first directed cycle found (the slice of
/// the current path from the revisited node onward, as `node_id`s). Nodes fully explored without
/// closing a cycle are marked `visited` so a later start does not re-walk them.
fn dfs_find_cycle<'a>(
    start: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
) -> Option<Vec<String>> {
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
                let from = path.iter().position(|&x| x == next).unwrap();
                return Some(path[from..].iter().map(|s| s.to_string()).collect());
            }
            if !visited.contains(next) {
                path.push(next);
                on_path.insert(next);
                cursor.push(0);
            }
        } else {
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

    #[test]
    fn walk_follows_real_edges_not_member_order() {
        // Real edges a->c->b->a; member order a,b,c. An order-derived ring would be a->b->c->a
        // (FABRICATED). The real walk is a->c->b->a — and it is the SAME regardless of input order.
        let members = [("a", "a"), ("b", "b"), ("c", "c")];
        let edges = [("a", "c"), ("c", "b"), ("b", "a")];
        assert_eq!(
            find_cycle_walk(&members, &edges),
            Some(vec!["a".to_string(), "c".to_string(), "b".to_string()])
        );
        // Shuffle the member order — same walk (determinism / seam invariant).
        let shuffled = [("c", "c"), ("a", "a"), ("b", "b")];
        assert_eq!(
            find_cycle_walk(&shuffled, &edges),
            find_cycle_walk(&members, &edges)
        );
    }

    #[test]
    fn no_or_empty_edges_is_none() {
        let members = [("a", "a"), ("b", "b")];
        assert_eq!(find_cycle_walk(&members, &[]), None);
    }

    #[test]
    fn walk_returns_display_not_node_id() {
        let members = [("u_a", "packages/a/src"), ("u_b", "packages/b/src")];
        let edges = [("u_a", "u_b"), ("u_b", "u_a")];
        assert_eq!(
            find_cycle_walk(&members, &edges),
            Some(vec![
                "packages/a/src".to_string(),
                "packages/b/src".to_string()
            ])
        );
    }

    #[test]
    fn intra_cycle_edges_keeps_only_members_drops_self() {
        let (edges, truncated) = intra_cycle_edges(
            &["a", "b"],
            &[("a", "b"), ("b", "a"), ("a", "a"), ("a", "z")],
        );
        assert_eq!(
            edges,
            vec![
                ("a".to_string(), "b".to_string()),
                ("b".to_string(), "a".to_string())
            ]
        );
        assert!(!truncated);
    }

    #[test]
    fn intra_cycle_edges_marks_truncation() {
        // 30 members, every ordered distinct pair (870 > EDGE_CAP) -> capped + truncated.
        let ids: Vec<String> = (0..30).map(|i| format!("m{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let mut all = Vec::new();
        for a in &refs {
            for b in &refs {
                if a != b {
                    all.push((*a, *b));
                }
            }
        }
        let (edges, truncated) = intra_cycle_edges(&refs, &all);
        assert_eq!(edges.len(), EDGE_CAP);
        assert!(truncated);
    }
}
