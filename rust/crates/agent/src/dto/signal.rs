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

// ── Freshness (ACR-6) ────────────────────────────────────────────
//
// Per-signal freshness for signals backed by Layer 2+ artifacts.
// Maps from artifact_contracts::FreshnessState but uses agent-facing DTO.
// Only populated when the signal is backed by freshness-tracked tables.

/// Freshness state for a signal's backing artifacts.
///
/// Maps from `artifact_contracts::FreshnessState`. Only three states
/// are surfaced in agent DTOs — `Stale` is not yet exposed (deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessStateDto {
    /// All backing artifacts are current (freshness_state = 'current').
    Current,
    /// At least one backing artifact is impacted by L0 changes.
    Impacted,
    /// Freshness state is unknown (no provenance tracked).
    Unknown,
}

impl FreshnessStateDto {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Impacted => "impacted",
            Self::Unknown => "unknown",
        }
    }
}

impl Serialize for FreshnessStateDto {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Freshness info for a signal backed by Layer 2+ artifacts.
///
/// Attached to signals whose evidence is derived from freshness-tracked
/// tables (boundary_contracts, inferences, module_candidates, etc.).
/// Omitted from signals backed by L0/L1 facts or governance overlays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FreshnessInfo {
    /// Aggregate freshness state (worst-of-all backing rows).
    pub state: FreshnessStateDto,

    /// When the backing artifacts became impacted (ISO 8601).
    /// Only present when state = Impacted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impacted_since: Option<String>,
}

impl FreshnessInfo {
    /// Create freshness info for current state.
    pub fn current() -> Self {
        Self {
            state: FreshnessStateDto::Current,
            impacted_since: None,
        }
    }

    /// Create freshness info for impacted state.
    pub fn impacted(since: impl Into<String>) -> Self {
        Self {
            state: FreshnessStateDto::Impacted,
            impacted_since: Some(since.into()),
        }
    }

    /// Create freshness info for impacted state without timestamp.
    pub fn impacted_unknown_time() -> Self {
        Self {
            state: FreshnessStateDto::Impacted,
            impacted_since: None,
        }
    }

