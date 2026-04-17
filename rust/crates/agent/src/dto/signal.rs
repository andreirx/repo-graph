//! Signal record — the central output unit of every agent
//! orientation response.
//!
//! A `Signal` is a typed, machine-stable record. The contract
//! requires:
//!
//!   - Each signal has a stable code, rank, severity, category,
//!     summary, typed evidence, and a source identifier.
//!   - Evidence is typed per-code. No `serde_json::Value` escape
//!     hatch. No shared "generic evidence" variant.
//!   - Signals of the same code always carry the same evidence
//!     variant. The invariant is enforced at construction time
//!     by per-code named constructors; there is no public raw
//!     `Signal { .. }` constructor.
//!
//! ── Serialization shape ──────────────────────────────────────
//!
//! The JSON envelope for one signal is:
//!
//! ```json
//! {
//!   "code": "GATE_FAIL",
//!   "rank": 1,
//!   "severity": "high",
//!   "category": "gate",
//!   "summary": "Gate fails: ...",
//!   "evidence": { ... },
//!   "source": "storage::..."
//! }
//! ```
//!
//! `evidence` is a single JSON object whose shape depends on
//! `code`. Since `code` is the discriminator, the
//! `SignalEvidence` enum must serialize as the inner struct
//! only, with no additional enum tag. This is implemented via a
//! hand-written `Serialize` impl that matches on the active
//! variant and forwards to the inner struct's serializer. Using
//! `#[serde(untagged)]` would work for serialization but is
//! deliberately avoided: it makes silent variant drift possible
//! the day someone adds a deserialization path, and the contract
//! is that `SignalEvidence` is produce-only today.

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;

use crate::dto::source::SourceRef;

// ── SignalScope ──────────────────────────────────────────────────

/// Whether a signal is directly computed for the focused entity or
/// inherited from its owning module context.
///
/// Serialization contract:
///   - `Direct` — the `scope` field is ABSENT from JSON output.
///     This preserves backward compatibility with all existing
///     repo/path/file pipeline output.
///   - `ModuleContext` — serialized as `"scope": "module_context"`.
///     Only symbol-scoped orient emits this variant, for signals
///     inherited from the owning module (boundary violations,
///     import cycles, gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalScope {
	Direct,
	ModuleContext,
}

impl SignalScope {
	/// Returns `true` when the scope is `Direct`. Used by
	/// `skip_serializing_if` to omit the field from JSON when
	/// no scope annotation is needed (backward compat).
	pub fn is_direct(self) -> bool {
		matches!(self, Self::Direct)
	}
}

impl Serialize for SignalScope {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			Self::Direct => serializer.serialize_str("direct"),
			Self::ModuleContext => serializer.serialize_str("module_context"),
		}
	}
}

// ── Severity ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
	// Order matters for Ord: Low < Medium < High. Ranking sorts
	// in descending severity, so we reverse at sort time.
	Low,
	Medium,
	High,
}

impl Severity {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Low => "low",
			Self::Medium => "medium",
			Self::High => "high",
		}
	}
}

impl Serialize for Severity {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(self.as_str())
	}
}

// ── Category ─────────────────────────────────────────────────────

/// Signal category. Ranking breaks ties within a severity tier
/// by category order: check > gate > boundary > trust > structure >
/// informational.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalCategory {
	Check,
	Gate,
	Boundary,
	Trust,
	Structure,
	Informational,
}

impl SignalCategory {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Check => "check",
			Self::Gate => "gate",
			Self::Boundary => "boundary",
			Self::Trust => "trust",
			Self::Structure => "structure",
			Self::Informational => "informational",
		}
	}

	/// Tie-breaking ordering. Lower return value wins (sorts
	/// earlier in the output). Check first, informational last.
	pub fn tie_break_ordinal(self) -> u8 {
		match self {
			Self::Check => 0,
			Self::Gate => 1,
			Self::Boundary => 2,
			Self::Trust => 3,
			Self::Structure => 4,
			Self::Informational => 5,
		}
	}
}

impl Serialize for SignalCategory {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(self.as_str())
	}
}

// ── SignalCode enumeration ───────────────────────────────────────

