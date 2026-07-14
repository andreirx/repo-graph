//! TRUST-LIVEGRAPH-IMPL: the coherence-wrapped trust report (the ratified HYBRID).
//!
//! PURE policy (Clean Architecture: high-level policy, no I/O). The trust analogue of
//! `repo_graph_agent::check_to_coherent`, but for trust's bespoke [`TrustReport`] (trust returns the full
//! report, NOT the shared `OrientResult`, so it does NOT reuse `CoherentOrientResult` — D-TRUST-1). It
//! converts a v1 [`TrustReport`] into `CoherenceEnvelope<CoherentTrustReport>`: the ratified two-half model
//! (`docs/slices/trust-livegraph-1.md` §3, coherence-layer-1.md D2):
//!
//!   - **Half B — residual extraction diagnostics (source = `sqlite`).** EVERY v1 axis is RETAINED, each
//!     re-typed to a [`CoherenceEnvelope`] LEAF whose payload is BYTE-IDENTICAL to the v1 report (P1) and
//!     LABELLED as the OUTGOING homegrown extractor's snapshot-scoped unresolved-edge model. No axis is ever
//!     `source = livegraph` (F5). The `dead_code` reliability axis stays INTERNAL — it rides inside the
//!     reused [`crate::types::TrustReliability`], whose `dead_code` field is `#[serde(skip_serializing)]`, so
//!     it is NEVER a leaf and NEVER on the wire (D-TRUST-5 / P2).
//!     The downgrade-triggers leaf is the one MULTI-SOURCE leaf `{sqlite, declaration}` (D-TRUST-4 / contract
//!     D8): `missing_entrypoint_declarations` reads the `declarations` Authority table on EVERY report.
//!   - **Half A — current-state reliability posture (source = `livegraph`).** A SINGLE composite leaf
//!     ([`LiveGraphPosture`]) PROJECTING the LiveGraph's existing per-answer posture into the coherence
//!     vocabulary — NOT a recomputed v1 reliability level (D-TRUST-2, the anti-Option-B guard). It is built
//!     by the IMPURE daemon adapter (`daemon-runtime/src/trust_coherence.rs`) from real LiveGraph runtime
//!     state and passed in here; this pure layer only FOLDS it into the report.
//!
//! The root folds by MEET over BOTH halves (D-TRUST-3 / contract D3): `root.freshness` is the MEET of the
//! Half-A LiveGraph freshness AND the Half-B snapshot freshness, so a Fresh LiveGraph over a Stale snapshot
//! (or a cold LiveGraph over a Fresh snapshot) reports a degraded root — the MEET is monotone and cannot mint
//! a false `Fresh`/`Exact` (RISK-T-A). The v1 computation (`service.rs` / `rules.rs`) is UNTOUCHED: this layer
//! WRAPS + LABELS the answer, it does not re-judge it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use repo_graph_coherence::{
    fold_parts, AnswerClass, CoherenceEnvelope, FreshnessState, Provenance, QueryCompleteness,
    Source, TrustPosture,
};

use crate::types::{
    EnrichmentStatus, ModuleTrustRow, TrustCategoryRow, TrustClassificationRow, TrustDowngrades,
    TrustReliability, TrustReport, UnknownCallsBlastRadiusBreakdown,
};

// ── Half-A posture DTOs (the current-state LiveGraph reliability posture) ──────────────────────────

/// One resident-partition posture row inside the Half-A [`LiveGraphPosture`] — a faithful projection of the
/// LiveGraph's `live_partitions()` for that partition. `freshness` is `Fresh` when the partition is at the
/// latest epoch and `Stale` otherwise (the coarse residency-derived freshness the partition snapshot
/// exposes; the leaf-level MEET freshness rides on the wrapper sibling). NO new producer: read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveGraphPartitionPosture {
    /// The partition id (the SCIP partition / package boundary the LiveGraph holds).
    pub partition_id: String,
    /// `Fresh` when the partition is resident at the latest epoch; `Stale` otherwise.
    pub freshness: FreshnessState,
    /// `true` when the partition is `LanguageSupport::TypeScriptPrimary` (the migrated-answer-supported
    /// language). A non-TS resident partition reports the posture honestly as not TS-primary.
    pub typescript_primary: bool,
    /// The `indexer@version` fingerprint that built this partition's IR (empty when never producer-built).
    pub producer_fingerprint: String,
}

