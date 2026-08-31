//! Enrichment status and execution accounting.
//!
//! Tracks:
//! - Whether enrichment has run for a snapshot
//! - Success/failure counts
//! - Language-specific breakdown
//! - Promotion statistics
//!
//! This is the data that feeds into the trust model's
//! `enrichment_state` axis.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::contracts::{EnrichmentLanguage, SkippedContext};

// ─────────────────────────────────────────────────────────────────────────────
// Enrichment State
// ─────────────────────────────────────────────────────────────────────────────

/// The enrichment state for a snapshot.
///
/// This is the value that the trust model checks:
/// - NotRun: enrichment has never been executed
/// - Ran: enrichment has been executed (success rate may vary)
/// - NotApplicable: no enrichable edges exist
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentState {
    /// Enrichment has not been executed for this snapshot.
    #[default]
    NotRun,

    /// Enrichment has been executed.
    Ran,

    /// No enrichable edges exist (nothing to enrich).
    NotApplicable,
}

impl EnrichmentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotRun => "not_run",
            Self::Ran => "ran",
            Self::NotApplicable => "not_applicable",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "not_run" => Some(Self::NotRun),
            "ran" => Some(Self::Ran),
            "not_applicable" => Some(Self::NotApplicable),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Enrichment Execution Report
// ─────────────────────────────────────────────────────────────────────────────

/// Complete report of an enrichment execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentReport {
    /// Repository identifier.
    pub repo_uid: String,

    /// Snapshot identifier.
    pub snapshot_uid: String,

    /// Total eligible edges queried.
    pub eligible_count: usize,

    /// Successfully enriched edges.
    pub enriched_count: usize,

    /// Failed enrichment attempts.
    pub failed_count: usize,

    /// Edges that were NOT ATTEMPTED because their project context had no usable resolver
    /// toolchain (ENRICH-ROOT-1 §2). Distinct from `failed_count` (attempted, could not resolve).
    /// The accounting invariant `eligible_count == enriched_count + failed_count +
    /// not_attempted_count` always holds for a non-cancelled pass. `#[serde(default)]` so a report
    /// serialized before this field deserializes to 0 (no skips recorded).
    #[serde(default)]
    pub not_attempted_count: usize,

    /// Per-context "not attempted" breakdown — one entry per (context_path, reason), with the
    /// count of eligible edges skipped there. Empty when nothing was skipped. `#[serde(default)]`
    /// for backward-compatible deserialization.
    #[serde(default)]
    pub skipped_contexts: Vec<SkippedContext>,

    /// Enrichment rate as percentage.
    pub enrichment_rate: f64,

    /// Breakdown by language.
    pub by_language: HashMap<EnrichmentLanguage, LanguageReport>,

    /// Promotion statistics (if promotion was run).
    pub promotion: Option<PromotionReport>,

    /// Top failure reasons.
    pub top_failure_reasons: Vec<FailureCount>,

    /// Top resolved types.
    pub top_types: Vec<TypeCount>,

    /// Actual rows persisted to storage.
    /// None if persistence not yet attempted.
    /// Compare against `attempted_persist_count()` to detect storage discrepancy.
    pub persisted_count: Option<usize>,
}

impl EnrichmentReport {
    /// Create a new empty report.
    pub fn new(repo_uid: String, snapshot_uid: String) -> Self {
        Self {
            repo_uid,
            snapshot_uid,
            eligible_count: 0,
            enriched_count: 0,
            failed_count: 0,
            not_attempted_count: 0,
            skipped_contexts: Vec::new(),
            enrichment_rate: 0.0,
            by_language: HashMap::new(),
            promotion: None,
            top_failure_reasons: Vec::new(),
            top_types: Vec::new(),
            persisted_count: None,
        }
    }

    /// Number of enrichment metadata rows we attempted to persist.
    /// This is enriched + failed, since we persist metadata for ALL ATTEMPTED results.
    /// Not-attempted edges are deliberately excluded — they are never persisted (they must stay
    /// eligible for a future pass), so they do not participate in the storage-discrepancy check.
    pub fn attempted_persist_count(&self) -> usize {
        self.enriched_count + self.failed_count
    }

