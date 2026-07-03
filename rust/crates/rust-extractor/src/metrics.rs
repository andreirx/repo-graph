//! Cyclomatic-complexity metrics for Rust functions and methods.
//!
//! METRIC-LANG-COVERAGE-1 (part B): Rust joins C and TypeScript in the measured
//! set. This module mirrors `c-extractor/src/metrics.rs` — same decision-point
//! counting semantics so cross-language complexity rankings are comparable — and
//! maps each C/TS decision point onto its Rust tree-sitter equivalent.
//!
//! ## Counting rules (comparable to C/TS)
//!
//! | Construct                         | tree-sitter kind          | Increment |
//! |-----------------------------------|---------------------------|-----------|
//! | Base                              | —                         | +1        |
//! | `if` / `if let` / `else if`       | `if_expression`           | +1        |
//! | `while` / `while let`             | `while_expression`        | +1        |
//! | `loop`                            | `loop_expression`         | +1        |
//! | `for`                             | `for_expression`          | +1        |
//! | `match` arm (non-wildcard)        | `match_arm`               | +1        |
//! | `match` bare `_` arm (no guard)   | `match_arm`               | +0        |
//! | `&&`, `\|\|`                      | `binary_expression`       | +1        |
//! | `?` (try operator)                | `try_expression`          | +1        |
//!
//! The base semantic is identical to C/TS: **each branch/decision point adds 1**.
//! The Rust-specific constructs (`match` arms, `?`, `if let`) are the ones named
//! by the METRIC-LANG-COVERAGE-1 contract §2.B. The `match`↔`switch` mapping
//! keeps values comparable: a bare `_` arm mirrors C's `default:` (+0), a
//! non-wildcard arm mirrors a `case X:` (+1). A `match` on N enum variants (no
//! `_`) scores +N, exactly like a C `switch` with N `case`s.
//!
//! ## Deliberate, bounded divergences (documented per contract §2.B)
//!
//! - **Or-patterns** (`1 | 2 | 3 => ...`) count as ONE arm (+1), not three; the
//!   equivalent C `case 1: case 2: case 3:` would score +3. Or-patterns are
//!   infrequent relative to total arms, so this does not systematically distort
//!   rankings (the STOP condition's bar). Precise or-pattern expansion is
//!   deferred, not a comparability break.
//! - **Guards** (`n if n > 0 => ...`) are not separately incremented; the arm's
//!   own +1 plus any `&&`/`||` inside the guard already account for the branch.
//! - **`else if` nesting depth** is not flattened (matches c-extractor, which
//!   also does not special-case it; tree-sitter-c and -rust nest else-if
//!   identically, so C and Rust `max_nesting_depth` stay comparable).
//!
//! ## Scope
//!
//! The walk descends into closures (`closure_expression`) — closures are not
//! extracted as their own symbols, so their decision points belong to the
//! enclosing function. It stops at nested `function_item`s, which ARE their own
//! symbols and carry their own metrics. This is the same scope boundary the
//! call-attribution walk uses (see `extract_calls_from_node`), so a function's
//! complexity scope and its call scope coincide.
//!
//! Per c-extractor, `function_length` and `cognitive_complexity` are left `None`
//! for Rust (cognitive complexity is explicitly out of scope for this slice).

use repo_graph_indexer::types::ExtractedMetrics;
use tree_sitter::Node;

/// Compute cyclomatic complexity, parameter count, and max nesting depth for a
/// Rust function/method.
///
/// `body` is the `block` node (the `body` field of a `function_item`).
/// `params` is the `parameters` node (optional), used for parameter counting.
pub fn compute_function_metrics(body: &Node, params: Option<&Node>) -> ExtractedMetrics {
    let mut complexity: u32 = 1; // base complexity
    let mut max_depth: u32 = 0;
    let mut current_depth: u32 = 0;

    walk(body, &mut complexity, &mut max_depth, &mut current_depth);

    ExtractedMetrics {
        cyclomatic_complexity: complexity,
        parameter_count: count_parameters(params),
        max_nesting_depth: max_depth,
        // Mirror c-extractor: these two kinds are not computed for Rust in this
        // slice. `None` = "not measured", never a false `0`.
        function_length: None,
        cognitive_complexity: None,
    }
}