/// Half A — the current-state reliability posture VALUE (source = `livegraph`). A PROJECTION of the
/// LiveGraph's EXISTING runtime state (residency / per-partition freshness / language maturity / producer
/// availability / migrated-answer capability), NOT a recomputed v1 reliability level (D-TRUST-2). Built by
/// the daemon adapter from real LiveGraph reads (`live_partitions()` + the repo-wide `module_stats()`
/// AnswerEnvelope); this crate only carries + serializes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveGraphPosture {
    /// Whether an in-memory LiveGraph with at least one resident partition exists for this repo (cold /
    /// non-preloaded => `false`, and the posture leaf is `Unavailable` — NOT empty-as-known-zero, F3).
    pub resident: bool,
    /// The per-resident-partition posture rows (empty when not resident).
    pub partitions: Vec<LiveGraphPartitionPosture>,
    /// Whether the SCIP producer was available for the resident partitions: `false` when the LiveGraph
    /// reports a `ProducerUnavailable` degradation (warm-loaded producer-absent, LIVEGRAPH-INTEGRATION-1C),
    /// or when nothing is resident.
    pub producer_available: bool,
    /// Whether the LiveGraph can currently serve `Exact`, `Fresh` migrated structural answers
    /// (callers/callees/imports/cycles/stats) over the resident graph — projected from the repo-wide
    /// current-state `module_stats()` answer class. `false` when degraded/cold.
    pub migrated_answer_capability: bool,
}

// ── Half-B leaf payload DTOs (split from the v1 `TrustSummary`; reused axis types stay byte-identical) ──

/// The diagnostics-meta leaf payload (source = `sqlite`) — the extraction-diagnostics blob's version +
/// presence. When the blob is absent the leaf is `Unavailable` (F3 / D-T4), never a Fresh known-zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsMeta {
    /// The diagnostics blob's version (`None` when the blob is absent).
    pub diagnostics_version: Option<u32>,
    /// Whether the snapshot carries an extraction-diagnostics blob.
    pub diagnostics_available: bool,
}

/// The resolution leaf payload (source = `sqlite`) — the edge/call counts + the Variant-A call-resolution
/// rate, split out of the v1 `TrustSummary` VERBATIM (values byte-identical, P1). The
/// `edges_resolved == edges_total` v1 quirk is retained unchanged (RISK-T-F).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionCounts {
    /// Total edges (from the diagnostics blob).
    pub edges_total: u64,
    /// Resolved edges (v1 assigns this `== edges_total`; retained verbatim, RISK-T-F).
    pub edges_resolved: u64,
    /// Total unresolved edges (from the diagnostics blob).
    pub unresolved_total: u64,
    /// Resolved CALLS edges (`count_edges_by_type(CALLS)`).
    pub resolved_calls: u64,
    /// Unresolved CALLS-family edges (from the diagnostics breakdown).
    pub unresolved_calls: u64,
    /// Unresolved CALLS classified `external_library_candidate` (Variant-A exclusion).
    pub unresolved_calls_external: u64,
    /// Unresolved CALLS minus the known-external subset — the in-scope-rate
    /// denominator. RELIABILITY-REFRAME-1 (review-3 §2): this is "in-scope OR
    /// UNCLASSIFIED", NOT known-internal (it includes `unknown` classifications);
    /// `unresolved_calls_unknown` below is its unclassified portion.
    pub unresolved_calls_internal_like: u64,
    /// The UNCLASSIFIED (`unknown`) portion of `unresolved_calls_internal_like`
    /// (review-3 §2). Additive; `#[serde(default)]` for pre-slice daemon JSON. The
    /// human render uses it for the reader-frame conservative-rate caveat.
    #[serde(default)]
    pub unresolved_calls_unknown: u64,
    /// `resolved_calls / (resolved_calls + internal_like)` (1.0 when no calls).
    pub call_resolution_rate: f64,
}

// ── The coherent trust report container (the wrapper root `value`) ─────────────────────────────────

