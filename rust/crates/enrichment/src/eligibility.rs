//! Eligibility query contracts for enrichment.
//!
//! This module defines the storage port interface for:
//! - Querying unresolved edges that are eligible for enrichment
//! - Persisting enrichment results back to storage
//! - Loading promotion candidates
//! - Inserting promoted edges
//!
//! Design principle: this is a PORT definition. Adapters implement the
//! actual database queries. The enrichment crate does not know about SQLite.

use crate::contracts::{
    EligibleEdge, EnrichmentLanguage, EnrichmentMetadata, PromotedEdge, PromotionCandidate,
    SymbolInfo, UnresolvedCategory,
};

// ─────────────────────────────────────────────────────────────────────────────
// Query Criteria
// ─────────────────────────────────────────────────────────────────────────────

/// Criteria for querying eligible edges.
#[derive(Debug, Clone, Default)]
pub struct EligibilityQuery {
    /// Snapshot to query.
    pub snapshot_uid: String,

    /// Filter by categories (if empty, all eligible categories).
    pub categories: Vec<UnresolvedCategory>,

    /// Filter by languages (if empty, all languages).
    pub languages: Vec<EnrichmentLanguage>,

    /// Maximum number of edges to return.
    pub limit: Option<usize>,

    /// Skip edges that already have enrichment metadata.
    pub exclude_already_enriched: bool,
}

