//! EXPLAIN-LIVEGRAPH-IMPL: assemble explain's `CoherenceEnvelope<CoherentOrientResult>` response.
//!
//! PURE policy (Clean Architecture: high-level policy, no I/O). Mirrors orient's
//! [`crate::dto::coherent::to_coherent`] for explain's HEAVIER but DIRECTLY-REUSED shape, and reuses the
//! SHARED `repo-graph-coherence` wrapper + the SHARED container [`CoherentOrientResult`] (contract D7) +
//! the SHARED confidence/limit derivations (`confidence_from_posture` / `min_confidence` /
//! `append_provenance_limits`) — nothing is forked. The daemon (`explain_coherence.rs`) supplies the
//! per-leaf LiveGraph DECISIONS via [`ExplainLgDecisions`] and the degraded-state `trust_briefing`; the
//! agent never reads the LiveGraph, SQLite, or any cert.
//!
//! ## explain's leaf source map (EXPLAIN-LIVEGRAPH-1 §2 / §3a; operator decision 2026-06-12)
//!
//! The daemon (`explain_coherence.rs`) genuinely SERVES each green LG-first leaf's VALUE from the LiveGraph
//! and swaps it into the bare result BEFORE this conversion; this pure conversion only LABELS each leaf by
//! its true construction (the honesty law: a leaf's `source` set matches what built its rendered evidence).
//! Single- vs multi-source is decided per code by [`explain_livegraph_served_is_multi_source`]:
//!
//! - **EXPLAIN_IMPORTS** (file focus), **EXPLAIN_CYCLES** (symbol module-context / path focus) — when served,
//!   the daemon BUILDS the value from the migrated LiveGraph surface (`live_import_view` /
//!   `module_import_cycles`), gated by the repo-wide import / module-cycle no-loss CERT (a field-exact
//!   whole-value proof). The rendered value IS the LiveGraph value → honestly **single-source `{livegraph}`**
//!   (the sibling of orient's IMPORT_CYCLES). On fallback: the proven SQLite primary, labelled with the
//!   cert-ladder reason.
//! - **EXPLAIN_IDENTITY** (D-EXPLAIN-IDENTITY; operator: RESTORE multi-source) — at SYMBOL focus, when the
//!   symbol's partition is resident + Fresh + TS, the daemon serves the identity ANCHOR (`name` / `subtype`)
//!   from current-state LiveGraph IR (`LiveGraph::node_display`, the SAME IR symbol-attributes substrate
//!   `stats` reads) while the snapshot-scoped COORDINATE fields (`line_start` / `file_path` / `language` /
//!   `module_path`) stay SQLite → the D8 **multi-source `{livegraph, sqlite}`** leaf. When the anchor is not
//!   live-servable (non-resident / stale / non-TS / no live node), that is a FAILED LG-first attempt → the
//!   daemon supplies a `SqliteFallback { reason }` decision and the whole leaf is a **LABELLED `{sqlite}`**
//!   fallback (the cert-ladder reason records WHY — review-7: a failed attempt is never silently unlabelled,
//!   which would mint false provenance). The UNLABELLED `{sqlite}` identity is reserved for the FILE/PATH-focus
//!   listings case (D-EXPLAIN-LISTINGS), where there is no symbol anchor: the daemon makes NO LiveGraph attempt
//!   and supplies NO decision (`None`), so a snapshot-only identity is the expected, non-degraded path there.
//! - **EXPLAIN_CALLERS / EXPLAIN_CALLEES** (symbol focus) — when served, the LiveGraph genuinely supplies the
//!   caller/callee KEY SET (the no-loss per-symbol key compare vs SQLite), but the rendered per-item
//!   `name` + owning `module` + the top-3 module grouping have NO LiveGraph/IR home (module discovery is a
//!   Layer-1/2 SQLite construct — MODULE→FILE `OWNS` edges; the IR substrate carries no module ownership),
//!   so they are SQLite-rendered. The honesty law REQUIRES **multi-source `{livegraph, sqlite}`** — exactly
//!   orient's ratified CALLERS_SUMMARY/CALLEES_SUMMARY treatment. Serving these single-source `{livegraph}`
//!   would mint false provenance over the SQLite-rendered module grouping. On fallback: SQLite primary +
//!   reason.
//! - **EXPLAIN_BOUNDARY — multi-source `{declaration, sqlite}`** (forbidden-import declaration + SQLite
//!   import edges); **EXPLAIN_GATE — `{declaration}`** (requirement/obligation/waiver). Tier-A1 Authority,
//!   overlay-preserves-computed (D-EXPLAIN-AUTH / contract D5).
//! - **EXPLAIN_SYMBOLS / EXPLAIN_FILES / EXPLAIN_TRUST — `{sqlite}`** (listing-coherence / trust-core).
//! - **EXPLAIN_MEASUREMENTS — dormant** (never emitted today; defensive `{sqlite}`).
//!
//! The ZERO-SIGNAL ambiguous / no-match terminals take the resolution-only root (D-EXPLAIN-ZEROSIGNAL =
//! orient D-ORIENT-4): operational-identity-only provenance, the static `High` confidence preserved, NEVER
//! a structural Exact. explain POPULATES the shared `trust_briefing` field `Some(..)` when degraded — it
//! is the SECOND populator after orient (unlike check, which always leaves it `None`).