/// trust's coherent command container (D-TRUST-1) — the trust analogue of agent's `CoherentOrientResult`.
/// Identity / operational meta stay pristine container fields; every v1 axis becomes a Half-B residual leaf
/// (`source = sqlite`, the downgrade-triggers leaf `{sqlite, declaration}`); the Half-A
/// [`LiveGraphPosture`] leaf (`source = livegraph`) is co-located beside them. ONE self-describing tree:
/// each leaf's provenance/freshness sits with the axis it labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoherentTrustReport {
    // ── identity / operational meta (pristine container fields; source described per §2) ──
    /// The snapshot the v1 diagnostics describe.
    pub snapshot_uid: String,
    /// Human-readable repo name (daemon-injected). Absent on the wire when `None`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub display_name: Option<String>,
    /// The snapshot's basis commit.
    pub basis_commit: Option<String>,
    /// The toolchain provenance object (or null).
    pub toolchain: Option<Map<String, Value>>,

    // ── Half B — residual extraction diagnostics leaves (source = sqlite) ──
    /// Diagnostics blob version + presence (`Unavailable` when the blob is absent).
    pub diagnostics: CoherenceEnvelope<DiagnosticsMeta>,
    /// Edge/call counts + the call-resolution rate.
    pub resolution: CoherenceEnvelope<ResolutionCounts>,
    /// The 3 serialized reliability axes (the `dead_code` axis stays internal — skip_serializing).
    pub reliability: CoherenceEnvelope<TrustReliability>,
    /// The 4 downgrade triggers — the MULTI-SOURCE `{sqlite, declaration}` leaf (D-TRUST-4 / D8).
    pub triggered_downgrades: CoherenceEnvelope<TrustDowngrades>,
    /// The unresolved-category breakdown.
    pub categories: CoherenceEnvelope<Vec<TrustCategoryRow>>,
    /// The classifier-bucket counts.
    pub classifications: CoherenceEnvelope<Vec<TrustClassificationRow>>,
    /// The unknown-CALLS blast-radius breakdown.
    pub unknown_calls_blast_radius: CoherenceEnvelope<Option<UnknownCallsBlastRadiusBreakdown>>,
    /// The enrichment status for unknown CALLS.
    pub enrichment_status: CoherenceEnvelope<Option<EnrichmentStatus>>,
    /// The per-module trust rows (degree + suspicion).
    pub modules: CoherenceEnvelope<Vec<ModuleTrustRow>>,
    /// The derived caveats.
    pub caveats: CoherenceEnvelope<Vec<String>>,

    // ── Half A — current-state reliability posture leaf (source = livegraph) ──
    /// The NEW current-state LiveGraph posture (D-TRUST-2).
    pub current_state_posture: CoherenceEnvelope<LiveGraphPosture>,
}

// ── Posture helpers ────────────────────────────────────────────────────────────────────────────────

/// The snapshot posture for a Half-B leaf that is always computable from a ready snapshot: `Exact`/`Fresh`
/// for a current index, `Stale`/`Degraded` when the backing index is stale. `contributing_languages` is
/// empty — these leaves are SQLite-snapshot-scoped, not language-partition-scoped (mirrors check / orient).
fn snapshot_posture(stale: bool) -> (TrustPosture, FreshnessState) {
    if stale {
        (TrustPosture::snapshot_stale(), FreshnessState::Stale)
    } else {
        (TrustPosture::snapshot_exact(), FreshnessState::Fresh)
    }
}

/// The posture for a Half-B leaf whose VALUE is derived from the extraction-diagnostics BLOB. When the blob
/// is ABSENT (`diagnostics_available == false`) the value is UNKNOWN — `Unavailable`/`Unknown`, NOT a Fresh
/// known-zero (F3 / D-T4: "null = unknown, empty = known-zero"). Otherwise the normal snapshot posture.
fn blob_posture(diagnostics_available: bool, stale: bool) -> (TrustPosture, FreshnessState) {
    if !diagnostics_available {
        (
            TrustPosture {
                class: AnswerClass::Unavailable,
                completeness: QueryCompleteness::Unknown,
                degradation_reasons: Vec::new(),
                contributing_languages: BTreeSet::new(),
            },
            FreshnessState::Unavailable,
        )
    } else {
        snapshot_posture(stale)
    }
}

