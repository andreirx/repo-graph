//! Receiver expression locator using tree-sitter (Rust grammar).
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** a deterministic, tree-sitter-based syntax localizer that, for a Rust method-call
//!   edge whose receiver is a compound field access (`self.field.method()`), returns the source
//!   position whose rust-analyzer *hover* yields the INTERMEDIATE receiver's type (`self.field`'s
//!   type) — so the promotion filter anchors the call on the FIELD's type, not `self`'s.
//! - **Concrete current user:** [`crate::client`]'s `resolve_compound_receiver` — the
//!   compound-receiver branch of `RustAnalyzerResolver::resolve_batch` (routed by receiver SHAPE,
//!   since Rust `self.field.method` carries the OBJ category, not the wildcard one). One caller today.
//! - **Named axis of variation:** *syntax localization* (deterministic, tree-sitter grammar) vs
//!   *semantic typing* (rust-analyzer LSP). These vary independently — grammar changes vs LSP
//!   behavior changes — and this is the SAME seam split the TypeScript resolver already ships
//!   (`tsserver-resolver::receiver_locator`). ENRICH-YIELD-3 mirrors that ratified seam for Rust.
//! - **Rejected simpler alternative:** hover the stored call-expression start directly (what the
//!   resolver did for all categories). Rejected: `col_start` is the call-expression start, which for
//!   `self.field.method()` is the `self` token — hovering there resolves `self`'s type, minting a
//!   FALSE Layer-0 CALLS edge (`Self::method` instead of `Field::method`). This is the exact EY1-C
//!   hazard. A tree-sitter locator is the minimum needed to point the hover at the field.
//!
//! # The problem (EY1-C)
//!
//! The Rust extractor stamps a call edge's location with the `call_expression`'s start position
//! (`rust-extractor::extractor::location_from_node`). For `self.field.method()` that start is the
//! `self` token. rust-analyzer hover at `self` returns `self`'s type — the enclosing impl's `Self`,
//! NOT `self.field`'s type. Promoting on that type is a false edge.
//!
//! # The fix — query the receiver's TYPE-BEARING token, not its start
//!
//! To get the type of the receiver expression `self.field`, hover at its trailing field identifier
//! (`field`) — hovering a field access's field returns the field's type (this is how the existing
//! simple path already works: hover `obj` → `obj`'s type). Returning the receiver's START would land
//! on `self` again — the very bug. So for a compound receiver this locator returns the position of
//! the receiver's own `field` child; for a simple receiver (`self`, an identifier) it returns the
//! receiver's start (whose own hover is already the receiver type).
//!
//! **This is the one deliberate divergence from the TS locator**, which returns the receiver's start
//! for every case. For `this.field` that is the `this` token, so the TS path resolves `this`'s type;
//! it is "safe" (no false edge) only because it then mostly fails to match a real type — not because
//! it resolves the field. The Rust locator resolves the field on purpose, which is what converts the
//! ~140 `self.field.method` rejections into REAL promotions. tsserver-resolver is out of scope here
//! (reference only); this divergence is flagged for review, not propagated.
//!
//! # Coordinate contract
//!
//! Input `line`/`column` and the returned `line`/`column` are **1-based line, 0-based column** —
//! identical to [`enrichment::EligibleEdge`] (`line_start` 1-based, `col_start` 0-based) and to what
//! `RustAnalyzerSession::resolve_type` expects. So the caller passes edge coordinates straight in and
//! the result straight to `resolve_type`, with no off-by-one adjustment. (tree-sitter's own `Point`
//! is 0-based row/column; the conversion is confined to this module.) This convention deliberately
//! differs from the TS locator's 0-based-line API so the wiring is correct-by-construction for the
//! Rust resolver's 1-based-line pipeline.
//!
//! # Maturity
//!
//! PROTOTYPE. Covers the `self.field.method` / `obj.field.method` shape and simple receivers;
//! everything else fails closed (`NoReceiver` / `UnsupportedPattern`), so an unhandled shape never
//! silently promotes.

use tree_sitter::{Language, Parser, Point, Tree};

// ─────────────────────────────────────────────────────────────────────────────
// Public Types
// ─────────────────────────────────────────────────────────────────────────────