    /// The accounting invariant (ENRICH-ROOT-1 §2): every eligible edge is enriched, failed, or
    /// not-attempted — no silent gap. Holds for a non-cancelled pass (a cancelled pass abandons its
    /// tail, so this may under-count there by design). Exposed for the binding invariant test.
    pub fn accounting_holds(&self) -> bool {
        self.eligible_count == self.enriched_count + self.failed_count + self.not_attempted_count
    }

    /// Check if there is a storage discrepancy (persisted != attempted).
    /// Returns true if storage wrote fewer rows than we attempted to persist.
    pub fn has_storage_discrepancy(&self) -> bool {
        match self.persisted_count {
            Some(persisted) => persisted != self.attempted_persist_count(),
            None => false,
        }
    }

    /// Compute the enrichment rate.
    pub fn compute_rate(&mut self) {
        if self.eligible_count > 0 {
            self.enrichment_rate =
                (self.enriched_count as f64 / self.eligible_count as f64) * 100.0;
        }
    }

    /// Get the resulting enrichment state.
    pub fn state(&self) -> EnrichmentState {
        if self.eligible_count == 0 {
            EnrichmentState::NotApplicable
        } else {
            EnrichmentState::Ran
        }
    }
}

/// Per-language enrichment statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguageReport {
    /// Eligible edges for this language.
    pub eligible: usize,

    /// Successfully enriched.
    pub enriched: usize,

    /// Failed attempts.
    pub failed: usize,

    /// Edges not attempted (project context lacked a resolver toolchain). `#[serde(default)]` for
    /// backward-compatible deserialization of reports written before ENRICH-ROOT-1.
    #[serde(default)]
    pub not_attempted: usize,

    /// Enrichment rate as percentage.
    pub rate: f64,
}

impl LanguageReport {
    pub fn compute_rate(&mut self) {
        if self.eligible > 0 {
            self.rate = (self.enriched as f64 / self.eligible as f64) * 100.0;
        }
    }
}

/// Promotion statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromotionReport {
    /// Number of candidates evaluated.
    pub candidates: usize,

    /// Number of edges promoted to resolved.
    pub promoted: usize,

    /// Skipped edges by reason (first-rejecting gate; `promote_edges` `continue`s after the first
    /// failing gate, so each candidate contributes to exactly one reason).
    pub skipped_reasons: HashMap<String, usize>,

    /// Candidates that REACHED each gate, keyed by gate number — ground truth counted live by
    /// `promote_edges` (ENRICH-YIELD-1 §2.1). Feeds the per-gate waterfall in [`Self::funnel`].
    /// `#[serde(default)]` so an older report without it deserializes to an empty (flat-only) funnel.
    #[serde(default)]
    pub gate_entered: BTreeMap<u8, usize>,

    /// Actual promoted edges persisted to storage (may differ if storage fails).
    pub persisted_count: Option<usize>,
}

impl PromotionReport {
    /// Decompose the 3.5% (ENRICH-YIELD-1): candidates → promoted, with BOTH the reader-frame per-gate
    /// waterfall (entering + first-rejected for each gate, from `gate_entered`) and the per-class
    /// first-rejection breakdown of the rest. Pure derivation of the counts already here; see
    /// [`crate::funnel::PromotionFunnel`].
    pub fn funnel(&self) -> crate::funnel::PromotionFunnel {
        crate::funnel::PromotionFunnel::from_counts(
            self.candidates,
            self.promoted,
            &self.skipped_reasons,
            &self.gate_entered,
        )
    }
}

/// A failure reason with count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureCount {
    pub reason: String,
    pub count: usize,
}

/// A type with count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeCount {
    pub type_name: String,
    pub is_external: bool,
    pub count: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Report Builder
// ─────────────────────────────────────────────────────────────────────────────