/// Project a leaf's coherence metadata into a unit-valued envelope for the root MEET fold. The value is
/// irrelevant to the fold (`fold_parts` reads only provenance/trust/freshness), so a `()` carrier lets the
/// HETEROGENEOUS trust leaves fold through the SAME public, invariant-preserving `fold_parts` the
/// uniform-`Signal` commands use — without replicating the private cross-axis cap.
fn meta_of<T>(leaf: &CoherenceEnvelope<T>) -> CoherenceEnvelope<()> {
    CoherenceEnvelope::new(
        (),
        leaf.provenance.clone(),
        leaf.trust.clone(),
        leaf.freshness,
    )
}

// ── Conversion ───────────────────────────────────────────────────────────────────────────────────

/// Convert a v1 [`TrustReport`] into the coherence wrapper `CoherenceEnvelope<CoherentTrustReport>`.
///
/// `posture` is the Half-A current-state posture leaf, BUILT by the daemon adapter from real LiveGraph
/// runtime state (`daemon-runtime/src/trust_coherence.rs`) and passed in (`source = livegraph`). `stale` =
/// whether the backing index is stale (`get_stale_files` non-empty), supplied by the daemon from an
/// AUTHORITATIVE storage read — NOT a post-budget signal — so the Half-B freshness labels stay honest.
///
/// The v1 axes are RETAINED byte-identical (Half B); only the surrounding wrapper adds source/freshness
/// labels and the Half-A leaf. The root folds by MEET over ALL leaves of BOTH halves.
pub fn trust_to_coherent(
    report: TrustReport,
    posture: CoherenceEnvelope<LiveGraphPosture>,
    stale: bool,
) -> CoherenceEnvelope<CoherentTrustReport> {
    let TrustReport {
        snapshot_uid,
        display_name,
        basis_commit,
        toolchain,
        diagnostics_version,
        summary,
        categories,
        classifications,
        unknown_calls_blast_radius,
        enrichment_status,
        modules,
        caveats,
        diagnostics_available,
        // serde(skip) internal disambiguator — never on the wire (P3); not part of the hybrid.
        enrichment_eligible_count: _,
        // serde(skip) in-process counter (RELIABILITY-REFRAME-1 review-3 §2): NOT on the
        // parity wire, but projected onto the coherent resolution leaf below so the human
        // render can fire the conservative-rate caveat.
        unresolved_calls_unknown,
    } = report;

    let resolution_counts = ResolutionCounts {
        edges_total: summary.edges_total,
        edges_resolved: summary.edges_resolved,
        unresolved_total: summary.unresolved_total,
        resolved_calls: summary.resolved_calls,
        unresolved_calls: summary.unresolved_calls,
        unresolved_calls_external: summary.unresolved_calls_external,
        unresolved_calls_internal_like: summary.unresolved_calls_internal_like,
        unresolved_calls_unknown,
        call_resolution_rate: summary.call_resolution_rate,
    };

    let (blob_trust, blob_fresh) = blob_posture(diagnostics_available, stale);
    let (snap_trust, snap_fresh) = snapshot_posture(stale);

    // ── Half B — every v1 axis retained as a residual leaf (source = sqlite; payloads byte-identical) ──
    //
    // BLOB-derived leaves (diagnostics / resolution / categories): `Unavailable` when the blob is absent
    // (F3). The rest carry the snapshot posture (they read separate tables / are always computed).
    let diagnostics_leaf = CoherenceEnvelope::new(
        DiagnosticsMeta {
            diagnostics_version,
            diagnostics_available,
        },
        Provenance::sqlite(),
        blob_trust.clone(),
        blob_fresh,
    );
    let resolution_leaf = CoherenceEnvelope::new(
        resolution_counts,
        Provenance::sqlite(),
        blob_trust.clone(),
        blob_fresh,
    );
    let categories_leaf =
        CoherenceEnvelope::new(categories, Provenance::sqlite(), blob_trust, blob_fresh);

    let reliability_leaf = CoherenceEnvelope::new(
        summary.reliability,
        Provenance::sqlite(),
        snap_trust.clone(),
        snap_fresh,
    );
    // The ONE multi-source leaf {sqlite, declaration} (D-TRUST-4 / D8): the missing-entrypoint trigger reads
    // the declarations Authority table on EVERY report, even when no downgrade fires.
    let downgrades_leaf = CoherenceEnvelope::new(
        summary.triggered_downgrades,
        Provenance::multi([Source::Sqlite, Source::Declaration]),
        snap_trust.clone(),
        snap_fresh,
    );
    let classifications_leaf = CoherenceEnvelope::new(
        classifications,
        Provenance::sqlite(),
        snap_trust.clone(),
        snap_fresh,
    );
    let blast_leaf = CoherenceEnvelope::new(
        unknown_calls_blast_radius,
        Provenance::sqlite(),
        snap_trust.clone(),
        snap_fresh,
    );
    let enrichment_leaf = CoherenceEnvelope::new(
        enrichment_status,
        Provenance::sqlite(),
        snap_trust.clone(),
        snap_fresh,
    );
    let modules_leaf = CoherenceEnvelope::new(
        modules,
        Provenance::sqlite(),
        snap_trust.clone(),
        snap_fresh,
    );
    let caveats_leaf =
        CoherenceEnvelope::new(caveats, Provenance::sqlite(), snap_trust, snap_fresh);

    // ── Root MEET over ALL leaves of BOTH halves (monotone; can only LOWER) ──
    let meta = [
        meta_of(&diagnostics_leaf),
        meta_of(&resolution_leaf),
        meta_of(&reliability_leaf),
        meta_of(&downgrades_leaf),
        meta_of(&categories_leaf),
        meta_of(&classifications_leaf),
        meta_of(&blast_leaf),
        meta_of(&enrichment_leaf),
        meta_of(&modules_leaf),
        meta_of(&caveats_leaf),
        meta_of(&posture),
    ];
    let (provenance, trust, freshness) = fold_parts(&meta);

    let value = CoherentTrustReport {
        snapshot_uid,
        display_name,
        basis_commit,
        toolchain,
        diagnostics: diagnostics_leaf,
        resolution: resolution_leaf,
        reliability: reliability_leaf,
        triggered_downgrades: downgrades_leaf,
        categories: categories_leaf,
        classifications: classifications_leaf,
        unknown_calls_blast_radius: blast_leaf,
        enrichment_status: enrichment_leaf,
        modules: modules_leaf,
        caveats: caveats_leaf,
        current_state_posture: posture,
    };

    CoherenceEnvelope::new(value, provenance, trust, freshness)
}

