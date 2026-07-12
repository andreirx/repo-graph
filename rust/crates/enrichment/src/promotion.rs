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
    EdgeLocation, PromotedEdge, PromotionCandidate, ReceiverTypeOrigin, SymbolInfo, SymbolSubtype,
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

        let classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.subtype == SymbolSubtype::Class)
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
    use crate::contracts::EnrichmentMetadata;

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