impl EligibilityQuery {
    pub fn new(snapshot_uid: impl Into<String>) -> Self {
        Self {
            snapshot_uid: snapshot_uid.into(),
            categories: vec![
                UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
                UnresolvedCategory::CallsThisWildcardMethodNeedsTypeInfo,
            ],
            languages: Vec::new(),
            limit: None,
            exclude_already_enriched: true,
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_languages(mut self, languages: Vec<EnrichmentLanguage>) -> Self {
        self.languages = languages;
        self
    }

    pub fn include_already_enriched(mut self) -> Self {
        self.exclude_already_enriched = false;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage Port
// ─────────────────────────────────────────────────────────────────────────────

/// Storage port for enrichment operations.
///
/// Implementations provide the actual database access. The enrichment
/// pipeline interacts with storage only through this interface.
pub trait EnrichmentStoragePort {
    /// Query eligible edges for enrichment.
    fn query_eligible_edges(&self, query: &EligibilityQuery) -> Result<Vec<EligibleEdge>, StorageError>;

    /// Persist enrichment metadata for edges.
    ///
    /// Merges enrichment data into existing metadata_json on each edge.
    fn persist_enrichments(
        &self,
        updates: &[(String, EnrichmentMetadata)], // (edge_uid, metadata)
    ) -> Result<usize, StorageError>;

    /// Load promotion candidates (enriched edges that might be promotable).
    fn load_promotion_candidates(
        &self,
        snapshot_uid: &str,
        limit: Option<usize>,
    ) -> Result<Vec<PromotionCandidate>, StorageError>;

    /// Load symbol information for promotion context.
    ///
    /// Returns symbols matching the given type names.
    fn load_symbols_by_names(
        &self,
        snapshot_uid: &str,
        type_names: &[String],
    ) -> Result<Vec<SymbolInfo>, StorageError>;

    /// Load methods for a class (by stable key).
    fn load_class_methods(
        &self,
        snapshot_uid: &str,
        class_stable_key: &str,
    ) -> Result<Vec<(String, SymbolInfo)>, StorageError>; // (method_name, method_info)

    /// Delete edges by UID (for idempotent re-promotion).
    fn delete_edges_by_uids(&self, edge_uids: &[String]) -> Result<usize, StorageError>;

    /// Insert promoted edges.
    fn insert_promoted_edges(&self, edges: &[PromotedEdge]) -> Result<usize, StorageError>;

    /// Get the repository root path.
    fn get_repo_root(&self, repo_uid: &str) -> Result<String, StorageError>;
}

/// Storage errors.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("repository not found: {0}")]
    RepoNotFound(String),

    #[error("snapshot not found: {0}")]
    SnapshotNotFound(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("{0}")]
    Other(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// In-Memory Adapter (for testing)
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory storage adapter for testing.
#[derive(Debug, Default)]
pub struct InMemoryEnrichmentStorage {
    pub eligible_edges: Vec<EligibleEdge>,
    pub enrichments: std::collections::HashMap<String, EnrichmentMetadata>,
    pub symbols: Vec<SymbolInfo>,
    pub class_methods: std::collections::HashMap<String, Vec<(String, SymbolInfo)>>,
    pub promoted_edges: Vec<PromotedEdge>,
    pub repo_root: String,
}

impl InMemoryEnrichmentStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_repo_root(mut self, root: impl Into<String>) -> Self {
        self.repo_root = root.into();
        self
    }

    pub fn add_eligible_edge(&mut self, edge: EligibleEdge) {
        self.eligible_edges.push(edge);
    }

    pub fn add_symbol(&mut self, symbol: SymbolInfo) {
        self.symbols.push(symbol);
    }

    pub fn add_class_methods(&mut self, class_key: &str, methods: Vec<(String, SymbolInfo)>) {
        self.class_methods.insert(class_key.to_string(), methods);
    }
}

impl EnrichmentStoragePort for InMemoryEnrichmentStorage {
    fn query_eligible_edges(&self, query: &EligibilityQuery) -> Result<Vec<EligibleEdge>, StorageError> {
        let mut edges: Vec<_> = self
            .eligible_edges
            .iter()
            .filter(|e| e.snapshot_uid == query.snapshot_uid)
            .filter(|e| {
                query.categories.is_empty() || query.categories.contains(&e.category)
            })
            .filter(|e| {
                query.languages.is_empty() || query.languages.contains(&e.language)
            })
            .filter(|e| {
                !query.exclude_already_enriched || !self.enrichments.contains_key(&e.edge_uid)
            })
            .cloned()
            .collect();

        if let Some(limit) = query.limit {
            edges.truncate(limit);
        }

        Ok(edges)
    }

    fn persist_enrichments(
        &self,
        _updates: &[(String, EnrichmentMetadata)],
    ) -> Result<usize, StorageError> {
        // In-memory: would need interior mutability for real implementation
        Ok(0)
    }

    fn load_promotion_candidates(
        &self,
        _snapshot_uid: &str,
        _limit: Option<usize>,
    ) -> Result<Vec<PromotionCandidate>, StorageError> {
        // Would build from enrichments
        Ok(Vec::new())
    }

    fn load_symbols_by_names(
        &self,
        _snapshot_uid: &str,
        type_names: &[String],
    ) -> Result<Vec<SymbolInfo>, StorageError> {
        let symbols: Vec<_> = self
            .symbols
            .iter()
            .filter(|s| {
                let name = s
                    .qualified_name
                    .as_ref()
                    .and_then(|qn| qn.rsplit('.').next())
                    .unwrap_or(&s.stable_key);
                type_names.contains(&name.to_string())
            })
            .cloned()
            .collect();
        Ok(symbols)
    }

    fn load_class_methods(
        &self,
        _snapshot_uid: &str,
        class_stable_key: &str,
    ) -> Result<Vec<(String, SymbolInfo)>, StorageError> {
        Ok(self
            .class_methods
            .get(class_stable_key)
            .cloned()
            .unwrap_or_default())
    }

    fn delete_edges_by_uids(&self, _edge_uids: &[String]) -> Result<usize, StorageError> {
        Ok(0)
    }

    fn insert_promoted_edges(&self, _edges: &[PromotedEdge]) -> Result<usize, StorageError> {
        Ok(0)
    }

    fn get_repo_root(&self, _repo_uid: &str) -> Result<String, StorageError> {
        Ok(self.repo_root.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eligibility_query_builder() {
        let query = EligibilityQuery::new("snap-1")
            .with_limit(100)
            .with_languages(vec![EnrichmentLanguage::TypeScript]);

        assert_eq!(query.snapshot_uid, "snap-1");
        assert_eq!(query.limit, Some(100));
        assert_eq!(query.languages, vec![EnrichmentLanguage::TypeScript]);
        assert!(query.exclude_already_enriched);
    }

    #[test]
    fn test_in_memory_storage() {
        let mut storage = InMemoryEnrichmentStorage::new();

        storage.add_eligible_edge(EligibleEdge {
            edge_uid: "edge-1".to_string(),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            source_node_uid: "source-1".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: "src/main.ts".to_string(),
            line_start: 10,
            col_start: 5,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language: EnrichmentLanguage::TypeScript,
        });

        let query = EligibilityQuery::new("snap-1");
        let edges = storage.query_eligible_edges(&query).unwrap();

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge_uid, "edge-1");
    }

    #[test]
    fn test_filter_by_language() {
        let mut storage = InMemoryEnrichmentStorage::new();

        storage.add_eligible_edge(EligibleEdge {
            edge_uid: "edge-1".to_string(),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            source_node_uid: "source-1".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: "src/main.ts".to_string(),
            line_start: 10,
            col_start: 5,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language: EnrichmentLanguage::TypeScript,
        });

        storage.add_eligible_edge(EligibleEdge {
            edge_uid: "edge-2".to_string(),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            source_node_uid: "source-2".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: "src/main.rs".to_string(),
            line_start: 20,
            col_start: 5,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language: EnrichmentLanguage::Rust,
        });

        let query = EligibilityQuery::new("snap-1")
            .with_languages(vec![EnrichmentLanguage::Rust]);
        let edges = storage.query_eligible_edges(&query).unwrap();

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge_uid, "edge-2");
    }
}