// ── Half-A posture leaf builders (PURE — shared by the daemon adapter and the unit tests) ──────────

impl LiveGraphPosture {
    /// The cold / non-resident posture VALUE (no LiveGraph loaded for this repo, or zero resident
    /// partitions). Pairs with [`LiveGraphPosture::unavailable_leaf`].
    pub fn cold() -> Self {
        Self {
            resident: false,
            partitions: Vec::new(),
            producer_available: false,
            migrated_answer_capability: false,
        }
    }

    /// Wrap a posture value as the Half-A leaf with an EXPLICIT coherence posture (`source = livegraph`).
    /// The daemon projects `trust` + `freshness` from the repo-wide `module_stats()` AnswerEnvelope (REAL
    /// LiveGraph serving); the unit tests construct them directly.
    pub fn into_leaf(
        self,
        trust: TrustPosture,
        freshness: FreshnessState,
    ) -> CoherenceEnvelope<LiveGraphPosture> {
        CoherenceEnvelope::new(self, Provenance::livegraph(), trust, freshness)
    }

    /// The cold-LiveGraph Half-A leaf: `Unavailable` / `Unknown` / `Unavailable` (F3 — unknown, NOT a Fresh
    /// known-zero), `source = livegraph`. Empty `contributing_languages` (no partition contributed). The
    /// structured `resident: false` value carries the reason; the class communicates unavailability without
    /// mislabelling a SCIP degradation reason onto a cold graph (the check-style honesty choice).
    pub fn unavailable_leaf() -> CoherenceEnvelope<LiveGraphPosture> {
        Self::cold().into_leaf(
            TrustPosture {
                class: AnswerClass::Unavailable,
                completeness: QueryCompleteness::Unknown,
                degradation_reasons: Vec::new(),
                contributing_languages: BTreeSet::new(),
            },
            FreshnessState::Unavailable,
        )
    }
}

#[cfg(test)]
#[path = "coherent_tests.rs"]
mod coherent_tests;