/// Builder for accumulating enrichment statistics during execution.
#[derive(Debug)]
pub struct ReportBuilder {
    repo_uid: String,
    snapshot_uid: String,
    by_language: HashMap<EnrichmentLanguage, LanguageReport>,
    failure_reasons: HashMap<String, usize>,
    type_counts: HashMap<(String, bool), usize>,
    /// Per-context skip aggregation, keyed by (context_path, reason) → edge count. A BTreeMap so
    /// the emitted `skipped_contexts` order is deterministic (VISION: same input → same output).
    skipped_contexts: BTreeMap<(String, String), usize>,
    promotion: Option<PromotionReport>,
    persisted_count: Option<usize>,
}

impl ReportBuilder {
    pub fn new(repo_uid: String, snapshot_uid: String) -> Self {
        Self {
            repo_uid,
            snapshot_uid,
            by_language: HashMap::new(),
            failure_reasons: HashMap::new(),
            type_counts: HashMap::new(),
            skipped_contexts: BTreeMap::new(),
            promotion: None,
            persisted_count: None,
        }
    }

    /// Record an eligible edge for a language.
    pub fn record_eligible(&mut self, lang: EnrichmentLanguage) {
        self.by_language.entry(lang).or_default().eligible += 1;
    }

    /// Record a successful enrichment.
    pub fn record_success(&mut self, lang: EnrichmentLanguage, type_name: &str, is_external: bool) {
        let entry = self.by_language.entry(lang).or_default();
        entry.enriched += 1;

        *self
            .type_counts
            .entry((type_name.to_string(), is_external))
            .or_insert(0) += 1;
    }

    /// Record a failed enrichment.
    pub fn record_failure(&mut self, lang: EnrichmentLanguage, reason: &str) {
        let entry = self.by_language.entry(lang).or_default();
        entry.failed += 1;

        *self.failure_reasons.entry(reason.to_string()).or_insert(0) += 1;
    }

    /// Record a per-context skip (edges the resolver did not attempt because the context lacked a
    /// toolchain). Increments the language's `not_attempted` by the skip's edge count and
    /// aggregates the (context_path, reason) breakdown (ENRICH-ROOT-1 §2).
    pub fn record_not_attempted(&mut self, lang: EnrichmentLanguage, skip: &SkippedContext) {
        self.by_language.entry(lang).or_default().not_attempted += skip.edge_count;
        *self
            .skipped_contexts
            .entry((skip.context_path.clone(), skip.reason.clone()))
            .or_insert(0) += skip.edge_count;
    }

    /// Set promotion statistics.
    pub fn set_promotion(&mut self, report: PromotionReport) {
        self.promotion = Some(report);
    }

    /// Set the actual count of rows persisted to storage.
    pub fn set_persisted_count(&mut self, count: usize) {
        self.persisted_count = Some(count);
    }

