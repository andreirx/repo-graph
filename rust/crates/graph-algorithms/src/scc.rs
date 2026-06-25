//! Tarjan's strongly connected components algorithm.
//!
//! Finds all SCCs in a directed graph in O(V + E) time.
//!
//! # Algorithm
//!
//! Tarjan's algorithm performs a single DFS traversal:
//! 1. Assign each node an index (discovery order) and lowlink
//! 2. Lowlink is the smallest index reachable from the subtree
//! 3. When lowlink == index, the node is an SCC root
//! 4. Pop the stack to extract the SCC members
//!
//! # Output
//!
//! Returns all SCCs. For cycle detection, filter to SCCs with size > 1.
//! Single-node SCCs without self-loops are trivial (not cycles).
//!
//! # Determinism
//!
//! Output order is deterministic given the same input edge order.
//! Within each SCC, members are in reverse finishing order (stack pop order).

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

/// Cooperative cancellation checkpoint (DAEMON-CANCEL-1, in-loop / Option A).
///
/// A closure consulted at bounded intervals inside a heavy Rust loop. Returning
/// [`ControlFlow::Break`] means "abandon the work and unwind"; [`ControlFlow::Continue`]
/// means "keep going". This is the SAME shape D5b uses for index/refresh abort:
/// the daemon builds a checkpoint that emits a heartbeat and reports `Break` when
/// the transport write fails (peer gone). The seam is a bare `&mut dyn FnMut`, so
/// no shared cancel-token type and no new crate cross the boundary — only
/// `std::ops::ControlFlow`.
///
/// It is `FnMut` (not `Fn`) because the daemon's heartbeat probe borrows the
/// progress emitter mutably. Threaded into the Rust-side query loops
/// (`find_sccs_cancellable` here; `LiveGraph::path_cancellable` reuses this same
/// type). SQL-bound heavy work cannot be checkpointed mid-statement from Rust and
/// uses `sqlite3_interrupt` instead (DAEMON-CANCEL-2).
pub type CancelCheck<'a> = &'a mut dyn FnMut() -> ControlFlow<()>;

/// Returned by the `*_cancellable` algorithms when a [`CancelCheck`] asked to
/// abandon the work. Read-only traversals hold no partial state, so a cancelled
/// computation simply discards its in-progress result — there is nothing to roll
/// back (DAEMON-CANCEL-1: "read-only ⇒ no rollback").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

/// How many DFS steps to take between cancellation checks. The check itself is a
/// virtual call (and, in the daemon, a transport write), so amortizing it keeps
/// the non-cancellable wrapper's overhead negligible while bounding cancel latency
/// to a few hundred cheap steps.
const CANCEL_CHECK_INTERVAL: usize = 256;

/// A directed edge in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectedEdge {
    pub source: String,
    pub target: String,
}

/// A strongly connected component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scc {
    /// Node IDs in this component (reverse finishing order).
    pub members: Vec<String>,
}

impl Scc {
    /// Number of nodes in this SCC.
    pub fn size(&self) -> usize {
        self.members.len()
    }

    /// True if this SCC represents a cycle (size > 1, or size == 1 with self-loop).
    pub fn is_cycle(&self) -> bool {
        self.members.len() > 1
    }
}

/// Result of SCC computation.
#[derive(Debug, Clone)]
pub struct SccResult {
    /// All strongly connected components found.
    /// Includes trivial single-node SCCs.
    pub all_sccs: Vec<Scc>,

    /// Only SCCs with size > 1 (actual cycles).
    pub cycles: Vec<Scc>,

    /// Total node count in the graph.
    pub node_count: usize,

    /// Total edge count in the graph.
    pub edge_count: usize,
}

impl SccResult {
    /// Number of cycles (non-trivial SCCs).
    pub fn cycle_count(&self) -> usize {
        self.cycles.len()
    }

    /// True if the graph is acyclic (no non-trivial SCCs).
    pub fn is_acyclic(&self) -> bool {
        self.cycles.is_empty()
    }
}

