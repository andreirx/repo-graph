//! Coherent orient output — `CoherentOrientResult` + the pure `to_coherent` conversion.
//!
//! This module realizes the ratified COHERENCE-LAYER-1 contract for `orient` (ORIENT-LIVEGRAPH-1):
//! `rmap orient` returns a `CoherenceEnvelope<CoherentOrientResult>` instead of a bare [`OrientResult`].
//!
//! ## What lives here vs the daemon
//!
//! This is PURE policy code (Clean Architecture). It owns:
//!   - the [`CoherentOrientResult`] command-container DTO (contract D7 = `OrientResult` with its
//!     `signals` slot re-typed to leaf [`CoherenceEnvelope<Signal>`], plus the ratified `trust_briefing`
//!     field, D-ORIENT-6 = O2),
//!   - per-signal SOURCE CLASSIFICATION (which signal code maps to which `Source`),
//!   - LEAF WRAPPING + the ROOT MEET fold (delegated to `repo-graph-coherence`),
//!   - the confidence derivation (D-ORIENT-4: confidence becomes the root MEET, capped ≤ the legacy
//!     `derive_repo_confidence` value), and
//!   - the zero-signal resolution-only carve-out (D-ORIENT-4 / ambiguous + no-match).
//!
//! It does NOT read the LiveGraph, SQLite, or any cert. The DAEMON owns those: it supplies the per-leaf
//! LiveGraph DECISIONS ([`OrientLgDecisions`]) for orient's reuse-backed LG-first signals and the
//! `trust_briefing` overlay, then calls [`to_coherent`]. This keeps the conversion fully off-target
//! unit-testable (the daemon's IO is the only part needing a live `RepoState`).
//!
//! ## LG-first leaf set (D-ORIENT-1)
//!
//! orient's ratified LG-first leaves are ALL FOUR of IMPORT_CYCLES, HIGH_COMPLEXITY, CALLERS_SUMMARY,
//! CALLEES_SUMMARY. Each is gated by a daemon-side NO-LOSS proof so a `livegraph` source is never minted
//! over un-corroborated current-state structure. The proof's SCOPE sets the leaf's source set — single- vs
//! multi-source (D8) — so the provenance never over-claims what the proof actually covers:
//!   - IMPORT_CYCLES — the repo MODULE-cycle no-loss cert is a FIELD-EXACT WHOLE-value compare (the proven
//!     LG cycle set IS the rendered evidence: same module identities + cycle lengths) → single-source
//!     `{livegraph}`.
//!   - HIGH_COMPLEXITY — the repo COMPLEXITY no-loss cert proves the LiveGraph repo-wide `value_facts`
//!     `(symbol_key, complexity)` SET equals SQLite `measurements`, served over the
//!     `LiveGraph::high_complexity` read. NO new producer/extraction — the cyclomatic facts are the SAME
//!     VALUE-JOIN-1 facts `value_facts(symbol)` already exposes, read repo-wide. But the emitted
//!     `HighComplexityEvidence` renders a top-N SAMPLE with each symbol's DISPLAY NAME + file path +
//!     ordering, which the cert does not compare and `LiveGraph::high_complexity` does not even carry — so
//!     that rendered evidence is SQLite-built → MULTI-source `{livegraph, sqlite}` (the LG corroborates the
//!     set; SQLite renders the sample). review-9 gap 2.
//!   - CALLERS_SUMMARY / CALLEES_SUMMARY — the migrated callers/callees `Auto` ladder PLUS a per-symbol
//!     no-loss KEY-SET compare (LG key set == SQLite `find_symbol_callers`/`find_symbol_callees`); the
//!     emitted summary is MODULE-grouped from SQLite rows the LG partition-grouped answer does not carry →
//!     MULTI-source `{livegraph, sqlite}`.
//!
//! The daemon supplies all four through [`OrientLgDecisions`]; the agent never reads the LiveGraph, SQLite,
//! or any cert. The single- vs multi-source split is owned by [`livegraph_served_is_multi_source`].

use std::collections::BTreeSet;

use serde::Serialize;

use repo_graph_coherence::{
    fold_parts, meet_freshness, AnswerClass, CoherenceEnvelope, CoherenceFallbackReason,
    DegradationReason, FreshnessState, LanguageSupport, Provenance, QueryCompleteness, Source,
    TrustPosture,
};

use crate::dto::envelope::{Confidence, DocumentationSection, Focus, NextAction};
use crate::dto::envelope::{OrientResult, ORIENT_SCHEMA};
use crate::dto::limit::{Limit, LimitCode};
use crate::dto::signal::{FreshnessStateDto, Signal, SignalCode};

// ── Per-leaf LiveGraph decision (daemon → agent) ──────────────────

/// The daemon's per-leaf decision for one of orient's reuse-backed LG-first signals.
///
/// The daemon computes this by reusing the existing cert-gated surfaces (the cycles no-loss cert for
/// IMPORT_CYCLES; the migrated callers/callees `Auto` ladder for CALLERS_SUMMARY / CALLEES_SUMMARY) and
/// hands it to [`to_coherent`]. The agent never reads the LiveGraph itself.
#[derive(Debug, Clone)]
pub enum OrientLeafLabel {
    /// The LiveGraph served this leaf (cert GREEN / answer Exact + Fresh + TS-only). The posture axes
    /// are projected verbatim from the LiveGraph `AnswerEnvelope` (contract Q1 / §3a). The served VALUE
    /// is byte-identical to the SQLite-computed signal (parity P1/P5), so only the wrapper gains labels.
    Livegraph {
        /// The answer class projected from the LiveGraph answer.
        class: AnswerClass,
        /// The completeness verdict projected from the LiveGraph answer.
        completeness: QueryCompleteness,
        /// The epoch axis projected from the LiveGraph answer.
        freshness: FreshnessState,
        /// The identity-degradation reasons projected from the LiveGraph answer.
        degradation_reasons: Vec<DegradationReason>,
        /// The contributing-language maturities projected from the LiveGraph answer.
        contributing_languages: BTreeSet<LanguageSupport>,
    },
    /// The LiveGraph could not serve this leaf: it falls back to the proven SQLite primary, labelled with
    /// the cert-ladder `reason` (the leaf's value IS the SQLite-computed signal).
    SqliteFallback {
        /// Why the LiveGraph-first leaf flipped to SQLite (the cert ladder).
        reason: CoherenceFallbackReason,
    },
}

