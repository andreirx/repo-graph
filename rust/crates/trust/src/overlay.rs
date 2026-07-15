//! Trust overlay for query surfaces.
//!
//! Lightweight trust summary embedded in query responses. This is
//! NOT a replacement for `rmap trust` — it is a projection of
//! repo/snapshot-level evidence quality into inline context so
//! agents do not need a separate trust call.
//!
//! ## Design principles
//!
//! - `summary_scope: "repo_snapshot"` labels the trust as repo-level context
//! - Reliability axes are included for agent decision-making
//! - Degradation flags are flattened for easy consumption
//! - Caveats are included when reliability is degraded
//! - Per-result markers are OPTIONAL and only present when degraded

use serde::{Deserialize, Serialize};

use crate::types::{EnrichmentTopType, ReliabilityLevel, TrustReliability, TrustReport};

// ── Reader-frame call-coverage facts (RELIABILITY-REFRAME-1) ─────

/// The call-coverage facts the reader-frame projection needs, carried additively
/// on the trust overlay so `orient` (and any query surface that embeds the
/// overlay) can build the SAME `repo_graph_agent::reliability::CallReliabilityView`
/// as `trust` and `check` — the ratified "one shared projection reaching every
/// surface" (RR1_BOUNDARY option A). These are a faithful subset of the full
/// `TrustSummary`/`ResolutionCounts` the `trust` command already exposes; carrying
/// the COUNTS (not a pre-computed rate) is deliberate — every surface derives the
/// in-scope rate + external share the same way, so no per-surface bespoke number
/// can drift (the MODULE-MODEL lesson).
///
/// Round-trippable (Serialize + Deserialize) so the CLI reuses this exact type
/// rather than mirroring it — the named-receiver shape is `EnrichmentTopType`, the
/// same wire shape `trust`'s Likely-External Receiver Calls section uses, so there
/// is ONE named-receiver shape across surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallCoverage {
    /// Resolved CALLS edges.
    pub resolved_calls: u64,
    /// Unresolved CALLS-family edges (all, resolved or not → the external-share denominator with `resolved_calls`).
    pub unresolved_calls: u64,
    /// Unresolved CALLS classified into external libraries (the Variant-A exclusion).
    pub unresolved_calls_external: u64,
    /// The in-scope-rate denominator with `resolved_calls`. RELIABILITY-REFRAME-1
    /// (review-3 §2): "in-scope OR UNCLASSIFIED" — external-EXCLUDED, so it includes
    /// `unknown` classifications (NOT known-internal); `unresolved_calls_unknown`
    /// below is its unclassified portion.
    pub unresolved_calls_internal_like: u64,
    /// The UNCLASSIFIED (`unknown`) portion of `unresolved_calls_internal_like`
    /// (review-3 §2) — lets `orient` fire the conservative-rate caveat from the SAME
    /// shared helper `trust`/`check` use. Additive; `#[serde(default)]` for older wire.
    #[serde(default)]
    pub unresolved_calls_unknown: u64,
    /// Top named external receiver targets — the trust service's `top_external_types`
    /// (external-FILTERED then truncated, review-3 §3), NOT `top_types` re-filtered, so a
    /// genuine top external is never dropped by the mixed top-15 cut. Absent when
    /// enrichment surfaced no external receivers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_targets: Vec<EnrichmentTopType>,
}

// ── Top-level trust overlay for query surfaces ───────────────────

