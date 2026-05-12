//! Core contracts for the enrichment subsystem.
//!
//! These DTOs define the data shapes that flow through enrichment:
//! - Eligible edges (input)
//! - Enrichment results (output)
//! - Promotion candidates
//! - Promoted edges
//!
//! Design principle: these contracts are pure data, no behavior.
//! Business logic lives in promotion.rs and resolver.rs.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Eligible Edge (input to enrichment)
// ─────────────────────────────────────────────────────────────────────────────

/// An unresolved edge eligible for enrichment.
///
/// Represents a `obj.method()` or `this.field.method()` call where the
/// syntax-only extractor could not determine the receiver type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EligibleEdge {
    /// Unique identifier of the unresolved edge row.
    pub edge_uid: String,

    /// Snapshot this edge belongs to.
    pub snapshot_uid: String,

    /// Repository this edge belongs to.
    pub repo_uid: String,

    /// Source node (the caller).
    pub source_node_uid: String,

    /// The unresolved target key (e.g., "receiver.method").
    pub target_key: String,

    /// Source file path (repo-relative).
    pub source_file_path: String,

    /// Line number where the call occurs (1-based).
    pub line_start: u32,

    /// Column number where the call occurs (0-based).
    pub col_start: u32,

    /// Classification category of the unresolved edge.
    pub category: UnresolvedCategory,

    /// Language of the source file.
    pub language: EnrichmentLanguage,
}

/// Classification categories for unresolved edges that are enrichment-eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedCategory {
    /// `obj.method()` where obj's type is unknown.
    CallsObjMethodNeedsTypeInfo,

    /// `this.field.method()` where the intermediate receiver type is unknown.
    CallsThisWildcardMethodNeedsTypeInfo,
}

impl UnresolvedCategory {
    /// Database string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CallsObjMethodNeedsTypeInfo => "calls_obj_method_needs_type_info",
            Self::CallsThisWildcardMethodNeedsTypeInfo => {
                "calls_this_wildcard_method_needs_type_info"
            }
        }
    }

    /// Parse from database string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "calls_obj_method_needs_type_info" => Some(Self::CallsObjMethodNeedsTypeInfo),
            "calls_this_wildcard_method_needs_type_info" => {
                Some(Self::CallsThisWildcardMethodNeedsTypeInfo)
            }
            _ => None,
        }
    }
}

/// Languages supported by enrichment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnrichmentLanguage {
    TypeScript,
    Rust,
    Java,
}

impl EnrichmentLanguage {
    /// Infer language from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs" => Some(Self::TypeScript),
            "rs" => Some(Self::Rust),
            "java" => Some(Self::Java),
            _ => None,
        }
    }

    /// Infer language from file path.
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = path.rsplit('.').next()?;
        Self::from_extension(ext)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Enrichment Result (output from resolver)
// ─────────────────────────────────────────────────────────────────────────────

/// Result of attempting to resolve the receiver type for one edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiverTypeResult {
    /// The edge UID this result corresponds to.
    pub edge_uid: String,

    /// The resolved receiver type name, if successful.
    pub receiver_type: Option<String>,

    /// How the receiver type was determined.
    pub origin: ReceiverTypeOrigin,

    /// Display name of the type (may differ from receiver_type for generics).
    pub type_display_name: Option<String>,

    /// Whether the type appears to be from an external package/crate.
    pub is_external_type: bool,

    /// Reason for failure, if origin is Failed.
    pub failure_reason: Option<String>,
}

impl ReceiverTypeResult {
    /// Create a successful result.
    pub fn success(
        edge_uid: String,
        receiver_type: String,
        type_display_name: Option<String>,
        is_external_type: bool,
    ) -> Self {
        Self {
            edge_uid,
            receiver_type: Some(receiver_type),
            origin: ReceiverTypeOrigin::Compiler,
            type_display_name,
            is_external_type,
            failure_reason: None,
        }
    }

    /// Create a failed result.
    pub fn failed(edge_uid: String, reason: impl Into<String>) -> Self {
        Self {
            edge_uid,
            receiver_type: None,
            origin: ReceiverTypeOrigin::Failed,
            type_display_name: None,
            is_external_type: false,
            failure_reason: Some(reason.into()),
        }
    }

    /// Whether this result successfully resolved a type.
    pub fn is_success(&self) -> bool {
        self.receiver_type.is_some() && self.origin == ReceiverTypeOrigin::Compiler
    }
}

/// How the receiver type was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiverTypeOrigin {
    /// Successfully resolved via compiler/LSP.
    Compiler,
    /// Resolution attempted but failed.
    Failed,
}

// ─────────────────────────────────────────────────────────────────────────────
// Enrichment Metadata (persisted on unresolved edge)
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata persisted on the unresolved edge after enrichment.
///
/// This is stored in the `metadata_json` column, merged with existing metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentMetadata {
    /// The resolved receiver type, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver_type: Option<String>,

    /// Display name of the type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_display_name: Option<String>,

    /// Whether the type is external.
    pub is_external_type: bool,

    /// How the type was determined.
    pub origin: ReceiverTypeOrigin,

    /// Failure reason if resolution failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

