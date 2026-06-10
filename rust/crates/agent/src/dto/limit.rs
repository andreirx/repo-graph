//! Enumerated limit codes surfaced by the agent orientation
//! surface.
//!
//! A limit record appears in the output when the use case knows
//! it cannot compute something a signal code would normally
//! cover. Every limit code carries a stable identifier and a
//! human-readable summary. The `summary` field is convenience;
//! the `code` field is the contract.
//!
//! Rules for adding a new limit code:
//!
//!   1. Every limit code is an explicit variant of `LimitCode`.
//!      No free-form strings.
//!   2. Every limit code has a stable wire-format string in
//!      `as_str()`. The string is the only stable surface.
//!   3. Limits are NOT a dumping ground for debug output. A
//!      limit code means "a specific capability is unavailable
//!      in this response". It is a product statement, not a log
//!      line.
//!
//! Rust-42 scope: only the limit codes the repo-level orient
//! pipeline can actually emit are listed here. The contract
//! reserves additional codes (e.g. `IMPORTS_ONE_HOP` for the
//! `imports` surface) for future commands.
//!
//! ACR-6 addition: `DegradationInfo` provides structured context
//! for limits that represent capability gaps or data staleness.
//! Not all limits carry degradation info — only those where the
//! limit represents a known architectural gap (unsupported on
//! embodiment) or data quality issue (stale/missing).

use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitCode {
    /// The repo has no active requirement declarations, so the
    /// gate pipeline has nothing to evaluate. This is an
    /// absence-of-configured-policy state, NOT a gate result.
    /// It does not become a `GATE_PASS` (which would imply
    /// obligations existed and all passed). It is a limit so
    /// the agent can distinguish "gate not configured" from
    /// "gate configured and passing".
    ///
    /// Replaces the `GATE_UNAVAILABLE` code used during Rust-42,
    /// which existed only because gate policy was trapped in the
    /// `rgr` binary crate and could not be called from the agent
    /// layer. After Rust-43A relocated gate into
    /// `repo-graph-gate`, gate evaluation IS available — the
    /// relevant "unavailable" state shifted from tooling to
    /// policy configuration.
    GateNotConfigured,

    /// Module discovery data (Layer-1 discovered modules catalog)
    /// is not yet queryable through the Rust storage path. The
    /// repo-level `MODULE_SUMMARY` signal falls back to raw
    /// snapshot totals instead of discovered module counts.
    ModuleDataUnavailable,

    /// This snapshot has no cyclomatic complexity measurements.
    /// `HIGH_COMPLEXITY` cannot be evaluated without measurement
    /// data. The absence may be due to indexer limitations or
    /// repository content.
    ComplexityUnavailable,

    /// Indexed languages may not cover the full repository. The
    /// Rust indexer supports a narrower set of languages than the
    /// file tree may contain. Files in unsupported languages are
    /// not reflected in `MODULE_SUMMARY` counts.
    LanguageCoveragePartial,

    // DeadCodeUnreliable — removed. Surface withdrawn.
    // See docs/TECH-DEBT.md for reintroduction conditions.
    /// Gate requirements exist but none of their obligations
    /// target the focused path area. The gate pipeline has
    /// nothing to evaluate within this scope. Distinct from
    /// `GateNotConfigured` (which means no requirements exist
    /// at all) and from gate signals (which evaluate specific
    /// obligations).
    GateNotApplicableToFocus,

    // ── Coherence ENVELOPE-level provenance-derived codes ─────────
    //
    // ORIENT-LIVEGRAPH-IMPL (COHERENCE-LAYER-1 contract :458; orient slice §ENVELOPE limits[] :546). These
    // make the coherence degradation/provenance MACHINE-DISCOVERABLE at the envelope level, not only inside
    // the per-leaf trust postures. They are DERIVED purely from the folded leaf provenance/freshness +
    // the snapshot stale flag (`coherent::append_provenance_limits`) — emitted WHEN AND ONLY WHEN the
    // matching condition occurred (validation E5). They are ADDITIVE: the pre-existing degradation limits
    // above (GateNotConfigured / ModuleDataUnavailable / ComplexityUnavailable) are orthogonal
    // known-zero/unavailable markers and are RETAINED unchanged (contract :549).
    /// A LiveGraph-first leaf answered but was NOT `Exact` (e.g. a non-resident contributing partition),
    /// so it fell back to the proven SQLite primary (`fallback_reason = LiveGraphPartial`). The answer is
    /// complete (SQLite is the source of truth); this marks that the LiveGraph acceleration was declined
    /// for partiality.
    LivegraphPartial,

    /// The backing SQLite index is STALE (`get_stale_files` non-empty) — the SQLite / Authority / FS leaves
    /// are snapshot-`Stale`, so the answer reflects a superseded index epoch. The SAME staleness the
    /// `TRUST_STALE_SNAPSHOT` signal reports, surfaced as an envelope-level provenance code.
    SqliteSnapshotStale,

    /// A user-authored Tier-A1 authority declaration (boundary / gate) contributed to this answer. Authority
    /// OVERLAYS a computed structural fact; it never erases it (D-ORIENT-5 / contract D5). This marks that
    /// the effective view is authority-shaped, so a consumer knows to reconcile against the computed view.
    AuthorityOverlayApplied,

    /// A LiveGraph-first leaf was served (or would serve) under an in-flight SCIP refresh — its freshness is
    /// `PrecisionPending`. The MEET caps the root below `Exact`; this surfaces the precision-pending epoch at
    /// the envelope level.
    PrecisionPending,

    /// A LiveGraph-first leaf found NO LiveGraph available for the repo/target (not preloaded/refreshed, or
    /// `Unavailable`) and fell back to the proven SQLite primary (`fallback_reason = LiveGraphUnavailable`).
    /// The LiveGraph producer never built a current-state graph for this answer.
    ProducerUnavailable,
}