/// The daemon-supplied LiveGraph decisions for orient's FOUR LG-first signals (D-ORIENT-1).
///
/// `None` for a field means the daemon did not (or could not) attempt the LiveGraph for that signal — the
/// leaf then defaults to the proven SQLite primary (`source = {sqlite}`, no fallback reason). Every field
/// is decided by a daemon-side NO-LOSS proof so a `livegraph` label is never a bare relabel of a SQLite
/// value (see the module doc).
#[derive(Debug, Clone, Default)]
pub struct OrientLgDecisions {
    /// IMPORT_CYCLES (repo / path focus + the symbol `ModuleContext` variant) — cycles no-loss cert.
    pub import_cycles: Option<OrientLeafLabel>,
    /// HIGH_COMPLEXITY (repo focus ONLY) — the complexity no-loss cert over the `high_complexity` read.
    pub high_complexity: Option<OrientLeafLabel>,
    /// CALLERS_SUMMARY (symbol focus) — migrated `callers` `Auto` ladder + per-symbol no-loss key compare.
    pub callers_summary: Option<OrientLeafLabel>,
    /// CALLEES_SUMMARY (symbol focus) — migrated `callees` `Auto` ladder + per-symbol no-loss key compare.
    pub callees_summary: Option<OrientLeafLabel>,
}

impl OrientLgDecisions {
    /// Look up the decision for a signal code (only the four LG-first codes have one).
    fn for_code(&self, code: SignalCode) -> Option<&OrientLeafLabel> {
        match code {
            SignalCode::ImportCycles => self.import_cycles.as_ref(),
            SignalCode::HighComplexity => self.high_complexity.as_ref(),
            SignalCode::CallersSummary => self.callers_summary.as_ref(),
            SignalCode::CalleesSummary => self.callees_summary.as_ref(),
            _ => None,
        }
    }
}

// ── CoherentOrientResult (contract D7) ────────────────────────────

/// The coherence command-container DTO: [`OrientResult`] with its `signals` slot re-typed to leaf
/// envelopes (contract D7) PLUS the ratified `trust_briefing` field (D-ORIENT-6 = O2).
///
/// Every non-`signals` field is copied VERBATIM from [`OrientResult`] — the per-fact value payload (each
/// `Signal` evidence) stays pristine; only the command CONTAINER changes shape (its `signals` slot now
/// holds leaf [`CoherenceEnvelope<Signal>`] and it gains `trust_briefing`). `confidence` carries the
/// MEET-derived value (D-ORIENT-4), NOT the raw legacy value.
#[derive(Debug, Clone, Serialize)]
pub struct CoherentOrientResult {
    /// Stable schema identifier (`rgr.agent.v1`).
    pub schema: &'static str,
    /// Stable command identifier (`orient`).
    pub command: &'static str,
    /// The repo NAME (not the uid) — copied verbatim from [`OrientResult`].
    pub repo: String,
    /// Human-readable repo name for CLI presentation (daemon-populated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The snapshot uid.
    pub snapshot: String,
    /// The focus resolution outcome.
    pub focus: Focus,
    /// The MEET-derived confidence (≤ the legacy `derive_repo_confidence` value; D-ORIENT-4). For the
    /// zero-signal ambiguous/no-match case it is the legacy STATIC `High` preserved verbatim.
    pub confidence: Confidence,
    /// The filesystem documentation section (copied verbatim; contributes `filesystem` to the root
    /// provenance set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<DocumentationSection>,
    /// The contract's re-typed slot: each emitted signal is now a LEAF `CoherenceEnvelope<Signal>` whose
    /// sibling fields carry that signal's provenance / trust / freshness. The inner `Signal` is pristine.
    pub signals: Vec<CoherenceEnvelope<Signal>>,
    /// Whether the signal list was budget-truncated (copied verbatim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signals_truncated: Option<bool>,
    /// Count of omitted signals on truncation (copied verbatim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signals_omitted_count: Option<usize>,
    /// The limits list (copied verbatim — limits are NOT re-typed; they are degradation markers).
    pub limits: Vec<Limit>,
    /// Whether the limit list was budget-truncated (copied verbatim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits_truncated: Option<bool>,
    /// Count of omitted limits on truncation (copied verbatim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits_omitted_count: Option<usize>,
    /// The next-actions list (copied verbatim; orient emits none today).
    pub next: Vec<NextAction>,
    /// Whether the next list was truncated (copied verbatim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_truncated: Option<bool>,
    /// Count of omitted next-actions on truncation (copied verbatim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_omitted_count: Option<usize>,
    /// Whether any section was truncated (copied verbatim).
    pub truncated: bool,
    /// D-ORIENT-6 = O2: the daemon's degraded-state trust briefing overlay, relocated from the old
    /// post-serialize top-level `trust` key onto this struct (populated before serialization, like
    /// `display_name`). Carried as an opaque `serde_json::Value` — the daemon serializes
    /// `repo_graph_trust::TrustOverlaySummary` into it byte-identically — so the agent crate keeps its
    /// documented no-dependency-on-`repo-graph-trust` boundary (D-ORIENT-6 CRATE-HOME option c). `None`
    /// (absent on the wire) unless degraded; disjoint from the envelope root `trust: TrustPosture`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_briefing: Option<serde_json::Value>,
    /// HONEST-DEGRADATION-IMPL-2 (D5): the daemon's toolchain-aware honest next-action line for a
    /// LOW-relationship-reliability repo (e.g. "run `rmap enrich` to resolve more" / "no
    /// semantic-resolution path exists for C on this build"). Carried as plain text (the daemon owns the
    /// resolver-availability keying; the agent crate stays toolchain-agnostic). Populated post-fold by the
    /// daemon adapter, like `display_name` / `trust_briefing`; `None` (absent on the wire) unless rendered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_next_action: Option<String>,
}

// ── Source classification (pure, by signal code) ──────────────────

/// The fixed (non-LG-first) source of a signal, derived from its code.
enum BaseSource {
    /// SQLite snapshot-scoped cache (the proven primary).
    Sqlite,
    /// Tier-A1 `declarations` Authority.
    Declaration,
    /// A leaf DERIVED from BOTH the structural import edges (sqlite) AND a forbidden-import declaration
    /// (Authority) — the D8 multi-source case (e.g. BOUNDARY_VIOLATIONS).
    SqliteAndDeclaration,
}

/// Is this code one of orient's FOUR LG-first signals (D-ORIENT-1)? Each is daemon-gated by a no-loss
/// proof: IMPORT_CYCLES (cycles cert), HIGH_COMPLEXITY (complexity cert over the `high_complexity` read),
/// CALLERS_SUMMARY / CALLEES_SUMMARY (the `Auto` ladder + a per-symbol key-set compare).
fn is_lg_first(code: SignalCode) -> bool {
    matches!(
        code,
        SignalCode::ImportCycles
            | SignalCode::HighComplexity
            | SignalCode::CallersSummary
            | SignalCode::CalleesSummary
    )
}