/// Stable enumeration of every signal the agent surface can
/// produce. Rust-42 only *constructs* a subset (repo-level focus);
/// codes reserved for module/symbol focus are declared so the
/// enumeration stays complete and ranking is exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalCode {
	// Check verdicts
	CheckPass,
	CheckFail,
	CheckIncomplete,
	// Governance
	GatePass,
	GateFail,
	GateIncomplete,
	BoundaryViolations,
	// Trust
	TrustLowResolution,
	TrustStaleSnapshot,
	TrustNoEnrichment,
	// Structure
	ImportCycles,
	DeadCode,
	HighComplexity,
	HighFanOut,
	HighInstability,
	CallersSummary,
	CalleesSummary,
	// Informational
	ModuleSummary,
	SnapshotInfo,
}

impl SignalCode {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::CheckPass => "CHECK_PASS",
			Self::CheckFail => "CHECK_FAIL",
			Self::CheckIncomplete => "CHECK_INCOMPLETE",
			Self::GatePass => "GATE_PASS",
			Self::GateFail => "GATE_FAIL",
			Self::GateIncomplete => "GATE_INCOMPLETE",
			Self::BoundaryViolations => "BOUNDARY_VIOLATIONS",
			Self::TrustLowResolution => "TRUST_LOW_RESOLUTION",
			Self::TrustStaleSnapshot => "TRUST_STALE_SNAPSHOT",
			Self::TrustNoEnrichment => "TRUST_NO_ENRICHMENT",
			Self::ImportCycles => "IMPORT_CYCLES",
			Self::DeadCode => "DEAD_CODE",
			Self::HighComplexity => "HIGH_COMPLEXITY",
			Self::HighFanOut => "HIGH_FAN_OUT",
			Self::HighInstability => "HIGH_INSTABILITY",
			Self::CallersSummary => "CALLERS_SUMMARY",
			Self::CalleesSummary => "CALLEES_SUMMARY",
			Self::ModuleSummary => "MODULE_SUMMARY",
			Self::SnapshotInfo => "SNAPSHOT_INFO",
		}
	}

	/// Explicit priority ordinal within the same
	/// (severity, category) tier. Lower number = higher
	/// priority in the ranking sort. Replaces the alphabetical
	/// code-string tiebreaker from Rust-42.
	///
	/// Only codes that can co-exist in the same tier need
	/// distinct values. Gate codes are mutually exclusive
	/// (only one fires at a time), so they all share 0.
	pub fn tier_priority(self) -> u8 {
		match self {
			// Check: only one fires at a time.
			Self::CheckPass => 0,
			Self::CheckFail => 0,
			Self::CheckIncomplete => 0,
			// Gate (High): only one fires at a time.
			Self::GatePass => 0,
			Self::GateFail => 0,
			Self::GateIncomplete => 0,
			// Boundary (High): sole occupant.
			Self::BoundaryViolations => 0,
			// Trust (Medium): low-resolution most urgent.
			Self::TrustLowResolution => 0,
			Self::TrustStaleSnapshot => 1,
			Self::TrustNoEnrichment => 2,
			// Structure (Medium): cycles > dead > complexity.
			Self::ImportCycles => 0,
			Self::DeadCode => 1,
			Self::HighComplexity => 2,
			// Structure (Low): fan-out > instability > callers > callees.
			Self::HighFanOut => 0,
			Self::HighInstability => 1,
			Self::CallersSummary => 2,
			Self::CalleesSummary => 3,
			// Informational (Low): summary > snapshot.
			Self::ModuleSummary => 0,
			Self::SnapshotInfo => 1,
		}
	}

	/// Canonical (code, category, severity) triple.
	///
	/// Every signal code carries its category and default
	/// severity as a compile-time fact. Aggregators MUST use
	/// these values via named constructors; they never override
	/// them at the call site. This prevents drift between the
	/// agent contract and the code.
	pub fn descriptor(self) -> (SignalCategory, Severity) {
		use SignalCategory::*;
		use Severity::*;
		match self {
			Self::CheckPass => (Check, Low),
			Self::CheckFail => (Check, High),
			Self::CheckIncomplete => (Check, Medium),
			Self::GatePass => (Gate, Low),
			Self::GateFail => (Gate, High),
			Self::GateIncomplete => (Gate, Medium),
			Self::BoundaryViolations => (Boundary, High),
			Self::TrustLowResolution => (Trust, Medium),
			Self::TrustStaleSnapshot => (Trust, Medium),
			Self::TrustNoEnrichment => (Trust, Low),
			Self::ImportCycles => (Structure, Medium),
			Self::DeadCode => (Structure, Medium),
			Self::HighComplexity => (Structure, Medium),
			Self::HighFanOut => (Structure, Low),
			Self::HighInstability => (Structure, Low),
			Self::CallersSummary => (Structure, Low),
			Self::CalleesSummary => (Structure, Low),
			Self::ModuleSummary => (Informational, Low),
			Self::SnapshotInfo => (Informational, Low),
		}
	}
}

