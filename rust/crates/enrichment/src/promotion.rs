//! Safe-subset edge promotion from enriched unresolved to resolved.
//!
//! This module implements the 8-gate safety filter that determines which
//! compiler-enriched unresolved edges can be safely promoted to resolved
//! graph edges.
//!
//! Design principle: this is pure business logic with no I/O. All symbol
//! resolution context is passed in, making the filter fully testable.
//!
//! # The 8 Gates
//!
//! An edge is promoted ONLY when ALL gates pass. The numbering is the authoritative one in
//! `docs/TECH-DEBT.md § 8-Gate Promotion Filter` (gate 2 is the config opt-in placeholder; gate 3 is
//! the compiler-resolved check that subsumes "enrichment exists"):
//!
//! 1. **Category**: Must be `calls_obj_method_needs_type_info` or
//!    `calls_this_wildcard_method_needs_type_info`
//! 2. **Config opt-in**: a no-op placeholder — no config surface gates promotion today, so it never
//!    rejects (counted as a waterfall stage only, for complete per-gate accounting)
//! 3. **Compiler-resolved**: enrichment origin must be "compiler" (not "failed") — this subsumes
//!    "enrichment exists" (a non-compiler origin is the resolution-failed case)
//! 4. **Internal type**: `is_external_type` must be false
//! 5. **Unique class**: Type maps to exactly ONE CLASS symbol in the graph
//! 6. **Unique method**: Method maps to exactly ONE METHOD on that class
//! 7. **No union/intersection**: Type display name has no "|" or "&"
//! 8. **Simple shape**: Target key is simple "receiver.method" or "this.field.method"

use std::collections::{BTreeMap, HashMap};

use crate::contracts::{
    EdgeLocation, PromotedEdge, PromotionCandidate, ReceiverTypeOrigin, SymbolInfo,
    UnresolvedCategory,
};
use crate::funnel::RejectionClass;
use crate::status::PromotionReport;

/// Version identifier for promoted edges.
const PROMOTER_VERSION: &str = "compiler-promotion:0.1.0";

/// Edge type for promoted edges (always CALLS).
const EDGE_TYPE_CALLS: &str = "CALLS";

/// Resolution method for promoted edges.
const RESOLUTION_INFERRED: &str = "inferred";

// ─────────────────────────────────────────────────────────────────────────────
// Promotion Filter
// ─────────────────────────────────────────────────────────────────────────────

/// Result of evaluating a batch of promotion candidates.
#[derive(Debug)]
pub struct PromotionResult {
    /// Edges that passed all gates and are ready for insertion.
    pub promoted: Vec<PromotedEdge>,

    /// Count of edges skipped per reason (first-rejecting gate).
    pub skipped_reasons: HashMap<String, usize>,

    /// ENRICH-YIELD-1: candidates that REACHED each gate, keyed by gate number — ground truth
    /// recorded live as each candidate flows through the filter. Feeds the per-gate waterfall
    /// (`entered`) in [`crate::funnel::PromotionFunnel`]; see [`promote_edges`]. Pure accounting: no
    /// gate predicate reads it, so it cannot change what promotes.
    pub gate_entered: BTreeMap<u8, usize>,
}

impl PromotionResult {
    /// Convert to a report for status tracking.
    ///
    /// `persisted_count` is the actual number of edges persisted to storage,
    /// which may differ from promoted count if storage fails.
    pub fn to_report(
        &self,
        candidate_count: usize,
        persisted_count: Option<usize>,
    ) -> PromotionReport {
        PromotionReport {
            candidates: candidate_count,
            promoted: self.promoted.len(),
            skipped_reasons: self.skipped_reasons.clone(),
            gate_entered: self.gate_entered.clone(),
            persisted_count,
        }
    }
}

/// Symbol resolution context for promotion gate checks.
///
/// Provides:
/// - Lookup of symbols by name
/// - Mapping from class stable key to its methods
pub struct PromotionContext {
    /// Map from type name to matching symbols.
    symbols_by_name: HashMap<String, Vec<SymbolInfo>>,

    /// Map from class stable key to (method name -> all methods with that name).
    /// Vec allows detecting overloaded methods (same name, different signatures).
    class_methods: HashMap<String, HashMap<String, Vec<SymbolInfo>>>,
}

impl PromotionContext {
    /// Create a new promotion context.
    pub fn new() -> Self {
        Self {
            symbols_by_name: HashMap::new(),
            class_methods: HashMap::new(),
        }
    }

    /// Add a symbol to the context.
    pub fn add_symbol(&mut self, symbol: SymbolInfo) {
        // Extract the simple name (last segment after last dot)
        let name = symbol
            .qualified_name
            .as_ref()
            .and_then(|qn| qn.rsplit('.').next())
            .unwrap_or(&symbol.stable_key);

        self.symbols_by_name
            .entry(name.to_string())
            .or_default()
            .push(symbol);
    }

    /// Register methods for a class.
    ///
    /// Multiple methods with the same name (overloads) are tracked separately
    /// to detect ambiguity in Gate 6.
    pub fn add_class_method(
        &mut self,
        class_stable_key: &str,
        method_name: &str,
        method: SymbolInfo,
    ) {
        self.class_methods
            .entry(class_stable_key.to_string())
            .or_default()
            .entry(method_name.to_string())
            .or_default()
            .push(method);
    }

    /// Look up symbols by name.
    pub fn get_symbols(&self, name: &str) -> Option<&Vec<SymbolInfo>> {
        self.symbols_by_name.get(name)
    }

    /// Look up methods by name on a class.
    ///
    /// Returns all methods with the given name (may be multiple for overloads).
    pub fn get_methods(
        &self,
        class_stable_key: &str,
        method_name: &str,
    ) -> Option<&Vec<SymbolInfo>> {
        self.class_methods
            .get(class_stable_key)
            .and_then(|methods| methods.get(method_name))
    }
}