/// The fixed source class for a non-LG-first signal (the orient source map, §2 / §3a).
fn base_source(code: SignalCode) -> BaseSource {
    use SignalCode::*;
    match code {
        // Authority + structural-edge half (D8 multi-source).
        BoundaryViolations => BaseSource::SqliteAndDeclaration,
        // Pure Tier-A1 Authority (requirement / obligation / waiver evaluation).
        GatePass | GateFail | GateIncomplete | CheckPass | CheckFail | CheckIncomplete => {
            BaseSource::Declaration
        }
        // Everything else orient emits is SQLite-first: SNAPSHOT_INFO, TRUST_*, BOUNDARY_LINKS_SUMMARY,
        // MODULE_SUMMARY, and any defensive default. (The four LG-first codes never reach `base_source` —
        // `is_lg_first` routes them through the daemon decision first.)
        _ => BaseSource::Sqlite,
    }
}

// ── Leaf construction ─────────────────────────────────────────────

/// Reconcile a signal's L2 `FreshnessInfo` (Current / Impacted / Unknown — a DIFFERENT vocabulary, from
/// `artifact_contracts`) into the AUTHORITATIVE leaf-envelope `FreshnessState` (D-ORIENT-7 / contract
/// RISK-G / spec §3c R1). This is the SINGLE mapping; the OUTER leaf freshness is authoritative, and the
/// inner `Signal.freshness` is kept PRISTINE (render-only detail) — never a second authoritative freshness
/// truth for one signal.
///
/// The mapping is honest — it can never mint false freshness:
///   - `Current` -> `Fresh` (all backing artifacts are current).
///   - `Impacted` -> `Stale` (>= 1 backing L0 fact changed, so the derived L2 summary is behind).
///   - `Unknown` -> `Stale` (no provenance tracked -> cannot vouch for currency; conservative, never Fresh).
///
/// `Unknown` maps to `Stale` rather than a weaker state because the trust model has no "unknown freshness":
/// the value IS present (so not `Unavailable`) and no refresh was attempted (so not `RefreshFailed`), while
/// reporting `Fresh` would mint false freshness. The Impacted-vs-Unknown distinction (and `impacted_since`)
/// is retained on the pristine inner `FreshnessInfo` for rendering.
fn l2_freshness_to_state(state: FreshnessStateDto) -> FreshnessState {
    match state {
        FreshnessStateDto::Current => FreshnessState::Fresh,
        FreshnessStateDto::Impacted | FreshnessStateDto::Unknown => FreshnessState::Stale,
    }
}

/// Wrap a signal whose source is fixed (non-LG-first): SQLite / Authority / multi-source.
///
/// The outer freshness is the MEET of the snapshot freshness (`stale`) and the signal's reconciled L2
/// `FreshnessInfo` (D-ORIENT-7): a `BOUNDARY_LINKS_SUMMARY` carrying `Impacted`/`Unknown` is `Stale` at the
/// leaf even when the SQLite snapshot itself is fresh, so it can never report a false `Fresh`/`Exact`. The
/// trust posture pairs with the freshness (`Stale` -> `snapshot_stale`), so the root MEET — and the derived
/// confidence — are lowered too. The inner `Signal` stays PRISTINE; its `FreshnessInfo` is render-only.
fn fixed_leaf(signal: Signal, base: BaseSource, stale: bool) -> CoherenceEnvelope<Signal> {
    let snapshot_freshness = if stale {
        FreshnessState::Stale
    } else {
        FreshnessState::Fresh
    };
    let freshness = match signal
        .freshness()
        .map(|info| l2_freshness_to_state(info.state))
    {
        // The leaf is as fresh as the WORSE of the snapshot and the reconciled L2 link freshness.
        Some(link_freshness) => meet_freshness(&[snapshot_freshness, link_freshness]),
        None => snapshot_freshness,
    };
    let trust = if freshness == FreshnessState::Fresh {
        TrustPosture::snapshot_exact()
    } else {
        TrustPosture::snapshot_stale()
    };
    let provenance = match base {
        BaseSource::Sqlite => Provenance::sqlite(),
        BaseSource::Declaration => Provenance::declaration(),
        BaseSource::SqliteAndDeclaration => {
            Provenance::multi([Source::Sqlite, Source::Declaration])
        }
    };
    CoherenceEnvelope::new(signal, provenance, trust, freshness)
}

/// Does a LiveGraph-SERVED leaf for this signal carry the SQLite snapshot as a CO-source (D8
/// multi-source), rather than single-source `{livegraph}`?
///
/// `true` for CALLERS_SUMMARY / CALLEES_SUMMARY / HIGH_COMPLEXITY — the leaves whose daemon no-loss gate
/// proves only that a CURRENT-STATE SET matches SQLite, while the EMITTED EVIDENCE the agent renders carries
/// fields the gate does NOT compare (and the LiveGraph surface does not even carry):
///   - CALLERS_SUMMARY / CALLEES_SUMMARY: the gate proves the caller/callee KEY SET equals SQLite
///     `find_symbol_callers`/`find_symbol_callees`, but the emitted summary is MODULE-grouped (count +
///     top-3 owning modules) from those SQLite rows' `module_path`, which the LiveGraph PARTITION-grouped
///     answer does not carry (review-6 pt3 / spec §2 CALLERS_SUMMARY data-shape note).
///   - HIGH_COMPLEXITY: the complexity no-loss cert proves the `(symbol_key, complexity)` SET equals SQLite
///     `measurements`, but the emitted `HighComplexityEvidence` carries the top-N SAMPLE with each symbol's
///     DISPLAY NAME (`symbol_name`) + file path + the top-N ordering — none of which the cert compares, and
///     none of which `LiveGraph::high_complexity` even exposes (`HighComplexityFact` carries only the
///     canonical key + value + optional file; it has NO display name). So the rendered evidence is
///     necessarily SQLite-built (review-9 gap 2).
///
/// In every case the SET is LiveGraph-CORROBORATED (current-state, Exact) while the SERVED EVIDENCE is
/// SQLite-computed → the honest source set is `{livegraph, sqlite}`. Labelling these single-source
/// `livegraph` would over-claim that the rendered evidence is LiveGraph-derived.
///
/// `false` for IMPORT_CYCLES ONLY: the cycle no-loss CERT is a FIELD-EXACT WHOLE-value compare (the
/// LiveGraph module-cycle set — the same module identities + cycle lengths the evidence renders — is proven
/// byte-identical to SQLite, parity P1), so the served value IS the LiveGraph value → honestly single-source
/// `{livegraph}`.
fn livegraph_served_is_multi_source(code: SignalCode) -> bool {
    matches!(
        code,
        SignalCode::CallersSummary | SignalCode::CalleesSummary | SignalCode::HighComplexity
    )
}