    /// Create freshness info for unknown state.
    pub fn unknown() -> Self {
        Self {
            state: FreshnessStateDto::Unknown,
            impacted_since: None,
        }
    }
}

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
    Explain,
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
            Self::Explain => "explain",
        }
    }

    /// Tie-breaking ordering. Lower return value wins (sorts
    /// earlier in the output). Check first, informational last.
    /// Explain sorts after informational.
    pub fn tie_break_ordinal(self) -> u8 {
        match self {
            Self::Check => 0,
            Self::Gate => 1,
            Self::Boundary => 2,
            Self::Trust => 3,
            Self::Structure => 4,
            Self::Informational => 5,
            Self::Explain => 6,
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
    BoundaryLinksSummary,
    // Trust
    TrustLowResolution,
    TrustStaleSnapshot,
    TrustNoEnrichment,
    // Structure
    ImportCycles,
    // DeadCode — surface withdrawn. Internal substrate preserved.
    // See docs/TECH-DEBT.md for reintroduction conditions.
    HighComplexity,
    HighFanOut,
    HighInstability,
    CallersSummary,
    CalleesSummary,
    // Informational
    ModuleSummary,
    SnapshotInfo,
    // Explain
    ExplainIdentity,
    ExplainCallers,
    ExplainCallees,
    ExplainImports,
    ExplainSymbols,
    ExplainFiles,
    ExplainCycles,
    ExplainBoundary,
    ExplainGate,
    ExplainTrust,
    ExplainMeasurements,
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
            Self::BoundaryLinksSummary => "BOUNDARY_LINKS_SUMMARY",
            Self::TrustLowResolution => "TRUST_LOW_RESOLUTION",
            Self::TrustStaleSnapshot => "TRUST_STALE_SNAPSHOT",
            Self::TrustNoEnrichment => "TRUST_NO_ENRICHMENT",
            Self::ImportCycles => "IMPORT_CYCLES",
            Self::HighComplexity => "HIGH_COMPLEXITY",
            Self::HighFanOut => "HIGH_FAN_OUT",
            Self::HighInstability => "HIGH_INSTABILITY",
            Self::CallersSummary => "CALLERS_SUMMARY",
            Self::CalleesSummary => "CALLEES_SUMMARY",
            Self::ModuleSummary => "MODULE_SUMMARY",
            Self::SnapshotInfo => "SNAPSHOT_INFO",
            Self::ExplainIdentity => "EXPLAIN_IDENTITY",
            Self::ExplainCallers => "EXPLAIN_CALLERS",
            Self::ExplainCallees => "EXPLAIN_CALLEES",
            Self::ExplainImports => "EXPLAIN_IMPORTS",
            Self::ExplainSymbols => "EXPLAIN_SYMBOLS",
            Self::ExplainFiles => "EXPLAIN_FILES",
            Self::ExplainCycles => "EXPLAIN_CYCLES",
            Self::ExplainBoundary => "EXPLAIN_BOUNDARY",
            Self::ExplainGate => "EXPLAIN_GATE",
            Self::ExplainTrust => "EXPLAIN_TRUST",
            Self::ExplainMeasurements => "EXPLAIN_MEASUREMENTS",
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
            // Boundary: violations most urgent, then summary.
            Self::BoundaryViolations => 0,
            Self::BoundaryLinksSummary => 1,
            // Trust (Medium): low-resolution most urgent.
            Self::TrustLowResolution => 0,
            Self::TrustStaleSnapshot => 1,
            Self::TrustNoEnrichment => 2,
            // Structure (Medium): cycles > complexity.
            Self::ImportCycles => 0,
            Self::HighComplexity => 1,
            // Structure (Low): fan-out > instability > callers > callees.
            Self::HighFanOut => 0,
            Self::HighInstability => 1,
            Self::CallersSummary => 2,
            Self::CalleesSummary => 3,
            // Informational (Low): summary > snapshot.
            Self::ModuleSummary => 0,
            Self::SnapshotInfo => 1,
            // Explain (Low): fixed section order by tier_priority.
            Self::ExplainIdentity => 0,
            Self::ExplainCallers => 1,
            Self::ExplainCallees => 2,
            Self::ExplainImports => 3,
            Self::ExplainSymbols => 4,
            Self::ExplainFiles => 5,
            Self::ExplainCycles => 6,
            Self::ExplainBoundary => 7,
            Self::ExplainGate => 8,
            Self::ExplainTrust => 9,
            Self::ExplainMeasurements => 10,
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
        use Severity::*;
        use SignalCategory::*;
        match self {
            Self::CheckPass => (Check, Low),
            Self::CheckFail => (Check, High),
            Self::CheckIncomplete => (Check, Medium),
            Self::GatePass => (Gate, Low),
            Self::GateFail => (Gate, High),
            Self::GateIncomplete => (Gate, Medium),
            Self::BoundaryViolations => (Boundary, High),
            Self::BoundaryLinksSummary => (Boundary, Low),
            Self::TrustLowResolution => (Trust, Medium),
            Self::TrustStaleSnapshot => (Trust, Medium),
            Self::TrustNoEnrichment => (Trust, Low),
            Self::ImportCycles => (Structure, Medium),
            Self::HighComplexity => (Structure, Medium),
            Self::HighFanOut => (Structure, Low),
            Self::HighInstability => (Structure, Low),
            Self::CallersSummary => (Structure, Low),
            Self::CalleesSummary => (Structure, Low),
            Self::ModuleSummary => (Informational, Low),
            Self::SnapshotInfo => (Informational, Low),
            Self::ExplainIdentity => (Explain, Low),
            Self::ExplainCallers => (Explain, Low),
            Self::ExplainCallees => (Explain, Low),
            Self::ExplainImports => (Explain, Low),
            Self::ExplainSymbols => (Explain, Low),
            Self::ExplainFiles => (Explain, Low),
            Self::ExplainCycles => (Explain, Low),
            Self::ExplainBoundary => (Explain, Low),
            Self::ExplainGate => (Explain, Low),
            Self::ExplainTrust => (Explain, Low),
            Self::ExplainMeasurements => (Explain, Low),
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
    /// The RAW total cycle count (every module cycle, test-only included). Preserved
    /// unchanged for back-compat; the headline now PREFERS [`Self::production_count`] when
    /// present.
    pub cycle_count: u64,
    /// ORIENT-CYCLES-DISAGREE-1: the FIXTURE-POLLUTION-1 non-test-only headline count — the
    /// SAME integer `cycles` renders as "N module-level cycle(s) found", derived from the
    /// shared [`crate::cycle_composition`] classifier over the SAME cycle set. `Some` on the
    /// SQLite-served path (where the stored `is_test` fact is reachable); `None` where it is
    /// not (LiveGraph module-cycle serve — §2.3 asymmetry — and focus/path-scoped reads),
    /// in which case the renderer falls back to [`Self::cycle_count`], matching `cycles` on
    /// those same paths. Additive: `None` is omitted from JSON (byte-identical for existing
    /// consumers). NEVER 0-as-unknown — absence is `None`, not zero (Fact Certainty Model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_count: Option<u64>,
    /// ORIENT-CYCLES-DISAGREE-1: the count of positively test-only cycles EXCLUDED from the
    /// headline — the SAME integer `cycles` renders as "+M test-only cycle(s) (excluded ...)".
    /// Paired with [`Self::production_count`] (both `Some` or both `None` from one
    /// classification). Additive; omitted from JSON when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_only_count: Option<u64>,
    /// ORIENT-CYCLES-DISAGREE-1 (operator ruling review-3, 2026-09-03 #2): the SUBSET of
    /// [`Self::production_count`] whose test-composition is UNPROVABLE (a member owns no tracked
    /// file, or a malformed node). Unknown cycles are NEVER demoted — they stay counted in
    /// `production_count` (the headline) — but must also never be counted INVISIBLY, so this
    /// discloses how many of the headline cycles are unknown. The renderer appends
    /// "; test-composition unknown for K" when this is `> 0`. Derived from the SAME classification
    /// as the other two counts: `Some` (possibly `Some(0)`) on the SQLite path, `None` where the
    /// split is not computed (LiveGraph/focus). Additive; omitted from JSON when `None`. NEVER
    /// 0-as-unknown — absence is `None`, a known-zero is `Some(0)` (Fact Certainty Model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_count: Option<u64>,
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
    /// The in-scope-OR-unclassified denominator (known-external calls excluded, but
    /// unclassified calls still IN) — the same denominator trust/orient/check use.
    pub total_count: u64,
    /// RELIABILITY-REFRAME-1 (review-5 §1, additive RR1_BOUNDARY): the UNCLASSIFIED
    /// (`unknown`) portion of `total_count`, so this denominator-bearing signal emits
    /// the SAME material-unclassified caveat as the other rate surfaces when that share
    /// is material. Zero on a fully-classified repo (caveat then silent).
    pub unclassified_count: u64,
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

/// Evidence for `BOUNDARY_LINKS_SUMMARY` signal.
///
/// Reports total boundary interaction links for the snapshot.
/// This is the first signal backed by a freshness-tracked L2 table
/// (`boundary_interaction_links`). The freshness state is attached
/// via `Signal.freshness`, not in the evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryLinksSummaryEvidence {
    /// Total number of boundary interaction links.
    pub link_count: u64,
}

// DeadCodeEvidence, DeadSymbolEvidence — removed.
// Surface withdrawn; internal substrate preserved.
// See docs/TECH-DEBT.md for reintroduction conditions.

/// Breakdown of discovered modules by kind.
///
/// This is only present when module discovery data exists. When
/// absent, the `MODULE_DATA_UNAVAILABLE` limit is emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleKindBreakdown {
    pub declared: u64,
    pub operational: u64,
    pub inferred: u64,
}

/// One named module with its owned-file count, for the dense
/// structure headline (ORIENT-DENSITY-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleSizeEvidence {
    /// Module canonical root path (e.g. `src/http`).
    pub path: String,
    /// Files this module owns in the snapshot.
    pub file_count: u64,
    /// ORIENT-SEGMENT-2 §2.2: the DECLARED module name (`display_name`), when the
    /// detector recorded one; `None` for an inferred/directory module. Carried per
    /// row so orient can render `name [manifest]` on a name collision / divergence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// ORIENT-SEGMENT-2 §2.2: the owning manifest filename (`pyproject.toml` /
    /// `package.json` / `Cargo.toml` / `settings.gradle`), derived from the module
    /// source; `None` for an inferred module (no manifest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
}