/// Result of receiver localization.
///
/// Coordinates are **1-based line, 0-based column** (see the module coordinate contract), matching
/// `EligibleEdge` and `RustAnalyzerSession::resolve_type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverLocation {
    /// Located the receiver expression; hover at `(line, column)` yields the receiver's type.
    Found {
        /// 1-based line of the type-bearing token.
        line: u32,
        /// 0-based column of the type-bearing token.
        column: u32,
        /// The full receiver expression text (for diagnostics only — e.g. `"self.field"`). The
        /// returned position points at the receiver's type-bearing token (the trailing field for a
        /// compound receiver), which may be inside this span.
        text: String,
    },
    /// No receiver expression (e.g., a plain `foo()` or path `Foo::bar()` call).
    NoReceiver,
    /// Receiver pattern not supported (complex expression) — fails closed, never promotes.
    UnsupportedPattern {
        /// Machine reason (surfaced as `receiver_locator_unsupported:<reason>`).
        reason: String,
    },
    /// Parse error or the position did not resolve to a call expression.
    ParseError {
        /// Machine reason (surfaced as `receiver_locator_parse_error:<reason>`).
        reason: String,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Locator
// ─────────────────────────────────────────────────────────────────────────────

/// Locates receiver expressions in Rust source.
pub struct ReceiverLocator {
    parser: Parser,
}

impl ReceiverLocator {
    /// Create a new locator for Rust files.
    pub fn new() -> Result<Self, String> {
        let mut parser = Parser::new();
        let language: Language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("failed to set Rust language: {}", e))?;
        Ok(Self { parser })
    }

    /// Locate the receiver expression for a call at the given position.
    ///
    /// `line` is **1-based**, `column` is **0-based** (matching `EligibleEdge`). The position may be
    /// anywhere inside the call expression (the extractor stamps its start); the locator walks up to
    /// the enclosing `call_expression` regardless.
    ///
    /// Returns the type-bearing position (**1-based line, 0-based column**).
    pub fn locate_receiver(&mut self, source: &str, line: u32, column: u32) -> ReceiverLocation {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => {
                return ReceiverLocation::ParseError {
                    reason: "tree_sitter_parse_returned_none".to_string(),
                }
            }
        };

        // 1-based line → 0-based tree-sitter row; column is already 0-based in both.
        let point = Point::new(line.saturating_sub(1) as usize, column as usize);
        locate_receiver_at_point(&tree, source, point)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Core Algorithm
// ─────────────────────────────────────────────────────────────────────────────

fn locate_receiver_at_point(tree: &Tree, source: &str, point: Point) -> ReceiverLocation {
    let root = tree.root_node();

    // Deepest node at the position (the extractor stamps the call-expression start = `self`).
    let node = match root.descendant_for_point_range(point, point) {
        Some(n) => n,
        None => {
            return ReceiverLocation::ParseError {
                reason: "no_node_at_position".to_string(),
            }
        }
    };

    // Walk up to the enclosing call_expression.
    let mut current = node;
    let call_expr = loop {
        if current.kind() == "call_expression" {
            break current;
        }
        match current.parent() {
            Some(p) => current = p,
            None => {
                return ReceiverLocation::ParseError {
                    reason: format!("no_call_expression_above:{}", node.kind()),
                }
            }
        }
    };

    // The callee: for a method call `a.b()` this is a `field_expression` (value=`a`, field=`b`).
    let function = match call_expr.child_by_field_name("function") {
        Some(f) => f,
        None => {
            return ReceiverLocation::ParseError {
                reason: "call_expression_has_no_function".to_string(),
            }
        }
    };

    extract_receiver_from_function(function, source)
}

fn extract_receiver_from_function(function: tree_sitter::Node, source: &str) -> ReceiverLocation {
    match function.kind() {
        // Method call: `self.field.method`, `obj.method`, `self.method`.
        "field_expression" => extract_receiver_from_field_expression(function, source),

        // Plain function call `foo()` or associated/path call `Foo::bar()` — no instance receiver.
        "identifier" | "scoped_identifier" => ReceiverLocation::NoReceiver,

        // Anything else (a parenthesized/try/await/etc. callee) — fail closed.
        other => ReceiverLocation::UnsupportedPattern {
            reason: format!("callee:{}", other),
        },
    }
}

/// The callee is `<receiver>.<method>`. Return the position whose hover yields `<receiver>`'s type.
fn extract_receiver_from_field_expression(
    field_expr: tree_sitter::Node,
    source: &str,
) -> ReceiverLocation {
    // `value` is the receiver of the `.method()` call.
    let receiver = match field_expr.child_by_field_name("value") {
        Some(v) => v,
        None => {
            return ReceiverLocation::ParseError {
                reason: "field_expression_has_no_value".to_string(),
            }
        }
    };

    match receiver.kind() {
        // COMPOUND receiver — `self.field` (or `obj.field`). To get the receiver EXPRESSION's type we
        // must hover its trailing field identifier: hovering its START would land on `self`/`obj` and
        // resolve THAT type — the EY1-C false edge. So return the receiver's own `field` child.
        "field_expression" => {
            let field = match receiver.child_by_field_name("field") {
                Some(f) => f,
                None => {
                    return ReceiverLocation::ParseError {
                        reason: "receiver_field_expression_has_no_field".to_string(),
                    }
                }
            };
            let start = field.start_position();
            let text = source
                .get(receiver.byte_range())
                .unwrap_or("<receiver>")
                .to_string();
            ReceiverLocation::Found {
                line: start.row as u32 + 1,
                column: start.column as u32,
                text,
            }
        }

        // SIMPLE receiver — `self`, a local/`obj`, or a path. Its own hover already IS the receiver
        // type, so return its start position.
        "self" | "identifier" | "scoped_identifier" => {
            let start = receiver.start_position();
            let text = source
                .get(receiver.byte_range())
                .unwrap_or("<receiver>")
                .to_string();
            ReceiverLocation::Found {
                line: start.row as u32 + 1,
                column: start.column as u32,
                text,
            }
        }

        // Method-call receiver `f().m()`, index `a[0].m()`, reference/paren/try/etc. — fail closed:
        // never guess a position, so an unhandled receiver shape can never mint a false edge.
        other => ReceiverLocation::UnsupportedPattern {
            reason: format!("receiver:{}", other),
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn locate(source: &str, line: u32, column: u32) -> ReceiverLocation {
        let mut locator = ReceiverLocator::new().expect("Rust grammar loads");
        locator.locate_receiver(source, line, column)
    }

    /// Return the 0-based column of the first occurrence of `needle` in `source` (single-line
    /// helper), for asserting a located position lands on the intended token.
    fn col_of(source: &str, needle: &str) -> u32 {
        source.find(needle).expect("needle present") as u32
    }

    // ── THE false-edge-prevention test (EY1-C) ───────────────────────────────────────────────────
    //
    // For `self.field.method()` the located position MUST be the `field` token, NOT the `self` token.
    // Hover at `field` yields `self.field`'s type; hover at `self` yields the enclosing `Self` — the
    // false edge. The extractor stamps the edge at the call-expression START (`self`), so we feed the
    // `self` column and prove the locator moves the query onto `field`.
    #[test]
    fn self_field_method_locates_the_field_not_self() {
        let source = "        self.field.method();";
        let self_col = col_of(source, "self");
        let field_col = col_of(source, "field");
        assert_ne!(self_col, field_col, "self and field are distinct columns");

        // Feed the call-expression start (the `self` token), as the extractor stamps it.
        let result = locate(source, 1, self_col);

        match result {
            ReceiverLocation::Found { line, column, text } => {
                assert_eq!(text, "self.field", "receiver expression text");
                assert_eq!(line, 1, "same line (1-based)");
                assert_eq!(
                    column, field_col,
                    "located position is the FIELD token (hover→field's type), NOT `self` \
                     (col {self_col}) — the EY1-C false-edge guard"
                );
            }
            other => panic!("expected Found at the field token, got {:?}", other),
        }
    }

    // Multi-line variant — the extractor's real coordinates are 1-based line / 0-based col.
    #[test]
    fn self_field_method_multiline_locates_the_field() {
        let source = "\
struct Outer;
impl Outer {
    fn caller(&self) {
        self.inner.run();
    }
}
";
        // Line 4 (1-based), `self` is the call-expression start.
        let self_col = col_of("        self.inner.run();", "self");
        let inner_col = col_of("        self.inner.run();", "inner");
        let result = locate(source, 4, self_col);
        match result {
            ReceiverLocation::Found { line, column, text } => {
                assert_eq!(text, "self.inner");
                assert_eq!(line, 4);
                assert_eq!(column, inner_col, "located the `inner` field, not `self`");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    // `obj.field.method()` — same mechanism with a non-self receiver.
    #[test]
    fn obj_field_method_locates_the_field() {
        let source = "    obj.field.run();";
        let obj_col = col_of(source, "obj");
        let field_col = col_of(source, "field");
        match locate(source, 1, obj_col) {
            ReceiverLocation::Found { column, text, .. } => {
                assert_eq!(text, "obj.field");
                assert_eq!(column, field_col, "located `field`, not `obj`");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    // Simple `self.method()` — the receiver is `self`; hover at `self` IS the receiver type, so the
    // located position is `self`'s start.
    #[test]
    fn self_method_locates_self() {
        let source = "        self.run();";
        let self_col = col_of(source, "self");
        match locate(source, 1, self_col) {
            ReceiverLocation::Found { column, text, .. } => {
                assert_eq!(text, "self");
                assert_eq!(column, self_col, "simple receiver → its own position");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    // Simple `obj.method()` — receiver is a local identifier.
    #[test]
    fn obj_method_locates_obj() {
        let source = "    obj.run();";
        let obj_col = col_of(source, "obj");
        match locate(source, 1, obj_col) {
            ReceiverLocation::Found { column, text, .. } => {
                assert_eq!(text, "obj");
                assert_eq!(column, obj_col);
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    // A deeper chain `self.a.b.method()`: the receiver is `self.a.b`; the locator still points at the
    // receiver's trailing field (`b`). Depth is NOT rejected here — gate 8 (`parse_method_name`) owns
    // that decision (single source of truth); a deep chain that resolves is still rejected at gate 8,
    // so no false edge results. This test pins the locator's behavior, not the promotion outcome.
    #[test]
    fn deep_chain_locates_trailing_field_gate8_rejects_later() {
        let source = "        self.a.b.run();";
        let last_b = source.rfind(".b.").map(|i| i + 1).expect("`.b.` present") as u32;
        match locate(source, 1, col_of(source, "self")) {
            ReceiverLocation::Found { column, text, .. } => {
                assert_eq!(text, "self.a.b");
                assert_eq!(
                    column, last_b,
                    "trailing field `b` of the receiver `self.a.b`"
                );
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    // Plain function call `foo()` — no receiver.
    #[test]
    fn plain_call_has_no_receiver() {
        assert!(matches!(
            locate("    foo();", 1, col_of("    foo();", "foo")),
            ReceiverLocation::NoReceiver
        ));
    }

    // Path/associated call `Foo::bar()` — no instance receiver.
    #[test]
    fn path_call_has_no_receiver() {
        let source = "    Foo::bar();";
        assert!(matches!(
            locate(source, 1, col_of(source, "Foo")),
            ReceiverLocation::NoReceiver
        ));
    }

    // An INDEX receiver `arr[0].run()` fails closed (UnsupportedPattern), never a guessed position —
    // so an unhandled receiver shape cannot mint a false edge.
    #[test]
    fn index_receiver_is_unsupported_fail_closed() {
        let source = "    arr[0].run();";
        match locate(source, 1, col_of(source, "arr")) {
            ReceiverLocation::UnsupportedPattern { reason } => {
                assert!(reason.starts_with("receiver:"), "reason: {reason}");
            }
            other => panic!("expected UnsupportedPattern, got {:?}", other),
        }
    }

    // A NESTED-CALL receiver `getter().run()`: the stored position (call-expression start = `getter`)
    // coincides with the INNER call `getter()`'s start, so walking up stops at the inner call, whose
    // callee is a plain identifier → `NoReceiver`. This is fail-closed (a failed resolve → no
    // promotion), documenting the nested-call short-circuit. The promotion filter independently
    // rejects nested-call target keys (parens in a chain segment) at gate 8, so even if a nested call
    // DID resolve a type, it could not promote.
    #[test]
    fn nested_call_receiver_fails_closed() {
        let source = "    getter().run();";
        assert!(
            matches!(
                locate(source, 1, col_of(source, "getter")),
                ReceiverLocation::NoReceiver
            ),
            "nested-call receiver short-circuits to the inner call → NoReceiver (fail-closed)"
        );
    }

    // A position not inside any call expression → ParseError (never a fabricated Found).
    #[test]
    fn position_not_in_a_call_is_parse_error() {
        assert!(matches!(
            locate("    let x = 1;", 1, 4),
            ReceiverLocation::ParseError { .. }
        ));
    }
}
