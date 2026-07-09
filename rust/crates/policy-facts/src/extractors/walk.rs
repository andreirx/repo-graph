//! Iterative tree-walk helpers shared by the policy-fact extractors.
//!
//! PERSIST-RECURSION-1: the extractors formerly descended the tree-sitter AST
//! with plain recursion, so the call-stack depth grew with tree depth and the
//! process aborted (`fatal runtime error: stack overflow`) on deeply nested /
//! generated C at scale. These helpers perform the same traversals with an
//! explicit heap-allocated work stack — depth is heap-bounded, never
//! stack-bounded — and preserve the recursive visit **order** exactly, so the
//! facts emitted on non-pathological input are byte-for-byte unchanged.

use std::ops::ControlFlow;

/// Visit `root` and all its descendants in pre-order — each node before its
/// children, children left-to-right. This is the exact order produced by the
/// natural recursive form:
///
/// ```ignore
/// visit(node);
/// for child in node.children() { recurse(child); }
/// ```
///
/// `visit` returns [`ControlFlow::Break`] to stop the entire walk early (used by
/// find-first walks) or [`ControlFlow::Continue`] to keep going (collect-all
/// walks). `root` itself IS visited first: callers whose recursive form only
/// examined descendants pass a `root` whose kind their closure ignores, so the
/// extra root visit is a proven no-op.
pub(crate) fn visit_preorder<'a>(
    root: tree_sitter::Node<'a>,
    mut visit: impl FnMut(tree_sitter::Node<'a>) -> ControlFlow<()>,
) {
    let mut stack: Vec<tree_sitter::Node<'a>> = vec![root];
    while let Some(node) = stack.pop() {
        if visit(node).is_break() {
            return;
        }
        // Push children in reverse document order so the left-most child is
        // popped (and thus visited) first — preserving pre-order.
        let mut cursor = node.walk();
        let children: Vec<tree_sitter::Node<'a>> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
}

/// Preprocessor-conditional node kinds descended into by `walk_preproc_for_functions`.
const PREPROC_KINDS: &[&str] = &[
    "preproc_ifdef",
    "preproc_if",
    "preproc_else",
    "preproc_elif",
];

/// Invoke `on_function` for every `function_definition` reachable from `root` by
/// descending ONLY through preprocessor conditional blocks — the shape shared by
/// the three per-family `walk_preproc_for_functions` extractors. `root`'s own
/// kind is not matched; only its children (and, transitively, the children of
/// any preprocessor block among them) are.
///
/// Order matches the recursive form: functions are reported in document order,
/// with a preprocessor block's inner functions reported at the block's position.
pub(crate) fn for_each_preproc_function<'a>(
    root: tree_sitter::Node<'a>,
    mut on_function: impl FnMut(tree_sitter::Node<'a>),
) {
    let mut stack: Vec<tree_sitter::Node<'a>> = Vec::new();
    push_preproc_frame_children(root, &mut stack);
    while let Some(node) = stack.pop() {
        match node.kind() {
            "function_definition" => on_function(node),
            k if PREPROC_KINDS.contains(&k) => push_preproc_frame_children(node, &mut stack),
            _ => {}
        }
    }
}

/// Push a frame node's `function_definition` and preprocessor-conditional
/// children (the only kinds the preproc walk acts on or descends into) in
/// reverse document order.
fn push_preproc_frame_children<'a>(
    node: tree_sitter::Node<'a>,
    stack: &mut Vec<tree_sitter::Node<'a>>,
) {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node<'a>> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        let k = child.kind();
        if k == "function_definition" || PREPROC_KINDS.contains(&k) {
            stack.push(child);
        }
    }
}
