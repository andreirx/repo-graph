//! Iterative tree-walk helpers shared by the TS/JS boundary detectors.
//!
//! PERSIST-RECURSION-1: the detectors formerly descended the tree-sitter AST
//! with plain recursion, so the call-stack depth grew with tree depth and the
//! process aborted (`fatal runtime error: stack overflow`) on deeply nested /
//! generated TS/JS at scale. These helpers perform the same traversals with an
//! explicit heap-allocated work stack — depth is heap-bounded, never
//! stack-bounded — and preserve the recursive visit **order** exactly, so the
//! facts emitted on non-pathological input are byte-for-byte unchanged.

use std::ops::ControlFlow;

/// Function-like node kinds that open a new "enclosing function" scope for
/// boundary-call attribution — the set every detector's `extract_from_node`
/// matched on the way down.
const ENCLOSING_FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "method_definition",
    "arrow_function",
];

/// Visit `root` and all its descendants in pre-order — each node before its
/// children, children left-to-right. `visit` returns [`ControlFlow::Break`] to
/// stop the whole walk early (find-first walks) or [`ControlFlow::Continue`] to
/// keep going (collect-all walks).
///
/// The detector callers of this helper are all order-insensitive (a boolean
/// OR-reduction, or `HashSet` inserts), so traversal order does not affect their
/// output; pre-order is used anyway for a single predictable contract.
pub(crate) fn visit_preorder<'a>(
    root: tree_sitter::Node<'a>,
    mut visit: impl FnMut(tree_sitter::Node<'a>) -> ControlFlow<()>,
) {
    let mut stack: Vec<tree_sitter::Node<'a>> = vec![root];
    while let Some(node) = stack.pop() {
        if visit(node).is_break() {
            return;
        }
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
}

/// Pre-order walk that tracks the enclosing-function name exactly as the
/// detectors' recursive `extract_from_node` did: on entering a function-like
/// node it saves the current name, sets it from the node's `name` field (if
/// present — arrow functions usually have none, so the name is left unchanged),
/// visits the whole subtree, then restores the saved name. Every other node is
/// passed to `on_node` with the current enclosing name; the closure decides
/// whether to emit (typically only for `call_expression` / `new_expression`).
///
/// Emission order is preserved: a node is handed to `on_node` before its
/// children, and children are pushed in reverse so the left-most is visited
/// first — identical to the recursive pre-order emission into the results `Vec`.
pub(crate) fn visit_with_enclosing<'a>(
    root: tree_sitter::Node<'a>,
    src: &[u8],
    mut on_node: impl FnMut(tree_sitter::Node<'a>, &str),
) {
    enum Work<'a> {
        Visit(tree_sitter::Node<'a>),
        Restore(String),
    }

    let mut enclosing = String::new();
    let mut stack: Vec<Work<'a>> = vec![Work::Visit(root)];
    while let Some(work) = stack.pop() {
        let node = match work {
            Work::Restore(prev) => {
                // Post-order: restore the enclosing name on the way back up.
                enclosing = prev;
                continue;
            }
            Work::Visit(n) => n,
        };

        if ENCLOSING_FUNCTION_KINDS.contains(&node.kind()) {
            // Save + set the enclosing name; the Restore marker is pushed BEFORE
            // the children so it pops only after the entire subtree is done.
            let prev = enclosing.clone();
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(src) {
                    enclosing = name.to_string();
                }
            }
            stack.push(Work::Restore(prev));
        } else {
            on_node(node, &enclosing);
        }

        // Both branches recurse into ALL children (the recursive form's body
        // loop / tail loop), pushed reversed to preserve left-to-right order.
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) {
                stack.push(Work::Visit(child));
            }
        }
    }
}
