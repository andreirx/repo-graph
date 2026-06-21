#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # repo-graph-coherence — the shared `CoherenceEnvelope<T>` answer-wrapper (COHERENCE-LAYER-1)
//!
//! Pure-domain support crate. Realizes the wrapper the ratified COHERENCE-LAYER-1 contract defines
//! (`docs/slices/coherence-layer-1.md` §"The shared coherence answer-envelope") and that ORIENT-LIVEGRAPH-1
//! consumes first; CHECK / EXPLAIN / TRUST consume it later. It adds ONE new generic carrier type plus the
//! two folds the contract specifies, and PROJECTS the existing
//! [`repo_graph_trust_model`] answer-vocabulary — it does NOT replace it.
//!
//! ## What this crate owns
//! - [`CoherenceEnvelope<T>`] — the wrapper, applied COMPOSITIONALLY at two granularities: a LEAF
//!   `CoherenceEnvelope<Signal>` per emitted signal, and a ROOT `CoherenceEnvelope<CoherentOrientResult>`
//!   per command. The leaf `value` stays PRISTINE (un-widened); the coherence metadata rides in the
//!   wrapper SIBLING fields ([`Provenance`] / [`TrustPosture`] / [`FreshnessState`]).
//! - [`Provenance`] — the NEW source axis. `source` is a `BTreeSet<Source>` at BOTH leaf and root (D8):
//!   a SINGLETON for a single-source leaf, MULTIPLE for a derived/composite leaf, and the set-UNION of
//!   its leaves at the root.
//! - [`TrustPosture`] — projects the [`AnswerEnvelope`] certainty axes (class / completeness /
//!   degradation_reasons / contributing_languages) VERBATIM.
//! - [`CoherenceFallbackReason`] — a faithful MIRROR of the daemon's LiveGraph `FallbackReason` ladder.
//!   The daemon maps its own enum INTO this one at the boundary; this crate never depends on the daemon
//!   (no dependency-rule inversion — see the crate manifest's architecture note).
//! - The **MEET fold** ([`meet_trust`] / [`meet_freshness`]) and the **set-UNION provenance fold**
//!   ([`union_provenance`]). The MEET is MONOTONE — it can only LOWER class/freshness/completeness,
//!   never raise — so no fold can manufacture an `Exact` root from non-`Exact` leaves. This is the
//!   formal anti-false-completeness guarantee (contract §invariant-preservation).
//!
//! ## What this crate does NOT do
//! It performs no I/O, no LiveGraph reads, no SQLite reads, no cert evaluation. It is the pure shape +
//! algebra; the agent assembles leaves and the daemon sources them (ORIENT-LIVEGRAPH-1).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

// Re-export the projected vocabulary so consumers have a self-contained public surface.
pub use repo_graph_trust_model::{
    AnswerClass, AnswerEnvelope, DegradationReason, FreshnessState, LanguageSupport,
    ProvenanceBasis, QueryCompleteness,
};

// ── Source axis ───────────────────────────────────────────────────

/// WHERE a coherence `value` came from — the NEW source axis the wrapper adds (contract Q6 + D8).
///
/// A leaf carries a SET of these (usually a singleton); the root carries the set-UNION of its leaves.
/// `Ord` is derived so the set is deterministically ordered for stable serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Current-state in-memory LiveGraph, served via the cert-gated fastpath.
    Livegraph,
    /// Durable snapshot-scoped SQLite cache — the PROVEN primary and the labelled fallback.
    Sqlite,
    /// Filesystem live-scan (e.g. the documentation inventory). Already current-state.
    Filesystem,
    /// Tier-A1 user-authored `declarations` authority. Overlays a computed fact, never erases it.
    Declaration,
}

// ── Fallback reason (mirror of the daemon LiveGraph ladder) ───────