use repo_graph_coherence::{
    fold_parts, meet_freshness, meet_trust, CoherenceEnvelope, FreshnessState, Provenance, Source,
    TrustPosture,
};

use crate::dto::coherent::{
    append_provenance_limits, confidence_from_posture, min_confidence, CoherentOrientResult,
    OrientLeafLabel,
};
use crate::dto::envelope::OrientResult;
use crate::dto::signal::{Signal, SignalCode};

// ── Per-leaf LiveGraph decisions (daemon → agent) ─────────────────

/// The daemon-supplied LiveGraph decisions for explain's FIVE LG-first signals (D-EXPLAIN-1 + identity).
///
/// Each reuses an EXISTING daemon no-loss proof (callers/callees per-symbol key compare; the repo-wide
/// module-cycle cert; the repo-wide import cert; the identity residency + `node_display` anchor) — no new
/// producer. A `None` field means the daemon made NO LiveGraph ATTEMPT for that signal (the focus does not
/// carry it as an LG-first leaf — e.g. the FILE/PATH-focus listings identity, D-EXPLAIN-LISTINGS), so the
/// leaf is the proven SQLite primary with `source = {sqlite}` and NO fallback reason. A committed attempt
/// that COULD NOT serve is `Some(SqliteFallback { reason })` — a LABELLED `{sqlite}` fallback, never `None`.
///
/// The leaf label type is the SHARED [`OrientLeafLabel`] (reused, not forked). For the leaves the daemon
/// genuinely VALUE-serves (imports / cycles / identity) the daemon also swaps the live-built `Signal` into
/// the bare result before this conversion runs; this struct then only carries the per-leaf POSTURE so the
/// pure conversion can label provenance/trust/freshness.
#[derive(Debug, Clone, Default)]
pub struct ExplainLgDecisions {
    /// EXPLAIN_IDENTITY (symbol focus) — residency + `node_display` anchor; served value = SQLite identity
    /// with the `name`/`subtype` anchor overridden from current-state LiveGraph IR → `{livegraph, sqlite}`.
    pub identity: Option<OrientLeafLabel>,
    /// EXPLAIN_CALLERS (symbol focus) — migrated `callers` `Auto` ladder + per-symbol no-loss key compare.
    pub callers: Option<OrientLeafLabel>,
    /// EXPLAIN_CALLEES (symbol focus) — migrated `callees` `Auto` ladder + per-symbol no-loss key compare.
    pub callees: Option<OrientLeafLabel>,
    /// EXPLAIN_IMPORTS (file focus) — the repo-wide import no-loss cert + the per-file residency precondition.
    pub imports: Option<OrientLeafLabel>,
    /// EXPLAIN_CYCLES (symbol module-context / path focus) — the repo-wide module-cycle no-loss cert.
    pub cycles: Option<OrientLeafLabel>,
}

impl ExplainLgDecisions {
    /// Look up the decision for a signal code (only the five LG-first codes have one).
    fn for_code(&self, code: SignalCode) -> Option<&OrientLeafLabel> {
        match code {
            SignalCode::ExplainIdentity => self.identity.as_ref(),
            SignalCode::ExplainCallers => self.callers.as_ref(),
            SignalCode::ExplainCallees => self.callees.as_ref(),
            SignalCode::ExplainImports => self.imports.as_ref(),
            SignalCode::ExplainCycles => self.cycles.as_ref(),
            _ => None,
        }
    }
}