/// Find all strongly connected components using Tarjan's algorithm.
///
/// Time complexity: O(V + E)
/// Space complexity: O(V)
///
/// # Arguments
///
/// * `edges` - Directed edges in the graph
///
/// # Returns
///
/// SCC result containing all components and filtered cycles.
///
/// # Example
///
/// ```
/// use repo_graph_algorithms::{find_sccs, DirectedEdge};
///
/// let edges = vec![
///     DirectedEdge { source: "A".into(), target: "B".into() },
///     DirectedEdge { source: "B".into(), target: "C".into() },
///     DirectedEdge { source: "C".into(), target: "A".into() }, // cycle: A -> B -> C -> A
/// ];
///
/// let result = find_sccs(&edges);
/// assert_eq!(result.cycle_count(), 1);
/// assert_eq!(result.cycles[0].size(), 3);
/// ```
pub fn find_sccs(edges: &[DirectedEdge]) -> SccResult {
    // The non-cancellable entry point is the cancellable one with a checkpoint
    // that never breaks. `expect` cannot fire: a never-breaking checkpoint makes
    // the `Cancelled` arm unreachable.
    find_sccs_cancellable(edges, &mut || ControlFlow::Continue(()))
        .expect("never-breaking checkpoint cannot cancel")
}

/// Find all strongly connected components, consulting `cancel` at bounded
/// intervals so a peer disconnect can abandon a long traversal mid-flight
/// (DAEMON-CANCEL-1). On [`ControlFlow::Break`] from `cancel` the traversal stops
/// and returns [`Cancelled`]; the partially-built result is dropped (read-only ⇒
/// nothing to roll back).
///
/// [`find_sccs`] is exactly this function with a never-breaking checkpoint, so the
/// two are byte-identical when cancellation is not triggered.
pub fn find_sccs_cancellable(
    edges: &[DirectedEdge],
    cancel: CancelCheck,
) -> Result<SccResult, Cancelled> {
    // Build adjacency list and collect all nodes
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut all_nodes: HashSet<&str> = HashSet::new();

    for edge in edges {
        adjacency
            .entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
        all_nodes.insert(edge.source.as_str());
        all_nodes.insert(edge.target.as_str());
    }

    // Ensure all nodes have adjacency entries (even if empty)
    for node in &all_nodes {
        adjacency.entry(node).or_default();
    }

    let node_count = all_nodes.len();
    let edge_count = edges.len();

    // Sort nodes for deterministic traversal order
    let mut sorted_nodes: Vec<&str> = all_nodes.into_iter().collect();
    sorted_nodes.sort();

    // Tarjan state
    let mut index_counter: usize = 0;
    let mut stack: Vec<&str> = Vec::new();
    let mut on_stack: HashSet<&str> = HashSet::new();
    let mut indices: HashMap<&str, usize> = HashMap::new();
    let mut lowlinks: HashMap<&str, usize> = HashMap::new();
    let mut all_sccs: Vec<Scc> = Vec::new();

    // Shared DFS-step counter so the cancellation cadence is bounded across the
    // whole traversal, not reset per start node (a single dense start node can
    // visit the entire graph in one `tarjan_visit` call).
    let mut steps_since_check: usize = 0;

    // Recursive DFS (using explicit stack to avoid stack overflow on deep graphs).
    // `&&` short-circuits: tarjan_visit only runs for an unvisited start node.
    for start_node in &sorted_nodes {
        if !indices.contains_key(start_node)
            && tarjan_visit(
                start_node,
                &adjacency,
                &mut index_counter,
                &mut stack,
                &mut on_stack,
                &mut indices,
                &mut lowlinks,
                &mut all_sccs,
                cancel,
                &mut steps_since_check,
            )
            .is_break()
        {
            return Err(Cancelled);
        }
    }

    // Filter to actual cycles (SCCs with size > 1)
    let cycles: Vec<Scc> = all_sccs
        .iter()
        .filter(|scc| scc.is_cycle())
        .cloned()
        .collect();

    Ok(SccResult {
        all_sccs,
        cycles,
        node_count,
        edge_count,
    })
}