/// Why a LiveGraph-first leaf flipped its `source` to SQLite (the cert ladder). A faithful MIRROR of
/// `repo_graph_daemon_runtime::livegraph_feed::FallbackReason` — duplicated here so this pure crate does
/// NOT depend on the daemon (the daemon maps its enum INTO this one at the boundary). `None` on a leaf's
/// provenance means the leaf's served value IS from its primary source (no fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoherenceFallbackReason {
    /// No LiveGraph for this repo / target (not preloaded/refreshed, or `Unavailable`).
    LiveGraphUnavailable,
    /// LiveGraph answered but not `Exact` (e.g. a non-resident contributing partition).
    LiveGraphPartial,
    /// LiveGraph answer is not `Fresh` (Stale / RefreshFailed / PrecisionPending — incl. producer-absent).
    LiveGraphStale,
    /// Contributing languages are not exclusively `TypeScriptPrimary` (the migrated fastpath D4 scope).
    LiveGraphUnsupportedLanguage,
    /// The LiveGraph answer could not be rendered into the response shape (reserved).
    LiveGraphRenderUnsupported,
    /// A rendered node lacks display metadata, so the DEFAULT path falls back to SQLite.
    LiveGraphDisplayMetadataUnavailable,
    /// The LiveGraph engine errored (reserved).
    LiveGraphError,
    /// A per-call no-loss compare found a SQLite resolved-local import the LiveGraph edge set LOST.
    LiveGraphImportRegression,
    /// The file has an AMBIGUOUS SQLite import the harness cannot confidently bucket.
    LiveGraphImportUnknown,
    /// The repo MODULE-cycle no-loss cert is NOT GREEN (a divergence) — serve SQLite cycles.
    LiveGraphCycleDivergence,
    /// The repo STATS no-loss cert is NOT GREEN (a per-module divergence) — serve SQLite stats.
    LiveGraphStatsDivergence,
    /// The repo COMPLEXITY no-loss cert is NOT GREEN — the LiveGraph repo-wide high-complexity set
    /// diverges from the SQLite `measurements` set (a missing/extra symbol or a value mismatch) — serve
    /// the SQLite HIGH_COMPLEXITY signal (labelled). ORIENT-LIVEGRAPH-IMPL.
    LiveGraphComplexityDivergence,
    /// A symbol-focus CALLERS/CALLEES no-loss compare found the LiveGraph callgraph key set diverges from
    /// SQLite `find_symbol_callers`/`find_symbol_callees` — serve the SQLite summary (labelled). The
    /// value-equivalence proof that lets orient label a summary `livegraph` only when LG == SQLite.
    LiveGraphCallgraphDivergence,
    /// The BOUNDED orient (b)-leaf serve was DECLINED — the bounded orient cert (focus-resolution ∧
    /// callgraph no-loss) was not GREEN, so the daemon served orient from the BARE SQLite read. The
    /// CALLERS/CALLEES leaf is SQLite-sourced this call and labelled honestly, NEVER re-certified
    /// `livegraph` from the callgraph cert state alone. Distinct from `LiveGraphCallgraphDivergence`: the
    /// callgraph contributor itself may be GREEN; a DIFFERENT bounded contributor (e.g. focus-resolution)
    /// was RED. COHERENCE-LEAF-SERVE-IMPL-1.
    LiveGraphBoundedServeDeclined,
}

impl CoherenceFallbackReason {
    /// Stable string for the JSON `fallback_reason` (matches the daemon `FallbackReason::as_str`).
    pub fn as_str(self) -> &'static str {
        match self {
            CoherenceFallbackReason::LiveGraphUnavailable => "LiveGraphUnavailable",
            CoherenceFallbackReason::LiveGraphPartial => "LiveGraphPartial",
            CoherenceFallbackReason::LiveGraphStale => "LiveGraphStale",
            CoherenceFallbackReason::LiveGraphUnsupportedLanguage => "LiveGraphUnsupportedLanguage",
            CoherenceFallbackReason::LiveGraphRenderUnsupported => "LiveGraphRenderUnsupported",
            CoherenceFallbackReason::LiveGraphDisplayMetadataUnavailable => {
                "LiveGraphDisplayMetadataUnavailable"
            }
            CoherenceFallbackReason::LiveGraphError => "LiveGraphError",
            CoherenceFallbackReason::LiveGraphImportRegression => "LiveGraphImportRegression",
            CoherenceFallbackReason::LiveGraphImportUnknown => "LiveGraphImportUnknown",
            CoherenceFallbackReason::LiveGraphCycleDivergence => "LiveGraphCycleDivergence",
            CoherenceFallbackReason::LiveGraphStatsDivergence => "LiveGraphStatsDivergence",
            CoherenceFallbackReason::LiveGraphComplexityDivergence => {
                "LiveGraphComplexityDivergence"
            }
            CoherenceFallbackReason::LiveGraphCallgraphDivergence => "LiveGraphCallgraphDivergence",
            CoherenceFallbackReason::LiveGraphBoundedServeDeclined => {
                "LiveGraphBoundedServeDeclined"
            }
        }
    }
}

// ── Provenance ────────────────────────────────────────────────────

/// WHERE a coherence value came from, with degradation detail. The `source` SET is the load-bearing
/// axis (D8); `basis` / `missing_partitions` reuse the [`AnswerEnvelope`] provenance/residency detail;
/// `fallback_reason` is set when a LiveGraph-first leaf flipped to SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The SET of contributing sources (D8). A leaf is usually a singleton; the root is the set-UNION.
    pub source: BTreeSet<Source>,
    /// Alias / reconciliation provenance (reused from [`AnswerEnvelope::provenance`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub basis: Vec<ProvenanceBasis>,
    /// Partitions whose non-residency makes the answer incomplete (reused residency axis).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_partitions: Vec<String>,
    /// Set when this leaf's `source` flipped LiveGraph -> SQLite via the cert ladder; `None` otherwise.
    /// At the root this carries the FIRST leaf fallback (a convenience signal); the authoritative,
    /// per-leaf reasons live on the leaves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<CoherenceFallbackReason>,
}

impl Provenance {
    /// A single-source provenance (the common leaf case): a singleton `source`, no basis/missing/fallback.
    pub fn single(source: Source) -> Self {
        Self {
            source: BTreeSet::from([source]),
            basis: Vec::new(),
            missing_partitions: Vec::new(),
            fallback_reason: None,
        }
    }

    /// A `livegraph`-sourced leaf provenance (cert GREEN, served from the LiveGraph).
    pub fn livegraph() -> Self {
        Self::single(Source::Livegraph)
    }

    /// A `sqlite`-sourced leaf provenance (the proven primary; not a fallback).
    pub fn sqlite() -> Self {
        Self::single(Source::Sqlite)
    }

    /// A `declaration` (Tier-A1 Authority) leaf provenance.
    pub fn declaration() -> Self {
        Self::single(Source::Declaration)
    }