/// Wrap an LG-first signal from the daemon's decision (livegraph posture, or labelled SQLite fallback).
fn lg_first_leaf(
    signal: Signal,
    label: &OrientLeafLabel,
    stale: bool,
) -> CoherenceEnvelope<Signal> {
    let code = signal.code();
    match label {
        OrientLeafLabel::Livegraph {
            class,
            completeness,
            freshness,
            degradation_reasons,
            contributing_languages,
        } => {
            // Single-source {livegraph} for the field-exact whole-value cert leaf (cycles only); multi-source
            // {livegraph, sqlite} for the SET-corroborated but SQLite-RENDERED leaves: callers/callees
            // (module grouping) and high-complexity (top-N display names + file paths the cert never covers).
            let provenance = if livegraph_served_is_multi_source(code) {
                Provenance::multi([Source::Livegraph, Source::Sqlite])
            } else {
                Provenance::livegraph()
            };
            CoherenceEnvelope::new(
                signal,
                provenance,
                TrustPosture {
                    class: *class,
                    completeness: *completeness,
                    degradation_reasons: degradation_reasons.clone(),
                    contributing_languages: contributing_languages.clone(),
                },
                *freshness,
            )
        }
        OrientLeafLabel::SqliteFallback { reason } => {
            // The served value is the SQLite proven primary -> the snapshot posture.
            CoherenceEnvelope::sqlite_fallback_leaf(signal, *reason, stale)
        }
    }
}

/// Append the contract's ENVELOPE-LEVEL provenance-derived limit codes (COHERENCE-LAYER-1 :458; orient
/// slice §ENVELOPE limits[] :546) so coherence degradation/provenance is MACHINE-DISCOVERABLE at the
/// envelope level, not only inside the per-leaf trust postures. DERIVED purely from the already-folded
/// leaves + the snapshot `stale` flag — NO new source read. Emitted WHEN AND ONLY WHEN the matching
/// condition occurred (validation E5). Additive: orient's pre-existing degradation limits
/// (MODULE_DATA_UNAVAILABLE / COMPLEXITY_UNAVAILABLE / GATE_NOT_CONFIGURED) are orthogonal and untouched.
///
/// `pub(crate)` so `explain`'s coherence assembly (`crate::explain::coherent`) reuses the IDENTICAL
/// provenance-limit derivation — EXPLAIN-LIVEGRAPH-1 §3b makes these limit codes net-new for explain (it
/// has no pre-existing limits), so the single source of truth is shared, not duplicated. Behaviour is
/// unchanged; only the visibility is widened (orient's `to_coherent` still calls it identically).
pub(crate) fn append_provenance_limits(
    limits: &mut Vec<Limit>,
    leaves: &[CoherenceEnvelope<Signal>],
    stale: bool,
) {
    use repo_graph_coherence::CoherenceFallbackReason as Fb;

    // SQLITE_SNAPSHOT_STALE: the backing index is stale -> SQLite/Authority/FS leaves are snapshot-Stale.
    if stale {
        push_unique_limit(limits, LimitCode::SqliteSnapshotStale);
    }
    for leaf in leaves {
        // PRECISION_PENDING: a LiveGraph leaf served under an in-flight SCIP refresh.
        if leaf.freshness == FreshnessState::PrecisionPending {
            push_unique_limit(limits, LimitCode::PrecisionPending);
        }
        // AUTHORITY_OVERLAY_APPLIED: a Tier-A1 declaration contributed (boundary/gate); overlay, never erase.
        if leaf.provenance.source.contains(&Source::Declaration) {
            push_unique_limit(limits, LimitCode::AuthorityOverlayApplied);
        }
        // An LG-first leaf that fell back -> surface the COARSE envelope reason (the per-leaf
        // `fallback_reason` keeps the fine detail). Other fallback reasons (cert divergence / stale /
        // unsupported-language) serve the proven SQLite primary with NO degradation, so they get no
        // envelope code — the answer is fully trustworthy, only the LiveGraph accelerant was declined.
        match leaf.provenance.fallback_reason {
            Some(Fb::LiveGraphUnavailable) => {
                push_unique_limit(limits, LimitCode::ProducerUnavailable)
            }
            Some(Fb::LiveGraphPartial) => push_unique_limit(limits, LimitCode::LivegraphPartial),
            _ => {}
        }
    }
}

/// Push a limit code iff it is not already present (idempotent — the provenance codes are a small fixed
/// set; a derived code never duplicates an aggregator-emitted one).
fn push_unique_limit(limits: &mut Vec<Limit>, code: LimitCode) {
    if !limits.iter().any(|l| l.code == code) {
        limits.push(Limit::from_code(code));
    }
}

// ── Confidence derivation (D-ORIENT-4) ────────────────────────────

/// Map a root trust posture to a `Confidence` band: `Exact`+`Complete` → High; `Stale`/`Unavailable` or
/// `Unknown` completeness → Low; otherwise (Partial / Degraded) → Medium. Monotone with the lattice.
///
/// `pub(crate)` so the SHARED D3 confidence-from-MEET mapping is reused by `check`'s coherence assembly
/// (`crate::check::coherent`, D-CHECK-3) — a single source of truth, not a duplicated mapping. Behavior
/// is unchanged; only the visibility widened (orient's `to_coherent` still calls it identically).
pub(crate) fn confidence_from_posture(trust: &TrustPosture) -> Confidence {
    match (trust.class, trust.completeness) {
        (AnswerClass::Exact, QueryCompleteness::Complete) => Confidence::High,
        (AnswerClass::Unavailable, _) | (_, QueryCompleteness::Unknown) => Confidence::Low,
        (AnswerClass::Stale, _) => Confidence::Low,
        _ => Confidence::Medium,
    }
}

fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::Low => 0,
        Confidence::Medium => 1,
        Confidence::High => 2,
    }
}

/// The MEET of two confidence bands (the weaker wins) — used to cap the coherent confidence at the legacy
/// value (D-ORIENT-4 / validation E1: coherent confidence ≤ legacy `derive_repo_confidence`).
///
/// `pub(crate)` so `check`'s coherence assembly reuses the SAME legacy-cap rule (D-CHECK-3). Visibility
/// widened only; orient's behavior is unchanged.
pub(crate) fn min_confidence(a: Confidence, b: Confidence) -> Confidence {
    if confidence_rank(a) <= confidence_rank(b) {
        a
    } else {
        b
    }
}

// ── The conversion ────────────────────────────────────────────────