/// One logical package/directory group for the `orient` STRUCTURE headline
/// (MODULE-MODEL-1 D2(i)/D4).
///
/// A Layer-0/1 directory-topology fact: a `src/main` + `src/test` merge of a
/// logical package, named with the common source-root prefix collapsed
/// (`owner`, not `src/main/java/org/.../owner`). DISTINCT from the
/// declared/inferred `module_candidates` notion the sibling
/// `discovered_module_count` reports — the two are separately labelled, never
/// collapsed (the cross-command coherence fix). Produced by
/// `package_groups::rollup_package_groups`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageGroupEvidence {
    /// Reader-facing package name (prefix-collapsed), e.g. `owner`.
    pub name: String,
    /// Files owned across `src/main` + `src/test`.
    pub file_count: u64,
    /// How many of those are test files.
    pub test_file_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleSummaryEvidence {
    // ── Snapshot totals (always present) ──────────────────────────
    pub file_count: u64,
    pub symbol_count: u64,
    pub languages: Vec<String>,

    // ── Module discovery data (present when module_candidates exist) ─
    /// Count of discovered modules. `None` when module discovery data
    /// is unavailable (triggers `MODULE_DATA_UNAVAILABLE` limit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_module_count: Option<u64>,
    /// Breakdown by module kind (declared/operational/inferred).
    /// `None` when module discovery data is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_kinds: Option<ModuleKindBreakdown>,
    /// Top modules by size, NAMED — the declared/inferred `module_candidates`
    /// breakdown (ORIENT-DENSITY-1). Additive, within the existing
    /// signal-evidence payload (NO `CoherenceEnvelope` shape change). Empty
    /// (and omitted from JSON) when module discovery data is unavailable.
    ///
    /// MODULE-MODEL-1: this is now the SECONDARY, separately-labelled
    /// "declared/inferred modules" notion. The PRIMARY structure the headline
    /// leads with is `package_groups` (directory topology) below.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_modules: Vec<ModuleSizeEvidence>,

    /// Directory/package groups (Layer 0/1 TOPOLOGY) — the structure the dense
    /// `orient` headline NAMES (MODULE-MODEL-1 D2(i)/D4). Present whenever files
    /// were indexed (independent of `module_candidates`); empty (and omitted)
    /// only when no directory owns files. DISTINCT from `top_modules` /
    /// `discovered_module_count` (the declared/inferred module notion) — the two
    /// are separately labelled, never collapsed. Additive within the existing
    /// evidence payload.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_groups: Vec<PackageGroupEvidence>,

    /// One reader-frame line stating a package-groups limitation, when one applies
    /// (ROOT-MANIFEST-POLYGLOT, ratified 2026-07-12): present only when a repo-root
    /// manifest exists but was SUPPRESSED because nested manifest roots also exist
    /// (so "." folds nothing and its directories degrade to directory groups). The
    /// exact wording comes from `package_groups::root_manifest_limitation` — the
    /// SAME string the `stats` surface carries, so the two agree. `None` (and
    /// omitted from JSON) when nothing is suppressed. This makes the deliberate
    /// honest-degradation VISIBLE on the primary surface, not buried in a comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_manifest_limitation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HighComplexityEvidence {
    /// Count of symbols exceeding the complexity threshold.
    pub high_complexity_count: u64,
    /// Threshold used (e.g., 20).
    pub threshold: u64,
    /// Top N most complex symbols.
    pub top_complex: Vec<ComplexSymbolEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComplexSymbolEvidence {
    /// Symbol name (function/method).
    pub symbol: String,
    /// Owning file path.
    pub file: Option<String>,
    /// Cyclomatic complexity value.
    pub complexity: u64,
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
    /// CHECK-SIGNAL-1 (§2.3): additive machine marker — `Some(true)` iff this condition rendered
    /// its PERMANENT-CEILING form (a LOW / "did not run" reclassified to a passing stated
    /// limitation because every materially-present language has no resolver on this build).
    /// `None` (skipped on the wire) for every ordinary condition, so existing consumers keyed on
    /// `{code, status, summary}` stay byte-compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling: Option<bool>,
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

// ── Explain evidence structs ────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExplainIdentityEvidence {
    pub target_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_test: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainCallerItem {
    pub stable_key: String,
    pub name: String,
    pub module: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainCallersEvidence {
    pub count: u64,
    pub top_modules: Vec<ModuleCountEvidence>,
    pub items: Vec<ExplainCallerItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainCalleeItem {
    pub stable_key: String,
    pub name: String,
    pub module: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainCalleesEvidence {
    pub count: u64,
    pub top_modules: Vec<ModuleCountEvidence>,
    pub items: Vec<ExplainCalleeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainImportItem {
    pub target_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainImportsEvidence {
    pub count: u64,
    pub items: Vec<ExplainImportItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainSymbolItem {
    pub name: String,
    pub subtype: Option<String>,
    pub line_start: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainSymbolsEvidence {
    pub count: u64,
    pub items: Vec<ExplainSymbolItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainFileItem {
    pub path: String,
    pub symbol_count: u64,
    pub is_test: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainFilesEvidence {
    pub count: u64,
    pub items: Vec<ExplainFileItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainCyclesEvidence {
    pub count: u64,
    pub items: Vec<CycleEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainBoundaryEvidence {
    pub violation_count: u64,
    pub items: Vec<BoundaryViolationEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainGateItem {
    pub req_id: String,
    pub obligation_id: String,
    pub method: String,
    pub effective_verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainGateEvidence {
    pub outcome: String,
    pub obligation_count: u64,
    pub items: Vec<ExplainGateItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExplainTrustEvidence {
    pub call_resolution_rate: f64,
    pub call_graph_reliability: String,
    // dead_code_reliability — removed. Surface withdrawn.
    pub enrichment_state: String,
    /// RELIABILITY-REFRAME-1 (review-1 §1): the IN-SCOPE call counts, additive (RR1_BOUNDARY
    /// option A — no existing field removed/renamed). `call_resolution_rate` alone cannot tell a
    /// genuine "100% resolved" from the trust service's 0-of-0 rate sentinel (`1.0`), so a
    /// zero-in-scope-call repo would render a fabricated "your code's calls 100% resolved".
    /// Carrying the counts lets the reader surface build the SAME `CallReliabilityView` as
    /// trust/check and render the honest "no in-scope calls measured" when the denominator is 0.
    /// `resolved_in_scope` ⊆ `in_scope_or_unclassified_total` (known-external calls excluded, but
    /// unclassified calls still IN — hence "or unclassified", review-5 §1: the field name must not
    /// claim the denominator is purely in-scope).
    pub resolved_in_scope: u64,
    pub in_scope_or_unclassified_total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExplainMeasurementItem {
    pub kind: String,
    pub aggregation: String,
    pub value: f64,
    pub subject_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExplainMeasurementsEvidence {
    pub items: Vec<ExplainMeasurementItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_omitted_count: Option<u64>,
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
    HighComplexity(HighComplexityEvidence),
    TrustLowResolution(TrustLowResolutionEvidence),
    TrustStaleSnapshot(TrustStaleSnapshotEvidence),
    TrustNoEnrichment(TrustNoEnrichmentEvidence),
    BoundaryViolations(BoundaryViolationsEvidence),
    BoundaryLinksSummary(BoundaryLinksSummaryEvidence),
    // DeadCode — variant removed. Surface withdrawn.
    ModuleSummary(ModuleSummaryEvidence),
    SnapshotInfo(SnapshotInfoEvidence),
    CallersSummary(CallersSummaryEvidence),
    CalleesSummary(CalleesSummaryEvidence),
    ExplainIdentity(ExplainIdentityEvidence),
    ExplainCallers(ExplainCallersEvidence),
    ExplainCallees(ExplainCalleesEvidence),
    ExplainImports(ExplainImportsEvidence),
    ExplainSymbols(ExplainSymbolsEvidence),
    ExplainFiles(ExplainFilesEvidence),
    ExplainCycles(ExplainCyclesEvidence),
    ExplainBoundary(ExplainBoundaryEvidence),
    ExplainGate(ExplainGateEvidence),
    ExplainTrust(ExplainTrustEvidence),
    ExplainMeasurements(ExplainMeasurementsEvidence),
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
            Self::HighComplexity(e) => e.serialize(serializer),
            Self::TrustLowResolution(e) => e.serialize(serializer),
            Self::TrustStaleSnapshot(e) => e.serialize(serializer),
            Self::TrustNoEnrichment(e) => e.serialize(serializer),
            Self::BoundaryViolations(e) => e.serialize(serializer),
            Self::BoundaryLinksSummary(e) => e.serialize(serializer),
            Self::ModuleSummary(e) => e.serialize(serializer),
            Self::SnapshotInfo(e) => e.serialize(serializer),
            Self::CallersSummary(e) => e.serialize(serializer),
            Self::CalleesSummary(e) => e.serialize(serializer),
            Self::ExplainIdentity(e) => e.serialize(serializer),
            Self::ExplainCallers(e) => e.serialize(serializer),
            Self::ExplainCallees(e) => e.serialize(serializer),
            Self::ExplainImports(e) => e.serialize(serializer),
            Self::ExplainSymbols(e) => e.serialize(serializer),
            Self::ExplainFiles(e) => e.serialize(serializer),
            Self::ExplainCycles(e) => e.serialize(serializer),
            Self::ExplainBoundary(e) => e.serialize(serializer),
            Self::ExplainGate(e) => e.serialize(serializer),
            Self::ExplainTrust(e) => e.serialize(serializer),
            Self::ExplainMeasurements(e) => e.serialize(serializer),
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
            Self::HighComplexity(_) => "HighComplexity",
            Self::TrustLowResolution(_) => "TrustLowResolution",
            Self::TrustStaleSnapshot(_) => "TrustStaleSnapshot",
            Self::TrustNoEnrichment(_) => "TrustNoEnrichment",
            Self::BoundaryViolations(_) => "BoundaryViolations",
            Self::BoundaryLinksSummary(_) => "BoundaryLinksSummary",
            Self::ModuleSummary(_) => "ModuleSummary",
            Self::SnapshotInfo(_) => "SnapshotInfo",
            Self::CallersSummary(_) => "CallersSummary",
            Self::CalleesSummary(_) => "CalleesSummary",
            Self::ExplainIdentity(_) => "ExplainIdentity",
            Self::ExplainCallers(_) => "ExplainCallers",
            Self::ExplainCallees(_) => "ExplainCallees",
            Self::ExplainImports(_) => "ExplainImports",
            Self::ExplainSymbols(_) => "ExplainSymbols",
            Self::ExplainFiles(_) => "ExplainFiles",
            Self::ExplainCycles(_) => "ExplainCycles",
            Self::ExplainBoundary(_) => "ExplainBoundary",
            Self::ExplainGate(_) => "ExplainGate",
            Self::ExplainTrust(_) => "ExplainTrust",
            Self::ExplainMeasurements(_) => "ExplainMeasurements",
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
    /// Freshness info for signals backed by Layer 2+ artifacts (ACR-6).
    /// None for signals backed by L0/L1 facts or governance overlays.
    pub(crate) freshness: Option<FreshnessInfo>,
}

impl Serialize for Signal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Count fields: 7 base + optional scope + optional freshness
        let mut field_count = 7;
        if !self.scope.is_direct() {
            field_count += 1;
        }
        if self.freshness.is_some() {
            field_count += 1;
        }
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
        if let Some(ref freshness) = self.freshness {
            state.serialize_field("freshness", freshness)?;
        }
        state.end()
    }
}

impl Signal {
    // ── Accessors ─────────────────────────────────────────────

    pub fn code(&self) -> SignalCode {
        self.code
    }
    pub fn rank(&self) -> u32 {
        self.rank
    }
    pub fn severity(&self) -> Severity {
        self.severity
    }
    pub fn category(&self) -> SignalCategory {
        self.category
    }
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub fn evidence(&self) -> &SignalEvidence {
        &self.evidence
    }
    pub fn source(&self) -> SourceRef {
        self.source
    }
    pub fn scope(&self) -> SignalScope {
        self.scope
    }
    pub fn freshness(&self) -> Option<&FreshnessInfo> {
        self.freshness.as_ref()
    }

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
    //
    // `freshness` is None by default. Signals backed by L2+
    // artifacts can attach freshness via `with_freshness()`.
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
            freshness: None,
        }
    }

    /// Attach freshness info to a signal (ACR-6).
    ///
    /// Use for signals backed by Layer 2+ artifacts. Returns self
    /// for chaining.
    pub(crate) fn with_freshness(mut self, freshness: FreshnessInfo) -> Self {
        self.freshness = Some(freshness);
        self
    }

    /// EXPLAIN-LIVEGRAPH-IMPL: adopt THIS signal's post-ranking `rank` + emission `scope` onto a
    /// freshly-built `replacement` of the SAME code. The daemon serves an LG-first leaf's VALUE from the
    /// LiveGraph (e.g. `EXPLAIN_IMPORTS` from `live_import_view`, `EXPLAIN_CYCLES` from
    /// `module_import_cycles`), builds the replacement via the public constructor (which resets `rank` to 0
    /// and `scope` to `Direct`), then calls this to restore the ranking pass's rank and the module-context
    /// scope — so the swapped leaf is byte-identical to the original EXCEPT the served value. Panics in debug
    /// if the codes differ (a programming error: only a same-code value-swap is valid).
    pub fn adopt_rank_and_scope(&self, mut replacement: Signal) -> Signal {
        debug_assert_eq!(
            self.code, replacement.code,
            "adopt_rank_and_scope is only valid for a same-code value swap"
        );
        replacement.rank = self.rank;
        replacement.scope = self.scope;
        replacement
    }

    /// EXPLAIN-LIVEGRAPH-IMPL: a clone of this signal's `ExplainIdentityEvidence` iff it is an
    /// `EXPLAIN_IDENTITY` signal (else `None`). The daemon reads the SQLite-built identity evidence, OVERRIDES
    /// the `name`/`subtype` anchor fields with the current-state LiveGraph values (`LiveGraph::node_display`),
    /// and rebuilds the leaf — the D8 multi-source `{livegraph, sqlite}` identity (anchor from LiveGraph,
    /// coordinate fields from SQLite). Keeps the `SignalEvidence` matching INSIDE the agent crate.
    pub fn explain_identity_evidence(&self) -> Option<ExplainIdentityEvidence> {
        match &self.evidence {
            SignalEvidence::ExplainIdentity(ev) => Some(ev.clone()),
            _ => None,
        }
    }

    /// EXPLAIN-LIVEGRAPH-IMPL: a clone of this signal's `ExplainCallersEvidence` iff it is an
    /// `EXPLAIN_CALLERS` signal (else `None`). The daemon reads the SQLite-built caller evidence (its
    /// SQL-ordered rendered item subset + `top_modules` grouping + full `count`) and rebuilds the leaf with
    /// each item's LIVE name from current-state LiveGraph IR (`LiveGraph::node_display`), gated by the migrated
    /// `callers` no-loss key compare — the multi-source `{livegraph, sqlite}` callgraph leaf (the caller
    /// identity set + names from LiveGraph; the per-item module, which has no LiveGraph/IR home, from SQLite).
    pub fn explain_callers_evidence(&self) -> Option<ExplainCallersEvidence> {
        match &self.evidence {
            SignalEvidence::ExplainCallers(ev) => Some(ev.clone()),
            _ => None,
        }
    }

    /// EXPLAIN-LIVEGRAPH-IMPL: the `EXPLAIN_CALLEES` dual of [`Signal::explain_callers_evidence`] — a clone of
    /// this signal's `ExplainCalleesEvidence` iff it is an `EXPLAIN_CALLEES` signal (else `None`).
    pub fn explain_callees_evidence(&self) -> Option<ExplainCalleesEvidence> {
        match &self.evidence {
            SignalEvidence::ExplainCallees(ev) => Some(ev.clone()),
            _ => None,
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
        // ORIENT-CYCLES-DISAGREE-1: the descriptor uses the SAME exclusion-aware count the
        // human headline and `cycles` render — the production count (+ test-only disclosure)
        // when the serving computation labeled the split, else the raw total (LiveGraph/focus
        // path). So no field of orient's own output states a different cycle count.
        let plural = |n: u64| if n == 1 { "" } else { "s" };
        // ORIENT-CYCLES-DISAGREE-1 (operator ruling review-3 #2): the disclosure clause carries
        // BOTH the excluded test-only count AND the unknown subset (when > 0) — an unknown cycle
        // stays counted in `prod` (never demoted) but is never invisible. Absent counts (the
        // LiveGraph/focus fallback) render nothing.
        let disclose = |test_only: Option<u64>, unknown: Option<u64>| -> String {
            let mut clauses: Vec<String> = Vec::new();
            if let Some(t) = test_only {
                if t > 0 {
                    clauses.push(format!("+{t} test-only excluded"));
                }
            }
            if let Some(k) = unknown {
                if k > 0 {
                    clauses.push(format!("test-composition unknown for {k}"));
                }
            }
            if clauses.is_empty() {
                String::new()
            } else {
                format!(" ({})", clauses.join("; "))
            }
        };
        let summary = match evidence.production_count {
            Some(prod) => format!(
                "{} import cycle{} detected at the module level{}.",
                prod,
                plural(prod),
                disclose(evidence.test_only_count, evidence.unknown_count)
            ),
            None => format!(
                "{} import cycle{} detected at the module level.",
                evidence.cycle_count,
                plural(evidence.cycle_count)
            ),
        };
        Self::build(
            SignalCode::ImportCycles,
            summary,
            SignalEvidence::ImportCycles(evidence),
            SourceRef::StorageFindModuleCycles,
        )
    }

    pub fn high_complexity(evidence: HighComplexityEvidence) -> Self {
        let summary = format!(
            "{} symbol{} exceed{} complexity threshold of {}.",
            evidence.high_complexity_count,
            if evidence.high_complexity_count == 1 {
                ""
            } else {
                "s"
            },
            if evidence.high_complexity_count == 1 {
                "s"
            } else {
                ""
            },
            evidence.threshold
        );
        Self::build(
            SignalCode::HighComplexity,
            summary,
            SignalEvidence::HighComplexity(evidence),
            SourceRef::StorageQueryHighComplexitySymbols,
        )
    }

    pub fn trust_low_resolution(evidence: TrustLowResolutionEvidence) -> Self {
        // RELIABILITY-REFRAME-1 (review-2 §2): route the "% resolved" WORDING through the ONE
        // shared projection (`reliability::resolved_phrase_pct`) so this reader-visible summary
        // can never re-derive an external-inclusive "% resolved" string that forks from the
        // orient headline / trust / check. review-5 §1: the denominator is "in-scope OR
        // unclassified" — known-external calls are excluded but unclassified ones stay IN, so the
        // label says so and the material-unclassified caveat rides here too (the SAME helper the
        // other rate surfaces use) when that share is material. Sentence-cased (stands alone).
        let head = crate::reliability::sentence_case(&crate::reliability::resolved_phrase_pct(
            evidence.resolution_rate * 100.0,
        ));
        let caveat = crate::reliability::unclassified_caveat(
            evidence.unclassified_count,
            evidence.total_count,
        )
        .map(|c| format!("{c}; "))
        .unwrap_or_default();
        let summary = format!(
            "{head} ({} of {} in-scope or unclassified) — calls into external libraries are \
             excluded; {caveat}verify call/dead claims against source.",
            evidence.resolved_count, evidence.total_count,
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
            if evidence.stale_file_count == 1 {
                ""
            } else {
                "s"
            }
        );
        Self::build(
            SignalCode::TrustStaleSnapshot,
            summary,
            SignalEvidence::TrustStaleSnapshot(evidence),
            SourceRef::StorageGetStaleFiles,
        )
    }

    pub fn trust_no_enrichment(evidence: TrustNoEnrichmentEvidence) -> Self {
        // CONTRADICTION-SWEEP-1 §2.4 (operator ruling CS1-4, OPTION A — one COMPUTATION per fact):
        // trust CALLS the ONE shared enrichment-state accessor `check::enrichment_state_summary`,
        // the SAME function `check`'s condition and the `reliability` breakdown call — it does NOT
        // reach for a baked phrase constant (phrase-sharing over an independent computation is the
        // divergence defect itself). This signal exists only for the NotRun state, so it resolves
        // that state THROUGH the shared computation; if `check` ever changes how a not-run phase is
        // worded at its one home, this trust clause moves with it (kills contradiction #4). Trust
        // appends only the resolution CONSEQUENCE this signal exists to state.
        let state_clause = crate::check::enrichment_state_summary(Some(
            crate::storage_port::EnrichmentState::NotRun,
        ));
        let summary =
            format!("{state_clause} Call graph resolution relies on syntax-only extraction.");
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
            if evidence.violation_count == 1 {
                ""
            } else {
                "s"
            }
        );
        Self::build(
            SignalCode::BoundaryViolations,
            summary,
            SignalEvidence::BoundaryViolations(evidence),
            SourceRef::StorageFindImportsBetweenPaths,
        )
    }

    /// Boundary interaction links summary with freshness.
    ///
    /// This is the first signal backed by a freshness-tracked L2 table.
    /// Freshness should be attached via `.with_freshness()` by the
    /// aggregator.
    pub fn boundary_links_summary(evidence: BoundaryLinksSummaryEvidence) -> Self {
        let summary = format!(
            "{} boundary interaction link{}.",
            evidence.link_count,
            if evidence.link_count == 1 { "" } else { "s" }
        );
        Self::build(
            SignalCode::BoundaryLinksSummary,
            summary,
            SignalEvidence::BoundaryLinksSummary(evidence),
            SourceRef::StorageGetBoundaryLinksFreshness,
        )
    }

    // Signal::dead_code() — constructor removed. Surface withdrawn.
    // See docs/TECH-DEBT.md for reintroduction conditions.

    pub fn module_summary(evidence: ModuleSummaryEvidence) -> Self {
        // Build summary text. Include module count when available.
        let base = format!(
            "{} file{}, {} symbol{} indexed",
            evidence.file_count,
            if evidence.file_count == 1 { "" } else { "s" },
            evidence.symbol_count,
            if evidence.symbol_count == 1 { "" } else { "s" }
        );
        let summary = match evidence.discovered_module_count {
            Some(count) => format!(
                "{}; {} discovered module{}.",
                base,
                count,
                if count == 1 { "" } else { "s" }
            ),
            None => format!("{}.", base),
        };
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
            if evidence.top_modules.len() == 1 {
                ""
            } else {
                "s"
            },
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
            if evidence.top_modules.len() == 1 {
                ""
            } else {
                "s"
            },
        );
        Self::build(
            SignalCode::CalleesSummary,
            summary,
            SignalEvidence::CalleesSummary(evidence),
            SourceRef::StorageFindSymbolCallees,
        )
    }

    // ── Explain constructors ────────────────────────────────────

    pub fn explain_identity(evidence: ExplainIdentityEvidence) -> Self {
        let summary = format!("Identity: {} target.", evidence.target_kind,);
        Self::build(
            SignalCode::ExplainIdentity,
            summary,
            SignalEvidence::ExplainIdentity(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_callers(evidence: ExplainCallersEvidence) -> Self {
        let summary = format!(
            "{} direct caller{}.",
            evidence.count,
            if evidence.count == 1 { "" } else { "s" },
        );
        Self::build(
            SignalCode::ExplainCallers,
            summary,
            SignalEvidence::ExplainCallers(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_callees(evidence: ExplainCalleesEvidence) -> Self {
        let summary = format!(
            "{} direct callee{}.",
            evidence.count,
            if evidence.count == 1 { "" } else { "s" },
        );
        Self::build(
            SignalCode::ExplainCallees,
            summary,
            SignalEvidence::ExplainCallees(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_imports(evidence: ExplainImportsEvidence) -> Self {
        let summary = format!(
            "{} imported file{}.",
            evidence.count,
            if evidence.count == 1 { "" } else { "s" },
        );
        Self::build(
            SignalCode::ExplainImports,
            summary,
            SignalEvidence::ExplainImports(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_symbols(evidence: ExplainSymbolsEvidence) -> Self {
        let summary = format!(
            "{} symbol{} in file.",
            evidence.count,
            if evidence.count == 1 { "" } else { "s" },
        );
        Self::build(
            SignalCode::ExplainSymbols,
            summary,
            SignalEvidence::ExplainSymbols(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_files(evidence: ExplainFilesEvidence) -> Self {
        let summary = format!(
            "{} file{} in path.",
            evidence.count,
            if evidence.count == 1 { "" } else { "s" },
        );
        Self::build(
            SignalCode::ExplainFiles,
            summary,
            SignalEvidence::ExplainFiles(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_cycles(evidence: ExplainCyclesEvidence) -> Self {
        let summary = format!(
            "{} import cycle{}.",
            evidence.count,
            if evidence.count == 1 { "" } else { "s" },
        );
        Self::build(
            SignalCode::ExplainCycles,
            summary,
            SignalEvidence::ExplainCycles(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_boundary(evidence: ExplainBoundaryEvidence) -> Self {
        let summary = format!(
            "{} boundary violation{}.",
            evidence.violation_count,
            if evidence.violation_count == 1 {
                ""
            } else {
                "s"
            },
        );
        Self::build(
            SignalCode::ExplainBoundary,
            summary,
            SignalEvidence::ExplainBoundary(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_gate(evidence: ExplainGateEvidence) -> Self {
        let summary = format!(
            "Gate {}: {} obligation{}.",
            evidence.outcome,
            evidence.obligation_count,
            if evidence.obligation_count == 1 {
                ""
            } else {
                "s"
            },
        );
        Self::build(
            SignalCode::ExplainGate,
            summary,
            SignalEvidence::ExplainGate(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_trust(evidence: ExplainTrustEvidence) -> Self {
        // RELIABILITY-REFRAME-1 (review-1 §1): build the reader-frame summary from the ONE shared
        // projection off the IN-SCOPE COUNTS — so 0-of-0 reads "no in-scope calls measured"
        // (unknown), never the `call_resolution_rate` 1.0 sentinel's fabricated 100%. This routes
        // the explain SIGNAL summary through the same `CallReliabilityView` orient/trust/check use,
        // folding rate+band into one reader-frame phrase. (DoD §4: NO reader surface grades
        // repo-graph's own pipeline coverage.)
        let view = crate::reliability::CallReliabilityView::derive(
            evidence.resolved_in_scope,
            evidence
                .in_scope_or_unclassified_total
                .saturating_sub(evidence.resolved_in_scope),
            0,
            evidence.in_scope_or_unclassified_total,
            Vec::new(),
            crate::reliability::band_from_wire(&evidence.call_graph_reliability),
        );
        let summary = format!("Trust: {}.", view.resolved_with_band());
        Self::build(
            SignalCode::ExplainTrust,
            summary,
            SignalEvidence::ExplainTrust(evidence),
            SourceRef::ExplainPipeline,
        )
    }

    pub fn explain_measurements(evidence: ExplainMeasurementsEvidence) -> Self {
        let summary = format!(
            "{} measurement{}.",
            evidence.items.len(),
            if evidence.items.len() == 1 { "" } else { "s" },
        );
        Self::build(
            SignalCode::ExplainMeasurements,
            summary,
            SignalEvidence::ExplainMeasurements(evidence),
            SourceRef::ExplainPipeline,
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
        // SignalCode::DeadCode — removed. Surface withdrawn.
        assert_eq!(
            SignalCode::SnapshotInfo.descriptor(),
            (SignalCategory::Informational, Severity::Low),
        );
    }

    #[test]
    fn category_tie_break_order_is_check_first() {
        assert!(
            SignalCategory::Check.tie_break_ordinal() < SignalCategory::Gate.tie_break_ordinal()
        );
        assert!(
            SignalCategory::Gate.tie_break_ordinal() < SignalCategory::Boundary.tie_break_ordinal()
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
            production_count: None,
            test_only_count: None,
            unknown_count: None,
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

    // constructor_invariant_dead_code — test removed. Surface withdrawn.

    #[test]
    fn constructor_invariant_module_summary() {
        // Fallback case: no module discovery data
        let s = Signal::module_summary(ModuleSummaryEvidence {
            file_count: 10,
            symbol_count: 100,
            languages: vec!["rust".into()],
            discovered_module_count: None,
            module_kinds: None,
            top_modules: Vec::new(),
            package_groups: Vec::new(),
            root_manifest_limitation: None,
        });
        assert_eq!(s.code, SignalCode::ModuleSummary);
        assert_eq!(s.category, SignalCategory::Informational);
        assert!(s.summary.contains("10 files"));
        assert!(!s.summary.contains("discovered module"));
    }

    #[test]
    fn constructor_invariant_module_summary_with_modules() {
        // Module discovery data present
        let s = Signal::module_summary(ModuleSummaryEvidence {
            file_count: 50,
            symbol_count: 200,
            languages: vec!["typescript".into(), "rust".into()],
            discovered_module_count: Some(5),
            module_kinds: Some(ModuleKindBreakdown {
                declared: 3,
                operational: 1,
                inferred: 1,
            }),
            top_modules: vec![
                ModuleSizeEvidence {
                    path: "src/http".into(),
                    file_count: 30,
                    name: None,
                    manifest: None,
                },
                ModuleSizeEvidence {
                    path: "src/core".into(),
                    file_count: 12,
                    name: None,
                    manifest: None,
                },
            ],
            package_groups: Vec::new(),
            root_manifest_limitation: None,
        });
        assert_eq!(s.code, SignalCode::ModuleSummary);
        assert!(s.summary.contains("50 files"));
        assert!(s.summary.contains("5 discovered modules"));
        // ORIENT-DENSITY-1: the NAMED modules ride in the evidence, carried
        // verbatim into the serialized payload (the dense structure headline
        // reads them; the summary string is unchanged).
        match s.evidence() {
            SignalEvidence::ModuleSummary(e) => {
                assert_eq!(e.top_modules.len(), 2);
                assert_eq!(e.top_modules[0].path, "src/http");
                assert_eq!(e.top_modules[0].file_count, 30);
            }
            other => panic!(
                "expected ModuleSummary evidence, got {}",
                other.variant_name()
            ),
        }
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
            unclassified_count: 0,
        });
        assert_eq!(s.code, SignalCode::TrustLowResolution);
        assert_eq!(s.category, SignalCategory::Trust);
        assert_eq!(s.severity, Severity::Medium);
    }

    #[test]
    fn trust_low_resolution_summary_is_shared_reader_frame() {
        // RELIABILITY-REFRAME-1 (review-2 §2): the summary's "% resolved" wording comes from the
        // ONE shared projection (`reliability::resolved_phrase_pct`), sentence-cased — never a
        // bespoke "Call resolution rate is …" string that could fork external-inclusive from the
        // orient headline / trust / check. review-5 §1: `total_count` is the "in-scope OR
        // unclassified" denominator; the label says so, and with the unclassified share immaterial
        // (0 here) NO caveat fires. The reader reads where THEIR code stands, not a grade of
        // repo-graph's pipeline.
        let s = Signal::trust_low_resolution(TrustLowResolutionEvidence {
            resolution_rate: 0.12,
            resolved_count: 3,
            total_count: 25,
            unclassified_count: 0,
        });
        assert_eq!(
            s.summary,
            "Your code's calls 12% resolved (3 of 25 in-scope or unclassified) — calls into \
             external libraries are excluded; verify call/dead claims against source."
        );
        // Never the old pipeline-grade wording, and no false "purely in-scope" label.
        assert!(!s.summary.contains("Call resolution rate is"));
        assert!(!s.summary.contains("in-scope)"));
    }

    #[test]
    fn trust_low_resolution_summary_emits_material_unclassified_caveat() {
        // RELIABILITY-REFRAME-1 (review-5 §1): this denominator-bearing rate surface emits the
        // SAME material-unclassified caveat the trust/orient/check surfaces do, so a "low" rate
        // that is really "mostly unclassified" reads honestly (the true resolved share may be
        // higher). 20 of 25 (80% ≥ 20% material) fires the caveat.
        let s = Signal::trust_low_resolution(TrustLowResolutionEvidence {
            resolution_rate: 0.12,
            resolved_count: 3,
            total_count: 25,
            unclassified_count: 20,
        });
        assert!(
            s.summary.contains("20 of these 25 calls are unclassified"),
            "material-unclassified caveat present: {}",
            s.summary
        );
        assert!(
            s.summary.contains("true resolved share may be higher"),
            "{}",
            s.summary
        );
        // Label still honest, and the caveat sits before the verify tail.
        assert!(s.summary.contains("(3 of 25 in-scope or unclassified)"));
        assert!(s
            .summary
            .ends_with("verify call/dead claims against source."));
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

    // CONTRADICTION-SWEEP-1 §2.4 (operator ruling CS1-4, OPTION A): trust words its enrichment STATE by
    // CALLING the ONE shared accessor `check::enrichment_state_summary` — not by baking a phrase. This
    // test proves the SHARED READ, not phrase equality: trust's clause is exactly whatever that function
    // returns for NotRun. If check re-words the not-run state at its one home, this assertion tracks it
    // (there is no local literal to fall out of sync). Doctor's LAST-PASS lifecycle is a distinct
    // daemon-wide fact with its own labelled line (ruling CS1-4), not this per-snapshot state.
    #[test]
    fn trust_no_enrichment_words_state_via_shared_accessor_call() {
        use crate::check::enrichment_state_summary;
        use crate::storage_port::EnrichmentState;
        let s = Signal::trust_no_enrichment(TrustNoEnrichmentEvidence {
            enrichment_eligible: 10,
            enrichment_enriched: 0,
        });
        // Resolve the NotRun state through the SAME function trust calls; trust's summary must open
        // with exactly that string. No trust-local constant is referenced — the accessor IS the source.
        let shared = enrichment_state_summary(Some(EnrichmentState::NotRun));
        assert!(
            s.summary.starts_with(shared),
            "trust must open with the shared accessor's NotRun phrase ({shared:?}): {}",
            s.summary
        );
    }

    #[test]
    fn signal_serializes_with_flat_evidence_object() {
        let s = Signal::import_cycles(ImportCyclesEvidence {
            cycle_count: 1,
            production_count: None,
            test_only_count: None,
            unknown_count: None,
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

    // ── Freshness tests (ACR-6) ──────────────────────────────────

    #[test]
    fn freshness_state_dto_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&FreshnessStateDto::Current).unwrap(),
            "\"current\""
        );
        assert_eq!(
            serde_json::to_string(&FreshnessStateDto::Impacted).unwrap(),
            "\"impacted\""
        );
        assert_eq!(
            serde_json::to_string(&FreshnessStateDto::Unknown).unwrap(),
            "\"unknown\""
        );
    }

    #[test]
    fn freshness_info_current_constructor() {
        let info = FreshnessInfo::current();
        assert_eq!(info.state, FreshnessStateDto::Current);
        assert!(info.impacted_since.is_none());
    }

    #[test]
    fn freshness_info_impacted_constructor() {
        let info = FreshnessInfo::impacted("2026-05-09T12:00:00Z");
        assert_eq!(info.state, FreshnessStateDto::Impacted);
        assert_eq!(info.impacted_since, Some("2026-05-09T12:00:00Z".into()));
    }

    #[test]
    fn freshness_info_unknown_constructor() {
        let info = FreshnessInfo::unknown();
        assert_eq!(info.state, FreshnessStateDto::Unknown);
        assert!(info.impacted_since.is_none());
    }

    #[test]
    fn freshness_info_serializes_without_null_impacted_since() {
        let info = FreshnessInfo::current();
        let s = serde_json::to_string(&info).unwrap();
        // impacted_since is None, so should be omitted
        assert!(!s.contains("impacted_since"));
        assert!(s.contains("\"state\":\"current\""));
    }

    #[test]
    fn freshness_info_serializes_with_impacted_since() {
        let info = FreshnessInfo::impacted("2026-05-09T12:00:00Z");
        let s = serde_json::to_string(&info).unwrap();
        assert!(s.contains("\"state\":\"impacted\""));
        assert!(s.contains("\"impacted_since\":\"2026-05-09T12:00:00Z\""));
    }

    #[test]
    fn signal_without_freshness_omits_field() {
        let s = Signal::snapshot_info(SnapshotInfoEvidence {
            snapshot_uid: "snap-1".into(),
            scope: "full".into(),
            basis_commit: None,
            created_at: "2026-05-09T00:00:00Z".into(),
        });
        let json = serde_json::to_string(&s).unwrap();
        // freshness is None, so should be omitted
        assert!(!json.contains("\"freshness\""));
    }

    #[test]
    fn signal_with_freshness_includes_field() {
        let s = Signal::snapshot_info(SnapshotInfoEvidence {
            snapshot_uid: "snap-1".into(),
            scope: "full".into(),
            basis_commit: None,
            created_at: "2026-05-09T00:00:00Z".into(),
        })
        .with_freshness(FreshnessInfo::impacted("2026-05-08T00:00:00Z"));

        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"freshness\":{"));
        assert!(json.contains("\"state\":\"impacted\""));
        assert!(json.contains("\"impacted_since\":\"2026-05-08T00:00:00Z\""));
    }
}