    /// A `filesystem` live-scan leaf provenance.
    pub fn filesystem() -> Self {
        Self::single(Source::Filesystem)
    }

    /// A LiveGraph-first leaf that FELL BACK to SQLite: `source = {sqlite}` + the cert ladder reason.
    /// The served value is the SQLite proven primary; the reason records why LiveGraph was not used.
    pub fn sqlite_fallback(reason: CoherenceFallbackReason) -> Self {
        Self {
            source: BTreeSet::from([Source::Sqlite]),
            basis: Vec::new(),
            missing_partitions: Vec::new(),
            fallback_reason: Some(reason),
        }
    }

    /// A composite/derived leaf carrying MULTIPLE contributing sources (D8) — e.g. a verdict folding
    /// SQLite + Authority facts.
    pub fn multi(sources: impl IntoIterator<Item = Source>) -> Self {
        Self {
            source: sources.into_iter().collect(),
            basis: Vec::new(),
            missing_partitions: Vec::new(),
            fallback_reason: None,
        }
    }
}

// ── TrustPosture (projection of the AnswerEnvelope axes) ──────────

/// The certainty posture — the [`AnswerEnvelope`] axes projected VERBATIM (contract Q6). It is NOT itself
/// invariant-constructed: it is a faithful read-only projection of an already-legal answer, or a
/// hand-built posture for a non-LiveGraph leaf / the resolution-only root. The legality is owned by the
/// source [`AnswerEnvelope`] (for LiveGraph leaves) and by the MEET (for the root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustPosture {
    /// The answer class (Exact | Partial | Unavailable | Stale).
    pub class: AnswerClass,
    /// The completeness verdict (Complete | Degraded | Unknown).
    pub completeness: QueryCompleteness,
    /// The identity-degradation reasons (may be empty for a non-`Exact` posture justified otherwise).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degradation_reasons: Vec<DegradationReason>,
    /// The `LanguageSupport` maturity of every partition that contributed (deduped, ordered). Empty for
    /// non-partition leaves (SQLite/Authority/FS) and for the resolution-only root — those are not
    /// language-partition-scoped.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub contributing_languages: BTreeSet<LanguageSupport>,
}

impl TrustPosture {
    /// Project a posture from an [`AnswerEnvelope`] (a LiveGraph or migrated-surface answer) VERBATIM.
    pub fn from_answer<T>(env: &AnswerEnvelope<T>) -> Self {
        Self {
            class: env.class(),
            completeness: env.completeness(),
            degradation_reasons: env.degradation_reasons().to_vec(),
            contributing_languages: env.contributing_languages().clone(),
        }
    }

    /// An `Exact` / `Complete` posture for a snapshot-scoped SQLite / Authority / FS leaf that is current
    /// for the snapshot (no LiveGraph epoch involved). `contributing_languages` is empty — these leaves
    /// are not language-partition-scoped (contract §3a: "a Fresh/Complete/Exact posture for the snapshot").
    pub fn snapshot_exact() -> Self {
        Self {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            degradation_reasons: Vec::new(),
            contributing_languages: BTreeSet::new(),
        }
    }

    /// A `Stale` posture for a snapshot-scoped leaf whose backing index is stale (`get_stale_files`
    /// non-empty). Pairs with [`FreshnessState::Stale`].
    pub fn snapshot_stale() -> Self {
        Self {
            class: AnswerClass::Stale,
            completeness: QueryCompleteness::Degraded,
            degradation_reasons: Vec::new(),
            contributing_languages: BTreeSet::new(),
        }
    }

    /// The D-ORIENT-4 ZERO-SIGNAL resolution-only posture (ambiguous / no-match focus): the resolution
    /// outcome is certain, but NO structure was analyzed. NEVER a structural `Exact` (`class` is
    /// `Partial`); `completeness` = `Complete` *as a resolution outcome*; `contributing_languages` is
    /// empty. The anti-false-completeness guard is the accompanying `provenance.source = {sqlite}`
    /// operational-identity-only and the static-`High` confidence on the container — a consumer reading
    /// provenance + the empty language set CANNOT mistake this for "structure analyzed, zero findings".
    pub fn resolution_only() -> Self {
        Self {
            class: AnswerClass::Partial,
            completeness: QueryCompleteness::Complete,
            degradation_reasons: Vec::new(),
            contributing_languages: BTreeSet::new(),
        }
    }
}

// ── Lattice ranks (the MEET algebra) ──────────────────────────────
//
// Each axis is a total order from BOTTOM (most degraded) to TOP (best). The MEET (greatest-lower-bound)
// of a non-empty set is the contributor with the MINIMUM rank. MONOTONE: the MEET never exceeds any
// contributor, so it can never raise class/freshness/completeness above the weakest leaf.

fn freshness_rank(f: FreshnessState) -> u8 {
    match f {
        FreshnessState::Unavailable => 0,
        FreshnessState::RefreshFailed => 1,
        FreshnessState::Stale => 2,
        FreshnessState::PrecisionPending => 3,
        FreshnessState::Fresh => 4,
    }
}

fn class_rank(c: AnswerClass) -> u8 {
    match c {
        AnswerClass::Unavailable => 0,
        AnswerClass::Stale => 1,
        AnswerClass::Partial => 2,
        AnswerClass::Exact => 3,
    }
}