/// Convert a bare [`OrientResult`] into the coherence wrapper `CoherenceEnvelope<CoherentOrientResult>`.
///
/// - `lg` carries the daemon's per-leaf LiveGraph decisions for the reuse-backed LG-first signals.
/// - `trust_briefing` is the daemon's degraded-state overlay (`Some` only when degraded; D-ORIENT-6).
/// - `stale` is whether the backing index is stale (`get_stale_files` non-empty) — it sets the SQLite /
///   Authority / FS leaves' freshness.
///
/// ZERO-SIGNAL (ambiguous / no-match): the empty-signal builders emit no leaves, so the root takes the
/// explicit resolution-only posture (D-ORIENT-4 / §3b) — NEVER the empty fold's lattice-TOP — and the
/// confidence is the legacy STATIC `High` preserved verbatim.
pub fn to_coherent(
    result: OrientResult,
    lg: &OrientLgDecisions,
    trust_briefing: Option<serde_json::Value>,
    stale: bool,
) -> CoherenceEnvelope<CoherentOrientResult> {
    let OrientResult {
        schema,
        command,
        repo,
        display_name,
        snapshot,
        focus,
        confidence,
        documentation,
        signals,
        signals_truncated,
        signals_omitted_count,
        limits,
        limits_truncated,
        limits_omitted_count,
        next,
        next_truncated,
        next_omitted_count,
        truncated,
    } = result;

    // ── ZERO-SIGNAL carve-out (ambiguous / no-match). ────────────
    // The empty-signal builders emit no leaves; the structural MEET has no inputs. Serve the explicit
    // resolution-only posture (operational-identity-only provenance; static `High` preserved), NEVER the
    // empty fold's lattice-TOP (which would falsely read Exact over un-analyzed structure). D-ORIENT-4.
    if signals.is_empty() {
        let value = CoherentOrientResult {
            schema,
            command,
            repo,
            display_name,
            snapshot,
            focus,
            confidence, // the legacy STATIC High, preserved verbatim (NOT recomputed)
            documentation,
            signals: Vec::new(),
            signals_truncated,
            signals_omitted_count,
            limits,
            limits_truncated,
            limits_omitted_count,
            next,
            next_truncated,
            next_omitted_count,
            truncated,
            trust_briefing,
            // D5 (IMPL-2) next-action is populated post-fold by the daemon adapter (build_orient_envelope).
            relationship_next_action: None,
        };
        return CoherenceEnvelope::resolution_only(value);
    }

    // ── Wrap each signal as a leaf. ──────────────────────────────
    let mut leaves: Vec<CoherenceEnvelope<Signal>> = Vec::with_capacity(signals.len());
    for signal in signals {
        let code = signal.code();
        let leaf = if is_lg_first(code) {
            match lg.for_code(code) {
                Some(label) => lg_first_leaf(signal, label, stale),
                // The daemon did not attempt the LiveGraph (no populated LG / not in scope) -> the proven
                // SQLite primary. Honest: source = {sqlite}, no fallback reason (no LG was tried).
                None => CoherenceEnvelope::sqlite_leaf(signal, stale),
            }
        } else {
            fixed_leaf(signal, base_source(code), stale)
        };
        leaves.push(leaf);
    }

    // ── Fold the root from the leaves. ───────────────────────────
    let (mut root_provenance, root_trust, root_freshness) = fold_parts(&leaves);

    // The documentation section is a filesystem live-scan (current-state). It is a COPIED field, not a
    // signal leaf, so it does not ride in `leaves`; but it IS a contributing source. Add `filesystem` to
    // the root provenance set. Its posture is Fresh/Complete/Exact, so it never lowers the MEET — hence it
    // does not need to participate in the trust/freshness fold (§3b).
    if documentation.is_some() {
        root_provenance.source.insert(Source::Filesystem);
    }

    // ── Confidence = MEET-derived, capped ≤ the legacy value (D-ORIENT-4 / E1). ──
    let coherent_confidence = min_confidence(confidence, confidence_from_posture(&root_trust));

    // ── ENVELOPE provenance-derived limits (machine-discoverable degradation; E5). ──
    // Computed from the folded leaves BEFORE `leaves` is moved into the container's `signals` slot.
    let mut limits = limits;
    append_provenance_limits(&mut limits, &leaves, stale);

    let value = CoherentOrientResult {
        schema,
        command,
        repo,
        display_name,
        snapshot,
        focus,
        confidence: coherent_confidence,
        documentation,
        signals: leaves,
        signals_truncated,
        signals_omitted_count,
        limits,
        limits_truncated,
        limits_omitted_count,
        next,
        next_truncated,
        next_omitted_count,
        truncated,
        trust_briefing,
        // D5 (IMPL-2) next-action is populated post-fold by the daemon adapter (build_orient_envelope).
        relationship_next_action: None,
    };

    CoherenceEnvelope::new(value, root_provenance, root_trust, root_freshness)
}

