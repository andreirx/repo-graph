//! Trust reporting type definitions.
//!
//! Mirror of `src/core/trust/types.ts`. Pure domain types.
//! No I/O, no CLI concerns.
//!
//! The trust surface answers: before acting on an `rgr` result,
//! how reliable is that result on THIS repo, given known
//! extraction limitations?
//!
//! ── DTO boundary discipline ───────────────────────────────────
//!
//! Per the Rust-4 lock:
//! - No `HashMap` in the public API. Deterministic collections
//!   only (`Vec<T>`, sorted DTOs, `BTreeMap` if key-value
//!   semantics are needed).
//! - `ExtractionDiagnostics.unresolved_breakdown` uses
//!   `BTreeMap<String, u64>`. BTreeMap enforces key uniqueness
//!   (matching TS `Record<string, number>` where keys cannot
//!   duplicate), has deterministic alphabetical iteration order
//!   (satisfying the no-HashMap rule), and serializes naturally
//!   as a JSON object. An earlier version used `Vec<(String, u64)>`
//!   with custom serde; that was corrected because Vec admits
//!   duplicate keys, which is an invalid state under the TS
//!   contract.
//! - `TrustReport.toolchain` uses
//!   `Option<serde_json::Map<String, serde_json::Value>>` because
//!   the TS source declares it as `Record<string, unknown> | null`.
//!   The top-level shape must be a JSON object or null — not an
//!   array, string, number, or boolean. Using `serde_json::Map`
//!   instead of bare `serde_json::Value` enforces the object-only
//!   constraint at the type level. An earlier version used
//!   `Option<Value>` which admitted non-object shapes; that was
//!   corrected because it widened the TS contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Map;

// ── Extraction diagnostics ───────────────────────────────────────

/// Extraction diagnostics payload stored on each snapshot.
/// Mirror of `ExtractionDiagnostics` from `types.ts:23`.
///
/// `unresolved_breakdown` uses `BTreeMap<String, u64>` which
/// enforces key uniqueness (matching the TS `Record<string, number>`
/// contract where keys cannot duplicate) and has deterministic
/// alphabetical iteration order (satisfying the no-HashMap rule).
/// Serde serializes BTreeMap naturally as a JSON object, no custom
/// serde needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtractionDiagnostics {
    pub diagnostics_version: u32,
    pub edges_total: u64,
    pub unresolved_total: u64,
    /// Category key → unresolved count.
    ///
    /// `BTreeMap` for three reasons:
    ///   1. Key uniqueness — matches TS `Record` where each
    ///      category key appears exactly once. A `Vec<(String, u64)>`
    ///      could encode duplicate keys, creating invalid JSON.
    ///   2. Deterministic alphabetical order — satisfies the
    ///      no-HashMap API rule from the Rust-4 lock.
    ///   3. Natural serde — BTreeMap serializes as a JSON object
    ///      without custom serde machinery.
    pub unresolved_breakdown: BTreeMap<String, u64>,
}

// ── Reliability ──────────────────────────────────────────────────

/// Reliability level for a trust axis. Mirror of
/// `ReliabilityLevel` from `types.ts:34`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReliabilityLevel {
    HIGH,
    MEDIUM,
    LOW,
}

/// Reliability assessment on one axis. Mirror of
/// `ReliabilityAxisScore` from `types.ts:42`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReliabilityAxisScore {
    pub level: ReliabilityLevel,
    pub reasons: Vec<String>,
}

impl Default for ReliabilityAxisScore {
    /// Default: HIGH with no reasons.
    ///
    /// Used when deserializing JSON that omits the dead_code axis
    /// (post-withdrawal). HIGH is the safest assumption: if data
    /// is missing, don't claim degraded reliability.
    fn default() -> Self {
        Self {
            level: ReliabilityLevel::HIGH,
            reasons: Vec::new(),
        }
    }
}

/// A downgrade trigger. Mirror of `DowngradeTrigger` from
/// `types.ts:51`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DowngradeTrigger {
    pub triggered: bool,
    pub reasons: Vec<String>,
}

/// The four named downgrade triggers. Mirror of
/// `TrustDowngrades` from `types.ts:56`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustDowngrades {
    pub framework_heavy_suspicion: DowngradeTrigger,
    pub registry_pattern_suspicion: DowngradeTrigger,
    pub missing_entrypoint_declarations: DowngradeTrigger,
    pub alias_resolution_suspicion: DowngradeTrigger,
}