/// Lightweight trust summary for query surface envelopes.
///
/// Embedded in `callers`, `callees`, `path`, `dead` responses.
/// This is repo/snapshot-level context, NOT per-result assessment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrustOverlaySummary {
    /// Always "repo_snapshot" — labels this as repo-level, not per-result.
    pub summary_scope: &'static str,

    /// What the graph is built from (e.g., "CALLS+IMPORTS").
    pub graph_basis: String,

    /// Reliability assessment on four axes.
    pub reliability: TrustReliability,

    /// Flattened list of triggered degradation flags.
    /// Empty if no degradations triggered.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub degradation_flags: Vec<String>,

    /// Caveats for non-HIGH reliability axes.
    /// Empty if all axes are HIGH.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,

    /// RELIABILITY-REFRAME-1: the reader-frame call-coverage facts, so `orient`
    /// builds the SAME shared projection as `trust`/`check` (in-scope rate +
    /// external share + named coverage map) rather than a bespoke per-surface
    /// number. Additive; absent from the wire when the overlay predates this
    /// field (`#[serde(default)]` on the reader).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_coverage: Option<CallCoverage>,
}

impl TrustOverlaySummary {
    /// Project a lightweight overlay from a full TrustReport.
    pub fn from_report(report: &TrustReport, graph_basis: &str) -> Self {
        let mut degradation_flags = Vec::new();

        if report
            .summary
            .triggered_downgrades
            .framework_heavy_suspicion
            .triggered
        {
            degradation_flags.push("framework_heavy_suspicion".to_string());
        }
        if report
            .summary
            .triggered_downgrades
            .registry_pattern_suspicion
            .triggered
        {
            degradation_flags.push("registry_pattern_suspicion".to_string());
        }
        if report
            .summary
            .triggered_downgrades
            .missing_entrypoint_declarations
            .triggered
        {
            degradation_flags.push("missing_entrypoint_declarations".to_string());
        }
        if report
            .summary
            .triggered_downgrades
            .alias_resolution_suspicion
            .triggered
        {
            degradation_flags.push("alias_resolution_suspicion".to_string());
        }

        // Filter caveats to only include the informational ones
        // (skip the permanent cycle caveat which is always present).
        let caveats: Vec<String> = report
            .caveats
            .iter()
            .filter(|c| !c.contains("Cycle payloads"))
            .cloned()
            .collect();

        // RELIABILITY-REFRAME-1: project the reader-frame call-coverage facts from the
        // SAME `report` (no new read). The named external targets are the trust service's
        // `top_external_types` (external-FILTERED then truncated, review-3 §3) — identical to
        // the trust command's Likely-External section source — so orient's named coverage map
        // cannot drift from trust's, AND a genuine top external is never dropped by the mixed
        // top-15 cut.
        let external_targets: Vec<EnrichmentTopType> = report
            .enrichment_status
            .as_ref()
            .map(|s| s.top_external_types.clone())
            .unwrap_or_default();
        let call_coverage = Some(CallCoverage {
            resolved_calls: report.summary.resolved_calls,
            unresolved_calls: report.summary.unresolved_calls,
            unresolved_calls_external: report.summary.unresolved_calls_external,
            unresolved_calls_internal_like: report.summary.unresolved_calls_internal_like,
            unresolved_calls_unknown: report.unresolved_calls_unknown,
            external_targets,
        });

        Self {
            summary_scope: "repo_snapshot",
            graph_basis: graph_basis.to_string(),
            reliability: report.summary.reliability.clone(),
            degradation_flags,
            caveats,
            call_coverage,
        }
    }

    /// Check if any reliability axis is degraded (not HIGH).
    pub fn has_degradation(&self) -> bool {
        self.reliability.call_graph.level != ReliabilityLevel::HIGH
            || self.reliability.dead_code.level != ReliabilityLevel::HIGH
            || self.reliability.import_graph.level != ReliabilityLevel::HIGH
            || self.reliability.change_impact.level != ReliabilityLevel::HIGH
    }
}

// ── Per-result trust markers ─────────────────────────────────────
//
// Dead-confidence stratification: explicit per-candidate confidence
// tiers surfaced in `rmap dead` output. This is evidence-tiering under
// current repo trust, NOT proof of symbol-local liveness.
//
// The confidence reflects what the graph can say about the candidate
// given the repo's overall trust profile. LOW confidence means "don't
// trust this deadness claim strongly" due to framework/registry/plugin
// opacity or high unresolved pressure — not "we traced the symbol and
// found it's alive."

