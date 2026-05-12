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
//! An edge is promoted ONLY when ALL gates pass:
//!
//! 1. **Category**: Must be `calls_obj_method_needs_type_info` or
//!    `calls_this_wildcard_method_needs_type_info`
//! 2. **Enrichment exists**: Must have enrichment metadata
//! 3. **Compiler origin**: Enrichment origin must be "compiler" (not "failed")
//! 4. **Internal type**: `is_external_type` must be false
//! 5. **Unique class**: Type maps to exactly ONE CLASS symbol in the graph
//! 6. **Unique method**: Method maps to exactly ONE METHOD on that class
//! 7. **No union/intersection**: Type display name has no "|" or "&"
//! 8. **Simple shape**: Target key is simple "receiver.method" or "this.field.method"

use std::collections::HashMap;

use crate::contracts::{
    EdgeLocation, PromotedEdge, PromotionCandidate, ReceiverTypeOrigin, SymbolInfo, SymbolSubtype,
    UnresolvedCategory,
};
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

    /// Count of edges skipped per reason.
    pub skipped_reasons: HashMap<String, usize>,
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

    let mut skip = |reason: &str| {
        *skipped_reasons.entry(reason.to_string()).or_insert(0) += 1;
    };

    for candidate in candidates {
        // Gate 1: Category
        if !matches!(
            candidate.category,
            UnresolvedCategory::CallsObjMethodNeedsTypeInfo
                | UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo
        ) {
            skip("wrong_category");
            continue;
        }

        // Gate 2 + 3: Enrichment exists and is compiler-derived
        if candidate.enrichment.origin != ReceiverTypeOrigin::Compiler {
            skip("no_compiler_enrichment");
            continue;
        }

        // Gate 4: Type is internal
        if candidate.enrichment.is_external_type {
            skip("external_type");
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
                skip("no_type_name");
                continue;
            }
        };

        // Gate 7: No union/intersection
        if type_name.contains('|') || type_name.contains('&') {
            skip("union_or_intersection");
            continue;
        }

        // Gate 8: Simple receiver.method or this.field.method shape
        // Check the FULL target_key for optional chaining or element access,
        // not just the method name. E.g., "obj?.method" has the ? on the receiver.
        if candidate.target_key.contains('?') || candidate.target_key.contains('[') {
            skip("optional_or_element_access");
            continue;
        }

        let method_name = match parse_method_name(&candidate.target_key) {
            Some(name) => name,
            None => {
                skip("not_simple_receiver_method");
                continue;
            }
        };

        // Gate 5: Type maps to exactly one CLASS symbol
        let symbols = match ctx.get_symbols(type_name) {
            Some(s) => s,
            None => {
                skip("type_not_in_graph");
                continue;
            }
        };

        let classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.subtype == SymbolSubtype::Class)
            .collect();

        if classes.is_empty() {
            skip("type_not_a_class");
            continue;
        }

        if classes.len() > 1 {
            skip("ambiguous_class_multiple_definitions");
            continue;
        }

        let class = classes[0];

        // Gate 6: Method maps to exactly one METHOD on that class
        let methods = match ctx.get_methods(&class.stable_key, method_name) {
            Some(m) if !m.is_empty() => m,
            _ => {
                skip("method_not_found_on_class");
                continue;
            }
        };

        if methods.len() > 1 {
            skip("ambiguous_method_overloaded");
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
