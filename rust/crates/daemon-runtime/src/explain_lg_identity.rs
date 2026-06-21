//! EXPLAIN-LIVEGRAPH-IMPL: the daemon-side LiveGraph anchor serving for EXPLAIN_IDENTITY (symbol focus).
//!
//! Split out of [`crate::explain_lg_serve`] so its labelled cert-ladder fallbacks keep that module within the
//! 500-line structural guardrail (review-7: refactor before expanding). The IMPURE adapter half for the
//! identity leaf only — the D8 multi-source `{livegraph, sqlite}` anchor case (D-EXPLAIN-IDENTITY).
//!
//! At SYMBOL focus the daemon ATTEMPTS to serve the identity ANCHOR (`name`/`subtype`) from current-state
//! LiveGraph IR (`LiveGraph::node_display`, the SAME IR symbol-attributes substrate `stats` serves
//! byte-preservingly), while the snapshot-scoped COORDINATE fields (`path`/`line_start`/`module_path`/
//! `language`) stay SQLite. When the anchor is live-servable (the symbol's owning partition is resident +
//! Fresh + TS AND `node_display` resolves the key) the rebuilt leaf is the multi-source `{livegraph, sqlite}`
//! identity; the served NAME is the live IR name, provably different from the snapshot name when the symbol
//! drifted.
//!
//! When ANY precondition fails, the attempt is a LABELLED SQLite fallback: the proven SQLite identity primary
//! is kept and the cert-ladder reason records WHY the anchor could not be LG-served — exactly like
//! callers/callees/imports/cycles (operator + review-7: a failed LG-first attempt is NEVER an unlabelled
//! `{sqlite}` leaf, which would mint false provenance — hide a real degradation as the proven primary). The
//! UNLABELLED `{sqlite}` identity belongs ONLY to the FILE/PATH-focus listings case (D-EXPLAIN-LISTINGS),
//! where there is no symbol anchor: `explain_coherence` makes NO LiveGraph attempt and supplies NO decision
//! (`None`), so this function is never called there.

use repo_graph_agent::{ExplainIdentityEvidence, OrientLeafLabel, Signal};
use repo_graph_coherence::CoherenceFallbackReason;

use crate::explain_lg_serve::lg_posture;
use crate::state::RepoState;

/// EXPLAIN_IDENTITY (symbol focus): serve the anchor `name`/`subtype` from current-state LiveGraph IR.
///
/// Returns the rebuilt identity `Signal` (anchor overridden, coordinate fields preserved) + the `Livegraph`
/// posture ONLY when the symbol's partition is resident + Fresh + TS AND `node_display` resolves the live
/// anchor → the D8 `{livegraph, sqlite}` identity (the daemon-side caller labels it via the pure conversion).
///
/// EVERY other path returns `(None, Some(SqliteFallback { reason }))`: the proven SQLite identity primary is
/// kept, LABELLED with the cert-ladder reason recording why the live anchor could not be served (review-7).
/// The reason ladder MIRRORS [`crate::explain_coherence::explain_imports_outcome`]'s residency decomposition
/// (the SAME freshness-before-language ordering): non-resident → `Partial`; stale → `Stale`; non-TS →
/// `UnsupportedLanguage`; a `node_display` miss → `DisplayMetadataUnavailable`; no LiveGraph / unreadable
/// anchor → `Unavailable`.
///
/// This is invoked ONLY at SYMBOL focus (the caller gates on `symbol_target`), so reaching it IS a committed
/// LG-first attempt — hence a fallback here is always labelled. The unlabelled `{sqlite}` identity (no daemon
/// decision) is the FILE/PATH-focus listings case (D-EXPLAIN-LISTINGS), where this function is never called.
pub(crate) fn serve_identity(
    repo_state: &RepoState,
    symbol_key: &str,
    original: &Signal,
) -> (Option<Signal>, Option<OrientLeafLabel>) {
    // Defensive: the caller only passes an EXPLAIN_IDENTITY signal, so a non-identity evidence is an
    // impossible state. Still labelled (a committed LG attempt is never silently unlabelled): the anchor
    // input could not be read to even attempt the LiveGraph.
    let Some(ev) = original.explain_identity_evidence() else {
        return fallback(CoherenceFallbackReason::LiveGraphUnavailable);
    };
    // No file coordinate → cannot locate a resident partition to anchor the live name (Partial: the symbol's
    // residency cannot be established).
    let Some(file) = ev.path.as_deref() else {
        return fallback(CoherenceFallbackReason::LiveGraphPartial);
    };
    let guard = repo_state.livegraph.read();
    let Some(lg) = guard.as_ref() else {
        return fallback(CoherenceFallbackReason::LiveGraphUnavailable);
    };
    // Decompose the per-file residency precondition into the specific cert-ladder reason (mirrors
    // `explain_imports_outcome`'s freshness-before-language ordering): non-resident → Partial; stale → Stale;
    // non-TS → UnsupportedLanguage.
    match lg.file_partition_status(file) {
        None => return fallback(CoherenceFallbackReason::LiveGraphPartial),
        Some(s) if !s.fresh => return fallback(CoherenceFallbackReason::LiveGraphStale),
        Some(s) if !s.ts_primary => {
            return fallback(CoherenceFallbackReason::LiveGraphUnsupportedLanguage)
        }
        Some(_) => {} // resident + Fresh + TS → the anchor may be served.
    }
    let key = repo_graph_ir::CanonicalKey::from_existing(symbol_key);
    let Some((name, subtype)) = lg.node_display(&key) else {
        // The resident partition carries no live IR node for this key (renamed/removed in current state, or no
        // display metadata) → the live IR cannot name the anchor; keep the SQLite primary, labelled.
        return fallback(CoherenceFallbackReason::LiveGraphDisplayMetadataUnavailable);
    };
    drop(guard);

    // Override ONLY the anchor fields; the snapshot-scoped coordinate fields (path/line_start/module_path/
    // language) stay SQLite → the D8 multi-source `{livegraph, sqlite}` leaf. The current-state name/subtype
    // is the same IR symbol-attributes substrate the `stats` fastpath serves.
    let served = ExplainIdentityEvidence {
        name: Some(name),
        subtype: Some(subtype),
        ..ev
    };
    let replacement = original.adopt_rank_and_scope(Signal::explain_identity(served));
    (Some(replacement), Some(lg_posture()))
}

/// A labelled SQLite-fallback identity decision: NO replacement value (the proven SQLite identity primary
/// stays in place), and the `reason` records why the live anchor could not be served. The pure
/// `explain_to_coherent` turns this into a `{sqlite}` leaf with `fallback_reason` set (never an unlabelled
/// `{sqlite}` — that is reserved for the no-attempt file/path-focus listings identity).
fn fallback(reason: CoherenceFallbackReason) -> (Option<Signal>, Option<OrientLeafLabel>) {
    (None, Some(OrientLeafLabel::SqliteFallback { reason }))
}
