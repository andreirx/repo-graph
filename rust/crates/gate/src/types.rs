//! Gate-owned domain types.
//!
//! Every DTO that enters or leaves the gate crate is defined
//! here. The crate does NOT import storage row types from
//! `repo-graph-storage`; the storage adapter is responsible for
//! mapping its internal rows into these types at the port
//! boundary.
//!
//! Naming discipline:
//!
//!   - Input data (requirements, waivers, measurements,
//!     inferences, boundary declarations, import edges) uses
//!     the `Gate*` prefix and is plain Rust without serde
//!     derives beyond what the output shape needs. These types
//!     are not serialized — they are consumed by `compute`.
//!
//!   - Output data (`GateReport`, `ObligationEvaluation`,
//!     `GateCounts`, `Verdict`, `EffectiveVerdict`,
//!     `WaiverBasis`) derives `Serialize` because the CLI
//!     command prints the report as JSON.

use serde::Serialize;

// ── Input types ──────────────────────────────────────────────────

/// One active requirement declaration. Carries an ordered
/// list of verification obligations.
///
/// `Eq` is intentionally NOT derived here (or on
/// `GateObligation`) because obligations carry `Option<f64>`
/// for threshold comparison, and `f64` has no `Eq`
/// implementation. Callers that need equality comparison must
/// use `PartialEq` and accept the NaN semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct GateRequirement {
	pub req_id: String,
	pub version: i64,
	pub obligations: Vec<GateObligation>,
}

/// One verification obligation attached to a requirement.
///
/// `method` is the gate method name (`arch_violations`,
/// `coverage_threshold`, `complexity_threshold`,
/// `hotspot_threshold`, …). `target` is an optional path
/// filter used by most methods. `threshold` and `operator` are
/// numeric-comparison parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GateObligation {
	pub obligation_id: String,
	pub obligation: String,
	pub method: String,
	pub target: Option<String>,
	pub threshold: Option<f64>,
	pub operator: Option<String>,
}

/// One boundary declaration attached to a source module. The
/// declaration forbids imports from `boundary_module` into
/// `forbids`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateBoundaryDeclaration {
	pub boundary_module: String,
	pub forbids: String,
	pub reason: Option<String>,
}

/// One IMPORTS edge between file-path prefixes. Used as
/// evidence for the arch_violations gate method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateImportEdge {
	pub source_file: String,
	pub target_file: String,
}

/// One measurement row from the measurements table. Gate reads
/// `line_coverage` (coverage_threshold method) and
/// `cyclomatic_complexity` (complexity_threshold method).
///
/// `value_json` is a raw JSON object string that carries the
/// numeric measurement in a `value` field (and any auxiliary
/// fields the measurement producer emitted). The gate compute
/// layer parses this string; if it is malformed, the affected
/// obligation becomes MISSING_EVIDENCE. The gate crate does not
/// error out over a single bad row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateMeasurement {
	pub target_stable_key: String,
	pub value_json: String,
}

/// One inference row. Gate reads `hotspot_score` inferences
/// (hotspot_threshold method). `value_json` carries a
/// `normalized_score` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateInference {
	pub target_stable_key: String,
	pub value_json: String,
}

/// Evaluated module boundary violations evidence (RS-MG-8).
///
/// Pre-computed by the storage adapter using RS-MG-1 through RS-MG-4
/// support modules from the classification crate. The gate compute
/// layer receives only the summary counts needed for verdict
/// determination.
///
/// Note: `stale_declarations_count` is informational only. Stale
/// declarations do not cause FAIL — they indicate boundaries
/// referencing modules that no longer exist in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GateModuleViolationEvidence {
	/// Total count of discovered-module boundary violations.
	/// PASS if 0, FAIL if > 0.
	pub violations_count: usize,
	/// Count of stale boundary declarations (informational).
	pub stale_declarations_count: usize,
}

/// One active waiver matching a specific
/// `(req_id, req_version, obligation_id)` tuple. First-matching
/// waiver wins at the overlay step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateWaiver {
	pub waiver_uid: String,
	pub reason: String,
	pub created_at: String,
	pub created_by: Option<String>,
	pub expires_at: Option<String>,
	pub rationale_category: Option<String>,
	pub policy_basis: Option<String>,
}

// ── Output types ─────────────────────────────────────────────────

/// Four-state computed verdict (truth about the evaluation).
///
/// Variant names match the TS verdict strings exactly. Enum
/// names retain underscore form (TS parity) even though Rust
/// naming convention would be CamelCase — the on-the-wire
/// string output is what matters here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(non_camel_case_types)]
pub enum Verdict {
	PASS,
	FAIL,
	MISSING_EVIDENCE,
	UNSUPPORTED,
}

impl std::fmt::Display for Verdict {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::PASS => write!(f, "PASS"),
			Self::FAIL => write!(f, "FAIL"),
			Self::MISSING_EVIDENCE => write!(f, "MISSING_EVIDENCE"),
			Self::UNSUPPORTED => write!(f, "UNSUPPORTED"),
		}
	}
}

