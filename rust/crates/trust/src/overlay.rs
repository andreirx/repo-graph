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

use serde::Serialize;

use crate::types::{ReliabilityLevel, TrustReport, TrustReliability};

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
}

impl TrustOverlaySummary {
    /// Project a lightweight overlay from a full TrustReport.
    pub fn from_report(report: &TrustReport, graph_basis: &str) -> Self {
        let mut degradation_flags = Vec::new();

        if report.summary.triggered_downgrades.framework_heavy_suspicion.triggered {
            degradation_flags.push("framework_heavy_suspicion".to_string());
        }
        if report.summary.triggered_downgrades.registry_pattern_suspicion.triggered {
            degradation_flags.push("registry_pattern_suspicion".to_string());
        }
        if report.summary.triggered_downgrades.missing_entrypoint_declarations.triggered {
            degradation_flags.push("missing_entrypoint_declarations".to_string());
        }
        if report.summary.triggered_downgrades.alias_resolution_suspicion.triggered {
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

        Self {
            summary_scope: "repo_snapshot",
            graph_basis: graph_basis.to_string(),
            reliability: report.summary.reliability.clone(),
            degradation_flags,
            caveats,
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
// These types are designed for per-result confidence markers but are
// not yet wired into CLI output. They remain pub(crate) until command
// contracts actually emit them.

/// Confidence tier for per-result trust markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ResultConfidence {
    High,
    Medium,
    Low,
}

/// Per-result trust marker for dead-code candidates.
///
/// Only included when confidence is not HIGH or when there are
/// specific reasons for concern. Absent marker = high confidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DeadResultTrust {
    pub confidence: ResultConfidence,
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
/// This is Option A: only produce markers when degraded. Returns
/// `None` when confidence is HIGH with no caveats.
///
/// Currently pub(crate) — not wired into CLI dead command output yet.
#[allow(dead_code)] // Reserved for future per-result trust markers
pub(crate) fn assess_dead_confidence(
    trust_report: &TrustReport,
    _symbol_stable_key: &str,
) -> Option<DeadResultTrust> {
    let dead_code_level = trust_report.summary.reliability.dead_code.level;
    let mut reasons = Vec::new();

    // Collect reasons from triggered downgrades
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

    // Check call graph reliability for unresolved pressure
    if trust_report.summary.reliability.call_graph.level != ReliabilityLevel::HIGH {
        reasons.push("unresolved_call_pressure".to_string());
    }

    // Map reliability level to confidence
    let confidence = match dead_code_level {
        ReliabilityLevel::HIGH => ResultConfidence::High,
        ReliabilityLevel::MEDIUM => ResultConfidence::Medium,
        ReliabilityLevel::LOW => ResultConfidence::Low,
    };

    // Option A: only return marker when degraded
    if confidence == ResultConfidence::High && reasons.is_empty() {
        return None;
    }

    Some(DeadResultTrust { confidence, reasons })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        DowngradeTrigger, ReliabilityAxisScore, TrustDowngrades, TrustReliability,
        TrustReport, TrustSummary,
    };

    fn minimal_report() -> TrustReport {
        TrustReport {
            snapshot_uid: "snap1".into(),
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
            unknown_calls_blast_radius: None,
            enrichment_status: None,
            modules: vec![],
            caveats: vec![],
            diagnostics_available: true,
            enrichment_eligible_count: 0,
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
        report.summary.triggered_downgrades.framework_heavy_suspicion.triggered = true;
        report.summary.reliability.dead_code.level = ReliabilityLevel::LOW;

        let overlay = TrustOverlaySummary::from_report(&report, "CALLS");

        assert!(overlay.degradation_flags.contains(&"framework_heavy_suspicion".to_string()));
        assert!(overlay.has_degradation());
    }

    #[test]
    fn dead_confidence_returns_none_when_high() {
        let report = minimal_report();
        let result = assess_dead_confidence(&report, "some::symbol");
        assert!(result.is_none());
    }

    #[test]
    fn dead_confidence_returns_marker_when_degraded() {
        let mut report = minimal_report();
        report.summary.triggered_downgrades.framework_heavy_suspicion.triggered = true;
        report.summary.reliability.dead_code.level = ReliabilityLevel::LOW;

        let result = assess_dead_confidence(&report, "some::symbol");
        assert!(result.is_some());

        let trust = result.unwrap();
        assert_eq!(trust.confidence, ResultConfidence::Low);
        assert!(trust.reasons.contains(&"framework_opaque".to_string()));
    }

    #[test]
    fn overlay_filters_permanent_cycle_caveat() {
        let mut report = minimal_report();
        report.caveats = vec![
            "Cycle payloads currently emit leaf module names only".to_string(),
            "Call-graph reliability is LOW".to_string(),
        ];

        let overlay = TrustOverlaySummary::from_report(&report, "CALLS");

        // Permanent cycle caveat should be filtered out
        assert_eq!(overlay.caveats.len(), 1);
        assert!(overlay.caveats[0].contains("Call-graph"));
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