// ── Source classification (pure, by signal code) ──────────────────

/// The fixed (non-LG-first) source of an explain signal, derived from its code.
enum BaseSource {
    /// SQLite snapshot-scoped cache (identity / symbols / files / trust — the proven primary).
    Sqlite,
    /// Pure Tier-A1 `declarations` Authority (EXPLAIN_GATE).
    Declaration,
    /// A leaf DERIVED from BOTH the structural import edges (sqlite) AND a forbidden-import declaration
    /// (Authority) — the D8 multi-source case (EXPLAIN_BOUNDARY).
    SqliteAndDeclaration,
}

/// Is this code one of explain's FIVE LG-first signals? Each is daemon-gated by an EXISTING no-loss proof /
/// residency check. EXPLAIN_IDENTITY is now LG-first (the daemon serves its anchor; operator 2026-06-12).
fn is_lg_first(code: SignalCode) -> bool {
    matches!(
        code,
        SignalCode::ExplainIdentity
            | SignalCode::ExplainCallers
            | SignalCode::ExplainCallees
            | SignalCode::ExplainImports
            | SignalCode::ExplainCycles
    )
}

/// Does a LiveGraph-SERVED leaf for this code carry the SQLite snapshot as a CO-source (D8 multi-source),
/// rather than single-source `{livegraph}`? Mirrors orient's `livegraph_served_is_multi_source` rule.
///
/// `true` (multi-source `{livegraph, sqlite}`):
///   - EXPLAIN_CALLERS / EXPLAIN_CALLEES — the no-loss gate proves the caller/callee KEY SET matches SQLite,
///     but the rendered per-item `name` + owning `module` + top-3 module grouping are SQLite-built (module
///     discovery has no LiveGraph/IR home).
///   - EXPLAIN_IDENTITY — the ANCHOR (`name`/`subtype`) is LiveGraph-served, but the coordinate fields
///     (`line_start`/`file_path`/`language`/`module_path`) are SQLite.
///
/// `false` (single-source `{livegraph}`):
///   - EXPLAIN_IMPORTS / EXPLAIN_CYCLES — served from the migrated `live_import_view` / `module_import_cycles`
///     surfaces under a FIELD-EXACT whole-value cert, so the rendered value IS the LiveGraph value.
fn explain_livegraph_served_is_multi_source(code: SignalCode) -> bool {
    matches!(
        code,
        SignalCode::ExplainCallers | SignalCode::ExplainCallees | SignalCode::ExplainIdentity
    )
}

/// The fixed source class for a non-LG-first explain signal (the explain source map, §2 / §3a).
fn base_source(code: SignalCode) -> BaseSource {
    use SignalCode::*;
    match code {
        // Authority + structural import-edge half (D8 multi-source) — D-EXPLAIN-AUTH.
        ExplainBoundary => BaseSource::SqliteAndDeclaration,
        // Pure Tier-A1 Authority (requirement / obligation / waiver evaluation) — D-EXPLAIN-AUTH.
        ExplainGate => BaseSource::Declaration,
        // EXPLAIN_SYMBOLS, EXPLAIN_FILES, EXPLAIN_TRUST, the dormant EXPLAIN_MEASUREMENTS, and any defensive
        // default are SQLite-first. (The five LG-first codes — identity/callers/callees/imports/cycles —
        // never reach `base_source`: `is_lg_first` routes them through the daemon decision first.)
        _ => BaseSource::Sqlite,
    }
}

// ── Leaf construction ─────────────────────────────────────────────