/// The four reliability axes. Mirror of `TrustReliability` from
/// `types.ts:63`.
///
/// ── Dead-code axis suppression ───────────────────────────────────
///
/// The `dead_code` axis is preserved internally for future
/// coverage-backed or heuristic reintroduction, but is NOT
/// serialized to user-facing JSON output. The `rmap dead` surface
/// is withdrawn; exposing `dead_code` in `rmap trust` would leak
/// the withdrawn concept. Internal computation continues; public
/// serialization stops.
///
/// If this axis is reintroduced, remove `skip_serializing` and
/// update `docs/cli/rmap-contracts.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustReliability {
    pub import_graph: ReliabilityAxisScore,
    pub call_graph: ReliabilityAxisScore,
    /// Internal-only. Not serialized to user-facing output.
    #[serde(skip_serializing, default)]
    pub dead_code: ReliabilityAxisScore,
    pub change_impact: ReliabilityAxisScore,
}

// ── Trust summary ────────────────────────────────────────────────

/// Aggregate trust summary with counts + reliability + downgrades.
/// Mirror of `TrustSummary` from `types.ts:70`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustSummary {
    pub edges_total: u64,
    pub edges_resolved: u64,
    pub unresolved_total: u64,
    pub resolved_calls: u64,
    pub unresolved_calls: u64,
    pub unresolved_calls_external: u64,
    pub unresolved_calls_internal_like: u64,
    pub call_resolution_rate: f64,
    pub reliability: TrustReliability,
    pub triggered_downgrades: TrustDowngrades,
}

// ── Category + classification rows ───────────────────────────────

/// One row in the per-category breakdown. Mirror of
/// `TrustCategoryRow` from `types.ts:109`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustCategoryRow {
    pub category: String,
    pub label: String,
    pub unresolved: u64,
}

/// One row in the classifier-bucket breakdown. Mirror of
/// `TrustClassificationRow` from `types.ts:145`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustClassificationRow {
    pub classification: String,
    pub count: u64,
}

/// One row in the basis-code breakdown (ATTRIBUTION-1) — the FINER axis behind
/// [`TrustClassificationRow`].
///
/// The coarse 4-value classification folds third-party dependencies, the standard
/// library, and runtime globals into one `external_library_candidate` bucket; the 17
/// `UnresolvedEdgeBasisCode` values keep them apart, which is what a reader-frame
/// attribution breakdown needs. The `basis_code` is the enum's snake_case
/// serialization as a `String` (consistent with the sibling classification row, and
/// forward-compatible: a newer daemon's unknown code deserializes fine and the rgr
/// renderer folds it into the honest "other" bucket rather than failing to parse).
///
/// Rust-only: this rides the COHERENT wire's `basis_classifications` leaf, NOT the
/// TS-parity v1 `TrustReport` wire (that field is `#[serde(skip)]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustBasisClassificationRow {
    pub basis_code: String,
    pub count: u64,
}

/// One named-dependency row in the external-dependency attribution (ATTRIBUTION-1) — the
/// report/wire mirror of [`crate::storage_port::NamedDependencyCount`] (same fields; the
/// service maps port → report, exactly as it maps `BasisCodeCountRow` →
/// [`TrustBasisClassificationRow`]).
///
/// `name` is the DECLARED dependency the reference resolved to (`repo-graph-indexer`,
/// `serde`) — the honest dependency identity, never a fabricated version. Rust-only: rides
/// the COHERENT wire's `external_dependencies` leaf inside
/// [`TrustExternalDependencyAttribution`], NOT the TS-parity v1 `TrustReport` wire (that
/// field is `#[serde(skip)]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustNamedDependencyRow {
    pub name: String,
    pub count: u64,
}

/// The reader-frame attribution of the EXTERNAL-import unresolved references (ATTRIBUTION-1
/// iteration 3) — the report/wire mirror of
/// [`crate::storage_port::ExternalDependencyAttribution`].
///
/// The provenance join names each external-import reference by its DECLARED dependency across
/// all three call bases (`repo-graph-indexer`, `serde`, `express`, `react`), reducing scoped
/// specifiers to the manifest name. `top` is the bounded leaders; `total_named` /
/// `unidentified` reconcile the "library call" class (`total_named + unidentified` == the
/// ExternalDependency class total), letting the renderer show "other declared dependencies"
/// for the identified-but-unlisted remainder and the honest "dependency not identified"
/// bucket. Rust-only: rides the COHERENT wire's `external_dependencies` leaf, NOT the
/// TS-parity v1 `TrustReport` wire (that field is `#[serde(skip)]`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustExternalDependencyAttribution {
    pub top: Vec<TrustNamedDependencyRow>,
    pub total_named: u64,
    pub unidentified: u64,
}