/// Stable schema id re-export so daemon/CLI callers can pin the coherent orient shape without reaching
/// into [`crate::dto::envelope`].
pub const COHERENT_ORIENT_SCHEMA: &str = ORIENT_SCHEMA;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::envelope::{Confidence, Focus, OrientResult, ORIENT_COMMAND, ORIENT_SCHEMA};
    use crate::dto::signal::Signal;

    // Build a minimal repo-focus OrientResult carrying the given signals.
    fn result_with(signals: Vec<Signal>, confidence: Confidence) -> OrientResult {
        OrientResult {
            schema: ORIENT_SCHEMA,
            command: ORIENT_COMMAND,
            repo: "demo".to_string(),
            display_name: Some("demo".to_string()),
            snapshot: "snap-1".to_string(),
            focus: Focus::repo(),
            confidence,
            documentation: None,
            signals,
            signals_truncated: None,
            signals_omitted_count: None,
            limits: Vec::new(),
            limits_truncated: None,
            limits_omitted_count: None,
            next: Vec::new(),
            next_truncated: None,
            next_omitted_count: None,
            truncated: false,
        }
    }

    fn snapshot_info() -> Signal {
        Signal::snapshot_info(crate::dto::signal::SnapshotInfoEvidence {
            snapshot_uid: "snap-1".to_string(),
            scope: "repo".to_string(),
            basis_commit: None,
            created_at: "2026-06-09T00:00:00Z".to_string(),
        })
    }

    fn import_cycles_signal() -> Signal {
        Signal::import_cycles(crate::dto::signal::ImportCyclesEvidence {
            cycle_count: 1,
            cycles: vec![crate::dto::signal::CycleEvidence {
                length: 2,
                modules: vec!["a".to_string(), "b".to_string()],
            }],
        })
    }

    fn high_complexity_signal() -> Signal {
        Signal::high_complexity(crate::dto::signal::HighComplexityEvidence {
            high_complexity_count: 2,
            threshold: 20,
            top_complex: vec![crate::dto::signal::ComplexSymbolEvidence {
                symbol: "complex_fn".to_string(),
                file: Some("src/a.ts".to_string()),
                complexity: 40,
            }],
        })
    }

    fn ts_langs() -> BTreeSet<LanguageSupport> {
        BTreeSet::from([LanguageSupport::TypeScriptPrimary])
    }

    fn lg_served() -> OrientLeafLabel {
        OrientLeafLabel::Livegraph {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            freshness: FreshnessState::Fresh,
            degradation_reasons: Vec::new(),
            contributing_languages: ts_langs(),
        }
    }

    #[test]
    fn snapshot_leaf_is_sqlite_fresh() {
        let result = result_with(vec![snapshot_info()], Confidence::High);
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        assert_eq!(env.value.signals.len(), 1);
        let leaf = &env.value.signals[0];
        assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert_eq!(leaf.freshness, FreshnessState::Fresh);
        assert!(leaf.provenance.fallback_reason.is_none());
    }

    #[test]
    fn import_cycles_served_from_livegraph_is_labelled_livegraph() {
        let result = result_with(
            vec![snapshot_info(), import_cycles_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            import_cycles: Some(lg_served()),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let cycles = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::ImportCycles)
            .expect("cycles leaf present");
        assert_eq!(
            cycles.provenance.source,
            BTreeSet::from([Source::Livegraph])
        );
        assert_eq!(cycles.trust.class, AnswerClass::Exact);
        // Root provenance is the UNION: both sqlite (snapshot) and livegraph (cycles).
        assert!(env.provenance.source.contains(&Source::Livegraph));
        assert!(env.provenance.source.contains(&Source::Sqlite));
    }

    #[test]
    fn import_cycles_fallback_is_labelled_sqlite_with_reason() {
        let result = result_with(
            vec![snapshot_info(), import_cycles_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            import_cycles: Some(OrientLeafLabel::SqliteFallback {
                reason: CoherenceFallbackReason::LiveGraphCycleDivergence,
            }),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let cycles = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::ImportCycles)
            .expect("cycles leaf present");
        assert_eq!(cycles.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert_eq!(
            cycles.provenance.fallback_reason,
            Some(CoherenceFallbackReason::LiveGraphCycleDivergence)
        );
        // Never livegraph-labelled on fallback.
        assert!(!cycles.provenance.source.contains(&Source::Livegraph));
    }

    #[test]
    fn high_complexity_served_from_livegraph_is_multi_source_livegraph_and_sqlite() {
        // review-9 gap 2: HIGH_COMPLEXITY is a FULL LG-first leaf, but its no-loss cert proves only the
        // `(symbol_key, complexity)` SET == SQLite — the rendered evidence (top-N display names + file paths
        // + ordering) is SQLite-built and not cert-covered (and `LiveGraph::high_complexity` carries no
        // display name), so the honest source set is MULTI-source {livegraph, sqlite}, never single
        // `livegraph`. The `livegraph` member records the current-state SET corroboration.
        let result = result_with(
            vec![snapshot_info(), high_complexity_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            high_complexity: Some(lg_served()),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::HighComplexity)
            .expect("high-complexity leaf present");
        assert_eq!(
            leaf.provenance.source,
            BTreeSet::from([Source::Livegraph, Source::Sqlite]),
            "high-complexity served from LiveGraph is jointly sourced (set corroborated by LiveGraph; \
             top-N sample with display names + files rendered from SQLite)"
        );
        assert_eq!(leaf.trust.class, AnswerClass::Exact);
        assert!(leaf.provenance.fallback_reason.is_none());
        // The root provenance UNION carries livegraph (complexity) + sqlite (snapshot).
        assert!(env.provenance.source.contains(&Source::Livegraph));
        assert!(env.provenance.source.contains(&Source::Sqlite));
    }

    #[test]
    fn high_complexity_fallback_is_labelled_sqlite_with_reason() {
        // Complexity cert NOT GREEN (LG set diverges from SQLite measurements) -> labelled SQLite fallback,
        // never a `livegraph` claim.
        let result = result_with(
            vec![snapshot_info(), high_complexity_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            high_complexity: Some(OrientLeafLabel::SqliteFallback {
                reason: CoherenceFallbackReason::LiveGraphComplexityDivergence,
            }),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::HighComplexity)
            .expect("high-complexity leaf present");
        assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert_eq!(
            leaf.provenance.fallback_reason,
            Some(CoherenceFallbackReason::LiveGraphComplexityDivergence)
        );
        assert!(!leaf.provenance.source.contains(&Source::Livegraph));
    }

    #[test]
    fn lg_first_without_decision_defaults_to_sqlite_no_fallback() {
        // No daemon decision supplied -> the proven SQLite primary, no fallback reason (no LG tried).
        let result = result_with(vec![import_cycles_signal()], Confidence::High);
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        let cycles = &env.value.signals[0];
        assert_eq!(cycles.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert!(cycles.provenance.fallback_reason.is_none());
    }

    #[test]
    fn stale_snapshot_makes_sqlite_leaves_stale_and_caps_root() {
        let result = result_with(vec![snapshot_info()], Confidence::High);
        let env = to_coherent(result, &OrientLgDecisions::default(), None, true);
        assert_eq!(env.value.signals[0].freshness, FreshnessState::Stale);
        // The root MEET cannot be Fresh when a leaf is Stale.
        assert_eq!(env.freshness, FreshnessState::Stale);
        assert_ne!(env.trust.class, AnswerClass::Exact);
        // Confidence is capped below High by the MEET.
        assert_ne!(env.value.confidence, Confidence::High);
    }

    #[test]
    fn confidence_never_exceeds_legacy() {
        // Legacy Medium + a Fresh/Exact MEET -> coherent stays Medium (capped at legacy).
        let result = result_with(vec![snapshot_info()], Confidence::Medium);
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        assert_eq!(env.value.confidence, Confidence::Medium);
    }

    #[test]
    fn zero_signal_is_resolution_only_not_structural_exact() {
        // Ambiguous / no-match: zero signals. Resolution-only root, confidence preserved verbatim.
        let mut result = result_with(Vec::new(), Confidence::High);
        result.focus = Focus::no_match("does-not-exist");
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        assert!(env.value.signals.is_empty());
        // Guard 1: never a structural Exact.
        assert_ne!(env.trust.class, AnswerClass::Exact);
        // Guard 2: operational-identity-only provenance.
        assert_eq!(env.provenance.source, BTreeSet::from([Source::Sqlite]));
        // Guard 3: no structural language partition contributed.
        assert!(env.trust.contributing_languages.is_empty());
        // Confidence preserved verbatim (the resolution outcome is certain).
        assert_eq!(env.value.confidence, Confidence::High);
    }

    #[test]
    fn boundary_violations_leaf_is_multi_source() {
        let result = result_with(
            vec![Signal::boundary_violations(
                crate::dto::signal::BoundaryViolationsEvidence {
                    violation_count: 1,
                    top_violations: vec![],
                },
            )],
            Confidence::High,
        );
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        let leaf = &env.value.signals[0];
        assert!(leaf.provenance.source.contains(&Source::Sqlite));
        assert!(leaf.provenance.source.contains(&Source::Declaration));
    }

    #[test]
    fn trust_briefing_rides_in_value() {
        let briefing = serde_json::json!({ "caveats": ["x"] });
        let result = result_with(vec![snapshot_info()], Confidence::High);
        let env = to_coherent(
            result,
            &OrientLgDecisions::default(),
            Some(briefing.clone()),
            false,
        );
        assert_eq!(env.value.trust_briefing, Some(briefing));
    }

    #[test]
    fn documentation_contributes_filesystem_source() {
        let mut result = result_with(vec![snapshot_info()], Confidence::High);
        result.documentation = Some(DocumentationSection {
            relevant_files: Vec::new(),
            count: 0,
        });
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        assert!(env.provenance.source.contains(&Source::Filesystem));
    }

    // ── review-7 pt1: BOUNDARY_LINKS_SUMMARY L2 FreshnessInfo -> FreshnessState reconciliation ──
    // (D-ORIENT-7 / contract RISK-G / spec §3c R1). The inner Signal.freshness (Current/Impacted/Unknown)
    // is reconciled to the AUTHORITATIVE outer leaf freshness; the inner FreshnessInfo stays render-only.

    fn boundary_links_signal(freshness: crate::dto::signal::FreshnessInfo) -> Signal {
        Signal::boundary_links_summary(crate::dto::signal::BoundaryLinksSummaryEvidence {
            link_count: 3,
        })
        .with_freshness(freshness)
    }

    fn boundary_links_leaf(
        env: &CoherenceEnvelope<CoherentOrientResult>,
    ) -> &CoherenceEnvelope<Signal> {
        env.value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::BoundaryLinksSummary)
            .expect("boundary-links leaf present")
    }

    #[test]
    fn boundary_links_current_is_fresh_exact() {
        // Current backing artifacts -> the leaf is Fresh/Exact (same as any healthy SQLite leaf).
        let result = result_with(
            vec![boundary_links_signal(
                crate::dto::signal::FreshnessInfo::current(),
            )],
            Confidence::High,
        );
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        let leaf = boundary_links_leaf(&env);
        assert_eq!(leaf.freshness, FreshnessState::Fresh);
        assert_eq!(leaf.trust.class, AnswerClass::Exact);
        assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
    }

    #[test]
    fn boundary_links_impacted_is_stale_even_when_snapshot_fresh_and_lowers_root() {
        // THE review-7 bug: an Impacted link must make the OUTER leaf freshness Stale even when the SQLite
        // snapshot is NOT stale — the inner FreshnessInfo is reconciled, never ignored. The root MEET (and
        // confidence) are lowered by that one Impacted leaf.
        let result = result_with(
            vec![
                snapshot_info(),
                boundary_links_signal(crate::dto::signal::FreshnessInfo::impacted(
                    "2026-05-01T00:00:00Z",
                )),
            ],
            Confidence::High,
        );
        // stale = false: the snapshot is fresh; ONLY the boundary link is impacted.
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        let leaf = boundary_links_leaf(&env);
        assert_eq!(leaf.freshness, FreshnessState::Stale);
        assert_eq!(leaf.trust.class, AnswerClass::Stale);
        // Root MEET is lowered by the Impacted leaf despite the fresh snapshot.
        assert_eq!(env.freshness, FreshnessState::Stale);
        assert_ne!(env.trust.class, AnswerClass::Exact);
        assert_ne!(env.value.confidence, Confidence::High);
        // The SQLite snapshot itself is fresh, so the snapshot-stale limit must NOT fire — the staleness is
        // link-level (carried by the leaf freshness), not a stale index.
        assert!(!has_limit(&env, LimitCode::SqliteSnapshotStale));
    }

    #[test]
    fn boundary_links_unknown_is_stale() {
        // Unknown provenance -> conservative Stale (never a false Fresh), trust paired to Stale.
        let result = result_with(
            vec![boundary_links_signal(
                crate::dto::signal::FreshnessInfo::unknown(),
            )],
            Confidence::High,
        );
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        let leaf = boundary_links_leaf(&env);
        assert_eq!(leaf.freshness, FreshnessState::Stale);
        assert_eq!(leaf.trust.class, AnswerClass::Stale);
    }

    #[test]
    fn boundary_links_inner_freshness_info_kept_render_only() {
        // The inner Signal stays PRISTINE: the FreshnessInfo (incl. Impacted vs Unknown + impacted_since) is
        // retained for rendering even though the AUTHORITATIVE freshness rides on the OUTER envelope (R1).
        let result = result_with(
            vec![boundary_links_signal(
                crate::dto::signal::FreshnessInfo::impacted("2026-05-01T00:00:00Z"),
            )],
            Confidence::High,
        );
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        let leaf = boundary_links_leaf(&env);
        assert_eq!(
            leaf.value.freshness().map(|f| f.state),
            Some(FreshnessStateDto::Impacted)
        );
        // The render-only detail is preserved; the outer axis is the authoritative Stale.
        assert_eq!(leaf.freshness, FreshnessState::Stale);
    }

    #[test]
    fn boundary_links_impacted_with_stale_snapshot_stays_stale() {
        // Both the snapshot AND the link are degraded -> the MEET is Stale (idempotent; never raised).
        let result = result_with(
            vec![boundary_links_signal(
                crate::dto::signal::FreshnessInfo::impacted("2026-05-01T00:00:00Z"),
            )],
            Confidence::High,
        );
        let env = to_coherent(result, &OrientLgDecisions::default(), None, true);
        let leaf = boundary_links_leaf(&env);
        assert_eq!(leaf.freshness, FreshnessState::Stale);
        assert_eq!(leaf.trust.class, AnswerClass::Stale);
    }

    // ── review-6 pt3: CALLERS_SUMMARY / CALLEES_SUMMARY livegraph-served leaves are MULTI-SOURCE ──

    fn callers_summary_signal() -> Signal {
        Signal::callers_summary(crate::dto::signal::CallersSummaryEvidence {
            count: 2,
            top_modules: vec![crate::dto::signal::ModuleCountEvidence {
                module: "mod_a".to_string(),
                count: 2,
            }],
        })
    }

    fn callees_summary_signal() -> Signal {
        Signal::callees_summary(crate::dto::signal::CalleesSummaryEvidence {
            count: 1,
            top_modules: vec![crate::dto::signal::ModuleCountEvidence {
                module: "mod_b".to_string(),
                count: 1,
            }],
        })
    }

    #[test]
    fn callers_summary_served_from_livegraph_is_multi_source_livegraph_and_sqlite() {
        // The daemon proved the caller KEY SET == LiveGraph (Exact), but the module-grouped summary
        // EVIDENCE is SQLite-computed -> the honest source set is {livegraph, sqlite}, never single
        // `livegraph` (review-6 pt3). The `livegraph` member records the current-state corroboration.
        let result = result_with(
            vec![snapshot_info(), callers_summary_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            callers_summary: Some(lg_served()),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::CallersSummary)
            .expect("callers leaf present");
        assert_eq!(
            leaf.provenance.source,
            BTreeSet::from([Source::Livegraph, Source::Sqlite]),
            "callers summary served from LiveGraph is jointly sourced (key set corroborated by \
             LiveGraph; module grouping computed from SQLite)"
        );
        assert!(leaf.provenance.fallback_reason.is_none());
        assert_eq!(leaf.trust.class, AnswerClass::Exact);
        // Root union carries both.
        assert!(env.provenance.source.contains(&Source::Livegraph));
        assert!(env.provenance.source.contains(&Source::Sqlite));
    }

    #[test]
    fn callees_summary_served_from_livegraph_is_multi_source_livegraph_and_sqlite() {
        let result = result_with(
            vec![snapshot_info(), callees_summary_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            callees_summary: Some(lg_served()),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::CalleesSummary)
            .expect("callees leaf present");
        assert_eq!(
            leaf.provenance.source,
            BTreeSet::from([Source::Livegraph, Source::Sqlite])
        );
        assert!(leaf.provenance.fallback_reason.is_none());
    }

    #[test]
    fn callers_summary_fallback_is_sqlite_only_with_reason() {
        // When the LiveGraph cannot corroborate the key set, the leaf is the proven SQLite primary —
        // single-source {sqlite} + the reason. NEVER a `livegraph` claim.
        let result = result_with(vec![callers_summary_signal()], Confidence::High);
        let lg = OrientLgDecisions {
            callers_summary: Some(OrientLeafLabel::SqliteFallback {
                reason: CoherenceFallbackReason::LiveGraphCallgraphDivergence,
            }),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let leaf = &env.value.signals[0];
        assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert!(!leaf.provenance.source.contains(&Source::Livegraph));
        assert_eq!(
            leaf.provenance.fallback_reason,
            Some(CoherenceFallbackReason::LiveGraphCallgraphDivergence)
        );
    }

    #[test]
    fn cycles_livegraph_leaf_stays_single_source_complexity_is_multi_source() {
        // IMPORT_CYCLES is the ONLY single-source {livegraph} LG-first leaf: its cert is a field-exact
        // WHOLE-value compare (the proven cycle set IS the rendered evidence). HIGH_COMPLEXITY (alongside
        // callers/callees) is multi-source {livegraph, sqlite} — the cert corroborates the SET but the
        // rendered top-N evidence is SQLite-built (review-9 gap 2). This pins the distinction in
        // `livegraph_served_is_multi_source`.
        let result = result_with(
            vec![import_cycles_signal(), high_complexity_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            import_cycles: Some(lg_served()),
            high_complexity: Some(lg_served()),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        let cycles = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::ImportCycles)
            .expect("cycles leaf present");
        assert_eq!(
            cycles.provenance.source,
            BTreeSet::from([Source::Livegraph]),
            "IMPORT_CYCLES is single-source livegraph (field-exact whole-value cert)"
        );
        let complexity = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::HighComplexity)
            .expect("complexity leaf present");
        assert_eq!(
            complexity.provenance.source,
            BTreeSet::from([Source::Livegraph, Source::Sqlite]),
            "HIGH_COMPLEXITY is multi-source (set corroborated by LiveGraph; sample rendered from SQLite)"
        );
    }

    // ── review-6 pt1: ENVELOPE provenance-derived limit codes (E5) ──

    fn has_limit(env: &CoherenceEnvelope<CoherentOrientResult>, code: LimitCode) -> bool {
        env.value.limits.iter().any(|l| l.code == code)
    }

    #[test]
    fn producer_unavailable_limit_when_lg_first_leaf_has_no_livegraph() {
        // An LG-first leaf with a LiveGraphUnavailable fallback -> the envelope gains PRODUCER_UNAVAILABLE.
        let result = result_with(
            vec![snapshot_info(), import_cycles_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            import_cycles: Some(OrientLeafLabel::SqliteFallback {
                reason: CoherenceFallbackReason::LiveGraphUnavailable,
            }),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        assert!(has_limit(&env, LimitCode::ProducerUnavailable));
        // No spurious codes when not stale / no partial / no authority / no precision-pending.
        assert!(!has_limit(&env, LimitCode::SqliteSnapshotStale));
        assert!(!has_limit(&env, LimitCode::LivegraphPartial));
    }

    #[test]
    fn livegraph_partial_limit_when_lg_first_leaf_is_partial() {
        let result = result_with(
            vec![snapshot_info(), import_cycles_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            import_cycles: Some(OrientLeafLabel::SqliteFallback {
                reason: CoherenceFallbackReason::LiveGraphPartial,
            }),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        assert!(has_limit(&env, LimitCode::LivegraphPartial));
    }

    #[test]
    fn sqlite_snapshot_stale_limit_when_stale() {
        let result = result_with(vec![snapshot_info()], Confidence::High);
        let env = to_coherent(result, &OrientLgDecisions::default(), None, true);
        assert!(has_limit(&env, LimitCode::SqliteSnapshotStale));
    }

    #[test]
    fn authority_overlay_applied_limit_when_declaration_source_present() {
        // BOUNDARY_VIOLATIONS is multi-source {sqlite, declaration} -> the Declaration source triggers
        // AUTHORITY_OVERLAY_APPLIED.
        let result = result_with(
            vec![Signal::boundary_violations(
                crate::dto::signal::BoundaryViolationsEvidence {
                    violation_count: 1,
                    top_violations: vec![],
                },
            )],
            Confidence::High,
        );
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        assert!(has_limit(&env, LimitCode::AuthorityOverlayApplied));
    }

    #[test]
    fn precision_pending_limit_when_leaf_is_precision_pending() {
        // Pure derivation test: GIVEN a PrecisionPending livegraph leaf decision, the envelope gains
        // PRECISION_PENDING. (orient's ladder currently falls back on non-Fresh, so the daemon does not
        // produce this input today — this guards the derivation for any future ladder that serves it.)
        let result = result_with(
            vec![snapshot_info(), import_cycles_signal()],
            Confidence::High,
        );
        let lg = OrientLgDecisions {
            import_cycles: Some(OrientLeafLabel::Livegraph {
                class: AnswerClass::Partial,
                completeness: QueryCompleteness::Degraded,
                freshness: FreshnessState::PrecisionPending,
                degradation_reasons: Vec::new(),
                contributing_languages: ts_langs(),
            }),
            ..Default::default()
        };
        let env = to_coherent(result, &lg, None, false);
        assert!(has_limit(&env, LimitCode::PrecisionPending));
    }

    #[test]
    fn no_provenance_limits_when_healthy() {
        // All-SQLite Fresh repo focus, no LG fallback -> none of the provenance codes fire.
        let result = result_with(vec![snapshot_info()], Confidence::High);
        let env = to_coherent(result, &OrientLgDecisions::default(), None, false);
        for code in [
            LimitCode::ProducerUnavailable,
            LimitCode::LivegraphPartial,
            LimitCode::SqliteSnapshotStale,
            LimitCode::AuthorityOverlayApplied,
            LimitCode::PrecisionPending,
        ] {
            assert!(
                !has_limit(&env, code),
                "{code:?} must not fire when healthy"
            );
        }
    }
}