/// Wrap an explain signal whose source is fixed (non-LG-first): SQLite / Authority / multi-source.
///
/// explain's signal constructors set NO inner `Signal.freshness` (EXPLAIN-LIVEGRAPH-1 §3c R1 / RISK-E-H —
/// the reconciliation is VACUOUS for explain), so the leaf freshness is the snapshot freshness directly: a
/// fresh index ⇒ `Fresh`/`Exact`, a stale index ⇒ `Stale`. The inner `Signal` stays PRISTINE.
fn fixed_leaf(signal: Signal, base: BaseSource, stale: bool) -> CoherenceEnvelope<Signal> {
    // Snapshot posture: Fresh/Exact unless the backing index is stale.
    match base {
        BaseSource::Sqlite => CoherenceEnvelope::sqlite_leaf(signal, stale),
        BaseSource::Declaration => CoherenceEnvelope::declaration_leaf(signal, stale),
        // EXPLAIN_BOUNDARY: the D8 multi-source {declaration, sqlite} leaf (the forbidden-import rule +
        // the SQLite import edges). The shared crate has no dedicated constructor for this pairing, so it
        // is assembled here from the snapshot posture (mirrors orient's BOUNDARY_VIOLATIONS leaf).
        BaseSource::SqliteAndDeclaration => {
            let (trust, freshness) = if stale {
                (TrustPosture::snapshot_stale(), FreshnessState::Stale)
            } else {
                (TrustPosture::snapshot_exact(), FreshnessState::Fresh)
            };
            CoherenceEnvelope::new(
                signal,
                Provenance::multi([Source::Sqlite, Source::Declaration]),
                trust,
                freshness,
            )
        }
    }
}

/// Wrap an LG-first signal from the daemon's decision.
///
/// When the LiveGraph SERVED (`Livegraph` posture), the daemon has already swapped the live-built value into
/// `signal` (for imports/cycles/identity/callers/callees); this labels provenance by the leaf's TRUE
/// construction ([`explain_livegraph_served_is_multi_source`]) and folds the leaf trust + freshness honestly:
///
/// - **single-source `{livegraph}`** (imports/cycles — the field-exact whole-value cert): the rendered value
///   IS the LiveGraph value with NO SQLite contributor, so the leaf adopts the LiveGraph posture VERBATIM.
///   The SQLite-snapshot `stale` flag does NOT apply (a Fresh LiveGraph import set beside a stale SQLite
///   snapshot is honest — the LiveGraph reflects current state; the root MEET still lowers via the other
///   SQLite leaves).
/// - **multi-source `{livegraph, sqlite}`** (identity/callers/callees — the LiveGraph anchor / key set + live
///   IR names, but the SQLite-rendered coordinates / module grouping / SQL order): the leaf value is DERIVED
///   from BOTH sources, so its trust + freshness are the INTERNAL MEET of BOTH contributor postures
///   (D-EXPLAIN-IDENTITY / contract §3a). The SQLite contributor is the snapshot posture — Fresh/Exact unless
///   `stale`. A STALE backing index therefore caps the WHOLE leaf to Stale: it must NEVER stay Fresh+Exact on
///   the LiveGraph half alone (the honesty law — never mint false freshness over a stale SQLite-rendered half;
///   review-5). The MEET REUSES the shared monotone-GLB algebra (`meet_trust`/`meet_freshness`) — the SAME
///   fold the root uses — never a hand-rolled min.
///
/// On FALLBACK the served value is the SQLite proven primary, labelled with the cert-ladder reason.
fn lg_first_leaf(
    signal: Signal,
    label: &OrientLeafLabel,
    stale: bool,
) -> CoherenceEnvelope<Signal> {
    match label {
        OrientLeafLabel::Livegraph {
            class,
            completeness,
            freshness,
            degradation_reasons,
            contributing_languages,
        } => {
            // The LiveGraph contributor posture, projected from the daemon label verbatim.
            let lg_posture = TrustPosture {
                class: *class,
                completeness: *completeness,
                degradation_reasons: degradation_reasons.clone(),
                contributing_languages: contributing_languages.clone(),
            };
            if explain_livegraph_served_is_multi_source(signal.code()) {
                // D8 multi-source {livegraph, sqlite}: MEET the LiveGraph contributor with the SQLite snapshot
                // contributor (Fresh/Exact unless `stale`). The MEET is monotone — it can only LOWER — so a
                // stale SQLite half forces the leaf to Stale (never Fresh+Exact on the LiveGraph half alone).
                let (sqlite_posture, sqlite_freshness) = if stale {
                    (TrustPosture::snapshot_stale(), FreshnessState::Stale)
                } else {
                    (TrustPosture::snapshot_exact(), FreshnessState::Fresh)
                };
                let trust = meet_trust(&[lg_posture, sqlite_posture]);
                let freshness = meet_freshness(&[*freshness, sqlite_freshness]);
                CoherenceEnvelope::new(
                    signal,
                    Provenance::multi([Source::Livegraph, Source::Sqlite]),
                    trust,
                    freshness,
                )
            } else {
                // Single-source {livegraph}: NO SQLite contributor, so `stale` does not apply — adopt the
                // LiveGraph posture verbatim (the sibling of orient's IMPORT_CYCLES leaf).
                CoherenceEnvelope::new(signal, Provenance::livegraph(), lg_posture, *freshness)
            }
        }
        OrientLeafLabel::SqliteFallback { reason } => {
            // The served value is the SQLite proven primary -> the snapshot posture + the ladder reason.
            CoherenceEnvelope::sqlite_fallback_leaf(signal, *reason, stale)
        }
    }
}

