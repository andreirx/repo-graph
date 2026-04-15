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
/// output consumers of `rgr-rust gate` see no shape change.
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

/// Per-verdict counts in the final reduction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GateCounts {
	pub total: usize,
	pub pass: usize,
	pub fail: usize,
	pub waived: usize,
	pub missing_evidence: usize,
	pub unsupported: usize,
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
}

/// Full gate report. Produced by `compute` and returned by
/// `assemble`. This is the single output type of the gate
/// crate; the CLI and the agent aggregator both read it.
///
/// The report includes both the reduced outcome and the full
/// list of per-obligation evaluations, so agents/CLIs can drill
/// into individual verdicts without re-running evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct GateReport {
	pub obligations: Vec<ObligationEvaluation>,
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