    /// Build the final report.
    pub fn build(mut self) -> EnrichmentReport {
        // Compute per-language rates
        for report in self.by_language.values_mut() {
            report.compute_rate();
        }

        // Aggregate totals
        let eligible_count: usize = self.by_language.values().map(|r| r.eligible).sum();
        let enriched_count: usize = self.by_language.values().map(|r| r.enriched).sum();
        let failed_count: usize = self.by_language.values().map(|r| r.failed).sum();
        let not_attempted_count: usize = self.by_language.values().map(|r| r.not_attempted).sum();

        // Per-context skip breakdown, deterministic (BTreeMap iteration order).
        let skipped_contexts: Vec<SkippedContext> = self
            .skipped_contexts
            .into_iter()
            .map(|((context_path, reason), edge_count)| SkippedContext {
                context_path,
                reason,
                edge_count,
            })
            .collect();

        // Top failure reasons (sorted by count descending)
        let mut failure_vec: Vec<_> = self
            .failure_reasons
            .into_iter()
            .map(|(reason, count)| FailureCount { reason, count })
            .collect();
        failure_vec.sort_by(|a, b| b.count.cmp(&a.count));
        failure_vec.truncate(10);

        // Top types (sorted by count descending)
        let mut type_vec: Vec<_> = self
            .type_counts
            .into_iter()
            .map(|((type_name, is_external), count)| TypeCount {
                type_name,
                is_external,
                count,
            })
            .collect();
        type_vec.sort_by(|a, b| b.count.cmp(&a.count));
        type_vec.truncate(20);

        let mut report = EnrichmentReport {
            repo_uid: self.repo_uid,
            snapshot_uid: self.snapshot_uid,
            eligible_count,
            enriched_count,
            failed_count,
            not_attempted_count,
            skipped_contexts,
            enrichment_rate: 0.0,
            by_language: self.by_language,
            promotion: self.promotion,
            top_failure_reasons: failure_vec,
            top_types: type_vec,
            persisted_count: self.persisted_count,
        };
        report.compute_rate();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrichment_state_roundtrip() {
        for state in [
            EnrichmentState::NotRun,
            EnrichmentState::Ran,
            EnrichmentState::NotApplicable,
        ] {
            assert_eq!(EnrichmentState::parse(state.as_str()), Some(state));
        }
    }

    #[test]
    fn test_report_builder() {
        let mut builder = ReportBuilder::new("repo-1".to_string(), "snap-1".to_string());

        // Record some eligible edges
        builder.record_eligible(EnrichmentLanguage::TypeScript);
        builder.record_eligible(EnrichmentLanguage::TypeScript);
        builder.record_eligible(EnrichmentLanguage::Rust);

        // Record results
        builder.record_success(EnrichmentLanguage::TypeScript, "MyClass", false);
        builder.record_failure(EnrichmentLanguage::TypeScript, "type is any");
        builder.record_success(EnrichmentLanguage::Rust, "Engine", false);

        let report = builder.build();

        assert_eq!(report.eligible_count, 3);
        assert_eq!(report.enriched_count, 2);
        assert_eq!(report.failed_count, 1);
        assert!((report.enrichment_rate - 66.66).abs() < 1.0);

        assert_eq!(report.state(), EnrichmentState::Ran);
    }

    #[test]
    fn test_empty_report() {
        let builder = ReportBuilder::new("repo-1".to_string(), "snap-1".to_string());
        let report = builder.build();

        assert_eq!(report.eligible_count, 0);
        assert_eq!(report.state(), EnrichmentState::NotApplicable);
    }

    #[test]
    fn test_storage_discrepancy_with_mixed_results() {
        // BUG FIX REGRESSION TEST: persisted_count must be compared against
        // attempted_persist_count (enriched + failed), NOT just enriched_count.
        // We persist metadata for ALL results, not just successes.
        let mut builder = ReportBuilder::new("repo-1".to_string(), "snap-1".to_string());

        // 6 successes + 4 failures = 10 total attempted
        for _ in 0..6 {
            builder.record_eligible(EnrichmentLanguage::TypeScript);
            builder.record_success(EnrichmentLanguage::TypeScript, "MyClass", false);
        }
        for _ in 0..4 {
            builder.record_eligible(EnrichmentLanguage::TypeScript);
            builder.record_failure(EnrichmentLanguage::TypeScript, "type is any");
        }

        // Storage persisted all 10 rows
        builder.set_persisted_count(10);

        let report = builder.build();

        assert_eq!(report.enriched_count, 6);
        assert_eq!(report.failed_count, 4);
        assert_eq!(report.attempted_persist_count(), 10);
        assert_eq!(report.persisted_count, Some(10));

        // No discrepancy - we attempted 10, persisted 10
        assert!(!report.has_storage_discrepancy());
    }

    #[test]
    fn test_storage_discrepancy_detected() {
        let mut builder = ReportBuilder::new("repo-1".to_string(), "snap-1".to_string());

        // 6 successes + 4 failures = 10 total attempted
        for _ in 0..6 {
            builder.record_eligible(EnrichmentLanguage::TypeScript);
            builder.record_success(EnrichmentLanguage::TypeScript, "MyClass", false);
        }
        for _ in 0..4 {
            builder.record_eligible(EnrichmentLanguage::TypeScript);
            builder.record_failure(EnrichmentLanguage::TypeScript, "type is any");
        }

        // Storage only persisted 8 rows (2 failed to persist)
        builder.set_persisted_count(8);

        let report = builder.build();

        assert_eq!(report.attempted_persist_count(), 10);
        assert_eq!(report.persisted_count, Some(8));

        // Discrepancy detected - we attempted 10, only persisted 8
        assert!(report.has_storage_discrepancy());
    }
}