// ── Blast-radius breakdown ───────────────────────────────────────

/// Blast-radius counts for unknown-classified CALLS edges. Mirror
/// of `UnknownCallsBlastRadiusBreakdown` from `types.ts:129`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownCallsBlastRadiusBreakdown {
    pub low: u64,
    pub medium: u64,
    pub high: u64,
}

// ── Enrichment status ────────────────────────────────────────────

/// Top-resolved-type entry for enrichment status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentTopType {
    #[serde(rename = "type")]
    pub type_name: String,
    pub count: u64,
    pub is_external: bool,
}

/// Enrichment status for unknown CALLS. Mirror of
/// `EnrichmentStatus` from `types.ts:136`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrichmentStatus {
    pub eligible: u64,
    pub enriched: u64,
    /// Top receiver types by count — MIXED internal + external, truncated to 15.
    /// The established TS-parity shape; UNCHANGED (RELIABILITY-REFRAME-1 keeps every
    /// existing consumer, incl. the `--json`/daemon reader, byte-identical).
    pub top_types: Vec<EnrichmentTopType>,
    /// RELIABILITY-REFRAME-1 (review-3 §3): the top EXTERNAL receiver types, selected
    /// by FILTERING all external types FIRST and truncating AFTER — so a genuine top
    /// external target is never dropped by `top_types`' top-15-MIXED cut (that cut can
    /// evict an external ranked below 15th overall even when it is a top external).
    /// The reader-frame coverage map (`trust`'s Likely-External section, `orient`'s map,
    /// `check`) draws from THIS, never from `top_types` re-filtered.
    ///
    /// Additive: `#[serde(default)]` so pre-slice JSON deserializes (empty), and
    /// `skip_serializing_if` so the two `enrichment_status: null` parity fixtures — and
    /// any snapshot with no external receivers — stay byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_external_types: Vec<EnrichmentTopType>,
}

// ── Module trust row ─────────────────────────────────────────────

/// Per-module trust assessment row. Mirror of `ModuleTrustRow`
/// from `types.ts:151`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleTrustRow {
    pub module_stable_key: String,
    pub qualified_name: String,
    pub fan_in: u64,
    pub fan_out: u64,
    pub file_count: u64,
    pub suspicious_zero_connectivity: bool,
    pub trust_notes: Vec<String>,
}

// ── Trust report (full output) ───────────────────────────────────