fn completeness_rank(c: QueryCompleteness) -> u8 {
    match c {
        QueryCompleteness::Unknown => 0,
        QueryCompleteness::Degraded => 1,
        QueryCompleteness::Complete => 2,
    }
}

/// The MEET of a set of freshness states — the MINIMUM-rank contributor (the worst freshness).
///
/// EMPTY input returns [`FreshnessState::Unavailable`] (the safe BOTTOM), NEVER `Fresh` (the TOP) — an
/// empty fold must not mint a false-fresh answer. The zero-leaf root (ambiguous / no-match) does NOT use
/// this fold: it takes the explicit resolution-only posture (D-ORIENT-4).
pub fn meet_freshness(states: &[FreshnessState]) -> FreshnessState {
    states
        .iter()
        .copied()
        .min_by_key(|f| freshness_rank(*f))
        .unwrap_or(FreshnessState::Unavailable)
}

/// The MEET of a set of trust postures — folded per axis, then made INVARIANT-CONSISTENT so the result
/// is always a legal `(class, completeness)` pairing under the folded freshness.
///
/// - `class` = the minimum-rank leaf class, then CAPPED by the folded `freshness`: a non-`Fresh` root can
///   never be `Exact` (under `PrecisionPending` it is capped at `Partial` — conservative: the root cannot
///   prove whole-answer non-SCIP-dependence at the posture level, so it never claims Exact-under-PP).
/// - `completeness` = forced `Complete` for an `Exact` result (all leaves were Complete), else capped at
///   `Degraded` (or `Unknown` for `Unavailable`).
/// - `degradation_reasons` / `contributing_languages` = the UNION of the leaves' (deduped, ordered) — no
///   leaf label is dropped.
///
/// EMPTY input returns an `Unavailable` / `Unknown` BOTTOM posture, NEVER `Exact` (the empty fold must not
/// mint false completeness). The zero-leaf root uses [`TrustPosture::resolution_only`] instead.
pub fn meet_trust(postures: &[TrustPosture]) -> TrustPosture {
    if postures.is_empty() {
        return TrustPosture {
            class: AnswerClass::Unavailable,
            completeness: QueryCompleteness::Unknown,
            degradation_reasons: Vec::new(),
            contributing_languages: BTreeSet::new(),
        };
    }

    // Per-axis minimum. The freshness MEET is folded separately by the caller (`fold_root`); the
    // cross-axis cap (`cap_posture`) reconciles class/completeness against that folded freshness.
    let min_class = postures
        .iter()
        .map(|p| p.class)
        .min_by_key(|c| class_rank(*c))
        .expect("non-empty checked above");
    let min_completeness = postures
        .iter()
        .map(|p| p.completeness)
        .min_by_key(|c| completeness_rank(*c))
        .expect("non-empty checked above");

    // Union the qualitative axes (never drop a leaf's label).
    let mut reasons: Vec<DegradationReason> = postures
        .iter()
        .flat_map(|p| p.degradation_reasons.iter().copied())
        .collect();
    reasons.sort();
    reasons.dedup();

    let contributing_languages: BTreeSet<LanguageSupport> = postures
        .iter()
        .flat_map(|p| p.contributing_languages.iter().copied())
        .collect();

    TrustPosture {
        class: min_class,
        completeness: min_completeness,
        degradation_reasons: reasons,
        contributing_languages,
    }
}

/// Make a `(class, completeness)` posture CONSISTENT with a folded `freshness` (the cap pass). Pulled out
/// of [`meet_trust`] because the freshness MEET is computed by the caller over the same leaves; this
/// applies the cross-axis invariants. NEVER raises a value.
fn cap_posture(mut posture: TrustPosture, freshness: FreshnessState) -> TrustPosture {
    // Class can never exceed what the freshness permits (invariants I1/I4/I6).
    let freshness_class_ceiling = match freshness {
        FreshnessState::Fresh => AnswerClass::Exact,
        // Conservative: a root under PrecisionPending cannot prove whole-answer non-SCIP-dependence at
        // the posture level, so it is capped at Partial (never Exact-under-PP).
        FreshnessState::PrecisionPending => AnswerClass::Partial,
        FreshnessState::Stale | FreshnessState::RefreshFailed => AnswerClass::Stale,
        FreshnessState::Unavailable => AnswerClass::Unavailable,
    };
    if class_rank(posture.class) > class_rank(freshness_class_ceiling) {
        posture.class = freshness_class_ceiling;
    }
    // Completeness must agree with the (possibly capped) class.
    posture.completeness = match posture.class {
        AnswerClass::Exact => QueryCompleteness::Complete,
        AnswerClass::Partial | AnswerClass::Stale => {
            // Cap at Degraded — Exact-only completeness cannot survive a non-Exact class.
            if completeness_rank(posture.completeness)
                > completeness_rank(QueryCompleteness::Degraded)
            {
                QueryCompleteness::Degraded
            } else {
                posture.completeness
            }
        }
        AnswerClass::Unavailable => QueryCompleteness::Unknown,
    };
    posture
}