impl LimitCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GateNotConfigured => "GATE_NOT_CONFIGURED",
            Self::ModuleDataUnavailable => "MODULE_DATA_UNAVAILABLE",
            Self::ComplexityUnavailable => "COMPLEXITY_UNAVAILABLE",
            Self::LanguageCoveragePartial => "LANGUAGE_COVERAGE_PARTIAL",
            Self::GateNotApplicableToFocus => "GATE_NOT_APPLICABLE_TO_FOCUS",
            Self::LivegraphPartial => "LIVEGRAPH_PARTIAL",
            Self::SqliteSnapshotStale => "SQLITE_SNAPSHOT_STALE",
            Self::AuthorityOverlayApplied => "AUTHORITY_OVERLAY_APPLIED",
            Self::PrecisionPending => "PRECISION_PENDING",
            Self::ProducerUnavailable => "PRODUCER_UNAVAILABLE",
        }
    }

    /// Canonical summary string that accompanies this code in
    /// the output envelope. Stable wording — changing this is a
    /// contract change.
    pub fn summary(self) -> &'static str {
        match self {
            Self::GateNotConfigured => {
                "No active requirement declarations. Gate has no \
				 obligations to evaluate."
            }
            Self::ModuleDataUnavailable => {
                "Module discovery data is not queryable through the Rust \
				 storage path. Repo-level counts fall back to raw \
				 snapshot totals."
            }
            Self::ComplexityUnavailable => {
                "No cyclomatic complexity measurements available for this \
				 snapshot. HIGH_COMPLEXITY cannot be evaluated."
            }
            Self::LanguageCoveragePartial => {
                "Indexed languages may not cover the full repository. \
				 Files in languages the indexer does not support are \
				 not reflected in MODULE_SUMMARY."
            }
            Self::GateNotApplicableToFocus => {
                "Gate is configured but no obligations target the \
				 focused area."
            }
            Self::LivegraphPartial => {
                "A LiveGraph-first signal was not Exact (a non-resident \
				 contributing partition) and fell back to the SQLite \
				 primary."
            }
            Self::SqliteSnapshotStale => {
                "The backing index is stale (some files changed since the \
				 snapshot). SQLite-sourced signals reflect a superseded \
				 index epoch."
            }
            Self::AuthorityOverlayApplied => {
                "A user-authored authority declaration (boundary/gate) \
				 contributed to this answer. Authority overlays a computed \
				 fact; it never erases it."
            }
            Self::PrecisionPending => {
                "A LiveGraph-first signal is served under an in-flight \
				 SCIP refresh (PrecisionPending), so overall confidence is \
				 capped below Exact."
            }
            Self::ProducerUnavailable => {
                "No current-state LiveGraph was available for this answer \
				 (not preloaded/refreshed); LiveGraph-first signals fell \
				 back to the SQLite primary."
            }
        }
    }
}