impl Default for PromotionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate a batch of promotion candidates and return promoted edges.
///
/// This is the main entry point for the promotion filter.
pub fn promote_edges(candidates: &[PromotionCandidate], ctx: &PromotionContext) -> PromotionResult {
    let mut promoted = Vec::new();
    let mut skipped_reasons: HashMap<String, usize> = HashMap::new();

    // First-rejection accounting: each candidate is promoted xor skipped exactly once (every skip is
    // followed by `continue`), so the recorded counts conserve — `candidates == promoted + Σ skipped`
    // (ENRICH-YIELD-1). `RejectionClass` is the single source of the reason strings; `reason_code()`
    // preserves the exact `skipped_reasons` keys, so this is byte-identical accounting, no behavior
    // change — it only lets the taxonomy (gate + reader label) stay in lockstep with the filter.
    let mut skip = |class: RejectionClass| {
        *skipped_reasons
            .entry(class.reason_code().to_string())
            .or_insert(0) += 1;
    };

    // Per-gate entry accounting (ENRICH-YIELD-1 §2.1): `enter(N)` is called at the TOP of each gate,
    // so `gate_entered[N]` is the GROUND-TRUTH count of candidates that reached gate N — recorded as
    // it happens, not derived, so the per-gate `entered` waterfall can never silently disagree with
    // the filter's real check order. The gates are visited in EVALUATION order (1, 2, 3, 4, 7, 8, 5,
    // 6): the cheap syntactic gates 7/8 run before the graph-lookup gates 5/6. `enter` is likewise
    // pure accounting — no predicate reads it.
    let mut gate_entered: BTreeMap<u8, usize> = BTreeMap::new();
    let mut enter = |gate: u8| {
        *gate_entered.entry(gate).or_insert(0) += 1;
    };

    for candidate in candidates {
        // Gate 1: Category
        enter(1);
        if !matches!(
            candidate.category,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo
                | UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo
        ) {
            skip(RejectionClass::WrongCategory);
            continue;
        }

        // Gate 2: config opt-in (placeholder). Per `docs/TECH-DEBT.md § 8-Gate Promotion Filter` this
        // gate is a no-op — no config surface gates promotion today, so it NEVER rejects. It is counted
        // as its own waterfall stage purely for complete per-gate (1–8) accounting (ENRICH-YIELD-1
        // §2.1): `enter(2)` records that every gate-1 survivor reached it, and because no
        // `RejectionClass` maps to gate 2 its first-rejection count is always 0. No predicate, no
        // `continue` — additive accounting only; the promotion decision is byte-identical.
        enter(2);

        // Gate 3: the receiver type was resolved by the compiler (origin == Compiler). This single
        // check subsumes "enrichment exists" — a non-compiler origin IS the resolution-failed case.
        enter(3);
        if candidate.enrichment.origin != ReceiverTypeOrigin::Compiler {
            skip(RejectionClass::NoCompilerEnrichment);
            continue;
        }

        // Gate 4: Type is internal (this check + the type-name precondition below are both gate 4).
        // `is_external_type` is set by the language's resolver. For Rust it is true for std/library
        // types AND (ENRICH-YIELD-2 EY1-B) language primitives (`str`, `usize`, …): a primitive is
        // never a repo-defined type, so it can never anchor an in-repo edge. Catching primitives on
        // THIS existing external path — rather than a separate predicate in this language-agnostic
        // filter — keeps the primitive-name set a Rust-language fact owned by the Rust resolver (a
        // TypeScript type named `i32` is not caught), and is promotion-neutral: a primitive would
        // have failed gate 5's graph lookup anyway, so only the attribution moves (gate 5 →
        // gate 4). See `rust-analyzer-resolver::types::is_external_type`.
        enter(4);
        if candidate.enrichment.is_external_type {
            skip(RejectionClass::ExternalType);
            continue;
        }

        // Get the type name
        let type_name = match candidate
            .enrichment
            .type_display_name
            .as_ref()
            .or(candidate.enrichment.receiver_type.as_ref())
        {
            Some(name) => name,
            None => {
                skip(RejectionClass::NoTypeName);
                continue;
            }
        };

        // Gate 7: No union/intersection (evaluated BEFORE gates 5/6 — a cheap string check).
        enter(7);
        if type_name.contains('|') || type_name.contains('&') {
            skip(RejectionClass::UnionOrIntersection);
            continue;
        }

        // Gate 8: Simple receiver.method or this.field.method shape (this check + the method-name
        // parse below are both gate 8). Check the FULL target_key for optional chaining or element
        // access, not just the method name. E.g., "obj?.method" has the ? on the receiver.
        enter(8);
        if candidate.target_key.contains('?') || candidate.target_key.contains('[') {
            skip(RejectionClass::OptionalOrElementAccess);
            continue;
        }

        let method_name = match parse_method_name(&candidate.target_key) {
            Some(name) => name,
            None => {
                skip(RejectionClass::NotSimpleReceiverMethod);
                continue;
            }
        };

        // Gate 5: Type maps to exactly one CLASS symbol (the first graph-lookup gate; these three
        // checks are all gate 5).
        enter(5);
        let symbols = match ctx.get_symbols(type_name) {
            Some(s) => s,
            None => {
                skip(RejectionClass::TypeNotInGraph);
                continue;
            }
        };

        // A usable receiver type is a CLASS *or* an ENUM (EY1-D). A Rust enum is a concrete,
        // single-answer type that owns methods via `impl` blocks exactly like a struct/class, so an
        // unambiguous enum with a valid method is a real Layer-0 promotion. Genuine ambiguity (2+
        // matching types) and non-type symbols still reject below. The predicate is
        // `SymbolSubtype::is_usable_receiver_type` — shared with the pipeline's method loader so the
        // gate and the loader cannot disagree on what a usable type is.
        let classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.subtype.is_usable_receiver_type())
            .collect();

        if classes.is_empty() {
            skip(RejectionClass::TypeNotAClass);
            continue;
        }

        if classes.len() > 1 {
            skip(RejectionClass::AmbiguousClassMultipleDefinitions);
            continue;
        }

        let class = classes[0];

        // Gate 6: Method maps to exactly one METHOD on that class (both checks are gate 6).
        enter(6);
        let methods = match ctx.get_methods(&class.stable_key, method_name) {
            Some(m) if !m.is_empty() => m,
            _ => {
                skip(RejectionClass::MethodNotFoundOnClass);
                continue;
            }
        };

        if methods.len() > 1 {
            skip(RejectionClass::AmbiguousMethodOverloaded);
            continue;
        }

        let method = &methods[0];

        // All 8 gates passed. Promote.
        let promoted_edge = PromotedEdge {
            edge_uid: format!("promoted:{}", candidate.edge_uid),
            snapshot_uid: candidate.snapshot_uid.clone(),
            repo_uid: candidate.repo_uid.clone(),
            source_node_uid: candidate.source_node_uid.clone(),
            target_node_uid: method.node_uid.clone(),
            edge_type: EDGE_TYPE_CALLS,
            resolution: RESOLUTION_INFERRED,
            extractor: PROMOTER_VERSION.to_string(),
            location: build_location(candidate),
            metadata_json: serde_json::json!({
                "promotedFrom": candidate.edge_uid,
                "receiverType": type_name,
                "methodName": method_name,
            })
            .to_string(),
        };

        promoted.push(promoted_edge);
    }

    PromotionResult {
        promoted,
        skipped_reasons,
        gate_entered,
    }
}