/// Five-state effective verdict used at the gate boundary.
///
/// WAIVED is ONLY an effective state — never a computed state.
/// Serializes to the same string values as Verdict, plus
/// "WAIVED".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(non_camel_case_types)]
pub enum EffectiveVerdict {
	PASS,
	FAIL,
	MISSING_EVIDENCE,
	UNSUPPORTED,
	WAIVED,
}

impl From<Verdict> for EffectiveVerdict {
	fn from(v: Verdict) -> Self {
		match v {
			Verdict::PASS => Self::PASS,
			Verdict::FAIL => Self::FAIL,
			Verdict::MISSING_EVIDENCE => Self::MISSING_EVIDENCE,
			Verdict::UNSUPPORTED => Self::UNSUPPORTED,
		}
	}
}

/// Audit trail of the waiver that suppressed a computed
/// verdict. Non-null iff `effective_verdict == WAIVED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaiverBasis {
	pub waiver_uid: String,
	pub reason: String,
	pub created_at: String,
	pub created_by: Option<String>,
	pub expires_at: Option<String>,
	pub rationale_category: Option<String>,
	pub policy_basis: Option<String>,
}

/// One evaluated obligation result.
///
/// Field order and names are byte-compatible with the
/// pre-relocation `gate::ObligationResult` so existing JSON
/// output consumers of `rmap gate` see no shape change.
#[derive(Debug, Clone, Serialize)]
pub struct ObligationEvaluation {
	pub req_id: String,
	pub req_version: i64,
	pub obligation_id: String,
	pub obligation: String,
	pub method: String,
	pub target: Option<String>,
	pub threshold: Option<f64>,
	pub operator: Option<String>,
	pub computed_verdict: Verdict,
	pub effective_verdict: EffectiveVerdict,
	pub evidence: serde_json::Value,
	pub waiver_basis: Option<WaiverBasis>,
}

/// Per-verdict counts in the final reduction (obligations).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GateCounts {
	pub total: usize,
	pub pass: usize,
	pub fail: usize,
	pub waived: usize,
	pub missing_evidence: usize,
	pub unsupported: usize,
}

/// Per-verdict counts for quality-policy assessments.
///
/// Separate from `GateCounts` because quality assessments have
/// different verdict semantics (NOT_APPLICABLE, NOT_COMPARABLE)
/// and severity-based blocking behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct GateQualityCounts {
	/// Total quality policies evaluated.
	pub total: usize,
	/// PASS verdicts.
	pub pass: usize,
	/// FAIL verdicts with severity=Fail (gate-blocking).
	pub fail: usize,
	/// FAIL verdicts with severity=Advisory (non-blocking).
	pub advisory_fail: usize,
	/// Missing assessments (no assessment row for active policy).
	pub missing: usize,
	/// NOT_COMPARABLE verdicts (comparative policy without baseline).
	pub not_comparable: usize,
	/// NOT_APPLICABLE verdicts (treated as non-blocking pass).
	pub not_applicable: usize,
}

/// Reduced gate outcome. `outcome` is a short string
/// identifier (`"pass"`, `"fail"`, `"incomplete"`) matching the
/// TS surface; `exit_code` mirrors the three-state CLI exit
/// policy; `mode` echoes the mode the caller requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GateOutcome {
	pub outcome: String,
	pub exit_code: i32,
	pub mode: String,
	pub counts: GateCounts,
	/// Quality-policy assessment counts (separate domain).
	pub quality_counts: GateQualityCounts,
}

/// Full gate report. Produced by `compute` and returned by
/// `assemble`. This is the single output type of the gate
/// crate; the CLI and the agent aggregator both read it.
///
/// The report includes both the reduced outcome and the full
/// list of per-obligation evaluations, so agents/CLIs can drill
/// into individual verdicts without re-running evaluation.
///
/// Quality assessments are reported separately from obligations
/// because they have different verdict semantics and severity-
/// based blocking behavior. Both contribute to the single
/// reduced `outcome`.
#[derive(Debug, Clone, Serialize)]
pub struct GateReport {
	pub obligations: Vec<ObligationEvaluation>,
	/// Quality-policy assessment evaluations (separate domain).
	pub quality_assessments: Vec<GateQualityAssessmentEvaluation>,
	pub outcome: GateOutcome,
}

// ── Gate mode ────────────────────────────────────────────────────

/// Gate reduction mode.
///
/// Determines how non-PASS effective verdicts map to exit
/// codes. Mode semantics are byte-identical to the
/// pre-relocation behavior in `rgr/src/gate.rs`. Do not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
	/// exit 0: all PASS/WAIVED (or empty).
	/// exit 1: any FAIL.
	/// exit 2: no FAIL but MISSING_EVIDENCE or UNSUPPORTED.
	Default,
	/// exit 0: all PASS/WAIVED.
	/// exit 1: any FAIL, MISSING_EVIDENCE, or UNSUPPORTED.
	Strict,
	/// exit 0: no FAIL (MISSING/UNSUPPORTED informational).
	/// exit 1: any FAIL.
	Advisory,
}