/// The set-UNION provenance fold for the ROOT: `source` = the union of every leaf's source set (root ⊇
/// every leaf — monotone, never drops a contributor); `basis` / `missing_partitions` are concatenated +
/// deduped; `fallback_reason` carries the FIRST leaf that fell back (a convenience root signal; the
/// per-leaf reasons remain authoritative). DISTINCT from the trust/freshness MEET.
pub fn union_provenance(leaves: &[&Provenance]) -> Provenance {
    let mut source: BTreeSet<Source> = BTreeSet::new();
    let mut basis: Vec<ProvenanceBasis> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut fallback_reason: Option<CoherenceFallbackReason> = None;
    for p in leaves {
        source.extend(p.source.iter().copied());
        for b in &p.basis {
            if !basis.contains(b) {
                basis.push(b.clone());
            }
        }
        for m in &p.missing_partitions {
            if !missing.contains(m) {
                missing.push(m.clone());
            }
        }
        if fallback_reason.is_none() {
            fallback_reason = p.fallback_reason;
        }
    }
    missing.sort();
    Provenance {
        source,
        basis,
        missing_partitions: missing,
        fallback_reason,
    }
}

// ── CoherenceEnvelope<T> ──────────────────────────────────────────

/// The shared coherence answer-wrapper (contract D1). The `value` stays pristine; the coherence metadata
/// rides in the sibling fields. Applied at LEAF (`T = Signal`) and ROOT (`T = CoherentOrientResult`)
/// granularity.
///
/// `Eq` is intentionally NOT derived: `T` (e.g. the agent `Signal` / `OrientResult`) is `PartialEq` but
/// not `Eq`. The sibling metadata is fully `Eq`, but the wrapper exposes only `PartialEq` to stay generic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoherenceEnvelope<T> {
    /// The answer payload — pristine (leaf `Signal` evidence is un-widened; root command container).
    pub value: T,
    /// WHERE `value` came from (the source set + degradation detail).
    pub provenance: Provenance,
    /// The certainty posture (projected `AnswerEnvelope` axes).
    pub trust: TrustPosture,
    /// The epoch axis (the `repo-graph-trust-model` enum verbatim).
    pub freshness: FreshnessState,
}

impl<T> CoherenceEnvelope<T> {
    /// Construct a wrapper from explicit parts.
    pub fn new(
        value: T,
        provenance: Provenance,
        trust: TrustPosture,
        freshness: FreshnessState,
    ) -> Self {
        Self {
            value,
            provenance,
            trust,
            freshness,
        }
    }

    /// A LEAF projected from a LiveGraph / migrated-surface [`AnswerEnvelope`]: trust + freshness come
    /// from the answer; `source = {livegraph}`. Use when the cert was GREEN and the LiveGraph served.
    pub fn livegraph_leaf<U>(value: T, answer: &AnswerEnvelope<U>) -> Self {
        Self {
            value,
            provenance: Provenance::livegraph(),
            trust: TrustPosture::from_answer(answer),
            freshness: answer.freshness(),
        }
    }

    /// A LEAF that fell back to SQLite: `source = {sqlite}` + the cert ladder `reason`. The served value
    /// IS the SQLite proven primary, so its posture is the snapshot posture (Fresh/Exact unless `stale`).
    pub fn sqlite_fallback_leaf(value: T, reason: CoherenceFallbackReason, stale: bool) -> Self {
        let (trust, freshness) = if stale {
            (TrustPosture::snapshot_stale(), FreshnessState::Stale)
        } else {
            (TrustPosture::snapshot_exact(), FreshnessState::Fresh)
        };
        Self {
            value,
            provenance: Provenance::sqlite_fallback(reason),
            trust,
            freshness,
        }
    }

    /// A LEAF whose fixed source is SQLite (the proven primary; not a fallback). `stale` => the backing
    /// index is stale.
    pub fn sqlite_leaf(value: T, stale: bool) -> Self {
        let (trust, freshness) = if stale {
            (TrustPosture::snapshot_stale(), FreshnessState::Stale)
        } else {
            (TrustPosture::snapshot_exact(), FreshnessState::Fresh)
        };
        Self {
            value,
            provenance: Provenance::sqlite(),
            trust,
            freshness,
        }
    }

    /// A LEAF whose fixed source is the Tier-A1 `declarations` Authority (boundary / gate). `stale` is
    /// passed through from the snapshot.
    pub fn declaration_leaf(value: T, stale: bool) -> Self {
        let (trust, freshness) = if stale {
            (TrustPosture::snapshot_stale(), FreshnessState::Stale)
        } else {
            (TrustPosture::snapshot_exact(), FreshnessState::Fresh)
        };
        Self {
            value,
            provenance: Provenance::declaration(),
            trust,
            freshness,
        }
    }

    /// A LEAF whose fixed source is the filesystem live-scan (always current-state).
    pub fn filesystem_leaf(value: T) -> Self {
        Self {
            value,
            provenance: Provenance::filesystem(),
            trust: TrustPosture::snapshot_exact(),
            freshness: FreshnessState::Fresh,
        }
    }

    /// The ROOT envelope for the ZERO-SIGNAL (ambiguous / no-match) case (D-ORIENT-4): an explicit
    /// resolution-only posture — NEVER the empty fold's lattice-TOP. `source = {sqlite}` operational
    /// identity only; freshness `Fresh` scoped to the snapshot identity; class `Partial` (never a
    /// structural `Exact`).
    pub fn resolution_only(value: T) -> Self {
        Self {
            value,
            provenance: Provenance::single(Source::Sqlite),
            trust: TrustPosture::resolution_only(),
            freshness: FreshnessState::Fresh,
        }
    }