impl From<&ReceiverTypeResult> for EnrichmentMetadata {
    fn from(r: &ReceiverTypeResult) -> Self {
        Self {
            receiver_type: r.receiver_type.clone(),
            type_display_name: r.type_display_name.clone(),
            is_external_type: r.is_external_type,
            origin: r.origin,
            failure_reason: r.failure_reason.clone(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Promotion Candidate (input to promotion filter)
// ─────────────────────────────────────────────────────────────────────────────

/// An enriched edge that may be eligible for promotion to resolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionCandidate {
    /// The unresolved edge UID.
    pub edge_uid: String,

    /// Snapshot UID.
    pub snapshot_uid: String,

    /// Repository UID.
    pub repo_uid: String,

    /// Source node (caller) UID.
    pub source_node_uid: String,

    /// The unresolved target key.
    pub target_key: String,

    /// Source location line.
    pub line_start: Option<u32>,

    /// Source location column.
    pub col_start: Option<u32>,

    /// Source location end line.
    pub line_end: Option<u32>,

    /// Source location end column.
    pub col_end: Option<u32>,

    /// Unresolved category.
    pub category: UnresolvedCategory,

    /// Enrichment metadata.
    pub enrichment: EnrichmentMetadata,
}

// ─────────────────────────────────────────────────────────────────────────────
// Promoted Edge (output from promotion filter)
// ─────────────────────────────────────────────────────────────────────────────

/// A promoted edge ready for insertion into the resolved edges table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotedEdge {
    /// New edge UID (prefixed with "promoted:").
    pub edge_uid: String,

    /// Snapshot UID.
    pub snapshot_uid: String,

    /// Repository UID.
    pub repo_uid: String,

    /// Source node (caller) UID.
    pub source_node_uid: String,

    /// Target node (resolved callee) UID.
    pub target_node_uid: String,

    /// Edge type (always CALLS for promoted edges).
    pub edge_type: &'static str,

    /// Resolution method.
    pub resolution: &'static str,

    /// Extractor identification.
    pub extractor: String,

    /// Source location.
    pub location: Option<EdgeLocation>,

    /// Metadata JSON.
    pub metadata_json: String,
}

/// Source location for an edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeLocation {
    pub line_start: u32,
    pub col_start: u32,
    pub line_end: u32,
    pub col_end: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Symbol Resolution Context (for promotion gates)
// ─────────────────────────────────────────────────────────────────────────────

/// Information about a symbol in the graph, used for promotion gate checks.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Unique node UID.
    pub node_uid: String,

    /// Stable key for the symbol.
    pub stable_key: String,

    /// Qualified name.
    pub qualified_name: Option<String>,

    /// Symbol subtype (CLASS, METHOD, FUNCTION, etc.).
    pub subtype: SymbolSubtype,
}

/// Symbol subtypes relevant for promotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolSubtype {
    Class,
    Method,
    Getter,
    Setter,
    Function,
    Other,
}

impl SymbolSubtype {
    pub fn parse(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "CLASS" => Self::Class,
            "METHOD" => Self::Method,
            "GETTER" => Self::Getter,
            "SETTER" => Self::Setter,
            "FUNCTION" => Self::Function,
            _ => Self::Other,
        }
    }

    pub fn is_method_like(&self) -> bool {
        matches!(self, Self::Method | Self::Getter | Self::Setter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension() {
        assert_eq!(
            EnrichmentLanguage::from_extension("ts"),
            Some(EnrichmentLanguage::TypeScript)
        );
        assert_eq!(
            EnrichmentLanguage::from_extension("tsx"),
            Some(EnrichmentLanguage::TypeScript)
        );
        assert_eq!(
            EnrichmentLanguage::from_extension("rs"),
            Some(EnrichmentLanguage::Rust)
        );
        assert_eq!(
            EnrichmentLanguage::from_extension("java"),
            Some(EnrichmentLanguage::Java)
        );
        assert_eq!(EnrichmentLanguage::from_extension("py"), None);
    }

    #[test]
    fn test_language_from_path() {
        assert_eq!(
            EnrichmentLanguage::from_path("src/main.ts"),
            Some(EnrichmentLanguage::TypeScript)
        );
        assert_eq!(
            EnrichmentLanguage::from_path("crates/core/src/lib.rs"),
            Some(EnrichmentLanguage::Rust)
        );
        assert_eq!(
            EnrichmentLanguage::from_path("src/Main.java"),
            Some(EnrichmentLanguage::Java)
        );
    }

    #[test]
    fn test_unresolved_category_roundtrip() {
        let cat = UnresolvedCategory::CallsObjMethodNeedsTypeInfo;
        assert_eq!(UnresolvedCategory::parse(cat.as_str()), Some(cat));

        let cat2 = UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo;
        assert_eq!(UnresolvedCategory::parse(cat2.as_str()), Some(cat2));
    }

    #[test]
    fn test_receiver_type_result_success() {
        let result = ReceiverTypeResult::success(
            "edge-1".to_string(),
            "MyClass".to_string(),
            Some("MyClass".to_string()),
            false,
        );
        assert!(result.is_success());
        assert_eq!(result.receiver_type, Some("MyClass".to_string()));
        assert_eq!(result.origin, ReceiverTypeOrigin::Compiler);
    }

    #[test]
    fn test_receiver_type_result_failed() {
        let result = ReceiverTypeResult::failed("edge-2".to_string(), "type is any");
        assert!(!result.is_success());
        assert_eq!(result.receiver_type, None);
        assert_eq!(result.origin, ReceiverTypeOrigin::Failed);
        assert_eq!(result.failure_reason, Some("type is any".to_string()));
    }
}