/// Confidence tier for per-result trust markers.
///
/// Three tiers only (v1). Do not overfit.
/// - HIGH: structurally dead with no significant counter-signal
/// - MEDIUM: orphaned but repo has some unresolved pressure
/// - LOW: known opacity pattern or strong liveness uncertainty
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResultConfidence {
    High,
    Medium,
    Low,
}

/// Per-result trust marker for dead-code candidates.
///
/// Every dead result carries this marker (no Option A hiding for dead).
/// Dead-code is destructive-action-adjacent; agents should never infer
/// "missing means high confidence."
///
/// The reasons are stable vocabulary:
/// - `framework_opaque`
/// - `registry_pattern_suspicion`
/// - `missing_entrypoint_declarations`
/// - `unresolved_call_pressure`
/// - `unresolved_import_pressure`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadResultTrust {
    /// Confidence tier: HIGH, MEDIUM, or LOW.
    pub dead_confidence: ResultConfidence,
    /// Reasons for degraded confidence. Empty when HIGH.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// Per-result trust marker for callers/callees edges.
///
/// Only included when the edge has degraded confidence.
/// Absent marker = exact resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(dead_code)] // Reserved for future per-edge trust markers
pub(crate) struct EdgeResultTrust {
    /// Confidence level for this edge.
    pub edge_confidence: ResultConfidence,
    /// How the edge was resolved (e.g., "exact", "promotion", "heuristic").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
    /// Reasons for degraded confidence.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

// ── Dead-code confidence assessment ──────────────────────────────

/// Assess confidence for a dead-code candidate based on repo-level
/// trust signals.
///
/// **Important:** This is evidence-tiering under current repo trust,
/// NOT proof of symbol-local liveness. The confidence reflects what
/// the graph can say about this candidate given the repo's trust
/// profile.
///
/// For dead-code, we always return a marker (no Option A hiding).
/// Dead-code is destructive-action-adjacent — agents should never
/// have to infer "missing means high confidence."
///
/// The `_symbol_stable_key` parameter is reserved for future
/// symbol-local evidence (e.g., known plugin registration patterns).
/// Currently unused because v1 confidence is repo-level.
pub fn assess_dead_confidence(
    trust_report: &TrustReport,
    _symbol_stable_key: &str,
) -> DeadResultTrust {
    let dead_code_level = trust_report.summary.reliability.dead_code.level;
    let mut reasons = Vec::new();

    // ── Collect reasons from triggered downgrades ────────────────
    //
    // These are explicit "don't trust dead strongly here" signals.

    if trust_report
        .summary
        .triggered_downgrades
        .framework_heavy_suspicion
        .triggered
    {
        reasons.push("framework_opaque".to_string());
    }

    if trust_report
        .summary
        .triggered_downgrades
        .registry_pattern_suspicion
        .triggered
    {
        reasons.push("registry_pattern_suspicion".to_string());
    }

    if trust_report
        .summary
        .triggered_downgrades
        .missing_entrypoint_declarations
        .triggered
    {
        reasons.push("missing_entrypoint_declarations".to_string());
    }

    // ── Check graph reliability for unresolved pressure ──────────
    //
    // Unresolved pressure weakens confidence that zero-caller means
    // actually-dead. The caller might exist but be unresolved.

    if trust_report.summary.reliability.call_graph.level != ReliabilityLevel::HIGH {
        reasons.push("unresolved_call_pressure".to_string());
    }

    if trust_report.summary.reliability.import_graph.level != ReliabilityLevel::HIGH {
        reasons.push("unresolved_import_pressure".to_string());
    }

    // ── Map reliability level to confidence tier ─────────────────
    //
    // Conservative v1 reduction:
    // - dead_code LOW → candidate LOW
    // - dead_code MEDIUM → candidate MEDIUM
    // - dead_code HIGH → candidate HIGH (unless degradation reasons)

    let confidence = match dead_code_level {
        ReliabilityLevel::LOW => ResultConfidence::Low,
        ReliabilityLevel::MEDIUM => ResultConfidence::Medium,
        ReliabilityLevel::HIGH => {
            // Even if dead_code axis is HIGH, framework/registry/entrypoint
            // degradation reasons still lower the candidate confidence.
            if reasons.iter().any(|r| {
                r == "framework_opaque"
                    || r == "registry_pattern_suspicion"
                    || r == "missing_entrypoint_declarations"
            }) {
                ResultConfidence::Low
            } else if !reasons.is_empty() {
                // Unresolved pressure only → MEDIUM
                ResultConfidence::Medium
            } else {
                ResultConfidence::High
            }
        }
    };

    DeadResultTrust {
        dead_confidence: confidence,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        DowngradeTrigger, ReliabilityAxisScore, TrustDowngrades, TrustReliability, TrustReport,
        TrustSummary,
    };

    fn minimal_report() -> TrustReport {
        TrustReport {
            snapshot_uid: "snap1".into(),
            display_name: None,
            basis_commit: None,
            toolchain: None,
            diagnostics_version: None,
            summary: TrustSummary {
                edges_total: 100,
                edges_resolved: 100,
                unresolved_total: 0,
                resolved_calls: 50,
                unresolved_calls: 0,
                unresolved_calls_external: 0,
                unresolved_calls_internal_like: 0,
                call_resolution_rate: 1.0,
                reliability: TrustReliability {
                    import_graph: ReliabilityAxisScore {
                        level: ReliabilityLevel::HIGH,
                        reasons: vec![],
                    },
                    call_graph: ReliabilityAxisScore {
                        level: ReliabilityLevel::HIGH,
                        reasons: vec![],
                    },
                    dead_code: ReliabilityAxisScore {
                        level: ReliabilityLevel::HIGH,
                        reasons: vec![],
                    },
                    change_impact: ReliabilityAxisScore {
                        level: ReliabilityLevel::HIGH,
                        reasons: vec![],
                    },
                },
                triggered_downgrades: TrustDowngrades {
                    framework_heavy_suspicion: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                    registry_pattern_suspicion: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                    missing_entrypoint_declarations: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                    alias_resolution_suspicion: DowngradeTrigger {
                        triggered: false,
                        reasons: vec![],
                    },
                },
            },
            categories: vec![],
            classifications: vec![],
            basis_classifications: vec![],
            external_dependencies: Default::default(),
            unknown_calls_blast_radius: None,
            enrichment_status: None,
            modules: vec![],
            caveats: vec![],
            diagnostics_available: true,
            enrichment_eligible_count: 0,
            unresolved_calls_unknown: 0,
        }
    }

    #[test]
    fn overlay_from_report_with_no_degradation() {
        let report = minimal_report();
        let overlay = TrustOverlaySummary::from_report(&report, "CALLS+IMPORTS");

        assert_eq!(overlay.summary_scope, "repo_snapshot");
        assert_eq!(overlay.graph_basis, "CALLS+IMPORTS");
        assert!(overlay.degradation_flags.is_empty());
        assert!(overlay.caveats.is_empty());
        assert!(!overlay.has_degradation());
    }

    #[test]
    fn overlay_from_report_with_framework_heavy() {
        let mut report = minimal_report();
        report
            .summary
            .triggered_downgrades
            .framework_heavy_suspicion
            .triggered = true;
        report.summary.reliability.dead_code.level = ReliabilityLevel::LOW;

        let overlay = TrustOverlaySummary::from_report(&report, "CALLS");

        assert!(overlay
            .degradation_flags
            .contains(&"framework_heavy_suspicion".to_string()));
        assert!(overlay.has_degradation());
    }

    #[test]
    fn dead_confidence_returns_high_when_clean() {
        let report = minimal_report();
        let result = assess_dead_confidence(&report, "some::symbol");

        assert_eq!(result.dead_confidence, ResultConfidence::High);
        assert!(result.reasons.is_empty());
    }

    #[test]
    fn dead_confidence_returns_low_with_framework_suspicion() {
        let mut report = minimal_report();
        report
            .summary
            .triggered_downgrades
            .framework_heavy_suspicion
            .triggered = true;
        report.summary.reliability.dead_code.level = ReliabilityLevel::LOW;

        let result = assess_dead_confidence(&report, "some::symbol");

        assert_eq!(result.dead_confidence, ResultConfidence::Low);
        assert!(result.reasons.contains(&"framework_opaque".to_string()));
    }

    #[test]
    fn dead_confidence_returns_low_with_registry_suspicion() {
        let mut report = minimal_report();
        report
            .summary
            .triggered_downgrades
            .registry_pattern_suspicion
            .triggered = true;
        report.summary.reliability.dead_code.level = ReliabilityLevel::LOW;

        let result = assess_dead_confidence(&report, "some::symbol");

        assert_eq!(result.dead_confidence, ResultConfidence::Low);
        assert!(result
            .reasons
            .contains(&"registry_pattern_suspicion".to_string()));
    }

    #[test]
    fn dead_confidence_returns_low_with_missing_entrypoints() {
        let mut report = minimal_report();
        report
            .summary
            .triggered_downgrades
            .missing_entrypoint_declarations
            .triggered = true;
        report.summary.reliability.dead_code.level = ReliabilityLevel::LOW;

        let result = assess_dead_confidence(&report, "some::symbol");

        assert_eq!(result.dead_confidence, ResultConfidence::Low);
        assert!(result
            .reasons
            .contains(&"missing_entrypoint_declarations".to_string()));
    }

    #[test]
    fn dead_confidence_returns_medium_with_unresolved_pressure() {
        let mut report = minimal_report();
        report.summary.reliability.call_graph.level = ReliabilityLevel::MEDIUM;
        // dead_code is still HIGH but call_graph is degraded

        let result = assess_dead_confidence(&report, "some::symbol");

        assert_eq!(result.dead_confidence, ResultConfidence::Medium);
        assert!(result
            .reasons
            .contains(&"unresolved_call_pressure".to_string()));
    }

    #[test]
    fn dead_confidence_includes_import_pressure_reason() {
        let mut report = minimal_report();
        report.summary.reliability.import_graph.level = ReliabilityLevel::MEDIUM;

        let result = assess_dead_confidence(&report, "some::symbol");

        assert!(result
            .reasons
            .contains(&"unresolved_import_pressure".to_string()));
    }

    #[test]
    fn dead_confidence_framework_suspicion_forces_low_even_with_high_dead_code_axis() {
        let mut report = minimal_report();
        // dead_code axis is HIGH but framework suspicion is triggered
        report
            .summary
            .triggered_downgrades
            .framework_heavy_suspicion
            .triggered = true;

        let result = assess_dead_confidence(&report, "some::symbol");

        // Should be LOW because framework suspicion overrides
        assert_eq!(result.dead_confidence, ResultConfidence::Low);
    }

    #[test]
    fn overlay_filters_permanent_cycle_caveat() {
        let mut report = minimal_report();
        report.caveats = vec![
            "Cycle payloads currently emit leaf module names only".to_string(),
            "Your code's calls resolve at LOW reliability on this repo.".to_string(),
        ];

        let overlay = TrustOverlaySummary::from_report(&report, "CALLS");

        // Permanent cycle caveat should be filtered out
        assert_eq!(overlay.caveats.len(), 1);
        assert!(overlay.caveats[0].contains("Your code's calls"));
    }

    #[test]
    fn result_confidence_serializes_screaming_snake() {
        assert_eq!(
            serde_json::to_string(&ResultConfidence::High).unwrap(),
            "\"HIGH\""
        );
        assert_eq!(
            serde_json::to_string(&ResultConfidence::Medium).unwrap(),
            "\"MEDIUM\""
        );
        assert_eq!(
            serde_json::to_string(&ResultConfidence::Low).unwrap(),
            "\"LOW\""
        );
    }
}