// ── The conversion ────────────────────────────────────────────────

/// Convert explain's bare [`OrientResult`] into the coherence wrapper
/// `CoherenceEnvelope<CoherentOrientResult>`.
///
/// - `lg` carries the daemon's per-leaf LiveGraph decisions for the FIVE LG-first reuse signals
///   (identity/callers/callees/imports/cycles).
/// - `trust_briefing` is the daemon's degraded-state overlay (`Some` only when degraded; D-EXPLAIN-TRUST-
///   BRIEFING — explain is the SECOND populator after orient, unlike check).
/// - `stale` is whether the backing index is stale (`get_stale_files` non-empty), supplied by the daemon
///   from an AUTHORITATIVE storage read (NOT a post-budget/truncated signal) so the freshness label is
///   faithful (the honesty requirement: never mint a false `Fresh`).
///
/// ZERO-SIGNAL (ambiguous / no-match): the empty-signal builders emit no leaves, so the root takes the
/// explicit resolution-only posture (D-EXPLAIN-ZEROSIGNAL) — NEVER the empty fold's lattice-TOP — and the
/// confidence is the legacy STATIC `High` preserved verbatim.
pub fn explain_to_coherent(
    result: OrientResult,
    lg: &ExplainLgDecisions,
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
    // empty fold's lattice-TOP. D-EXPLAIN-ZEROSIGNAL (= orient D-ORIENT-4). `trust_briefing` still follows
    // the focus-INDEPENDENT snapshot-degradation gate, so a degraded-snapshot ambiguous/no-match MAY carry
    // it (the daemon decides; here we just thread it through).
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
            // D5 (IMPL-2) next-action is an orient/stats surface; explain never renders it.
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
                // No daemon decision = the daemon made NO LiveGraph ATTEMPT for this leaf at this focus (e.g.
                // the FILE/PATH-focus listings identity, D-EXPLAIN-LISTINGS — there is no symbol anchor). The
                // proven SQLite primary, honestly unlabelled: source = {sqlite}, no fallback reason (no LG was
                // tried — distinct from a FAILED attempt, which arrives as Some(SqliteFallback { reason })).
                None => CoherenceEnvelope::sqlite_leaf(signal, stale),
            }
        } else {
            fixed_leaf(signal, base_source(code), stale)
        };
        leaves.push(leaf);
    }

    // ── Fold the root from the leaves (MEET; monotone — can only LOWER). ──
    let (root_provenance, root_trust, root_freshness) = fold_parts(&leaves);

    // explain has NO filesystem documentation section (always `None`; §1d), so the root provenance never
    // gains `filesystem` — unlike orient.

    // ── Confidence = MEET-derived, capped ≤ the legacy value (D-EXPLAIN-CONF / E1). ──
    let coherent_confidence = min_confidence(confidence, confidence_from_posture(&root_trust));

    // ── ENVELOPE provenance-derived limits (machine-discoverable degradation; §3b / E5). ──
    // explain has NO pre-existing limits, so these codes are NET-NEW — its PRIMARY machine-honesty channel
    // (it has no labelled-limit channel otherwise; RISK-E-C). Reuses the SHARED derivation.
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
        // D5 (IMPL-2) next-action is an orient/stats surface; explain never renders it.
        relationship_next_action: None,
    };

    CoherenceEnvelope::new(value, root_provenance, root_trust, root_freshness)
}

#[cfg(test)]
#[path = "coherent_tests.rs"]
mod tests;