impl Serialize for SignalCode {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(self.as_str())
	}
}

// ── Evidence variants ────────────────────────────────────────────
//
// One struct per signal code actually *constructed* at repo-level
// in Rust-42. Variants for future slices (HighComplexity,
// CallersSummary, etc.) are not declared here yet because there
// is no constructor site for them; they will be added in the
// slice that introduces module/symbol focus.

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatePassEvidence {
	pub pass_count: u64,
	pub waived_count: u64,
	pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GateFailEvidence {
	pub fail_count: u64,
	pub total_count: u64,
	/// Flat `"{req_id}/{obligation_id}"` identifiers for the
	/// failing obligations. Agents consume this as a follow-up
	/// lookup key; full per-obligation detail stays in the raw
	/// gate report which the `gate` CLI command surfaces.
	pub failing_obligations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GateIncompleteEvidence {
	pub missing_count: u64,
	pub unsupported_count: u64,
	pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportCyclesEvidence {
	pub cycle_count: u64,
	pub cycles: Vec<CycleEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CycleEvidence {
	pub length: usize,
	pub modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrustLowResolutionEvidence {
	pub resolution_rate: f64,
	pub resolved_count: u64,
	pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrustStaleSnapshotEvidence {
	pub stale_file_count: u64,
	pub snapshot_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrustNoEnrichmentEvidence {
	pub enrichment_eligible: u64,
	pub enrichment_enriched: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryViolationsEvidence {
	pub violation_count: u64,
	pub top_violations: Vec<BoundaryViolationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryViolationEvidence {
	pub source_module: String,
	pub target_module: String,
	pub edge_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadCodeEvidence {
	pub dead_count: u64,
	pub top_dead: Vec<DeadSymbolEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadSymbolEvidence {
	pub symbol: String,
	pub file: Option<String>,
	pub line_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleSummaryEvidence {
	pub file_count: u64,
	pub symbol_count: u64,
	pub languages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotInfoEvidence {
	pub snapshot_uid: String,
	pub scope: String,
	pub basis_commit: Option<String>,
	pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallersSummaryEvidence {
	pub count: u64,
	pub top_modules: Vec<ModuleCountEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalleesSummaryEvidence {
	pub count: u64,
	pub top_modules: Vec<ModuleCountEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleCountEvidence {
	pub module: String,
	pub count: u64,
}

// ── Check condition evidence ─────────────────────────────────────

/// One evaluated condition, serialized into check signal evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckConditionEvidence {
	pub code: String,
	pub status: String,
	pub summary: String,
}

/// Evidence for `CHECK_PASS`: all conditions passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckPassEvidence {
	pub conditions: Vec<CheckConditionEvidence>,
}

/// Evidence for `CHECK_FAIL`: at least one condition failed, none
/// incomplete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckFailEvidence {
	pub fail_conditions: Vec<CheckConditionEvidence>,
	pub passing: Vec<CheckConditionEvidence>,
}

/// Evidence for `CHECK_INCOMPLETE`: at least one condition
/// incomplete (takes precedence over fail).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckIncompleteEvidence {
	pub incomplete_conditions: Vec<CheckConditionEvidence>,
	pub fail_conditions: Vec<CheckConditionEvidence>,
	pub passing: Vec<CheckConditionEvidence>,
}

// ── SignalEvidence enum ──────────────────────────────────────────

/// Typed evidence variants. Exactly one variant per signal code
/// that the aggregator pipeline constructs in Rust-42.
///
/// Ser format: the enum is produce-only. `Serialize` is
/// hand-written to forward to the inner struct (no tag, no
/// `untagged` attribute). If this crate ever needs
/// deserialization of signals, that will require re-designing
/// the discriminator — which is intentionally out of scope
/// today.
#[derive(Debug, Clone, PartialEq)]
pub enum SignalEvidence {
	CheckPass(CheckPassEvidence),
	CheckFail(CheckFailEvidence),
	CheckIncomplete(CheckIncompleteEvidence),
	GatePass(GatePassEvidence),
	GateFail(GateFailEvidence),
	GateIncomplete(GateIncompleteEvidence),
	ImportCycles(ImportCyclesEvidence),
	TrustLowResolution(TrustLowResolutionEvidence),
	TrustStaleSnapshot(TrustStaleSnapshotEvidence),
	TrustNoEnrichment(TrustNoEnrichmentEvidence),
	BoundaryViolations(BoundaryViolationsEvidence),
	DeadCode(DeadCodeEvidence),
	ModuleSummary(ModuleSummaryEvidence),
	SnapshotInfo(SnapshotInfoEvidence),
	CallersSummary(CallersSummaryEvidence),
	CalleesSummary(CalleesSummaryEvidence),
}

impl Serialize for SignalEvidence {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			Self::CheckPass(e) => e.serialize(serializer),
			Self::CheckFail(e) => e.serialize(serializer),
			Self::CheckIncomplete(e) => e.serialize(serializer),
			Self::GatePass(e) => e.serialize(serializer),
			Self::GateFail(e) => e.serialize(serializer),
			Self::GateIncomplete(e) => e.serialize(serializer),
			Self::ImportCycles(e) => e.serialize(serializer),
			Self::TrustLowResolution(e) => e.serialize(serializer),
			Self::TrustStaleSnapshot(e) => e.serialize(serializer),
			Self::TrustNoEnrichment(e) => e.serialize(serializer),
			Self::BoundaryViolations(e) => e.serialize(serializer),
			Self::DeadCode(e) => e.serialize(serializer),
			Self::ModuleSummary(e) => e.serialize(serializer),
			Self::SnapshotInfo(e) => e.serialize(serializer),
			Self::CallersSummary(e) => e.serialize(serializer),
			Self::CalleesSummary(e) => e.serialize(serializer),
		}
	}
}

impl SignalEvidence {
	/// Runtime variant discriminant used exclusively by unit
	/// tests to assert a given signal code carries the expected
	/// evidence variant. Not part of the JSON contract.
	#[cfg(test)]
	pub(crate) fn variant_name(&self) -> &'static str {
		match self {
			Self::CheckPass(_) => "CheckPass",
			Self::CheckFail(_) => "CheckFail",
			Self::CheckIncomplete(_) => "CheckIncomplete",
			Self::GatePass(_) => "GatePass",
			Self::GateFail(_) => "GateFail",
			Self::GateIncomplete(_) => "GateIncomplete",
			Self::ImportCycles(_) => "ImportCycles",
			Self::TrustLowResolution(_) => "TrustLowResolution",
			Self::TrustStaleSnapshot(_) => "TrustStaleSnapshot",
			Self::TrustNoEnrichment(_) => "TrustNoEnrichment",
			Self::BoundaryViolations(_) => "BoundaryViolations",
			Self::DeadCode(_) => "DeadCode",
			Self::ModuleSummary(_) => "ModuleSummary",
			Self::SnapshotInfo(_) => "SnapshotInfo",
			Self::CallersSummary(_) => "CallersSummary",
			Self::CalleesSummary(_) => "CalleesSummary",
		}
	}
}

// ── Signal record ────────────────────────────────────────────────

/// One signal in the output envelope.
///
/// Field visibility is deliberately `pub(crate)`. External
/// callers (tests in `tests/`, the CLI wiring in a future slice)
/// cannot build a `Signal` via the record syntax. The only way
/// to create one is through the per-code named constructors
/// below, which enforce the code ↔ category ↔ severity invariant
/// by looking up `SignalCode::descriptor()`. Serde's derive
/// expansion lives inside this module and has full access to
/// the private fields, so JSON serialization still works.
///
/// Read access for callers goes through explicit accessor
/// methods (`code()`, `rank()`, etc.) so tests can assert on the
/// record without having to bypass privacy.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
	pub(crate) code: SignalCode,
	pub(crate) rank: u32,
	pub(crate) severity: Severity,
	pub(crate) category: SignalCategory,
	pub(crate) summary: String,
	pub(crate) evidence: SignalEvidence,
	pub(crate) source: SourceRef,
	pub(crate) scope: SignalScope,
}

impl Serialize for Signal {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		// Count fields: 7 base + 1 optional scope
		let field_count = if self.scope.is_direct() { 7 } else { 8 };
		let mut state = serializer.serialize_struct("Signal", field_count)?;
		state.serialize_field("code", &self.code)?;
		state.serialize_field("rank", &self.rank)?;
		state.serialize_field("severity", &self.severity)?;
		state.serialize_field("category", &self.category)?;
		state.serialize_field("summary", &self.summary)?;
		state.serialize_field("evidence", &self.evidence)?;
		state.serialize_field("source", &self.source)?;
		if !self.scope.is_direct() {
			state.serialize_field("scope", &self.scope)?;
		}
		state.end()
	}
}

impl Signal {
	// ── Accessors ─────────────────────────────────────────────

	pub fn code(&self) -> SignalCode { self.code }
	pub fn rank(&self) -> u32 { self.rank }
	pub fn severity(&self) -> Severity { self.severity }
	pub fn category(&self) -> SignalCategory { self.category }
	pub fn summary(&self) -> &str { &self.summary }
	pub fn evidence(&self) -> &SignalEvidence { &self.evidence }
	pub fn source(&self) -> SourceRef { self.source }
	pub fn scope(&self) -> SignalScope { self.scope }

	/// Rank is assigned by the ranking pass after all signals
	/// are collected. Callers must never set rank directly; this
	/// method is `pub(crate)` so only the ranking module can use
	/// it.
	pub(crate) fn set_rank(&mut self, rank: u32) {
		self.rank = rank;
	}

	/// Mark this signal as inherited from the owning module
	/// context. Returns self for chaining.
	pub(crate) fn with_module_context(mut self) -> Self {
		self.scope = SignalScope::ModuleContext;
		self
	}

	// Internal constructor. Looks up descriptor for the code and
	// assigns category/severity automatically. `rank` is set to
	// 0 at construction time and overwritten by the ranking
	// pass — no aggregator computes its own rank.
	fn build(
		code: SignalCode,
		summary: String,
		evidence: SignalEvidence,
		source: SourceRef,
	) -> Self {
		let (category, severity) = code.descriptor();
		Self {
			code,
			rank: 0,
			severity,
			category,
			summary,
			evidence,
			source,
			scope: SignalScope::Direct,
		}
	}

	// ── Named constructors (one per emitted code) ────────────

	pub fn check_pass(evidence: CheckPassEvidence) -> Self {
		let n = evidence.conditions.len();
		let summary = format!(
			"Check passes: all {} condition{} pass.",
			n,
			if n == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::CheckPass,
			summary,
			SignalEvidence::CheckPass(evidence),
			SourceRef::CheckReducer,
		)
	}

	pub fn check_fail(evidence: CheckFailEvidence) -> Self {
		let n = evidence.fail_conditions.len();
		let summary = format!(
			"Check fails: {} condition{} failing.",
			n,
			if n == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::CheckFail,
			summary,
			SignalEvidence::CheckFail(evidence),
			SourceRef::CheckReducer,
		)
	}

	pub fn check_incomplete(evidence: CheckIncompleteEvidence) -> Self {
		let n = evidence.incomplete_conditions.len();
		let summary = format!(
			"Check incomplete: {} condition{} missing data.",
			n,
			if n == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::CheckIncomplete,
			summary,
			SignalEvidence::CheckIncomplete(evidence),
			SourceRef::CheckReducer,
		)
	}

	pub fn gate_pass(evidence: GatePassEvidence) -> Self {
		let summary = if evidence.total_count == 0 {
			"Gate has no obligations; trivially passing.".to_string()
		} else if evidence.waived_count == 0 {
			format!(
				"Gate passes: all {} obligation{} pass.",
				evidence.total_count,
				if evidence.total_count == 1 { "" } else { "s" }
			)
		} else {
			format!(
				"Gate passes: {} of {} obligation{} pass, {} waived.",
				evidence.pass_count,
				evidence.total_count,
				if evidence.total_count == 1 { "" } else { "s" },
				evidence.waived_count,
			)
		};
		Self::build(
			SignalCode::GatePass,
			summary,
			SignalEvidence::GatePass(evidence),
			SourceRef::GateAssemble,
		)
	}

	pub fn gate_fail(evidence: GateFailEvidence) -> Self {
		let summary = format!(
			"Gate fails: {} of {} obligation{} failing.",
			evidence.fail_count,
			evidence.total_count,
			if evidence.total_count == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::GateFail,
			summary,
			SignalEvidence::GateFail(evidence),
			SourceRef::GateAssemble,
		)
	}

	pub fn gate_incomplete(evidence: GateIncompleteEvidence) -> Self {
		let summary = format!(
			"Gate incomplete: {} missing, {} unsupported (of {}).",
			evidence.missing_count, evidence.unsupported_count, evidence.total_count
		);
		Self::build(
			SignalCode::GateIncomplete,
			summary,
			SignalEvidence::GateIncomplete(evidence),
			SourceRef::GateAssemble,
		)
	}

	pub fn import_cycles(evidence: ImportCyclesEvidence) -> Self {
		let summary = format!(
			"{} import cycle{} detected at the module level.",
			evidence.cycle_count,
			if evidence.cycle_count == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::ImportCycles,
			summary,
			SignalEvidence::ImportCycles(evidence),
			SourceRef::StorageFindModuleCycles,
		)
	}

	pub fn trust_low_resolution(evidence: TrustLowResolutionEvidence) -> Self {
		let summary = format!(
			"Call resolution rate is {:.0}% ({} of {} calls resolved).",
			evidence.resolution_rate * 100.0,
			evidence.resolved_count,
			evidence.total_count
		);
		Self::build(
			SignalCode::TrustLowResolution,
			summary,
			SignalEvidence::TrustLowResolution(evidence),
			SourceRef::StorageGetTrustSummary,
		)
	}

	pub fn trust_stale_snapshot(evidence: TrustStaleSnapshotEvidence) -> Self {
		// Deliberate wording per Sub-Decision B1: describe the
		// storage-internal condition, not a filesystem/git
		// comparison the use case never performs.
		let summary = format!(
			"Snapshot has {} stale file{} recorded in storage.",
			evidence.stale_file_count,
			if evidence.stale_file_count == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::TrustStaleSnapshot,
			summary,
			SignalEvidence::TrustStaleSnapshot(evidence),
			SourceRef::StorageGetStaleFiles,
		)
	}

	pub fn trust_no_enrichment(evidence: TrustNoEnrichmentEvidence) -> Self {
		let summary = String::from(
			"Enrichment phase did not run; call graph resolution \
			 relies on syntax-only extraction.",
		);
		Self::build(
			SignalCode::TrustNoEnrichment,
			summary,
			SignalEvidence::TrustNoEnrichment(evidence),
			SourceRef::StorageGetTrustSummary,
		)
	}

	pub fn boundary_violations(evidence: BoundaryViolationsEvidence) -> Self {
		let summary = format!(
			"{} boundary violation{} detected across import edges.",
			evidence.violation_count,
			if evidence.violation_count == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::BoundaryViolations,
			summary,
			SignalEvidence::BoundaryViolations(evidence),
			SourceRef::StorageFindImportsBetweenPaths,
		)
	}

	pub fn dead_code(evidence: DeadCodeEvidence) -> Self {
		let summary = format!(
			"{} unreferenced symbol{} detected.",
			evidence.dead_count,
			if evidence.dead_count == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::DeadCode,
			summary,
			SignalEvidence::DeadCode(evidence),
			SourceRef::StorageFindDeadNodes,
		)
	}

	pub fn module_summary(evidence: ModuleSummaryEvidence) -> Self {
		let summary = format!(
			"{} file{}, {} symbol{} indexed.",
			evidence.file_count,
			if evidence.file_count == 1 { "" } else { "s" },
			evidence.symbol_count,
			if evidence.symbol_count == 1 { "" } else { "s" }
		);
		Self::build(
			SignalCode::ModuleSummary,
			summary,
			SignalEvidence::ModuleSummary(evidence),
			SourceRef::StorageComputeRepoSummary,
		)
	}

	pub fn snapshot_info(evidence: SnapshotInfoEvidence) -> Self {
		let summary = format!(
			"Snapshot {} ({}).",
			short_uid(&evidence.snapshot_uid),
			evidence.scope
		);
		Self::build(
			SignalCode::SnapshotInfo,
			summary,
			SignalEvidence::SnapshotInfo(evidence),
			SourceRef::StorageGetLatestSnapshot,
		)
	}

	pub fn callers_summary(evidence: CallersSummaryEvidence) -> Self {
		let summary = format!(
			"{} direct caller{} across {} module{}.",
			evidence.count,
			if evidence.count == 1 { "" } else { "s" },
			evidence.top_modules.len(),
			if evidence.top_modules.len() == 1 { "" } else { "s" },
		);
		Self::build(
			SignalCode::CallersSummary,
			summary,
			SignalEvidence::CallersSummary(evidence),
			SourceRef::StorageFindSymbolCallers,
		)
	}

	pub fn callees_summary(evidence: CalleesSummaryEvidence) -> Self {
		let summary = format!(
			"{} direct callee{} across {} module{}.",
			evidence.count,
			if evidence.count == 1 { "" } else { "s" },
			evidence.top_modules.len(),
			if evidence.top_modules.len() == 1 { "" } else { "s" },
		);
		Self::build(
			SignalCode::CalleesSummary,
			summary,
			SignalEvidence::CalleesSummary(evidence),
			SourceRef::StorageFindSymbolCallees,
		)
	}
}

// ── Small helpers ────────────────────────────────────────────────

fn short_uid(uid: &str) -> String {
	// Human-friendly abbreviation: keep the last 8 characters
	// for long UIDs, the whole thing for short ones. This never
	// touches the contract — the full UID is always in evidence.
	if uid.len() <= 12 {
		uid.to_string()
	} else {
		format!("…{}", &uid[uid.len() - 8..])
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn severity_serializes_lowercase() {
		let s = serde_json::to_string(&Severity::High).unwrap();
		assert_eq!(s, "\"high\"");
	}

	#[test]
	fn category_serializes_lowercase() {
		let s = serde_json::to_string(&SignalCategory::Gate).unwrap();
		assert_eq!(s, "\"gate\"");
	}

	#[test]
	fn signal_code_serializes_screaming_snake() {
		let s = serde_json::to_string(&SignalCode::BoundaryViolations).unwrap();
		assert_eq!(s, "\"BOUNDARY_VIOLATIONS\"");
	}

	#[test]
	fn descriptor_is_stable_per_code() {
		assert_eq!(
			SignalCode::GateFail.descriptor(),
			(SignalCategory::Gate, Severity::High),
		);
		assert_eq!(
			SignalCode::BoundaryViolations.descriptor(),
			(SignalCategory::Boundary, Severity::High),
		);
		assert_eq!(
			SignalCode::TrustLowResolution.descriptor(),
			(SignalCategory::Trust, Severity::Medium),
		);
		assert_eq!(
			SignalCode::DeadCode.descriptor(),
			(SignalCategory::Structure, Severity::Medium),
		);
		assert_eq!(
			SignalCode::SnapshotInfo.descriptor(),
			(SignalCategory::Informational, Severity::Low),
		);
	}

	#[test]
	fn category_tie_break_order_is_check_first() {
		assert!(
			SignalCategory::Check.tie_break_ordinal()
				< SignalCategory::Gate.tie_break_ordinal()
		);
		assert!(
			SignalCategory::Gate.tie_break_ordinal()
				< SignalCategory::Boundary.tie_break_ordinal()
		);
		assert!(
			SignalCategory::Boundary.tie_break_ordinal()
				< SignalCategory::Trust.tie_break_ordinal()
		);
		assert!(
			SignalCategory::Trust.tie_break_ordinal()
				< SignalCategory::Structure.tie_break_ordinal()
		);
		assert!(
			SignalCategory::Structure.tie_break_ordinal()
				< SignalCategory::Informational.tie_break_ordinal()
		);
	}

	#[test]
	fn constructor_invariant_import_cycles() {
		let s = Signal::import_cycles(ImportCyclesEvidence {
			cycle_count: 2,
			cycles: vec![],
		});
		assert_eq!(s.code, SignalCode::ImportCycles);
		assert_eq!(s.category, SignalCategory::Structure);
		assert_eq!(s.severity, Severity::Medium);
		assert_eq!(s.evidence.variant_name(), "ImportCycles");
		assert_eq!(s.rank, 0); // Rank is assigned by ranking pass.
	}

	#[test]
	fn constructor_invariant_boundary_violations() {
		let s = Signal::boundary_violations(BoundaryViolationsEvidence {
			violation_count: 3,
			top_violations: vec![],
		});
		assert_eq!(s.code, SignalCode::BoundaryViolations);
		assert_eq!(s.category, SignalCategory::Boundary);
		assert_eq!(s.severity, Severity::High);
		assert_eq!(s.evidence.variant_name(), "BoundaryViolations");
	}

	#[test]
	fn constructor_invariant_dead_code() {
		let s = Signal::dead_code(DeadCodeEvidence {
			dead_count: 1,
			top_dead: vec![],
		});
		assert_eq!(s.code, SignalCode::DeadCode);
		assert_eq!(s.evidence.variant_name(), "DeadCode");
	}

	#[test]
	fn constructor_invariant_module_summary() {
		let s = Signal::module_summary(ModuleSummaryEvidence {
			file_count: 10,
			symbol_count: 100,
			languages: vec!["rust".into()],
		});
		assert_eq!(s.code, SignalCode::ModuleSummary);
		assert_eq!(s.category, SignalCategory::Informational);
	}

	#[test]
	fn constructor_invariant_snapshot_info() {
		let s = Signal::snapshot_info(SnapshotInfoEvidence {
			snapshot_uid: "snap-long-uid-1234567890".into(),
			scope: "full".into(),
			basis_commit: None,
			created_at: "2026-04-15T00:00:00Z".into(),
		});
		assert_eq!(s.code, SignalCode::SnapshotInfo);
		assert_eq!(s.category, SignalCategory::Informational);
	}

	#[test]
	fn constructor_invariant_trust_low_resolution() {
		let s = Signal::trust_low_resolution(TrustLowResolutionEvidence {
			resolution_rate: 0.10,
			resolved_count: 1,
			total_count: 10,
		});
		assert_eq!(s.code, SignalCode::TrustLowResolution);
		assert_eq!(s.category, SignalCategory::Trust);
		assert_eq!(s.severity, Severity::Medium);
	}

	#[test]
	fn constructor_invariant_trust_stale_snapshot() {
		let s = Signal::trust_stale_snapshot(TrustStaleSnapshotEvidence {
			stale_file_count: 3,
			snapshot_uid: "snap1".into(),
		});
		assert_eq!(s.code, SignalCode::TrustStaleSnapshot);
		assert!(
			s.summary.contains("stale file"),
			"summary must describe storage-internal stale state: {}",
			s.summary
		);
		assert!(
			!s.summary.to_lowercase().contains("changed since"),
			"summary must not overclaim filesystem/git staleness: {}",
			s.summary
		);
	}

	#[test]
	fn constructor_invariant_trust_no_enrichment() {
		let s = Signal::trust_no_enrichment(TrustNoEnrichmentEvidence {
			enrichment_eligible: 10,
			enrichment_enriched: 0,
		});
		assert_eq!(s.code, SignalCode::TrustNoEnrichment);
		assert_eq!(s.severity, Severity::Low);
	}

	#[test]
	fn signal_serializes_with_flat_evidence_object() {
		let s = Signal::import_cycles(ImportCyclesEvidence {
			cycle_count: 1,
			cycles: vec![CycleEvidence {
				length: 2,
				modules: vec!["m1".into(), "m2".into()],
			}],
		});
		let json = serde_json::to_value(&s).unwrap();
		assert_eq!(json["code"], "IMPORT_CYCLES");
		assert_eq!(json["category"], "structure");
		assert_eq!(json["severity"], "medium");
		// Evidence is a flat object — NO discriminator tag inside.
		let ev = &json["evidence"];
		assert_eq!(ev["cycle_count"], 1);
		assert!(ev["cycles"].is_array());
		// No stray "type" or "variant" fields leaked in.
		assert!(ev.get("type").is_none());
		assert!(ev.get("variant").is_none());
	}
}