impl Serialize for LimitCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

// ── Degradation (ACR-6) ──────────────────────────────────────────
//
// Degradation info provides structured context for limits that
// represent capability gaps or data quality issues. The status
// vocabulary maps from artifact-contracts::DegradationPolicy
// semantics but is surfaced as agent-facing DTO.

/// Degradation status for a limit.
///
/// Indicates why a capability is unavailable or degraded.
/// Maps from `artifact_contracts::DegradationPolicy` but uses
/// agent-facing vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationStatus {
    /// Feature is not supported on this indexer embodiment.
    ///
    /// Not a bug — a capability gap. The Rust indexer may not
    /// populate certain tables that the TS prototype did.
    Unsupported,

    /// Required data is missing when it should be present.
    ///
    /// Source files exist but expected artifacts were not extracted.
    Missing,

    /// Data exists but is partially stale/impacted.
    ///
    /// Some backing rows have freshness_state != 'current'.
    PartiallyStale,
}

impl DegradationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Missing => "missing",
            Self::PartiallyStale => "partially_stale",
        }
    }
}

impl Serialize for DegradationStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Structured degradation context for a limit.
///
/// Attached to limits where the unavailability has a known
/// architectural or data-quality cause. Not all limits carry
/// degradation info — only those with explicit backing reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DegradationInfo {
    /// Why the capability is degraded.
    pub status: DegradationStatus,

    /// The artifact family affected (e.g., "ModuleCandidates", "ProjectSurfaces").
    pub family: String,

    /// Human-readable explanation.
    pub reason: String,

    /// Suggested action to resolve (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
}

impl DegradationInfo {
    /// Create degradation info for an unsupported feature.
    pub fn unsupported(family: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            status: DegradationStatus::Unsupported,
            family: family.into(),
            reason: reason.into(),
            recommendation: None,
        }
    }

    /// Create degradation info for an unsupported feature with recommendation.
    pub fn unsupported_with_recommendation(
        family: impl Into<String>,
        reason: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self {
            status: DegradationStatus::Unsupported,
            family: family.into(),
            reason: reason.into(),
            recommendation: Some(recommendation.into()),
        }
    }

    /// Create degradation info for missing data.
    pub fn missing(family: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            status: DegradationStatus::Missing,
            family: family.into(),
            reason: reason.into(),
            recommendation: None,
        }
    }

    /// Create degradation info for partially stale data.
    pub fn partially_stale(family: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            status: DegradationStatus::PartiallyStale,
            family: family.into(),
            reason: reason.into(),
            recommendation: Some("consider refreshing the repository".into()),
        }
    }
}

/// One limit record in the output envelope.
///
/// Shape rules:
///
///   - `code` is a stable enumerated identifier.
///   - `summary` is looked up from the code — callers cannot
///     supply their own text. This keeps the per-code wording
///     as a single-site contract.
///   - `reasons` is a free-form list of human-readable strings
///     describing WHY the limit fired. Most limits have no
///     reasons and serialize without the field. Limits
///     triggered by an upstream policy layer carry the reasons
///     through to the output envelope so an agent can display
///     or match on them.
///   - `degradation` (ACR-6) is structured context for limits
///     that represent capability gaps or data quality issues.
///     Most limits do not carry degradation info.
///
/// The `reasons` and `degradation` fields are skipped during
/// serialization when empty/None, preserving backward compat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Limit {
    pub code: LimitCode,
    pub summary: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation: Option<DegradationInfo>,
}

impl Limit {
    /// Construct a limit record from a code. The summary is
    /// looked up by the code — callers cannot supply their own
    /// summary string, which is how the contract stays stable.
    ///
    /// No reasons or degradation attached.
    pub fn from_code(code: LimitCode) -> Self {
        Self {
            code,
            summary: code.summary(),
            reasons: Vec::new(),
            degradation: None,
        }
    }