/// Iterative Tarjan DFS to avoid stack overflow on deep graphs.
///
/// Uses explicit state stack instead of recursion.
///
/// Returns [`ControlFlow::Break`] if `cancel` asked to abandon the traversal
/// (checked every [`CANCEL_CHECK_INTERVAL`] DFS steps via the shared
/// `steps_since_check` counter), otherwise [`ControlFlow::Continue`] once the
/// component rooted at `start` is fully explored.
#[allow(clippy::too_many_arguments)]
fn tarjan_visit<'a>(
    start: &'a str,
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
    index_counter: &mut usize,
    stack: &mut Vec<&'a str>,
    on_stack: &mut HashSet<&'a str>,
    indices: &mut HashMap<&'a str, usize>,
    lowlinks: &mut HashMap<&'a str, usize>,
    result: &mut Vec<Scc>,
    cancel: CancelCheck,
    steps_since_check: &mut usize,
) -> ControlFlow<()> {
    // State for iterative DFS: (node, neighbor_index, phase)
    // phase 0: entering node
    // phase 1: processing neighbors
    // phase 2: finishing node
    #[derive(Clone)]
    struct Frame<'a> {
        node: &'a str,
        neighbor_idx: usize,
        phase: u8,
    }

    let mut call_stack: Vec<Frame> = vec![Frame {
        node: start,
        neighbor_idx: 0,
        phase: 0,
    }];

    while let Some(frame) = call_stack.last().cloned() {
        // Cooperative cancellation: every CANCEL_CHECK_INTERVAL DFS steps, ask the
        // checkpoint whether to abandon. Read-only ⇒ on Break we simply unwind and
        // the caller drops the partial result — nothing to roll back.
        *steps_since_check += 1;
        if *steps_since_check >= CANCEL_CHECK_INTERVAL {
            *steps_since_check = 0;
            if cancel().is_break() {
                return ControlFlow::Break(());
            }
        }

        match frame.phase {
            0 => {
                // Entering node
                let node = frame.node;

                if indices.contains_key(node) {
                    // Already visited
                    call_stack.pop();
                    continue;
                }

                // Initialize node
                indices.insert(node, *index_counter);
                lowlinks.insert(node, *index_counter);
                *index_counter += 1;
                stack.push(node);
                on_stack.insert(node);

                // Move to neighbor processing
                call_stack.last_mut().unwrap().phase = 1;
            }
            1 => {
                // Processing neighbors
                let node = frame.node;
                let neighbors = adjacency.get(node).map(|v| v.as_slice()).unwrap_or(&[]);

                if frame.neighbor_idx < neighbors.len() {
                    let neighbor = neighbors[frame.neighbor_idx];

                    // Advance neighbor index for next iteration
                    call_stack.last_mut().unwrap().neighbor_idx += 1;

                    if !indices.contains_key(neighbor) {
                        // Neighbor not visited: recurse
                        call_stack.push(Frame {
                            node: neighbor,
                            neighbor_idx: 0,
                            phase: 0,
                        });
                    } else if on_stack.contains(neighbor) {
                        // Neighbor on stack: update lowlink
                        let neighbor_index = indices[neighbor];
                        let current_lowlink = lowlinks[node];
                        if neighbor_index < current_lowlink {
                            lowlinks.insert(node, neighbor_index);
                        }
                    }
                } else {
                    // All neighbors processed, finish node
                    call_stack.last_mut().unwrap().phase = 2;
                }
            }
            2 => {
                // Finishing node
                let node = frame.node;
                call_stack.pop();

                // Update parent's lowlink if we came from a recursive call
                if let Some(parent_frame) = call_stack.last() {
                    if parent_frame.phase == 1 {
                        let parent = parent_frame.node;
                        let child_lowlink = lowlinks[node];
                        let parent_lowlink = lowlinks[parent];
                        if child_lowlink < parent_lowlink {
                            lowlinks.insert(parent, child_lowlink);
                        }
                    }
                }

                // Check if this is an SCC root
                if lowlinks[node] == indices[node] {
                    // Pop stack to form SCC
                    let mut scc_members: Vec<String> = Vec::new();
                    loop {
                        let popped = stack.pop().expect("stack should not be empty");
                        on_stack.remove(popped);
                        scc_members.push(popped.to_string());
                        if popped == node {
                            break;
                        }
                    }
                    // Reverse to get discovery order (optional, for determinism)
                    scc_members.reverse();
                    result.push(Scc {
                        members: scc_members,
                    });
                }
            }
            _ => unreachable!(),
        }
    }

    // Component fully explored without cancellation.
    ControlFlow::Continue(())
}

