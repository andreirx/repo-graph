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

use crate::contracts::{
    BatchResolution, EligibleEdge, EnrichmentLanguage, EnrichmentMetadata, ReceiverTypeResult,
};
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
    ///
    /// The manual (client-driven) path: runs to completion. Delegates to
    /// [`run_cancellable`](Self::run_cancellable) with a cancel check that never fires,
    /// so this is byte-for-byte the same work — resolution is grouped identically, and a
    /// never-cancelling run persists every batch and promotes once, exactly as before.
    pub fn run(
        &mut self,
        repo_uid: &str,
        snapshot_uid: &str,
        config: &EnrichmentConfig,
    ) -> Result<EnrichmentReport, PipelineError> {
        self.run_cancellable(repo_uid, snapshot_uid, config, &|| false)
    }

    /// Run enrichment, yielding cooperatively at batch boundaries when `cancel` fires
    /// (ENRICH-LIFECYCLE-1 running-yield).
    ///
    /// `cancel` is polled at TWO boundary classes so an explicit index/refresh can make a
    /// running background pass release the DB write lock without a mid-LSP-request abort:
    /// 1. **Between languages** — here, before initializing the next language's resolver.
    /// 2. **Within a language** — threaded into `resolve_batch`, which polls it before
    ///    starting each per-project LSP session (never pays a fresh warm-up on cancel) and
    ///    before each per-edge resolve (yields within one LSP request).
    ///
    /// **Each language's persist is a complete additive fact** (`persist_enrichments` is an
    /// additive per-edge `metadata_json` UPDATE): a cancelled pass leaves the batches it
    /// finished durably enriched, never a torn state. On cancel the trailing promotion is
    /// **skipped** — the superseding index re-enriches and re-promotes the fresh snapshot,
    /// so promoting a doomed snapshot would only lengthen the yield. A completed
    /// (never-cancelled) run persists every batch and promotes once at the end, unchanged.
    pub fn run_cancellable(
        &mut self,
        repo_uid: &str,
        snapshot_uid: &str,
        config: &EnrichmentConfig,
        cancel: &dyn Fn() -> bool,
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

        // Resolve each language batch, persisting the batch before moving on (a complete
        // additive fact). Between languages, honor a yield request.
        let mut persisted_total = 0usize;
        for (lang, lang_edges) in &edges_by_language {
            if cancel() {
                break;
            }

            let batch: BatchResolution = if let Some(resolver) = self.registry.get_mut(*lang) {
                // Initialize resolver
                resolver.initialize(repo_path)?;

                // Resolve batch (cancel threaded into the resolver's own per-session /
                // per-edge boundaries; a cancelled batch returns partial results).
                let batch = resolver.resolve_batch(repo_path, lang_edges, None, Some(cancel));

                // Record ATTEMPTED results (success or failure).
                for result in &batch.results {
                    if result.is_success() {
                        let type_name = result
                            .type_display_name
                            .as_ref()
                            .or(result.receiver_type.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("unknown");
                        builder.record_success(*lang, type_name, result.is_external_type);
                    } else {
                        let reason = result.failure_reason.as_deref().unwrap_or("unknown");
                        builder.record_failure(*lang, reason);
                    }
                }

                // Record NOT-ATTEMPTED edges (per-context skips) — the honesty fix
                // (ENRICH-ROOT-1 §2): these were silently dropped before, so `eligible`
                // exceeded `enriched + failed`. Now they count toward `not_attempted` and
                // surface with their context path + reason.
                for skip in &batch.skipped_contexts {
                    builder.record_not_attempted(*lang, skip);
                }

                // Shutdown resolver
                resolver.shutdown();
                batch
            } else {
                // No resolver for this language - mark all as failed
                let mut results = Vec::with_capacity(lang_edges.len());
                for edge in lang_edges {
                    builder.record_failure(*lang, "no_resolver_available");
                    results.push(ReceiverTypeResult::failed(
                        edge.edge_uid.clone(),
                        "no resolver available",
                    ));
                }
                BatchResolution::from_results(results)
            };

            // Persist this batch (skip in dry-run mode — persistence not attempted, which
            // leaves persisted_count as None so has_storage_discrepancy() cannot misfire).
            // Only ATTEMPTED results are persisted: a not-attempted edge must NOT get an
            // enrichment marker, or it would be excluded from the next pass and never retried
            // once its toolchain becomes available.
            if !config.dry_run {
                let updates: Vec<_> = batch
                    .results
                    .iter()
                    .map(|r| (r.edge_uid.clone(), EnrichmentMetadata::from(r)))
                    .collect();
                persisted_total += self.storage.persist_enrichments(&updates)?;
            }
        }

        if !config.dry_run {
            builder.set_persisted_count(persisted_total);

            // Promote once over everything persisted — unless we yielded (a superseding
            // index will re-enrich + re-promote the fresh snapshot; skipping bounds the yield).
            if config.promote && !cancel() {
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
        let symbols = self
            .storage
            .load_symbols_by_names(snapshot_uid, &type_names)?;
        for symbol in symbols {
            ctx.add_symbol(symbol.clone());

            // Load methods for any usable receiver type — a class OR an enum (EY1-D). This MUST use
            // the same predicate as promotion gate 5 (`is_usable_receiver_type`): if the gate accepts
            // a subtype whose methods we never load here, gate 6 finds zero methods and the edge
            // silently fails to promote. `load_class_methods` associates methods by type name /
            // qualified_name, so it already returns an enum's `impl` methods.
            if symbol.subtype.is_usable_receiver_type() {
                let methods = self
                    .storage
                    .load_class_methods(snapshot_uid, &symbol.stable_key)?;
                for (method_name, method_info) in methods {
                    ctx.add_class_method(&symbol.stable_key, &method_name, method_info);
                }
            }
        }

        // Run promotion filter
        let result = promote_edges(&candidates, &ctx);

        // Persist the promotion result ATOMICALLY (EC-1 M-3b): delete
        // previously-promoted uids (idempotency), insert the new set, and
        // adjust the persisted resolved-call aggregate by the net CALLS-row
        // delta — one transaction, so the aggregate and the rows move
        // together on every success/failure exit (never stale).
        let persisted_count = self
            .storage
            .apply_promotion(snapshot_uid, &result.promoted)?;

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
    use crate::contracts::UnresolvedCategory;
    use crate::eligibility::InMemoryEnrichmentStorage;
    use crate::resolver::NullResolver;

    #[test]
    fn test_pipeline_with_no_edges() {
        let storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");
        let mut pipeline = EnrichmentPipeline::new(storage);

        let report = pipeline
            .run("repo-1", "snap-1", &EnrichmentConfig::default())
            .unwrap();

        assert_eq!(report.eligible_count, 0);
        assert_eq!(
            report.state(),
            crate::status::EnrichmentState::NotApplicable
        );
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

    // ── ENRICH-LIFECYCLE-1 running-yield: run_cancellable batch-boundary cancellation ──────────────

    fn eligible(uid: &str, lang: EnrichmentLanguage) -> EligibleEdge {
        EligibleEdge {
            edge_uid: uid.to_string(),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            source_node_uid: "src".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: "src/main.ts".to_string(),
            line_start: 1,
            col_start: 1,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language: lang,
        }
    }

    /// A fake resolver that SUCCEEDS on every edge and honors the cancel check at its per-edge
    /// boundary — the hermetic stand-in for a real LSP resolver (no rust-analyzer/tsserver process),
    /// so the pipeline's batch-boundary cancellation is provable deterministically.
    struct CountingResolver {
        lang: EnrichmentLanguage,
    }
    impl crate::resolver::ReceiverTypeResolver for CountingResolver {
        fn language(&self) -> EnrichmentLanguage {
            self.lang
        }
        fn resolve_batch(
            &self,
            _repo_root: &Path,
            edges: &[EligibleEdge],
            _progress: Option<&dyn crate::resolver::ResolverProgress>,
            cancel: Option<&dyn Fn() -> bool>,
        ) -> BatchResolution {
            let mut out = Vec::new();
            for e in edges {
                if cancel.is_some_and(|c| c()) {
                    break;
                }
                out.push(ReceiverTypeResult::success(
                    e.edge_uid.clone(),
                    "SomeType".to_string(),
                    Some("SomeType".to_string()),
                    false,
                ));
            }
            BatchResolution::from_results(out)
        }
        fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
            Ok(())
        }
        fn shutdown(&mut self) {}
    }

    // A yield that fires at the between-language boundary stops before any resolution AND skips
    // promotion (the loop breaks before `run_promotion` is ever reached).
    #[test]
    fn run_cancellable_breaks_the_language_loop_on_immediate_cancel() {
        let mut storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");
        for i in 0..3 {
            storage.add_eligible_edge(eligible(&format!("e{i}"), EnrichmentLanguage::TypeScript));
        }
        let mut pipeline = EnrichmentPipeline::new(storage);
        pipeline.registry_mut().register(Box::new(CountingResolver {
            lang: EnrichmentLanguage::TypeScript,
        }));

        let report = pipeline
            .run_cancellable(
                "repo-1",
                "snap-1",
                &EnrichmentConfig::new().with_promotion(),
                &|| true,
            )
            .unwrap();

        assert_eq!(report.eligible_count, 3, "eligible is recorded up front");
        assert_eq!(
            report.enriched_count + report.failed_count,
            0,
            "cancel before the language body → nothing processed, promotion skipped"
        );
    }

    // A yield that fires mid-resolution stops the batch partway — the completed edges are a partial,
    // additive commit; the rest are abandoned.
    #[test]
    fn run_cancellable_stops_within_the_batch_on_mid_cancel() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mut storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");
        for i in 0..8 {
            storage.add_eligible_edge(eligible(&format!("e{i}"), EnrichmentLanguage::TypeScript));
        }
        let mut pipeline = EnrichmentPipeline::new(storage);
        pipeline.registry_mut().register(Box::new(CountingResolver {
            lang: EnrichmentLanguage::TypeScript,
        }));

        // Trip after a few polls (one between-language poll + per-edge polls threaded into the
        // resolver) so the batch stops partway — robust to the exact poll count via a range assert.
        let polls = AtomicUsize::new(0);
        let cancel = || polls.fetch_add(1, Ordering::SeqCst) >= 4;
        let report = pipeline
            .run_cancellable(
                "repo-1",
                "snap-1",
                &EnrichmentConfig::new().with_promotion(),
                &cancel,
            )
            .unwrap();

        assert_eq!(report.eligible_count, 8, "all recorded eligible");
        let processed = report.enriched_count + report.failed_count;
        assert!(
            processed > 0 && processed < 8,
            "cancel stopped the batch partway (processed {processed}/8)"
        );
    }

    // ── ENRICH-ROOT-1 §2: not-attempted accounting + the binding invariant ──────────────────────

    /// A hermetic resolver that ATTEMPTS the first `attempt` edges (success) and reports the rest as
    /// ONE not-attempted context skip — the shape the real tsserver produces when a project context
    /// has no toolchain. Lets the pipeline's not_attempted accounting be proven without an LSP.
    struct SkippingResolver {
        lang: EnrichmentLanguage,
        attempt: usize,
        context_path: String,
        reason: String,
    }
    impl crate::resolver::ReceiverTypeResolver for SkippingResolver {
        fn language(&self) -> EnrichmentLanguage {
            self.lang
        }
        fn resolve_batch(
            &self,
            _repo_root: &Path,
            edges: &[EligibleEdge],
            _progress: Option<&dyn crate::resolver::ResolverProgress>,
            _cancel: Option<&dyn Fn() -> bool>,
        ) -> BatchResolution {
            let results: Vec<ReceiverTypeResult> = edges
                .iter()
                .take(self.attempt)
                .map(|e| {
                    ReceiverTypeResult::success(
                        e.edge_uid.clone(),
                        "SomeType".to_string(),
                        Some("SomeType".to_string()),
                        false,
                    )
                })
                .collect();
            let skipped = edges.len().saturating_sub(self.attempt);
            let skipped_contexts = if skipped > 0 {
                vec![crate::contracts::SkippedContext {
                    context_path: self.context_path.clone(),
                    reason: self.reason.clone(),
                    edge_count: skipped,
                }]
            } else {
                Vec::new()
            };
            BatchResolution {
                results,
                skipped_contexts,
            }
        }
        fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
            Ok(())
        }
        fn shutdown(&mut self) {}
    }

    /// The binding invariant (slice §2): a not-attempted context is COUNTED, surfaced with its
    /// context path + reason, and `eligible == enriched + failed + not_attempted` holds exactly.
    #[test]
    fn not_attempted_context_is_counted_and_satisfies_the_accounting_invariant() {
        let mut storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");
        for i in 0..5 {
            storage.add_eligible_edge(eligible(&format!("e{i}"), EnrichmentLanguage::TypeScript));
        }
        let mut pipeline = EnrichmentPipeline::new(storage);
        pipeline.registry_mut().register(Box::new(SkippingResolver {
            lang: EnrichmentLanguage::TypeScript,
            attempt: 3,
            context_path: "packages/legacy".to_string(),
            reason: "no tsserver for this project context".to_string(),
        }));

        let report = pipeline
            .run("repo-1", "snap-1", &EnrichmentConfig::default())
            .unwrap();

        assert_eq!(report.eligible_count, 5);
        assert_eq!(report.enriched_count, 3);
        assert_eq!(report.failed_count, 0, "a skip is NOT a failure");
        assert_eq!(report.not_attempted_count, 2, "2 edges were not attempted");

        // The invariant holds exactly — no silent gap.
        assert!(
            report.accounting_holds(),
            "eligible({}) must equal enriched({}) + failed({}) + not_attempted({})",
            report.eligible_count,
            report.enriched_count,
            report.failed_count,
            report.not_attempted_count
        );

        // The skip is surfaced with its context path + reason + edge count.
        assert_eq!(report.skipped_contexts.len(), 1);
        let sc = &report.skipped_contexts[0];
        assert_eq!(sc.context_path, "packages/legacy");
        assert_eq!(sc.edge_count, 2);
        assert!(sc.reason.contains("tsserver"));

        // Per-language accounting also carries the not-attempted count.
        let ts = &report.by_language[&EnrichmentLanguage::TypeScript];
        assert_eq!(ts.not_attempted, 2);
    }

    /// A pass with no skips keeps the invariant AND leaves the not-attempted surface empty (0 / []),
    /// so a clean run is byte-compatible with the pre-slice report shape.
    #[test]
    fn no_skips_leaves_not_attempted_empty_and_invariant_intact() {
        let mut storage = InMemoryEnrichmentStorage::new().with_repo_root("/repo");
        for i in 0..4 {
            storage.add_eligible_edge(eligible(&format!("e{i}"), EnrichmentLanguage::TypeScript));
        }
        let mut pipeline = EnrichmentPipeline::new(storage);
        pipeline.registry_mut().register(Box::new(SkippingResolver {
            lang: EnrichmentLanguage::TypeScript,
            attempt: 4, // attempt all → no skip
            context_path: "unused".to_string(),
            reason: "unused".to_string(),
        }));
        let report = pipeline
            .run("repo-1", "snap-1", &EnrichmentConfig::default())
            .unwrap();
        assert_eq!(report.enriched_count, 4);
        assert_eq!(report.not_attempted_count, 0);
        assert!(report.skipped_contexts.is_empty());
        assert!(report.accounting_holds());
    }
}