    /// Construct a limit record from a code with an attached
    /// reasons list. Reasons are passed through verbatim — the
    /// caller is responsible for the vocabulary.
    pub fn from_code_with_reasons(code: LimitCode, reasons: Vec<String>) -> Self {
        Self {
            code,
            summary: code.summary(),
            reasons,
            degradation: None,
        }
    }

    /// Construct a limit record with degradation info (ACR-6).
    ///
    /// Use for limits that represent capability gaps or data
    /// quality issues where structured context is valuable.
    pub fn from_code_with_degradation(code: LimitCode, degradation: DegradationInfo) -> Self {
        Self {
            code,
            summary: code.summary(),
            reasons: Vec::new(),
            degradation: Some(degradation),
        }
    }

    /// Construct a limit record with both reasons and degradation.
    pub fn from_code_with_reasons_and_degradation(
        code: LimitCode,
        reasons: Vec<String>,
        degradation: DegradationInfo,
    ) -> Self {
        Self {
            code,
            summary: code.summary(),
            reasons,
            degradation: Some(degradation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_code_serializes_as_screaming_snake() {
        let s = serde_json::to_string(&LimitCode::GateNotConfigured).unwrap();
        assert_eq!(s, "\"GATE_NOT_CONFIGURED\"");
    }

    #[test]
    fn limit_carries_canonical_summary() {
        let l = Limit::from_code(LimitCode::GateNotConfigured);
        assert_eq!(l.code, LimitCode::GateNotConfigured);
        assert!(l.summary.contains("requirement declarations"));
    }

    #[test]
    fn limit_serializes_with_code_and_summary() {
        let l = Limit::from_code(LimitCode::ComplexityUnavailable);
        let s = serde_json::to_string(&l).unwrap();
        assert!(s.contains("\"code\":\"COMPLEXITY_UNAVAILABLE\""));
        assert!(s.contains("\"summary\":"));
    }

    // ── Degradation tests (ACR-6) ────────────────────────────────

    #[test]
    fn degradation_status_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&DegradationStatus::Unsupported).unwrap(),
            "\"unsupported\""
        );
        assert_eq!(
            serde_json::to_string(&DegradationStatus::Missing).unwrap(),
            "\"missing\""
        );
        assert_eq!(
            serde_json::to_string(&DegradationStatus::PartiallyStale).unwrap(),
            "\"partially_stale\""
        );
    }

    #[test]
    fn degradation_info_unsupported_constructor() {
        let info =
            DegradationInfo::unsupported("ModuleCandidates", "not populated on Rust indexer path");
        assert_eq!(info.status, DegradationStatus::Unsupported);
        assert_eq!(info.family, "ModuleCandidates");
        assert!(info.reason.contains("Rust indexer"));
        assert!(info.recommendation.is_none());
    }

    #[test]
    fn degradation_info_with_recommendation() {
        let info = DegradationInfo::unsupported_with_recommendation(
            "ProjectSurfaces",
            "requires module_candidates",
            "use TypeScript indexer",
        );
        assert_eq!(info.status, DegradationStatus::Unsupported);
        assert_eq!(info.recommendation, Some("use TypeScript indexer".into()));
    }

    #[test]
    fn limit_with_degradation_serializes_correctly() {
        let degradation =
            DegradationInfo::unsupported("ModuleCandidates", "not supported on this embodiment");
        let l = Limit::from_code_with_degradation(LimitCode::ModuleDataUnavailable, degradation);
        let s = serde_json::to_string(&l).unwrap();

        assert!(s.contains("\"code\":\"MODULE_DATA_UNAVAILABLE\""));
        assert!(s.contains("\"degradation\":{"));
        assert!(s.contains("\"status\":\"unsupported\""));
        assert!(s.contains("\"family\":\"ModuleCandidates\""));
    }

    #[test]
    fn limit_without_degradation_omits_field() {
        let l = Limit::from_code(LimitCode::GateNotConfigured);
        let s = serde_json::to_string(&l).unwrap();

        // Should NOT contain degradation field
        assert!(!s.contains("\"degradation\""));
    }

    #[test]
    fn degradation_info_serializes_without_null_recommendation() {
        let info = DegradationInfo::unsupported("Test", "reason");
        let s = serde_json::to_string(&info).unwrap();

        // recommendation is None, so should be omitted
        assert!(!s.contains("\"recommendation\""));
    }
}