impl GateMode {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Default => "default",
			Self::Strict => "strict",
			Self::Advisory => "advisory",
		}
	}
}

// ── Quality-Policy Assessment Types (Gate-Owned) ─────────────────────

/// Assessment state: whether an assessment row exists for a policy.
///
/// Gate needs to distinguish "no assessment computed" from "assessment
/// computed with specific verdict" to detect missing required assessments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateAssessmentState {
	/// Assessment row exists for this policy + snapshot.
	Present,
	/// No assessment row found (assessment not run or stale).
	Missing,
}

impl GateAssessmentState {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Present => "present",
			Self::Missing => "missing",
		}
	}
}

/// Quality-policy kind (gate-owned mirror of storage enum).
///
/// Gate needs this for reporting and to understand whether the
/// policy is comparative (requires baseline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateQualityPolicyKind {
	AbsoluteMax,
	AbsoluteMin,
	NoNew,
	NoWorsened,
}

impl GateQualityPolicyKind {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::AbsoluteMax => "absolute_max",
			Self::AbsoluteMin => "absolute_min",
			Self::NoNew => "no_new",
			Self::NoWorsened => "no_worsened",
		}
	}

	/// Whether this policy kind requires a baseline snapshot.
	pub fn is_comparative(self) -> bool {
		matches!(self, Self::NoNew | Self::NoWorsened)
	}
}

/// Quality-policy severity (gate-owned mirror of storage enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateQualityPolicySeverity {
	/// Gate-blocking: FAIL contributes to non-zero exit code.
	Fail,
	/// Informational: FAIL reported but does not block gate.
	Advisory,
}

impl GateQualityPolicySeverity {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Fail => "fail",
			Self::Advisory => "advisory",
		}
	}
}

/// Quality-assessment computed verdict (gate-owned mirror).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateAssessmentVerdict {
	Pass,
	Fail,
	NotApplicable,
	NotComparable,
}

impl GateAssessmentVerdict {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Pass => "PASS",
			Self::Fail => "FAIL",
			Self::NotApplicable => "NOT_APPLICABLE",
			Self::NotComparable => "NOT_COMPARABLE",
		}
	}
}

/// Enriched quality-assessment fact for gate consumption.
///
/// One entry per active quality-policy declaration. The storage adapter
/// joins declarations with assessment rows and produces this DTO.
///
/// If no assessment exists for a policy, `assessment_state = Missing`
/// and the verdict/count fields are `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct GateQualityAssessmentFact {
	// ── Policy identity (from declaration) ──
	/// Storage-assigned declaration UID.
	pub policy_uid: String,
	/// Human-readable policy ID (e.g., "QP-001").
	pub policy_id: String,
	/// Policy version.
	pub policy_version: i64,
	/// Policy kind (determines comparative vs absolute).
	pub policy_kind: GateQualityPolicyKind,
	/// Severity (determines gate-blocking vs advisory).
	pub severity: GateQualityPolicySeverity,

	// ── Assessment state ──
	/// Whether an assessment row exists.
	pub assessment_state: GateAssessmentState,

	// ── Assessment data (present only when state = Present) ──
	/// Computed verdict from the assessment.
	pub computed_verdict: Option<GateAssessmentVerdict>,
	/// Baseline snapshot UID (for comparative assessments).
	pub baseline_snapshot_uid: Option<String>,
	/// Count of measurements evaluated.
	pub measurements_evaluated: Option<i64>,
	/// Count of violations found.
	pub violations_count: Option<usize>,
}

/// Evaluated quality-assessment result in the gate report.
///
/// Separate from `ObligationEvaluation` to preserve domain boundaries.
/// Quality assessments and generic obligations reduce to one outcome
/// but report separately.
#[derive(Debug, Clone, Serialize)]
pub struct GateQualityAssessmentEvaluation {
	pub policy_id: String,
	pub policy_version: i64,
	pub policy_kind: String,
	pub severity: String,
	pub assessment_state: String,
	pub computed_verdict: Option<String>,
	pub is_comparative: bool,
	pub violations_count: Option<usize>,
}

impl From<&GateQualityAssessmentFact> for GateQualityAssessmentEvaluation {
	fn from(fact: &GateQualityAssessmentFact) -> Self {
		Self {
			policy_id: fact.policy_id.clone(),
			policy_version: fact.policy_version,
			policy_kind: fact.policy_kind.as_str().to_string(),
			severity: fact.severity.as_str().to_string(),
			assessment_state: fact.assessment_state.as_str().to_string(),
			computed_verdict: fact.computed_verdict.map(|v| v.as_str().to_string()),
			is_comparative: fact.policy_kind.is_comparative(),
			violations_count: fact.violations_count,
		}
	}
}
