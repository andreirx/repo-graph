//! Iterative AST-walk helper shared by the re-parse postpass detectors that
//! live in this crate (`express_detector`, `react_detector`).
//!
//! PERSIST-RECURSION-1: those detectors formerly descended the tree-sitter AST
//! with plain recursion, so the call-stack depth grew with tree depth and the
//! index process could abort (`fatal runtime error: stack overflow`) on deeply
//! nested / generated TSX at scale — the same failure class the boundary and
//! policy postpasses hit. This helper performs the same pre-order traversal with
//! an explicit heap-allocated work stack (depth is heap-bounded) and preserves
//! the recursive visit order exactly, so emitted facts are byte-for-byte
//! unchanged on non-pathological input.

use std::ops::ControlFlow;
use tree_sitter::Node;

/// Maximum AST depth a re-parse postpass descends before skipping a file's facts
/// for that postpass. Generous: real source is orders of magnitude shallower, so
/// this bounds only pathological / generated files. The postpass AST walks are
/// iterative (heap-bounded), so any depth up to and beyond this is safe — the
/// guard is honest degradation plus a resource bound, not the overflow fix itself.
///
/// Shared by the compose-level postpasses (policy-facts, C/TS boundary
/// interactions) and the in-crate detectors (express, react), so every re-parse
/// postpass applies the SAME bound (PERSIST-RECURSION-1 item 2).
pub(crate) const MAX_POSTPASS_TREE_DEPTH: usize = 10_000;

/// Return `true` if any node in `root`'s subtree is deeper than `limit`
/// (`root` alone counts as depth 1). Iterative — it never recurses, so the depth
/// check itself cannot overflow — and early-exits the instant the limit is passed.
pub(crate) fn tree_exceeds_depth(root: &Node, limit: usize) -> bool {
    let mut stack: Vec<(Node, usize)> = vec![(*root, 1)];
    while let Some((node, depth)) = stack.pop() {
        if depth > limit {
            return true;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push((child, depth + 1));
        }
    }
    false
}

/// Visit `root` and all its descendants in pre-order — each node before its
/// children, children left-to-right — the exact order of the natural recursive
/// `visit(node); for child in node.children() { recurse(child) }`. `visit`
/// returns [`ControlFlow::Break`] to stop the whole walk early (find-first
/// walks) or [`ControlFlow::Continue`] to keep going (collect-all walks).
pub(crate) fn visit_preorder<'a>(
    root: Node<'a>,
    mut visit: impl FnMut(Node<'a>) -> ControlFlow<()>,
) {
    let mut stack: Vec<Node<'a>> = vec![root];
    while let Some(node) = stack.pop() {
        if visit(node).is_break() {
            return;
        }
        // Push children in reverse document order so the left-most child is
        // popped (visited) first — preserving pre-order.
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
}