/// Parse the method name from a target key.
///
/// Valid shapes:
/// - "receiver.method" -> Some("method")
/// - "this.field.method" -> Some("method")
///
/// Invalid shapes:
/// - "a.b.c.d" (too deep)
/// - "method" (no dot)
fn parse_method_name(target_key: &str) -> Option<&str> {
    let parts: Vec<_> = target_key.split('.').collect();

    match parts.len() {
        // Simple: obj.method
        2 => Some(parts[1]),
        // this.field.method
        3 if parts[0] == "this" => Some(parts[2]),
        // Invalid shape
        _ => None,
    }
}

/// Build edge location from candidate.
fn build_location(candidate: &PromotionCandidate) -> Option<EdgeLocation> {
    candidate.line_start.map(|line_start| EdgeLocation {
        line_start,
        col_start: candidate.col_start.unwrap_or(0),
        line_end: candidate.line_end.unwrap_or(line_start),
        col_end: candidate.col_end.unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{EnrichmentMetadata, SymbolSubtype};
    use std::collections::BTreeSet;

    fn make_candidate(
        edge_uid: &str,
        target_key: &str,
        receiver_type: Option<&str>,
        is_external: bool,
        origin: ReceiverTypeOrigin,
        category: UnresolvedCategory,
    ) -> PromotionCandidate {
        PromotionCandidate {
            edge_uid: edge_uid.to_string(),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            source_node_uid: "source-1".to_string(),
            target_key: target_key.to_string(),
            line_start: Some(10),
            col_start: Some(5),
            line_end: Some(10),
            col_end: Some(20),
            category,
            enrichment: EnrichmentMetadata {
                receiver_type: receiver_type.map(|s| s.to_string()),
                type_display_name: receiver_type.map(|s| s.to_string()),
                is_external_type: is_external,
                origin,
                failure_reason: None,
            },
        }
    }

    fn make_context() -> PromotionContext {
        let mut ctx = PromotionContext::new();

        // Add a class
        let class = SymbolInfo {
            node_uid: "class-1".to_string(),
            stable_key: "MyClass".to_string(),
            qualified_name: Some("MyClass".to_string()),
            subtype: SymbolSubtype::Class,
        };
        ctx.add_symbol(class);

        // Add methods for the class
        let method = SymbolInfo {
            node_uid: "method-1".to_string(),
            stable_key: "MyClass.doSomething".to_string(),
            qualified_name: Some("MyClass.doSomething".to_string()),
            subtype: SymbolSubtype::Method,
        };
        ctx.add_class_method("MyClass", "doSomething", method);

        ctx
    }

    #[test]
    fn test_successful_promotion() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "obj.doSomething",
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert_eq!(result.promoted.len(), 1);
        assert!(result.skipped_reasons.is_empty());

        let promoted = &result.promoted[0];
        assert_eq!(promoted.edge_uid, "promoted:edge-1");
        assert_eq!(promoted.target_node_uid, "method-1");
        assert_eq!(promoted.edge_type, "CALLS");
        assert_eq!(promoted.resolution, "inferred");
    }

    #[test]
    fn test_gate_1_wrong_category() {
        let ctx = make_context();
        // Wrong category - using a valid category string but simulating wrong one
        let candidate = make_candidate(
            "edge-1",
            "obj.doSomething",
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );
        // With a valid category, should pass Gate 1
        // (In real code, this gate checks for specific categories)

        let result = promote_edges(&[candidate], &ctx);
        // Should pass since we used valid category
        assert_eq!(result.promoted.len(), 1);
    }

    #[test]
    fn test_gate_3_failed_enrichment() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "obj.doSomething",
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Failed, // Failed origin
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert!(result.promoted.is_empty());
        assert_eq!(
            result.skipped_reasons.get("no_compiler_enrichment"),
            Some(&1)
        );
    }

    #[test]
    fn test_gate_4_external_type() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "obj.doSomething",
            Some("MyClass"),
            true, // External type
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert!(result.promoted.is_empty());
        assert_eq!(result.skipped_reasons.get("external_type"), Some(&1));
    }

    #[test]
    fn test_gate_5_type_not_in_graph() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "obj.doSomething",
            Some("UnknownClass"), // Not in context
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert!(result.promoted.is_empty());
        assert_eq!(result.skipped_reasons.get("type_not_in_graph"), Some(&1));
    }

    #[test]
    fn test_gate_6_method_not_found() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "obj.unknownMethod", // Method doesn't exist
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert!(result.promoted.is_empty());
        assert_eq!(
            result.skipped_reasons.get("method_not_found_on_class"),
            Some(&1)
        );
    }

    #[test]
    fn test_gate_7_union_type() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "obj.doSomething",
            Some("MyClass | OtherClass"), // Union type
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert!(result.promoted.is_empty());
        assert_eq!(
            result.skipped_reasons.get("union_or_intersection"),
            Some(&1)
        );
    }

    #[test]
    fn test_gate_8_deep_chain() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "a.b.c.d", // Too deep
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert!(result.promoted.is_empty());
        assert_eq!(
            result.skipped_reasons.get("not_simple_receiver_method"),
            Some(&1)
        );
    }

    #[test]
    fn test_this_field_method_pattern() {
        let ctx = make_context();
        let candidate = make_candidate(
            "edge-1",
            "this.field.doSomething", // this.field.method is valid
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        assert_eq!(result.promoted.len(), 1);
    }

    #[test]
    fn test_parse_method_name() {
        assert_eq!(parse_method_name("obj.method"), Some("method"));
        assert_eq!(parse_method_name("this.field.method"), Some("method"));
        assert_eq!(parse_method_name("a.b.c.d"), None);
        assert_eq!(parse_method_name("method"), None);
        assert_eq!(parse_method_name(""), None);
    }

    #[test]
    fn test_multiple_candidates_mixed() {
        let ctx = make_context();
        let candidates = vec![
            // Should pass
            make_candidate(
                "edge-1",
                "obj.doSomething",
                Some("MyClass"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // Should fail - external type
            make_candidate(
                "edge-2",
                "obj.doSomething",
                Some("MyClass"),
                true,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // Should fail - method not found
            make_candidate(
                "edge-3",
                "obj.unknownMethod",
                Some("MyClass"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
        ];

        let result = promote_edges(&candidates, &ctx);

        assert_eq!(result.promoted.len(), 1);
        assert_eq!(result.skipped_reasons.get("external_type"), Some(&1));
        assert_eq!(
            result.skipped_reasons.get("method_not_found_on_class"),
            Some(&1)
        );
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // ENRICH-YIELD-1: funnel accounting over the real filter
    // ─────────────────────────────────────────────────────────────────────────────

    // Conservation: `candidates == promoted + Σ rejected`, computed over a real promote_edges run
    // spanning several gates. The invariant the whole funnel surface leans on.
    #[test]
    fn funnel_conserves_over_a_real_mixed_run() {
        let ctx = make_context();
        let candidates = vec![
            // promotes (all gates pass)
            make_candidate(
                "e1",
                "obj.doSomething",
                Some("MyClass"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 4: external
            make_candidate(
                "e2",
                "obj.doSomething",
                Some("MyClass"),
                true,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 3: resolution failed
            make_candidate(
                "e3",
                "obj.doSomething",
                Some("MyClass"),
                false,
                ReceiverTypeOrigin::Failed,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 6: method not found
            make_candidate(
                "e4",
                "obj.nope",
                Some("MyClass"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 7: union
            make_candidate(
                "e5",
                "obj.doSomething",
                Some("A | B"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
        ];
        let n = candidates.len();
        let funnel = promote_edges(&candidates, &ctx).to_report(n, None).funnel();

        assert_eq!(funnel.candidates, 5);
        assert_eq!(funnel.promoted, 1);
        assert_eq!(funnel.rejected, 4);
        assert!(
            funnel.conserves(),
            "1 promoted + 4 rejected == 5 candidates: {funnel:?}"
        );
        // Four distinct first-rejection classes, one candidate each.
        assert_eq!(funnel.rejections.len(), 4);
        for r in &funnel.rejections {
            assert_eq!(
                r.count, 1,
                "each gate rejected exactly one candidate: {r:?}"
            );
        }
    }

    // First-rejection attribution: a candidate that fails MULTIPLE gates is attributed to the FIRST
    // one only (the loop `continue`s), never double-counted. Here e2 is BOTH external (gate 4) AND
    // its method does not exist (gate 6) — it must count once, under gate 4.
    #[test]
    fn funnel_attributes_to_the_first_failing_gate_only() {
        let ctx = make_context();
        let candidate = make_candidate(
            "e-multi",
            "obj.methodThatDoesNotExist", // would also fail gate 6
            Some("MyClass"),
            true, // fails gate 4 FIRST
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );
        let funnel = promote_edges(&[candidate], &ctx)
            .to_report(1, None)
            .funnel();

        assert!(funnel.conserves());
        assert_eq!(
            funnel.rejections.len(),
            1,
            "attributed to ONE gate, not two"
        );
        assert_eq!(
            funnel.rejections[0].reason, "external_type",
            "the FIRST failing gate (4)"
        );
        assert_eq!(funnel.rejections[0].gate, 4);
        // The later-gate reason never appears.
        assert!(
            !funnel
                .rejections
                .iter()
                .any(|r| r.reason == "method_not_found_on_class"),
            "a later gate must not be counted once an earlier one fired: {funnel:?}"
        );
    }

    // Ground-truth per-gate waterfall over the REAL filter (ENRICH-YIELD-1 §2.1): candidates entering
    // AND first-rejected for each gate, plus promoted, all conserving. `entered` is what the pass
    // actually counted, so if the eval-order table in `funnel.rs` ever drifts from THIS function's
    // check order, `conserves()` fails here.
    #[test]
    fn funnel_gate_waterfall_counts_entered_and_rejected_per_gate() {
        let ctx = make_context(); // MyClass + MyClass.doSomething
        let candidates = vec![
            // promotes
            make_candidate(
                "e1",
                "obj.doSomething",
                Some("MyClass"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 4: external
            make_candidate(
                "e2",
                "obj.doSomething",
                Some("MyClass"),
                true,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 7: union
            make_candidate(
                "e3",
                "obj.doSomething",
                Some("A | B"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 5: type not in graph
            make_candidate(
                "e4",
                "obj.doSomething",
                Some("UnknownClass"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
            // gate 6: method not found on MyClass
            make_candidate(
                "e5",
                "obj.nope",
                Some("MyClass"),
                false,
                ReceiverTypeOrigin::Compiler,
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            ),
        ];
        let funnel = promote_edges(&candidates, &ctx)
            .to_report(candidates.len(), None)
            .funnel();

        assert!(
            funnel.conserves(),
            "the whole waterfall conserves: {funnel:?}"
        );
        let gate = |n: u8| funnel.gates.iter().find(|g| g.gate == n).unwrap();
        // Evaluation-order waterfall: 5 → (−1 external) 4 → (−1 union) 3 → 3 → (−1 not-in-graph) 2 →
        // (−1 method) 1 promoted.
        assert_eq!((gate(1).entered, gate(1).rejected), (5, 0));
        // Gate 2 (config opt-in placeholder) is a no-op stage: every gate-1 survivor reaches it and it
        // rejects nothing, so its entrants mirror gate 3's. Present for complete per-gate accounting.
        assert_eq!((gate(2).entered, gate(2).rejected), (5, 0));
        assert_eq!(gate(2).entered, gate(3).entered, "gate 2 is a pass-through");
        assert_eq!((gate(3).entered, gate(3).rejected), (5, 0));
        assert_eq!((gate(4).entered, gate(4).rejected), (5, 1)); // e2 external
        assert_eq!((gate(7).entered, gate(7).rejected), (4, 1)); // e3 union
        assert_eq!((gate(8).entered, gate(8).rejected), (3, 0));
        assert_eq!((gate(5).entered, gate(5).rejected), (3, 1)); // e4 not in graph
        assert_eq!((gate(6).entered, gate(6).rejected), (2, 1)); // e5 method not found
        assert_eq!(funnel.promoted, 1);
    }

    // Evaluation order is NOT gate-number order: a candidate that is BOTH a union type (gate 7) AND
    // has a class not in the graph (gate 5) is attributed to gate 7 — because the filter evaluates the
    // cheap syntactic gate 7 BEFORE the graph-lookup gate 5. So gate 5 never even sees it (its
    // `entered` excludes it). If the funnel used numeric order this would be misattributed to gate 5.
    #[test]
    fn gate_entered_follows_evaluation_order_not_gate_number() {
        let ctx = make_context();
        // "Nope | Other": a union (gate 7) whose members are also not classes in the graph (gate 5).
        let candidate = make_candidate(
            "e-eval",
            "obj.doSomething",
            Some("Nope | Other"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );
        let funnel = promote_edges(&[candidate], &ctx)
            .to_report(1, None)
            .funnel();

        assert!(funnel.conserves());
        let gate = |n: u8| funnel.gates.iter().find(|g| g.gate == n).unwrap();
        assert_eq!(
            gate(7).rejected,
            1,
            "attributed to gate 7 (union), evaluated first"
        );
        assert_eq!(gate(7).entered, 1);
        assert_eq!(gate(5).entered, 0, "gate 5 never reached — 7 fired first");
        assert_eq!(gate(5).rejected, 0);
        // And the reader-frame reason is the union one, not the not-in-graph one.
        assert_eq!(funnel.rejections.len(), 1);
        assert_eq!(funnel.rejections[0].reason, "union_or_intersection");
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // ENRICH-YIELD-2 EY1-B: primitive receiver reattribution (gate 4, promotion-neutral)
    // ─────────────────────────────────────────────────────────────────────────────

    // A primitive receiver (`str`, `usize`, …) lands at gate 4 (external path), NOT gate 5's
    // `type_not_in_graph`. The Rust resolver classifies primitives as external
    // (`rust-analyzer-resolver::types::is_external_type` — proven by its own
    // `primitives_classify_as_external`), so the candidate arrives with `is_external_type=true` and
    // is rejected by the EXISTING gate-4 external check. This is the reattribution lever: a primitive
    // is never a repo type, so "we looked for it in the repo and didn't find it" (gate 5) would
    // mislead. It rides the external path (no separate disposition, no primitive predicate in this
    // language-agnostic filter) — the ratified corrected EY1-B cell.
    #[test]
    fn primitive_receiver_classified_external_lands_at_gate_4() {
        let ctx = make_context();
        let candidate = make_candidate(
            "e-prim",
            "s.len",
            Some("str"),
            true, // the Rust resolver marks primitives external (is_external_type("str") == true)
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );
        let funnel = promote_edges(&[candidate], &ctx)
            .to_report(1, None)
            .funnel();

        assert!(funnel.conserves());
        assert_eq!(funnel.promoted, 0);
        assert_eq!(funnel.rejections.len(), 1);
        assert_eq!(funnel.rejections[0].reason, "external_type");
        assert_eq!(
            funnel.rejections[0].gate, 4,
            "primitive is attributed to gate 4 (external path), not gate 5"
        );
        // The label honestly names both cases — a primitive is not a library type.
        assert_eq!(
            funnel.rejections[0].label,
            "receiver type is external to this repo (a std/library type or language primitive)"
        );
        // Gate 4 counts it; the graph-lookup gate 5 never even sees it.
        let gate = |n: u8| funnel.gates.iter().find(|g| g.gate == n).unwrap();
        assert_eq!(gate(4).rejected, 1);
        assert_eq!(
            gate(5).entered,
            0,
            "a primitive never reaches the graph-lookup gate"
        );
    }

    // EY1-B promotion-neutrality — the RATIFIED proof (OPERATOR_NOTE 2026-07-12, EY2-B-PROOF), now
    // over a REAL CAPTURED SELF-INDEX CORPUS (review-2). The stop condition "promoted set BEFORE ==
    // AFTER" binds EY1-B *in isolation*; it CANNOT be shown by comparing two live enrichment runs
    // (rust-analyzer is nondeterministic, and EY1-D adds enum promotions by design). So we CAPTURE ONE
    // real candidate corpus + its symbol/method context from an isolated `rmap index` + live
    // rust-analyzer enrichment of this workspace (`testdata/ey1b_selfindex_corpus.json` — see its
    // `_provenance`; the read-only `testdata/capture_ey1b_corpus.py` reproduces
    // `storage::enrichment_impl`'s `load_promotion_candidates` + `load_symbols_by_names` +
    // `load_class_methods`), and replay THAT identical corpus through the two classifications EY1-B
    // moves between:
    //   - post-EY1-B: `is_external_type` = the REAL persisted value (STD_TYPES ∪ PRIMITIVES);
    //   - pre-EY1-B:  `is_external_type` = post AND NOT `receiver_is_primitive` (STD-only), because
    //     EY1-B's ONLY change to `is_external_type` was `|| PRIMITIVES.contains(name)`.
    // The corpus (821 candidates, 54 primitive; 146 symbols; 922 methods) is byte-identical between the
    // two passes; only `is_external_type` on the primitive receivers differs. We assert:
    //   (a) the promoted edge set is EXACTLY equal pre vs post (THE stop condition, in isolation), and
    //   (b) the ONLY candidates whose disposition changes are the primitives — each moving to gate 4
    //       (`external_type`) post-EY1-B FROM a pre-EY1-B NON-promotion rejection; every non-primitive
    //       is byte-identical.
    // HONEST NUANCE the real corpus reveals (the old synthetic replay hid it): a primitive's pre-EY1-B
    // rejection gate is `type_not_in_graph@gate5` for a simple `recv.method` (49/54 in this corpus),
    // but a DEEPER-chain primitive (e.g. `file_path.rsplit('/').next().unwrap_or`) is rejected at gate 8
    // (`not_simple_receiver_method`, 5/54) — because gate 4 is evaluated BEFORE gate 8, so post-EY1-B
    // catches it at gate 4 first. EITHER WAY the primitive NEVER promotes in either pass, so the
    // promoted set is unchanged (neutrality holds); EY1-B only moves the ATTRIBUTION of an
    // already-failing primitive earlier, to the honest external gate. This REPLACES the prior
    // hand-built `primitives_are_promotion_neutral_deterministic_replay`.
    #[test]
    fn primitives_are_promotion_neutral_over_real_self_index_corpus() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            candidates: Vec<FxCandidate>,
            symbols: Vec<FxSymbol>,
            class_methods: Vec<FxMethod>,
        }
        #[derive(serde::Deserialize)]
        struct FxCandidate {
            edge_uid: String,
            target_key: String,
            receiver_type: Option<String>,
            type_display_name: Option<String>,
            category: String,
            /// The REAL persisted `$.enrichment.isExternalType` (post-EY1-B = STD_TYPES ∪ PRIMITIVES).
            is_external_post: bool,
            /// Membership in the resolver's PRIMITIVES set (the receivers EY1-B moves).
            receiver_is_primitive: bool,
        }
        #[derive(serde::Deserialize)]
        struct FxSymbol {
            node_uid: String,
            stable_key: String,
            qualified_name: Option<String>,
            subtype: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct FxMethod {
            class_stable_key: String,
            method_name: String,
            method: FxSymbol,
        }

        let raw = include_str!("testdata/ey1b_selfindex_corpus.json");
        let fx: Fixture = serde_json::from_str(raw).expect("fixture parses");

        // Rebuild the promotion context exactly as `EnrichmentPipeline::run_promotion` does: add every
        // captured symbol; add every captured (usable-type) method. The subtype defaults mirror the
        // storage adapter (symbols default `Other`, methods default `Method`).
        let mut ctx = PromotionContext::new();
        for s in &fx.symbols {
            ctx.add_symbol(SymbolInfo {
                node_uid: s.node_uid.clone(),
                stable_key: s.stable_key.clone(),
                qualified_name: s.qualified_name.clone(),
                subtype: s
                    .subtype
                    .as_deref()
                    .map(SymbolSubtype::parse)
                    .unwrap_or(SymbolSubtype::Other),
            });
        }
        for m in &fx.class_methods {
            ctx.add_class_method(
                &m.class_stable_key,
                &m.method_name,
                SymbolInfo {
                    node_uid: m.method.node_uid.clone(),
                    stable_key: m.method.stable_key.clone(),
                    qualified_name: m.method.qualified_name.clone(),
                    subtype: m
                        .method
                        .subtype
                        .as_deref()
                        .map(SymbolSubtype::parse)
                        .unwrap_or(SymbolSubtype::Method),
                },
            );
        }

        // Classify the identical corpus two ways. `is_external_pre = is_external_post && !primitive`
        // is the pre-EY1-B (STD-only) classification; `is_external_post` is the real persisted one.
        let build = |c: &FxCandidate, is_external: bool| PromotionCandidate {
            edge_uid: c.edge_uid.clone(),
            snapshot_uid: "fixture".to_string(),
            repo_uid: "fixture".to_string(),
            source_node_uid: "fixture".to_string(),
            target_key: c.target_key.clone(),
            line_start: None,
            col_start: None,
            line_end: None,
            col_end: None,
            category: UnresolvedCategory::parse(&c.category)
                .expect("fixture holds only accepted categories"),
            enrichment: EnrichmentMetadata {
                receiver_type: c.receiver_type.clone(),
                type_display_name: c.type_display_name.clone(),
                is_external_type: is_external,
                origin: ReceiverTypeOrigin::Compiler,
                failure_reason: None,
            },
        };
        let pre: Vec<_> = fx
            .candidates
            .iter()
            .map(|c| build(c, c.is_external_post && !c.receiver_is_primitive))
            .collect();
        let post: Vec<_> = fx
            .candidates
            .iter()
            .map(|c| build(c, c.is_external_post))
            .collect();

        // (a) The promoted set is EXACTLY equal pre vs post — THE stop condition, over the real corpus.
        let promoted_set = |cands: &[PromotionCandidate]| -> BTreeSet<String> {
            promote_edges(cands, &ctx)
                .promoted
                .into_iter()
                .map(|e| e.edge_uid)
                .collect()
        };
        let pre_set = promoted_set(&pre);
        let post_set = promoted_set(&post);
        assert_eq!(
            pre_set, post_set,
            "EY1-B is promotion-neutral on the real self-index corpus: identical corpus, identical \
             promoted set pre vs post"
        );
        // Non-vacuous AND fidelity-pinned: replaying the captured corpus reproduces the live
        // `rmap enrich` funnel's promoted count (47 — see this fixture's `_provenance` capture run and
        // the build report). This cross-checks that the captured symbol/method context is COMPLETE,
        // not merely real: an under-captured context would promote fewer than the live run did. (If the
        // fixture is regenerated from a different self-index run, update this count.)
        assert_eq!(
            post_set.len(),
            47,
            "post-EY1-B replay reproduces the live self-index funnel's 47 promotions"
        );

        // Per-candidate disposition over the real filter: "PROMOTED" or "<reason>@gate<N>". Each
        // candidate is evaluated independently by `promote_edges` (no cross-candidate state).
        fn disposition(cand: &PromotionCandidate, ctx: &PromotionContext) -> String {
            let f = promote_edges(std::slice::from_ref(cand), ctx)
                .to_report(1, None)
                .funnel();
            if f.promoted == 1 {
                "PROMOTED".to_string()
            } else {
                let r = &f.rejections[0];
                format!("{}@gate{}", r.reason, r.gate)
            }
        }

        // (b) The ONLY candidates that move are the primitives; each moves to gate 4 (external) FROM a
        // pre-EY1-B non-promotion rejection. Every non-primitive is byte-identical.
        let mut moved = 0usize;
        for (c, (cand_pre, cand_post)) in fx.candidates.iter().zip(pre.iter().zip(post.iter())) {
            let d_pre = disposition(cand_pre, &ctx);
            let d_post = disposition(cand_post, &ctx);
            if c.receiver_is_primitive {
                moved += 1;
                assert_ne!(
                    d_pre, d_post,
                    "a primitive's attribution moves under EY1-B: {} ({})",
                    c.edge_uid, c.target_key
                );
                assert_eq!(
                    d_post, "external_type@gate4",
                    "post-EY1-B a primitive is caught at gate 4 (external): {} ({})",
                    c.edge_uid, c.target_key
                );
                assert_ne!(
                    d_pre, "PROMOTED",
                    "a primitive NEVER promoted pre-EY1-B, so moving it is promotion-neutral: {} ({})",
                    c.edge_uid, c.target_key
                );
                // Pre-EY1-B a primitive is an already-failing rejection at gate 5 (type_not_in_graph,
                // simple call) OR gate 8 (deeper chain) — never gate 6, since no repo symbol is named
                // after a primitive (verified by promoted-set equality above).
                assert!(
                    d_pre == "type_not_in_graph@gate5"
                        || d_pre == "not_simple_receiver_method@gate8"
                        || d_pre == "optional_or_element_access@gate8",
                    "pre-EY1-B a primitive is an already-failing rejection at gate 5 or gate 8, not \
                     {d_pre}: {} ({})",
                    c.edge_uid,
                    c.target_key
                );
            } else {
                assert_eq!(
                    d_pre, d_post,
                    "a non-primitive candidate's disposition is untouched by EY1-B: {} ({})",
                    c.edge_uid, c.target_key
                );
            }
        }
        // Non-vacuous: the corpus really contains the primitive receivers EY1-B reclassifies (54 —
        // matches this fixture's `_provenance.counts.primitive_candidates`; the receivers observed were
        // `bool`, `char`, `i32`, `str`, `u64`). Update if the fixture is regenerated.
        assert_eq!(
            moved, 54,
            "the captured corpus contains the 54 primitive receivers EY1-B reclassifies"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // ENRICH-YIELD-2 EY1-D: Rust enum receiver promotion (gate 5 widened to Class|Enum)
    // ─────────────────────────────────────────────────────────────────────────────

    // An unambiguous Rust enum with a valid method PROMOTES: gate 5 accepts Class|Enum. Before EY1-D
    // the enum's `"ENUM"` subtype collapsed to `Other` and gate 5 rejected it as `type_not_a_class`
    // despite an enum being a concrete, single-answer type that owns methods via `impl` blocks.
    #[test]
    fn enum_receiver_with_valid_method_promotes() {
        let mut ctx = PromotionContext::new();
        ctx.add_symbol(SymbolInfo {
            node_uid: "enum-1".to_string(),
            stable_key: "Status".to_string(),
            qualified_name: Some("Status".to_string()),
            subtype: SymbolSubtype::Enum,
        });
        ctx.add_class_method(
            "Status",
            "is_active",
            SymbolInfo {
                node_uid: "m-1".to_string(),
                stable_key: "Status.is_active".to_string(),
                qualified_name: Some("Status.is_active".to_string()),
                subtype: SymbolSubtype::Method,
            },
        );

        let candidate = make_candidate(
            "e-enum",
            "status.is_active",
            Some("Status"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );
        let result = promote_edges(&[candidate], &ctx);

        assert_eq!(
            result.promoted.len(),
            1,
            "an unambiguous enum method call promotes; skipped: {:?}",
            result.skipped_reasons
        );
        assert_eq!(result.promoted[0].target_node_uid, "m-1");
    }

    // EY1-D does NOT relax genuine ambiguity or admit non-types: a name resolving to BOTH an enum and
    // a class (2 usable types) is still `ambiguous_class_multiple_definitions`, and a symbol that is
    // neither class nor enum (e.g. a type alias → `Other`) is still `type_not_a_class`. Only
    // unambiguous concrete types promote — the honesty boundary the ratification kept.
    #[test]
    fn enum_widening_still_rejects_ambiguous_and_non_usable() {
        // (a) ambiguous: an enum AND a class share the name "Dup" → 2 usable types.
        let mut ctx = PromotionContext::new();
        ctx.add_symbol(SymbolInfo {
            node_uid: "d-enum".to_string(),
            stable_key: "Dup".to_string(),
            qualified_name: Some("Dup".to_string()),
            subtype: SymbolSubtype::Enum,
        });
        ctx.add_symbol(SymbolInfo {
            node_uid: "d-class".to_string(),
            stable_key: "Dup".to_string(),
            qualified_name: Some("Dup".to_string()),
            subtype: SymbolSubtype::Class,
        });
        let amb = make_candidate(
            "e-amb",
            "d.go",
            Some("Dup"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );
        let r = promote_edges(&[amb], &ctx);
        assert!(r.promoted.is_empty());
        assert_eq!(
            r.skipped_reasons
                .get("ambiguous_class_multiple_definitions"),
            Some(&1),
            "enum+class homonym stays ambiguous"
        );

        // (b) non-usable: "Alias" resolves only to an `Other`-subtype symbol (e.g. a type alias).
        let mut ctx2 = PromotionContext::new();
        ctx2.add_symbol(SymbolInfo {
            node_uid: "a-1".to_string(),
            stable_key: "Alias".to_string(),
            qualified_name: Some("Alias".to_string()),
            subtype: SymbolSubtype::Other,
        });
        let non = make_candidate(
            "e-non",
            "a.go",
            Some("Alias"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );
        let r2 = promote_edges(&[non], &ctx2);
        assert!(r2.promoted.is_empty());
        assert_eq!(
            r2.skipped_reasons.get("type_not_a_class"),
            Some(&1),
            "a non-class, non-enum type is still rejected"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Regression tests for fixed bugs
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_gate_6_overloaded_method_ambiguity() {
        // BUG FIX: Gate 6 should reject when there are multiple methods with the same name
        // (method overloads). Previously, duplicate methods were silently overwritten.
        let mut ctx = PromotionContext::new();

        let class = SymbolInfo {
            node_uid: "class-1".to_string(),
            stable_key: "OverloadedClass".to_string(),
            qualified_name: Some("OverloadedClass".to_string()),
            subtype: SymbolSubtype::Class,
        };
        ctx.add_symbol(class);

        // Add two methods with the SAME name (simulating overloads)
        ctx.add_class_method(
            "OverloadedClass",
            "process",
            SymbolInfo {
                node_uid: "method-1".to_string(),
                stable_key: "OverloadedClass.process#1".to_string(),
                qualified_name: Some("OverloadedClass.process".to_string()),
                subtype: SymbolSubtype::Method,
            },
        );
        ctx.add_class_method(
            "OverloadedClass",
            "process",
            SymbolInfo {
                node_uid: "method-2".to_string(),
                stable_key: "OverloadedClass.process#2".to_string(),
                qualified_name: Some("OverloadedClass.process".to_string()),
                subtype: SymbolSubtype::Method,
            },
        );

        let candidate = make_candidate(
            "edge-1",
            "obj.process",
            Some("OverloadedClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        // Should REJECT due to ambiguous method (overloaded)
        assert!(result.promoted.is_empty());
        assert_eq!(
            result.skipped_reasons.get("ambiguous_method_overloaded"),
            Some(&1)
        );
    }

    #[test]
    fn test_gate_8_optional_chaining_on_receiver() {
        // BUG FIX: Gate 8 should reject optional chaining anywhere in the target key,
        // not just in the method name. "obj?.method" has the ? on the receiver side.
        let ctx = make_context();

        let candidate = make_candidate(
            "edge-1",
            "obj?.doSomething", // Optional chaining on receiver
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        // Should REJECT due to optional chaining
        assert!(result.promoted.is_empty());
        assert_eq!(
            result.skipped_reasons.get("optional_or_element_access"),
            Some(&1)
        );
    }

    #[test]
    fn test_gate_8_element_access_on_receiver() {
        // BUG FIX: Gate 8 should reject element access anywhere in the target key.
        let ctx = make_context();

        let candidate = make_candidate(
            "edge-1",
            "obj[0].doSomething", // Element access on receiver
            Some("MyClass"),
            false,
            ReceiverTypeOrigin::Compiler,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
        );

        let result = promote_edges(&[candidate], &ctx);

        // Should REJECT due to element access
        assert!(result.promoted.is_empty());
        assert_eq!(
            result.skipped_reasons.get("optional_or_element_access"),
            Some(&1)
        );
    }
}
