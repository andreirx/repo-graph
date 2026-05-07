//! Enrichment pipeline orchestration.
//!
//! This module ties together:
//! - Eligibility queries
//! - Language routing
//! - Resolver dispatch
//! - Result persistence
//! - Optional promotion
//!
//! The pipeline is the main entry point for running enrichment.

use std::collections::HashMap;
use std::path::Path;

use crate::contracts::{EligibleEdge, EnrichmentLanguage, EnrichmentMetadata, ReceiverTypeResult};
use crate::eligibility::{EligibilityQuery, EnrichmentStoragePort, StorageError};
use crate::promotion::{promote_edges, PromotionContext};
use crate::resolver::{ResolverError, ResolverRegistry};
use crate::status::{EnrichmentReport, PromotionReport, ReportBuilder};

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for an enrichment run.
#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    /// Maximum edges to enrich per run.
    pub limit: usize,

    /// Whether to run promotion after enrichment.
    pub promote: bool,

    /// Languages to enrich (empty = all available).
    pub languages: Vec<EnrichmentLanguage>,

    /// Whether to re-enrich already-enriched edges.
    pub force: bool,

    /// Dry run mode: resolve types but don't persist to database.
    pub dry_run: bool,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            limit: 10_000,
            promote: false,
            languages: Vec::new(),
            force: false,
            dry_run: false,
        }
    }
}

impl EnrichmentConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_promotion(mut self) -> Self {
        self.promote = true;
        self
    }

    pub fn with_languages(mut self, languages: Vec<EnrichmentLanguage>) -> Self {
        self.languages = languages;
        self
    }

    pub fn with_force(mut self) -> Self {
        self.force = true;
        self
    }

    pub fn with_dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during pipeline execution.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("resolver error: {0}")]
    Resolver(#[from] ResolverError),

    #[error("no resolvers available for languages: {0:?}")]
    NoResolvers(Vec<EnrichmentLanguage>),

    #[error("repository not found: {0}")]
    RepoNotFound(String),

    #[error("{0}")]
    Other(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline
// ─────────────────────────────────────────────────────────────────────────────

/// The enrichment pipeline.
///
/// Orchestrates the full enrichment flow from eligible edge query
/// through optional promotion.
pub struct EnrichmentPipeline<S: EnrichmentStoragePort> {
    storage: S,
    registry: ResolverRegistry,
}

impl<S: EnrichmentStoragePort> EnrichmentPipeline<S> {
    /// Create a new pipeline with storage and an empty resolver registry.
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            registry: ResolverRegistry::new(),
        }
    }

    /// Create a new pipeline with storage and a pre-configured registry.
    pub fn with_registry(storage: S, registry: ResolverRegistry) -> Self {
        Self { storage, registry }
    }

    /// Get mutable access to the resolver registry.
    pub fn registry_mut(&mut self) -> &mut ResolverRegistry {
        &mut self.registry
    }

    /// Run enrichment for a repository/snapshot.
    pub fn run(
        &mut self,
        repo_uid: &str,
        snapshot_uid: &str,
        config: &EnrichmentConfig,
    ) -> Result<EnrichmentReport, PipelineError> {
        let mut builder = ReportBuilder::new(repo_uid.to_string(), snapshot_uid.to_string());

        // Get repository root path
        let repo_root = self.storage.get_repo_root(repo_uid)?;
        let repo_path = Path::new(&repo_root);

        // Query eligible edges
        let query = EligibilityQuery::new(snapshot_uid)
            .with_limit(config.limit)
            .with_languages(config.languages.clone());

        let query = if config.force {
            query.include_already_enriched()
        } else {
            query
        };

        let edges = self.storage.query_eligible_edges(&query)?;

        if edges.is_empty() {
            return Ok(builder.build());
        }

        // Group edges by language
        let edges_by_language = group_by_language(&edges);

        // Record eligible counts
        for (lang, lang_edges) in &edges_by_language {
            for _ in lang_edges {
                builder.record_eligible(*lang);
            }
        }

        // Resolve each language batch
        let mut all_results: Vec<ReceiverTypeResult> = Vec::new();

        for (lang, lang_edges) in &edges_by_language {
            if let Some(resolver) = self.registry.get_mut(*lang) {
                // Initialize resolver
                resolver.initialize(repo_path)?;

                // Resolve batch
                let results = resolver.resolve_batch(repo_path, lang_edges, None);

                // Record results
                for result in &results {
                    if result.is_success() {
                        let type_name = result
                            .type_display_name
                            .as_ref()
                            .or(result.receiver_type.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("unknown");
                        builder.record_success(*lang, type_name, result.is_external_type);
                    } else {
                        let reason = result
                            .failure_reason
                            .as_ref()
                            .map(|s| s.as_str())
                            .unwrap_or("unknown");
                        builder.record_failure(*lang, reason);
                    }
                }

                all_results.extend(results);

                // Shutdown resolver
                resolver.shutdown();
            } else {
                // No resolver for this language - mark all as failed
                for edge in lang_edges {
                    builder.record_failure(*lang, "no_resolver_available");
                    all_results.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "no resolver available",
                    ));
                }
            }
        }

        // Persist enrichments (skip in dry-run mode)
        if config.dry_run {
            // In dry-run mode, do not set persisted_count.
            // Leaving it as None signals "persistence not attempted" and
            // prevents has_storage_discrepancy() from falsely triggering.
        } else {
            let updates: Vec<_> = all_results
                .iter()
                .map(|r| (r.edge_uid.clone(), EnrichmentMetadata::from(r)))
                .collect();

            let persisted_count = self.storage.persist_enrichments(&updates)?;
            builder.set_persisted_count(persisted_count);

            // Run promotion if requested (also skipped in dry-run)
            if config.promote {
                let promotion_report = self.run_promotion(snapshot_uid)?;
                builder.set_promotion(promotion_report);
            }
        }

        Ok(builder.build())
    }

    /// Run promotion on already-enriched edges.
    fn run_promotion(&self, snapshot_uid: &str) -> Result<PromotionReport, PipelineError> {
        // Load promotion candidates
        let candidates = self.storage.load_promotion_candidates(snapshot_uid, None)?;

        if candidates.is_empty() {
            return Ok(PromotionReport::default());
        }

        // Collect unique type names for symbol lookup
        let type_names: Vec<_> = candidates
            .iter()
            .filter_map(|c| {
                c.enrichment
                    .type_display_name
                    .as_ref()
                    .or(c.enrichment.receiver_type.as_ref())
            })
            .cloned()
            .collect();

        // Build promotion context
        let mut ctx = PromotionContext::new();

        // Load symbols
        let symbols = self.storage.load_symbols_by_names(snapshot_uid, &type_names)?;
        for symbol in symbols {
            ctx.add_symbol(symbol.clone());

            // If it's a class, load its methods
            if symbol.subtype == crate::contracts::SymbolSubtype::Class {
                let methods = self.storage.load_class_methods(snapshot_uid, &symbol.stable_key)?;
                for (method_name, method_info) in methods {
                    ctx.add_class_method(&symbol.stable_key, &method_name, method_info);
                }
            }
        }

        // Run promotion filter
        let result = promote_edges(&candidates, &ctx);

        // Delete previously promoted edges (idempotency)
        let promoted_uids: Vec<_> = result.promoted.iter().map(|e| e.edge_uid.clone()).collect();
        self.storage.delete_edges_by_uids(&promoted_uids)?;

        // Insert newly promoted edges
        let persisted_count = self.storage.insert_promoted_edges(&result.promoted)?;

        Ok(result.to_report(candidates.len(), Some(persisted_count)))
    }
}