fn walk(node: &Node, complexity: &mut u32, max_depth: &mut u32, current_depth: &mut u32) {
    // `dominated` == "this node opens a nested control-flow scope" (drives
    // max_nesting_depth). Mirrors c-extractor's `dominated` flag: only the loop
    // and `if` families nest; `match` (like C's `switch`) does not.
    let dominated = match node.kind() {
        "if_expression" | "while_expression" | "loop_expression" | "for_expression" => {
            *complexity += 1;
            true
        }
        "match_arm" => {
            if !is_default_arm(node) {
                *complexity += 1;
            }
            false
        }
        "try_expression" => {
            *complexity += 1;
            false
        }
        "binary_expression" => {
            if let Some(op) = node.child_by_field_name("operator") {
                if matches!(op.kind(), "&&" | "||") {
                    *complexity += 1;
                }
            }
            false
        }
        _ => false,
    };

    if dominated {
        *current_depth += 1;
        if *current_depth > *max_depth {
            *max_depth = *current_depth;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // A nested named function is its own symbol with its own metrics; do not
        // fold its complexity into the enclosing function. Closures and every
        // other node ARE folded in (see module docs).
        if child.kind() != "function_item" {
            walk(&child, complexity, max_depth, current_depth);
        }
    }

    if dominated {
        *current_depth -= 1;
    }
}

/// A `match` arm is the analog of a C `default:` (contributes +0) when its
/// pattern is a bare wildcard `_` with no guard. Every other arm is a `case X:`
/// (+1). In tree-sitter-rust 0.23 the wildcard `_` is an ANONYMOUS node (kind
/// `"_"`, `named: false`), so it must be read via `child(0)`, not
/// `named_child(0)` (which would skip it). A top-level `_` only — an inner `_`
/// like `Some(_)` sits under a `tuple_struct_pattern` and stays a real case.
/// The guard, when present, is the `condition` field of the `match_pattern`.
fn is_default_arm(match_arm: &Node) -> bool {
    let Some(pattern) = match_arm.child_by_field_name("pattern") else {
        return false;
    };
    // A guard makes the arm conditional -> it is a real decision point (+1).
    if pattern.child_by_field_name("condition").is_some() {
        return false;
    }
    pattern.child(0).map(|p| p.kind() == "_").unwrap_or(false)
}

/// Count `parameter` and `self_parameter` children of a `parameters` node.
///
/// `self` is counted: at a call site it is a real argument (`x.f(y)` passes `x`
/// as the receiver), mirroring how C counts an explicit `self`-pointer first
/// parameter in the OO-in-C idiom.
fn count_parameters(params: Option<&Node>) -> u32 {
    let Some(params) = params else {
        return 0;
    };
    let mut count = 0u32;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if matches!(child.kind(), "parameter" | "self_parameter") {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    /// Parse `source` and return the tree plus the byte offset machinery needed
    /// to locate the first `function_item`'s body and parameters.
    fn metrics_for_first_fn(source: &str) -> ExtractedMetrics {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let func = find_first(&root, "function_item").expect("no function_item in source");
        let body = func.child_by_field_name("body").expect("fn has no body");
        let params = func.child_by_field_name("parameters");
        compute_function_metrics(&body, params.as_ref())
    }

    fn find_first<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(*node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_first(&child, kind) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn empty_function_has_base_complexity() {
        let m = metrics_for_first_fn("fn foo() {}");
        assert_eq!(m.cyclomatic_complexity, 1);
        assert_eq!(m.max_nesting_depth, 0);
        assert_eq!(m.parameter_count, 0);
        // Mirror c-extractor: Rust does not emit these two kinds this slice.
        assert_eq!(m.function_length, None);
        assert_eq!(m.cognitive_complexity, None);
    }

    #[test]
    fn if_adds_complexity_and_depth() {
        let m = metrics_for_first_fn("fn foo(x: bool) { if x { bar(); } }");
        assert_eq!(m.cyclomatic_complexity, 2); // base + if
        assert_eq!(m.max_nesting_depth, 1);
    }

    #[test]
    fn if_let_counts_as_if() {
        let m = metrics_for_first_fn("fn foo(o: Option<i32>) { if let Some(n) = o { bar(n); } }");
        assert_eq!(m.cyclomatic_complexity, 2); // base + if let
    }

    #[test]
    fn loop_for_while_each_add_one() {
        let m =
            metrics_for_first_fn("fn foo() { loop { break; } for _ in 0..3 {} while cond() {} }");
        // base + loop + for + while = 4
        assert_eq!(m.cyclomatic_complexity, 4);
    }

    #[test]
    fn logical_operators_add_complexity() {
        let m = metrics_for_first_fn("fn foo(a: bool, b: bool, c: bool) { if a && b || c {} }");
        // base(1) + if(1) + &&(1) + ||(1) = 4
        assert_eq!(m.cyclomatic_complexity, 4);
    }

    #[test]
    fn try_operator_adds_complexity() {
        let m = metrics_for_first_fn(
            "fn foo() -> Result<(), E> { let x = bar()?; let y = baz()?; Ok(()) }",
        );
        // base(1) + ?(1) + ?(1) = 3
        assert_eq!(m.cyclomatic_complexity, 3);
    }

    // ── the contract-required `match` proof (hand-computed) ──────────

    #[test]
    fn match_arms_count_like_cases_wildcard_is_default() {
        // match with 2 real arms + a bare `_` (the C `default:`):
        //   base(1) + Some(1)(1) + Some(2)(1) + _(0) = 3
        let m = metrics_for_first_fn(
            "fn foo(o: Option<i32>) -> i32 { match o { Some(1) => 10, Some(_) => 20, _ => 0 } }",
        );
        assert_eq!(m.cyclomatic_complexity, 3);
    }

    #[test]
    fn exhaustive_enum_match_no_wildcard() {
        // 3 variant arms, no `_`: base(1) + 3 = 4 (mirrors a C switch with 3 cases).
        let m = metrics_for_first_fn(
            "fn foo(c: Color) -> u8 { match c { Color::Red => 1, Color::Green => 2, Color::Blue => 3 } }",
        );
        assert_eq!(m.cyclomatic_complexity, 4);
    }

    #[test]
    fn guarded_arm_is_a_decision_point() {
        // A guard makes the arm conditional -> it counts (+1), even though the
        // fall-through is a bare `_`:
        //   base(1) + (n if n > 5)(1) + _(0) = 2
        let m =
            metrics_for_first_fn("fn foo(x: i32) -> i32 { match x { n if n > 5 => n, _ => 0 } }");
        assert_eq!(m.cyclomatic_complexity, 2);
    }

    #[test]
    fn nested_control_flow_depth() {
        let m = metrics_for_first_fn("fn foo(a: bool, b: bool) { if a { if b { bar(); } } }");
        assert_eq!(m.cyclomatic_complexity, 3); // base + if + if
        assert_eq!(m.max_nesting_depth, 2);
    }

    #[test]
    fn nested_function_not_folded_in() {
        // Inner fn's `if` must NOT contribute to the outer fn's complexity;
        // the inner fn is its own symbol with its own metrics.
        let m = metrics_for_first_fn("fn outer() { fn inner(x: bool) { if x {} } }");
        assert_eq!(m.cyclomatic_complexity, 1);
        assert_eq!(m.max_nesting_depth, 0);
    }

    #[test]
    fn closure_body_is_folded_in() {
        // Closures are NOT their own symbols, so their decision points belong to
        // the enclosing function.
        let m = metrics_for_first_fn("fn foo() { let f = |x: bool| { if x { bar(); } }; }");
        assert_eq!(m.cyclomatic_complexity, 2); // base + if inside closure
    }

    #[test]
    fn parameter_count_includes_self() {
        let m = metrics_for_first_fn("fn method(&self, a: i32, b: i32) {}");
        assert_eq!(m.parameter_count, 3); // self + a + b
    }

    #[test]
    fn parameter_count_free_function() {
        let m = metrics_for_first_fn("fn foo(a: i32, b: &str, c: bool) {}");
        assert_eq!(m.parameter_count, 3);
    }
}