    /// Map the `value`, preserving the coherence metadata. Useful to lift a leaf's payload into another
    /// representation without recomputing provenance/trust/freshness.
    pub fn map_value<U>(self, f: impl FnOnce(T) -> U) -> CoherenceEnvelope<U> {
        CoherenceEnvelope {
            value: f(self.value),
            provenance: self.provenance,
            trust: self.trust,
            freshness: self.freshness,
        }
    }
}

/// Fold the ROOT (provenance, trust, freshness) PARTS from a set of leaf envelopes — the contract's
/// combine model (Q7-2) WITHOUT yet attaching a root value.
///
/// Use this (instead of [`fold_root`]) when the root value's OWN fields depend on the folded trust — e.g.
/// orient's `confidence` is DERIVED from the root MEET (D-ORIENT-4): compute the parts first, build the
/// value with the derived field, then assemble with [`CoherenceEnvelope::new`]. It also lets a caller add
/// a non-leaf contributor to the root provenance set after folding (e.g. orient's filesystem
/// documentation section, which is a copied field rather than a signal leaf) without disturbing the MEET.
///
/// For a NON-EMPTY leaf set this is the monotone MEET. For an EMPTY leaf set it returns a conservative
/// `Unavailable`/`Unknown` posture — callers serving the zero-signal ambiguous/no-match case must use
/// [`CoherenceEnvelope::resolution_only`] instead (D-ORIENT-4), because a resolved-but-unanalyzed focus
/// is NOT `Unavailable`.
pub fn fold_parts<L>(
    leaves: &[CoherenceEnvelope<L>],
) -> (Provenance, TrustPosture, FreshnessState) {
    let postures: Vec<TrustPosture> = leaves.iter().map(|l| l.trust.clone()).collect();
    let freshness_states: Vec<FreshnessState> = leaves.iter().map(|l| l.freshness).collect();
    let provenances: Vec<&Provenance> = leaves.iter().map(|l| &l.provenance).collect();

    let freshness = meet_freshness(&freshness_states);
    let trust = cap_posture(meet_trust(&postures), freshness);
    let provenance = union_provenance(&provenances);
    (provenance, trust, freshness)
}