/// Group edges by language.
fn group_by_language(edges: &[EligibleEdge]) -> HashMap<EnrichmentLanguage, Vec<EligibleEdge>> {
    let mut groups: HashMap<EnrichmentLanguage, Vec<EligibleEdge>> = HashMap::new();
    for edge in edges {
        groups.entry(edge.language).or_default().push(edge.clone());
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eligibility::InMemoryEnrichmentStorage;
    use crate::contracts::UnresolvedCategory;
    use crate::resolver::NullResolver;

    #[test]
    fn test_pipeline_with_no_edges() {
        let storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");
        let mut pipeline = EnrichmentPipeline::new(storage);

        let report = pipeline
            .run("repo-1", "snap-1", &EnrichmentConfig::default())
            .unwrap();

        assert_eq!(report.eligible_count, 0);
        assert_eq!(report.state(), crate::status::EnrichmentState::NotApplicable);
    }

    #[test]
    fn test_pipeline_with_null_resolver() {
        let mut storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");

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

        let mut pipeline = EnrichmentPipeline::new(storage);
        pipeline
            .registry_mut()
            .register(Box::new(NullResolver::new(EnrichmentLanguage::TypeScript)));

        let report = pipeline
            .run("repo-1", "snap-1", &EnrichmentConfig::default())
            .unwrap();

        assert_eq!(report.eligible_count, 1);
        assert_eq!(report.enriched_count, 0);
        assert_eq!(report.failed_count, 1);
        assert_eq!(report.state(), crate::status::EnrichmentState::Ran);
    }

    #[test]
    fn test_config_builder() {
        let config = EnrichmentConfig::new()
            .with_limit(500)
            .with_promotion()
            .with_languages(vec![EnrichmentLanguage::Rust])
            .with_force();

        assert_eq!(config.limit, 500);
        assert!(config.promote);
        assert_eq!(config.languages, vec![EnrichmentLanguage::Rust]);
        assert!(config.force);
    }

    #[test]
    fn test_dry_run_does_not_persist_or_trigger_discrepancy() {
        // Dry-run should:
        // 1. Leave persisted_count as None (persistence not attempted)
        // 2. Not trigger has_storage_discrepancy()
        let mut storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");

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

        let mut pipeline = EnrichmentPipeline::new(storage);
        pipeline
            .registry_mut()
            .register(Box::new(NullResolver::new(EnrichmentLanguage::TypeScript)));

        let config = EnrichmentConfig::new().with_dry_run();
        let report = pipeline.run("repo-1", "snap-1", &config).unwrap();

        // Resolver ran and recorded failure
        assert_eq!(report.eligible_count, 1);
        assert_eq!(report.failed_count, 1);

        // Persistence was not attempted
        assert!(
            report.persisted_count.is_none(),
            "dry-run must not set persisted_count"
        );

        // No false discrepancy signal
        assert!(
            !report.has_storage_discrepancy(),
            "dry-run must not trigger storage discrepancy"
        );
    }
}