/// Check if a graph has any cycles.
///
/// More efficient than `find_sccs` when you only need a boolean answer,
/// but currently just delegates to `find_sccs`. Could be optimized later
/// to early-exit on first cycle found.
#[allow(dead_code)] // Public API for future use
pub fn has_cycles(edges: &[DirectedEdge]) -> bool {
    !find_sccs(edges).is_acyclic()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(source: &str, target: &str) -> DirectedEdge {
        DirectedEdge {
            source: source.to_string(),
            target: target.to_string(),
        }
    }

    #[test]
    fn empty_graph_has_no_cycles() {
        let result = find_sccs(&[]);
        assert!(result.is_acyclic());
        assert_eq!(result.cycle_count(), 0);
        assert_eq!(result.node_count, 0);
        assert_eq!(result.edge_count, 0);
    }

    #[test]
    fn single_edge_no_cycle() {
        let edges = vec![edge("A", "B")];
        let result = find_sccs(&edges);
        assert!(result.is_acyclic());
        assert_eq!(result.cycle_count(), 0);
        assert_eq!(result.node_count, 2);
    }

    #[test]
    fn simple_cycle_detected() {
        let edges = vec![edge("A", "B"), edge("B", "C"), edge("C", "A")];
        let result = find_sccs(&edges);
        assert!(!result.is_acyclic());
        assert_eq!(result.cycle_count(), 1);
        assert_eq!(result.cycles[0].size(), 3);
    }

    #[test]
    fn two_separate_cycles() {
        let edges = vec![
            // Cycle 1: A -> B -> A
            edge("A", "B"),
            edge("B", "A"),
            // Cycle 2: C -> D -> E -> C
            edge("C", "D"),
            edge("D", "E"),
            edge("E", "C"),
        ];
        let result = find_sccs(&edges);
        assert_eq!(result.cycle_count(), 2);

        let sizes: Vec<usize> = result.cycles.iter().map(|c| c.size()).collect();
        assert!(sizes.contains(&2)); // A-B cycle
        assert!(sizes.contains(&3)); // C-D-E cycle
    }

    #[test]
    fn dag_no_cycles() {
        // Diamond DAG: A -> B -> D, A -> C -> D
        let edges = vec![
            edge("A", "B"),
            edge("A", "C"),
            edge("B", "D"),
            edge("C", "D"),
        ];
        let result = find_sccs(&edges);
        assert!(result.is_acyclic());
        assert_eq!(result.node_count, 4);
        assert_eq!(result.edge_count, 4);
    }

    #[test]
    fn self_loop_not_counted_as_cycle_by_size() {
        // Self-loop: A -> A
        // This creates an SCC of size 1, which is_cycle() returns false for
        // (by our definition, cycles need size > 1)
        let edges = vec![edge("A", "A")];
        let result = find_sccs(&edges);
        // All SCCs includes the self-loop SCC
        assert_eq!(result.all_sccs.len(), 1);
        // But cycles (size > 1) is empty
        assert_eq!(result.cycle_count(), 0);
    }

    #[test]
    fn complex_graph_with_multiple_sccs() {
        // Graph with mixed structure:
        // SCC1: 1 -> 2 -> 3 -> 1
        // SCC2: 4 -> 5 -> 4
        // Non-cycle: 6 -> 1 (entry to SCC1)
        // Non-cycle: 3 -> 4 (bridge between SCCs)
        let edges = vec![
            edge("1", "2"),
            edge("2", "3"),
            edge("3", "1"), // completes cycle
            edge("4", "5"),
            edge("5", "4"), // completes cycle
            edge("6", "1"), // entry edge
            edge("3", "4"), // bridge edge
        ];
        let result = find_sccs(&edges);
        assert_eq!(result.cycle_count(), 2);
        assert_eq!(result.node_count, 6);
        assert_eq!(result.edge_count, 7);
    }

    #[test]
    fn deterministic_output_order() {
        let edges = vec![edge("A", "B"), edge("B", "C"), edge("C", "A")];

        let result1 = find_sccs(&edges);
        let result2 = find_sccs(&edges);

        assert_eq!(result1.cycles[0].members, result2.cycles[0].members);
    }

    #[test]
    fn large_linear_chain_no_stack_overflow() {
        // Create a long chain: 0 -> 1 -> 2 -> ... -> 999
        let edges: Vec<DirectedEdge> = (0..1000)
            .map(|i| edge(&i.to_string(), &(i + 1).to_string()))
            .collect();

        let result = find_sccs(&edges);
        assert!(result.is_acyclic());
        assert_eq!(result.node_count, 1001);
    }

    #[test]
    fn has_cycles_helper_works() {
        let no_cycle = vec![edge("A", "B")];
        let with_cycle = vec![edge("A", "B"), edge("B", "A")];

        assert!(!has_cycles(&no_cycle));
        assert!(has_cycles(&with_cycle));
    }

    // ── DAEMON-CANCEL-1: cooperative in-loop cancellation ────────────────────

    /// A long linear chain so the DFS while-loop runs far more than
    /// `CANCEL_CHECK_INTERVAL` steps — enough that a checkpoint fires mid-traversal.
    fn long_chain(n: usize) -> Vec<DirectedEdge> {
        (0..n)
            .map(|i| edge(&i.to_string(), &(i + 1).to_string()))
            .collect()
    }

    #[test]
    fn cancellable_matches_plain_when_never_cancelled() {
        // A never-breaking checkpoint must produce results identical to the plain
        // entry point — cancellation is transparent when not triggered.
        let edges = vec![
            edge("1", "2"),
            edge("2", "3"),
            edge("3", "1"),
            edge("3", "4"),
        ];
        let plain = find_sccs(&edges);
        let cancellable = find_sccs_cancellable(&edges, &mut || ControlFlow::Continue(())).unwrap();
        assert_eq!(plain.node_count, cancellable.node_count);
        assert_eq!(plain.edge_count, cancellable.edge_count);
        assert_eq!(plain.cycle_count(), cancellable.cycle_count());
        assert_eq!(
            plain.cycles.first().map(|c| &c.members),
            cancellable.cycles.first().map(|c| &c.members)
        );
    }

    #[test]
    fn cancellable_stops_mid_traversal() {
        // 50k-node chain ⇒ the DFS while-loop runs ~150k steps. A checkpoint that
        // breaks on its FIRST call cancels after ~CANCEL_CHECK_INTERVAL steps — long
        // before the traversal could complete. This proves IN-FLIGHT cancellation
        // (the loop stops mid-traversal), not before-start. Deterministic, no clock.
        let edges = long_chain(50_000);
        let mut checks = 0usize;
        let result = find_sccs_cancellable(&edges, &mut || {
            checks += 1;
            ControlFlow::Break(())
        });
        assert!(matches!(result, Err(Cancelled)));
        assert_eq!(
            checks, 1,
            "should break on the first checkpoint, mid-traversal (≈256 steps in, not 150k)"
        );
    }

    #[test]
    fn cancellable_completes_if_checkpoint_allows() {
        // Same large graph, but a checkpoint that always continues: the traversal
        // runs to completion and reports the acyclic chain. Confirms the checkpoint
        // is consulted many times yet never forces a spurious cancel.
        let edges = long_chain(2_000);
        let mut checks = 0usize;
        let result = find_sccs_cancellable(&edges, &mut || {
            checks += 1;
            ControlFlow::Continue(())
        })
        .expect("continue checkpoint never cancels");
        assert!(result.is_acyclic());
        assert_eq!(result.node_count, 2_001);
        assert!(
            checks > 1,
            "checkpoint should fire repeatedly on a long traversal"
        );
    }
}