/// Fold a ROOT envelope from its leaves: trust = [`meet_trust`] (capped by the freshness MEET), freshness
/// = [`meet_freshness`], provenance = [`union_provenance`]. The `root_value` is the command container.
/// Convenience wrapper over [`fold_parts`] that also attaches the value.
///
/// For a NON-EMPTY leaf set this is the monotone MEET (the contract's combine model, Q7-2). For an EMPTY
/// leaf set it returns a conservative `Unavailable` posture — callers serving the zero-signal
/// ambiguous/no-match case must use [`CoherenceEnvelope::resolution_only`] instead (D-ORIENT-4), NOT this
/// fold, because a resolved-but-unanalyzed focus is NOT `Unavailable`.
pub fn fold_root<T, L>(root_value: T, leaves: &[CoherenceEnvelope<L>]) -> CoherenceEnvelope<T> {
    let (provenance, trust, freshness) = fold_parts(leaves);
    CoherenceEnvelope {
        value: root_value,
        provenance,
        trust,
        freshness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn langs_ts() -> BTreeSet<LanguageSupport> {
        BTreeSet::from([LanguageSupport::TypeScriptPrimary])
    }

    // ── Source serialization ──

    #[test]
    fn source_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&Source::Livegraph).unwrap(),
            "\"livegraph\""
        );
        assert_eq!(
            serde_json::to_string(&Source::Sqlite).unwrap(),
            "\"sqlite\""
        );
        assert_eq!(
            serde_json::to_string(&Source::Declaration).unwrap(),
            "\"declaration\""
        );
    }

    #[test]
    fn source_set_is_ordered() {
        let s = BTreeSet::from([Source::Sqlite, Source::Livegraph, Source::Declaration]);
        // BTreeSet orders by the enum's declaration order (Livegraph < Sqlite < Filesystem < Declaration).
        let v: Vec<Source> = s.into_iter().collect();
        assert_eq!(
            v,
            vec![Source::Livegraph, Source::Sqlite, Source::Declaration]
        );
    }

    // ── CoherenceFallbackReason mirror ──

    #[test]
    fn fallback_reason_as_str_matches_variant_name() {
        assert_eq!(
            CoherenceFallbackReason::LiveGraphUnavailable.as_str(),
            "LiveGraphUnavailable"
        );
        // serde uses the same variant names so the wire string matches the daemon's today.
        assert_eq!(
            serde_json::to_string(&CoherenceFallbackReason::LiveGraphStale).unwrap(),
            "\"LiveGraphStale\""
        );
    }

    // ── TrustPosture projection ──

    #[test]
    fn trust_posture_projects_answer_axes_verbatim() {
        let env = AnswerEnvelope::exact(
            7u32,
            QueryCompleteness::Complete,
            FreshnessState::Fresh,
            vec![],
            langs_ts(),
        )
        .unwrap();
        let p = TrustPosture::from_answer(&env);
        assert_eq!(p.class, AnswerClass::Exact);
        assert_eq!(p.completeness, QueryCompleteness::Complete);
        assert!(p.degradation_reasons.is_empty());
        assert_eq!(p.contributing_languages, langs_ts());
    }

    #[test]
    fn trust_posture_projects_partial_reasons() {
        let env = AnswerEnvelope::partial(
            Some(1u32),
            vec![DegradationReason::ScipFallbackIdentity],
            vec!["engine".into()],
            FreshnessState::Fresh,
            vec![],
            langs_ts(),
        )
        .unwrap();
        let p = TrustPosture::from_answer(&env);
        assert_eq!(p.class, AnswerClass::Partial);
        assert_eq!(
            p.degradation_reasons,
            vec![DegradationReason::ScipFallbackIdentity]
        );
    }

    // ── meet_freshness ──

    #[test]
    fn meet_freshness_all_fresh_is_fresh() {
        assert_eq!(
            meet_freshness(&[FreshnessState::Fresh, FreshnessState::Fresh]),
            FreshnessState::Fresh
        );
    }

    #[test]
    fn meet_freshness_one_stale_lowers() {
        assert_eq!(
            meet_freshness(&[FreshnessState::Fresh, FreshnessState::Stale]),
            FreshnessState::Stale
        );
    }

    #[test]
    fn meet_freshness_precision_pending_between_fresh_and_stale() {
        assert_eq!(
            meet_freshness(&[FreshnessState::Fresh, FreshnessState::PrecisionPending]),
            FreshnessState::PrecisionPending
        );
        assert_eq!(
            meet_freshness(&[FreshnessState::PrecisionPending, FreshnessState::Stale]),
            FreshnessState::Stale
        );
    }

    #[test]
    fn meet_freshness_unavailable_dominates() {
        assert_eq!(
            meet_freshness(&[
                FreshnessState::Fresh,
                FreshnessState::Unavailable,
                FreshnessState::Stale
            ]),
            FreshnessState::Unavailable
        );
    }

    #[test]
    fn meet_freshness_empty_is_unavailable_never_fresh() {
        assert_eq!(meet_freshness(&[]), FreshnessState::Unavailable);
    }

    // ── meet_trust + cap_posture (the anti-false-completeness core) ──

    fn exact_ts() -> TrustPosture {
        TrustPosture {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            degradation_reasons: vec![],
            contributing_languages: langs_ts(),
        }
    }

    #[test]
    fn meet_trust_all_exact_stays_exact_under_fresh() {
        let posture = meet_trust(&[exact_ts(), exact_ts()]);
        let capped = cap_posture(posture, FreshnessState::Fresh);
        assert_eq!(capped.class, AnswerClass::Exact);
        assert_eq!(capped.completeness, QueryCompleteness::Complete);
    }

    #[test]
    fn meet_trust_one_partial_lowers_root_to_partial() {
        let partial = TrustPosture {
            class: AnswerClass::Partial,
            completeness: QueryCompleteness::Degraded,
            degradation_reasons: vec![DegradationReason::AnonymousStructuralMember],
            contributing_languages: langs_ts(),
        };
        let posture = meet_trust(&[exact_ts(), partial]);
        let capped = cap_posture(posture, FreshnessState::Fresh);
        assert_eq!(capped.class, AnswerClass::Partial);
        assert_eq!(capped.completeness, QueryCompleteness::Degraded);
        // The leaf's reason is unioned into the root, never dropped.
        assert!(capped
            .degradation_reasons
            .contains(&DegradationReason::AnonymousStructuralMember));
    }

    #[test]
    fn precision_pending_caps_exact_root_to_partial() {
        // Two Exact leaves, but the freshness MEET is PrecisionPending -> root capped at Partial.
        let posture = meet_trust(&[exact_ts(), exact_ts()]);
        let capped = cap_posture(posture, FreshnessState::PrecisionPending);
        assert_eq!(capped.class, AnswerClass::Partial);
        assert_eq!(capped.completeness, QueryCompleteness::Degraded);
    }

    #[test]
    fn stale_freshness_caps_root_to_stale() {
        let posture = meet_trust(&[exact_ts(), exact_ts()]);
        let capped = cap_posture(posture, FreshnessState::Stale);
        assert_eq!(capped.class, AnswerClass::Stale);
    }

    #[test]
    fn no_fold_manufactures_exact_from_non_exact() {
        // The formal guarantee: if any leaf is non-Exact, the root is never Exact.
        let stale = TrustPosture::snapshot_stale();
        let posture = meet_trust(&[exact_ts(), stale]);
        let capped = cap_posture(
            posture,
            meet_freshness(&[FreshnessState::Fresh, FreshnessState::Stale]),
        );
        assert_ne!(capped.class, AnswerClass::Exact);
    }

    #[test]
    fn meet_trust_languages_union_never_collapsed() {
        let a = TrustPosture {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            degradation_reasons: vec![],
            contributing_languages: BTreeSet::from([LanguageSupport::TypeScriptPrimary]),
        };
        let b = TrustPosture {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            degradation_reasons: vec![],
            contributing_languages: BTreeSet::from([LanguageSupport::RustPartialBeta]),
        };
        let posture = meet_trust(&[a, b]);
        assert_eq!(posture.contributing_languages.len(), 2);
    }

    #[test]
    fn meet_trust_empty_is_unavailable_never_exact() {
        let posture = meet_trust(&[]);
        assert_eq!(posture.class, AnswerClass::Unavailable);
        assert_eq!(posture.completeness, QueryCompleteness::Unknown);
    }

    // ── union_provenance ──

    #[test]
    fn union_provenance_unions_sources_root_superset_of_leaves() {
        let lg = Provenance::livegraph();
        let sql = Provenance::sqlite();
        let decl = Provenance::declaration();
        let root = union_provenance(&[&lg, &sql, &decl]);
        assert!(root.source.contains(&Source::Livegraph));
        assert!(root.source.contains(&Source::Sqlite));
        assert!(root.source.contains(&Source::Declaration));
        assert_eq!(root.source.len(), 3);
    }

    #[test]
    fn union_provenance_carries_first_fallback() {
        let lg = Provenance::livegraph();
        let fb = Provenance::sqlite_fallback(CoherenceFallbackReason::LiveGraphPartial);
        let root = union_provenance(&[&lg, &fb]);
        assert_eq!(
            root.fallback_reason,
            Some(CoherenceFallbackReason::LiveGraphPartial)
        );
    }

    // ── fold_root (end-to-end) ──

    #[test]
    fn fold_root_healthy_case_is_exact_fresh() {
        let leaves = vec![
            CoherenceEnvelope::new(
                1u32,
                Provenance::livegraph(),
                exact_ts(),
                FreshnessState::Fresh,
            ),
            CoherenceEnvelope::new(
                2u32,
                Provenance::sqlite(),
                TrustPosture::snapshot_exact(),
                FreshnessState::Fresh,
            ),
        ];
        let root = fold_root("root", &leaves);
        assert_eq!(root.trust.class, AnswerClass::Exact);
        assert_eq!(root.freshness, FreshnessState::Fresh);
        assert!(root.provenance.source.contains(&Source::Livegraph));
        assert!(root.provenance.source.contains(&Source::Sqlite));
    }

    #[test]
    fn fold_root_one_precision_pending_leaf_caps_root() {
        // An IMPORT_CYCLES leaf is PrecisionPending (SCIP refresh pending); a SQLite leaf is Fresh.
        let pp_leaf = CoherenceEnvelope::new(
            1u32,
            Provenance::livegraph(),
            TrustPosture {
                class: AnswerClass::Partial,
                completeness: QueryCompleteness::Degraded,
                degradation_reasons: vec![],
                contributing_languages: langs_ts(),
            },
            FreshnessState::PrecisionPending,
        );
        let sql_leaf = CoherenceEnvelope::new(
            2u32,
            Provenance::sqlite(),
            TrustPosture::snapshot_exact(),
            FreshnessState::Fresh,
        );
        let root = fold_root("root", &[pp_leaf, sql_leaf]);
        assert_eq!(root.freshness, FreshnessState::PrecisionPending);
        assert_ne!(root.trust.class, AnswerClass::Exact);
    }

    #[test]
    fn fold_root_monotone_never_raises() {
        // Property: root rank <= min leaf rank on each axis.
        let leaves = vec![
            CoherenceEnvelope::new(
                1u32,
                Provenance::livegraph(),
                exact_ts(),
                FreshnessState::Fresh,
            ),
            CoherenceEnvelope::new(
                2u32,
                Provenance::sqlite_fallback(CoherenceFallbackReason::LiveGraphStale),
                TrustPosture::snapshot_stale(),
                FreshnessState::Stale,
            ),
        ];
        let min_class = leaves
            .iter()
            .map(|l| class_rank(l.trust.class))
            .min()
            .unwrap();
        let min_fresh = leaves
            .iter()
            .map(|l| freshness_rank(l.freshness))
            .min()
            .unwrap();
        let root = fold_root("root", &leaves);
        assert!(class_rank(root.trust.class) <= min_class);
        assert!(freshness_rank(root.freshness) <= min_fresh);
    }

    // ── resolution_only (D-ORIENT-4 zero-signal) ──

    #[test]
    fn resolution_only_is_never_structural_exact() {
        let env = CoherenceEnvelope::resolution_only("ambiguous-result");
        assert_ne!(env.trust.class, AnswerClass::Exact);
        // Guard 1: operational-identity-only provenance.
        assert_eq!(env.provenance.source, BTreeSet::from([Source::Sqlite]));
        // Guard 2: no structural language partition contributed.
        assert!(env.trust.contributing_languages.is_empty());
        // The snapshot identity is current.
        assert_eq!(env.freshness, FreshnessState::Fresh);
    }

    // ── serde round-trip of the full envelope ──

    #[test]
    fn envelope_serde_round_trips() {
        let env = CoherenceEnvelope::new(
            vec![10u32, 20u32],
            Provenance {
                source: BTreeSet::from([Source::Livegraph, Source::Sqlite]),
                basis: vec![],
                missing_partitions: vec!["engine".into()],
                fallback_reason: Some(CoherenceFallbackReason::LiveGraphPartial),
            },
            TrustPosture {
                class: AnswerClass::Partial,
                completeness: QueryCompleteness::Degraded,
                degradation_reasons: vec![DegradationReason::ScipFallbackIdentity],
                contributing_languages: langs_ts(),
            },
            FreshnessState::PrecisionPending,
        );
        let json = serde_json::to_string(&env).unwrap();
        let back: CoherenceEnvelope<Vec<u32>> = serde_json::from_str(&json).unwrap();
        assert_eq!(env, back);
    }
}