/// The full trust report for a snapshot. Mirror of `TrustReport`
/// from `types.ts:163`.
///
/// `toolchain` is `Option<Map<String, Value>>` because the TS
/// source declares it as `Record<string, unknown> | null`. The
/// top-level shape must be a JSON object (not an array, string,
/// number, or boolean) or null. Using `serde_json::Map` instead
/// of `serde_json::Value` enforces the object-only constraint at
/// the type level, matching the TS contract exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustReport {
    pub snapshot_uid: String,
    /// Human-readable repo name for CLI presentation.
    /// Populated by daemon from registry alias (if present) or
    /// basename of the canonical repo path. `None` only when
    /// identity resolution failed (defensive) or when
    /// deserializing legacy JSON that lacks the field.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub display_name: Option<String>,
    pub basis_commit: Option<String>,
    pub toolchain: Option<Map<String, serde_json::Value>>,
    pub diagnostics_version: Option<u32>,
    pub summary: TrustSummary,
    pub categories: Vec<TrustCategoryRow>,
    pub classifications: Vec<TrustClassificationRow>,
    pub unknown_calls_blast_radius: Option<UnknownCallsBlastRadiusBreakdown>,
    pub enrichment_status: Option<EnrichmentStatus>,
    pub modules: Vec<ModuleTrustRow>,
    pub caveats: Vec<String>,
    pub diagnostics_available: bool,

    /// Internal Rust-only counter exposing the number of
    /// `CallsObjMethodNeedsTypeInfo` samples seen by the
    /// enrichment phase.
    ///
    /// This field is NOT serialized (`#[serde(skip)]`) — it
    /// does not appear in TS parity fixtures or Rust-produced
    /// JSON, so it does not affect the TrustReport wire
    /// contract. `#[serde(default)]` makes Rust-side
    /// deserialization of TS-produced JSON populate it as
    /// `0`, which is the correct sentinel for "no information
    /// about eligibility" when reading an external report.
    ///
    /// ── Why this exists ──────────────────────────────────
    ///
    /// `enrichment_status: Option<EnrichmentStatus>` alone
    /// cannot distinguish three real-world states the agent
    /// layer needs to tell apart:
    ///
    ///   1. No eligible samples at all → NotApplicable
    ///   2. Eligible samples but enrichment phase never ran
    ///      → NotRun
    ///   3. Phase ran on one or more eligible samples → Ran
    ///
    /// The current `enrichment_status.is_some()` flag lumps
    /// (1) and (2) together because both produce `None`. The
    /// storage adapter (see
    /// `rust/crates/storage/src/agent_impl.rs::get_trust_summary`)
    /// reads this counter to disambiguate case (1) (counter
    /// == 0) from case (2) (counter > 0 with
    /// `enrichment_status.is_none()`).
    ///
    /// Added after the Rust-43 spike review
    /// (`docs/spikes/2026-04-15-orient-on-repo-graph.md`)
    /// identified that the agent would incorrectly emit
    /// `TRUST_NO_ENRICHMENT` and degrade confidence on repos
    /// with zero eligible samples.
    #[serde(skip, default)]
    pub enrichment_eligible_count: u64,

    /// Unresolved CALLS the classifier could NOT attribute (classification
    /// `unknown`), within the in-scope denominator. RELIABILITY-REFRAME-1
    /// (review-3 §2): `unresolved_calls_internal_like` = external-excluded =
    /// internal-candidate ∪ UNKNOWN, so it is "in-scope OR unclassified", NOT
    /// known-internal. This counter is the unclassified portion; a reader surface
    /// uses it to fire the conservative-rate caveat
    /// (`repo_graph_agent::reliability::unclassified_caveat`) when the unclassified
    /// share is material.
    ///
    /// `#[serde(skip, default)]` — like `enrichment_eligible_count`, it is NOT on
    /// the TS-parity `TrustReport` wire (adding a serialized `TrustSummary` field
    /// would change both `report__*` fixtures AND diverge from the live TS side).
    /// It is consumed IN-PROCESS (daemon-side) by `trust_to_coherent`,
    /// `TrustOverlaySummary::from_report`, and the agent storage adapter, which put
    /// it on THEIR own (non-parity) wires.
    #[serde(skip, default)]
    pub unresolved_calls_unknown: u64,

    /// ATTRIBUTION-1: the basis-code breakdown — the finer axis behind
    /// `classifications`, which the rgr presentation layer maps to reader-frame
    /// attribution classes (library call / standard library / runtime built-in / your
    /// own code / dynamic dispatch / unattributed).
    ///
    /// `#[serde(skip, default)]` — NOT on the TS-parity v1 wire (exactly like
    /// `unresolved_calls_unknown`: adding a serialized field would break both
    /// `report__*` fixtures AND diverge from the live TS side, which does not compute
    /// it). It is populated in-process by `assemble_trust_report` and projected onto
    /// the coherent `basis_classifications` leaf by `trust_to_coherent`.
    #[serde(skip, default)]
    pub basis_classifications: Vec<TrustBasisClassificationRow>,

    /// ATTRIBUTION-1 iteration 3: the reader-frame attribution of the external-import
    /// unresolved references — each named by its DECLARED dependency across all three call
    /// bases (the provenance join), plus the named/unidentified totals that reconcile the
    /// "library call" class. Replaces the review-1 `top_named_dependencies` (which named
    /// only the specifier basis, by raw specifier); the renderer draws
    /// "library call → serde: N references", "other declared dependencies", and the honest
    /// "dependency not identified" from this one bundle.
    ///
    /// `#[serde(skip, default)]` — NOT on the TS-parity v1 wire (same rationale as
    /// `basis_classifications`). Populated in-process by `assemble_trust_report` and
    /// projected onto the coherent `external_dependencies` leaf by `trust_to_coherent`.
    #[serde(skip, default)]
    pub external_dependencies: TrustExternalDependencyAttribution,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reliability_level_serializes_uppercase() {
        assert_eq!(
            serde_json::to_string(&ReliabilityLevel::HIGH).unwrap(),
            "\"HIGH\""
        );
        assert_eq!(
            serde_json::to_string(&ReliabilityLevel::MEDIUM).unwrap(),
            "\"MEDIUM\""
        );
        assert_eq!(
            serde_json::to_string(&ReliabilityLevel::LOW).unwrap(),
            "\"LOW\""
        );
    }

    #[test]
    fn extraction_diagnostics_breakdown_serializes_as_json_object() {
        let mut breakdown = BTreeMap::new();
        breakdown.insert("calls_obj_method_needs_type_info".into(), 15);
        breakdown.insert("imports_file_not_found".into(), 5);
        let diag = ExtractionDiagnostics {
            diagnostics_version: 1,
            edges_total: 100,
            unresolved_total: 20,
            unresolved_breakdown: breakdown,
        };
        let json = serde_json::to_string(&diag).unwrap();
        assert!(json.contains("\"calls_obj_method_needs_type_info\":15"));
        assert!(json.contains("\"imports_file_not_found\":5"));
    }

    #[test]
    fn extraction_diagnostics_breakdown_roundtrips_from_json_object() {
        let json = r#"{
			"diagnostics_version": 1,
			"edges_total": 50,
			"unresolved_total": 10,
			"unresolved_breakdown": {
				"calls_function_ambiguous_or_missing": 7,
				"other": 3
			}
		}"#;
        let parsed: ExtractionDiagnostics = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.unresolved_breakdown.len(), 2);
        assert_eq!(
            parsed
                .unresolved_breakdown
                .get("calls_function_ambiguous_or_missing"),
            Some(&7)
        );
        assert_eq!(parsed.unresolved_breakdown.get("other"), Some(&3));
    }

    #[test]
    fn extraction_diagnostics_breakdown_rejects_duplicate_keys_at_type_level() {
        // BTreeMap enforces key uniqueness. Inserting the same key
        // twice overwrites the first value, preventing the invalid
        // duplicate-key state that Vec<(String, u64)> would allow.
        let mut breakdown: BTreeMap<String, u64> = BTreeMap::new();
        breakdown.insert("same_key".into(), 10);
        breakdown.insert("same_key".into(), 20);
        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown.get("same_key"), Some(&20));
    }

    #[test]
    fn toolchain_serializes_as_object_and_rejects_non_object_shapes() {
        // Pins the object-only constraint on TrustReport.toolchain.
        // The TS contract is Record<string, unknown> | null, which
        // means the top-level shape must be a JSON object or null.
        // Using Map<String, Value> enforces this at the type level.

        // Object → accepted.
        let json_obj = r#"{"snapshot_uid":"s1","basis_commit":null,"toolchain":{"schema_compat":1},"diagnostics_version":null,"summary":{"edges_total":0,"edges_resolved":0,"unresolved_total":0,"resolved_calls":0,"unresolved_calls":0,"unresolved_calls_external":0,"unresolved_calls_internal_like":0,"call_resolution_rate":1.0,"reliability":{"import_graph":{"level":"HIGH","reasons":[]},"call_graph":{"level":"HIGH","reasons":[]},"dead_code":{"level":"HIGH","reasons":[]},"change_impact":{"level":"HIGH","reasons":[]}},"triggered_downgrades":{"framework_heavy_suspicion":{"triggered":false,"reasons":[]},"registry_pattern_suspicion":{"triggered":false,"reasons":[]},"missing_entrypoint_declarations":{"triggered":false,"reasons":[]},"alias_resolution_suspicion":{"triggered":false,"reasons":[]}}},"categories":[],"classifications":[],"unknown_calls_blast_radius":null,"enrichment_status":null,"modules":[],"caveats":[],"diagnostics_available":false}"#;
        let parsed: TrustReport = serde_json::from_str(json_obj).unwrap();
        assert!(parsed.toolchain.is_some());

        // Null → accepted (Option<Map> deserializes null as None).
        let json_null =
            json_obj.replace(r#""toolchain":{"schema_compat":1}"#, r#""toolchain":null"#);
        let parsed_null: TrustReport = serde_json::from_str(&json_null).unwrap();
        assert!(parsed_null.toolchain.is_none());

        // Array → rejected at deserialize time.
        let json_arr = json_obj.replace(
            r#""toolchain":{"schema_compat":1}"#,
            r#""toolchain":[1,2,3]"#,
        );
        assert!(
            serde_json::from_str::<TrustReport>(&json_arr).is_err(),
            "toolchain as an array must be rejected (object-only contract)"
        );

        // String → rejected.
        let json_str = json_obj.replace(
            r#""toolchain":{"schema_compat":1}"#,
            r#""toolchain":"not an object""#,
        );
        assert!(
            serde_json::from_str::<TrustReport>(&json_str).is_err(),
            "toolchain as a string must be rejected (object-only contract)"
        );
    }

    #[test]
    fn enrichment_top_type_uses_type_rename() {
        let t = EnrichmentTopType {
            type_name: "HashMap".into(),
            count: 5,
            is_external: false,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"type\":\"HashMap\""));
        assert!(!json.contains("\"typeName\""));
        assert!(!json.contains("\"type_name\""));
    }
}
