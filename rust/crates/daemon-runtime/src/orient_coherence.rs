//! ORIENT-LIVEGRAPH-IMPL: assemble orient's `CoherenceEnvelope<CoherentOrientResult>` response.
//!
//! This is the IMPURE adapter (Clean Architecture: mechanism). It reads the daemon `RepoState` (the
//! in-memory LiveGraph + the cycles/complexity no-loss certs + SQLite) to supply the agent's PURE
//! [`repo_graph_agent::to_coherent`] with:
//!   - the per-leaf LiveGraph DECISIONS for orient's FOUR LG-first signals (D-ORIENT-1), mapping the
//!     daemon `FallbackReason` -> the pure [`CoherenceFallbackReason`]:
//!       * IMPORT_CYCLES — the cycles no-loss cert,
//!       * HIGH_COMPLEXITY — the complexity no-loss cert over the repo-wide `high_complexity` read,
//!       * CALLERS_SUMMARY / CALLEES_SUMMARY — the migrated callers/callees `Auto` ladder + a per-symbol
//!         no-loss key-set compare (the value-equivalence proof), and
//!   - the degraded-state `trust_briefing` overlay (D-ORIENT-6 = O2), computed by the SAME
//!     `compute_trust_overlay_for_snapshot` + degraded-only gate the daemon used before (preserved
//!     verbatim), now placed onto `value.trust_briefing` rather than a post-serialize top-level `trust`
//!     key.
//!
//! It is a SEPARATE module (not appended to the 6739-line `dispatch.rs`) per the structural guardrail:
//! this is a genuinely NEW responsibility (coherence assembly), so it lives on its own. `handle_orient`
//! just calls [`build_orient_envelope`]. Every `livegraph` label is gated by a daemon-side NO-LOSS proof —
//! a `livegraph` source is never a bare relabel of a SQLite-built value.

use repo_graph_agent::{
    to_coherent, CoherentOrientResult, Focus, OrientLeafLabel, OrientLgDecisions, OrientResult,
    ResolvedKind, SignalCode,
};
use repo_graph_coherence::{CoherenceEnvelope, CoherenceFallbackReason};

use crate::livegraph_feed::FallbackReason;
use crate::orient_lg_decisions::{
    orient_callees_outcome, orient_callers_outcome, orient_complexity_outcome,
    orient_cycles_outcome, OrientLgOutcome,
};
use crate::state::RepoState;
use crate::util::compute_trust_overlay_for_snapshot;

/// Build orient's coherence-wrapped response from the agent's bare [`OrientResult`].
///
/// `repo_uid` is the resolved repo uid; `display_name` is already set on `result` by the handler. The
/// returned envelope is what the daemon serializes for `rmap orient`.
pub(crate) fn build_orient_envelope(
    repo_state: &RepoState,
    repo_uid: &str,
    result: OrientResult,
) -> CoherenceEnvelope<CoherentOrientResult> {
    let snapshot_uid = result.snapshot.clone();

    // `stale` = the backing index is stale. AUTHORITATIVE source: a direct `get_stale_files` read — the
    // SAME condition the spec names (orient-livegraph-1.md:462/626: "Stale leaf when get_stale_files
    // non-empty"). It is deliberately NOT derived from the emitted `TRUST_STALE_SNAPSHOT` signal: that
    // signal is ranked + budget-TRUNCATED and focus-dependent, so its presence is an unreliable proxy — a
    // truncated or focus-omitted signal would read a genuinely stale index as fresh and mint a false
    // `Fresh`/`Exact` on the SQLite/Authority/FS leaves and the root (review-9 gap 1; Fact Certainty Model).
    // Reading storage here is budget-/focus-independent, so the staleness verdict is faithful regardless of
    // ranking. CONSERVATIVE on read error -> STALE: a stale-files read failure cannot vouch for freshness, so
    // it degrades rather than mints a false `Fresh` (matching the codebase's Unknown->Stale discipline). In
    // practice the agent already read this same surface successfully earlier in the request (trust
    // aggregator), so the error branch is defensive-only. (Ambiguous/no-match emit no signals and take the
    // resolution-only path where `stale` is ignored, so this read is harmless there.)
    let stale = match repo_state.storage.get_stale_files(&snapshot_uid) {
        Ok(files) => !files.is_empty(),
        Err(_) => true,
    };

    // Which LG-first signals are present? Only decide those (avoid needless LiveGraph reads + cert builds).
    let mut present_cycles = false;
    let mut present_complexity = false;
    let mut present_callers = false;
    let mut present_callees = false;
    for s in &result.signals {
        match s.code() {
            SignalCode::ImportCycles => present_cycles = true,
            SignalCode::HighComplexity => present_complexity = true,
            SignalCode::CallersSummary => present_callers = true,
            SignalCode::CalleesSummary => present_callees = true,
            _ => {}
        }
    }

    let mut decisions = OrientLgDecisions::default();

    if present_cycles {
        decisions.import_cycles = Some(map_outcome(orient_cycles_outcome(
            repo_state,
            &snapshot_uid,
        )));
    }

    // HIGH_COMPLEXITY is repo-focus only (the agent emits it only there); the complexity no-loss cert
    // gates the `livegraph` label over the repo-wide `high_complexity` read.
    if present_complexity {
        decisions.high_complexity = Some(map_outcome(orient_complexity_outcome(
            repo_state,
            &snapshot_uid,
        )));
    }

    // CALLERS_SUMMARY / CALLEES_SUMMARY are symbol-focus only; the focus carries the symbol stable key.
    // The per-symbol no-loss key compare needs the snapshot to read the SQLite caller/callee key set.
    if let Some(target) = symbol_target(&result.focus) {
        if present_callers {
            decisions.callers_summary = Some(map_outcome(orient_callers_outcome(
                repo_state,
                target,
                &snapshot_uid,
            )));
        }
        if present_callees {
            decisions.callees_summary = Some(map_outcome(orient_callees_outcome(
                repo_state,
                target,
                &snapshot_uid,
            )));
        }
    }

    let trust_briefing = compute_trust_briefing(repo_state, repo_uid, &snapshot_uid);

    to_coherent(result, &decisions, trust_briefing, stale)
}

/// The symbol stable key when the focus resolved to a SYMBOL node (CALLERS/CALLEES targets); else `None`.
fn symbol_target(focus: &Focus) -> Option<&str> {
    if focus.resolved_kind == Some(ResolvedKind::Symbol) {
        focus.resolved_key.as_deref()
    } else {
        None
    }
}

/// Map a daemon LiveGraph outcome to the agent's leaf label (project the posture verbatim, or a labelled
/// SQLite fallback with the mapped reason).
///
/// `pub(crate)` so EXPLAIN-LIVEGRAPH-IMPL (`explain_coherence.rs`) maps its reused outcome functions
/// (`orient_callers_outcome`/`orient_callees_outcome`/`orient_cycles_outcome` + the explain-specific
/// `explain_imports_outcome`) through the IDENTICAL daemon→agent label mapping — the `OrientLeafLabel` is
/// the shared leaf-decision vocabulary, not forked per command.
pub(crate) fn map_outcome(outcome: OrientLgOutcome) -> OrientLeafLabel {
    match outcome {
        OrientLgOutcome::Livegraph {
            class,
            completeness,
            freshness,
            degradation_reasons,
            contributing_languages,
        } => OrientLeafLabel::Livegraph {
            class,
            completeness,
            freshness,
            degradation_reasons,
            contributing_languages,
        },
        OrientLgOutcome::Fallback { reason } => OrientLeafLabel::SqliteFallback {
            reason: map_fallback(reason),
        },
    }
}

/// Map the daemon LiveGraph `FallbackReason` -> the pure-crate `CoherenceFallbackReason` mirror (1:1; the
/// variants are identical by construction — the mirror is faithful).
fn map_fallback(reason: FallbackReason) -> CoherenceFallbackReason {
    match reason {
        FallbackReason::LiveGraphUnavailable => CoherenceFallbackReason::LiveGraphUnavailable,
        FallbackReason::LiveGraphPartial => CoherenceFallbackReason::LiveGraphPartial,
        FallbackReason::LiveGraphStale => CoherenceFallbackReason::LiveGraphStale,
        FallbackReason::LiveGraphUnsupportedLanguage => {
            CoherenceFallbackReason::LiveGraphUnsupportedLanguage
        }
        FallbackReason::LiveGraphRenderUnsupported => {
            CoherenceFallbackReason::LiveGraphRenderUnsupported
        }
        FallbackReason::LiveGraphDisplayMetadataUnavailable => {
            CoherenceFallbackReason::LiveGraphDisplayMetadataUnavailable
        }
        FallbackReason::LiveGraphError => CoherenceFallbackReason::LiveGraphError,
        FallbackReason::LiveGraphImportRegression => {
            CoherenceFallbackReason::LiveGraphImportRegression
        }
        FallbackReason::LiveGraphImportUnknown => CoherenceFallbackReason::LiveGraphImportUnknown,
        FallbackReason::LiveGraphCycleDivergence => {
            CoherenceFallbackReason::LiveGraphCycleDivergence
        }
        FallbackReason::LiveGraphStatsDivergence => {
            CoherenceFallbackReason::LiveGraphStatsDivergence
        }
        FallbackReason::LiveGraphComplexityDivergence => {
            CoherenceFallbackReason::LiveGraphComplexityDivergence
        }
        FallbackReason::LiveGraphCallgraphDivergence => {
            CoherenceFallbackReason::LiveGraphCallgraphDivergence
        }
    }
}

/// Compute the degraded-state trust briefing overlay as an opaque JSON value (D-ORIENT-6 = O2). PRESERVES
/// the exact pre-existing gate: `compute_trust_overlay_for_snapshot(.., "CALLS+IMPORTS")`, attached only
/// when `has_degradation() || !caveats.is_empty()`. `None` (absent on the wire) otherwise.
///
/// `pub(crate)` so EXPLAIN-LIVEGRAPH-IMPL (`explain_coherence.rs`) reuses the IDENTICAL briefing
/// computation + degraded-only gate — explain injects the SAME `"CALLS+IMPORTS"` overlay as orient
/// (verified first-hand: dispatch.rs handle_explain), so it is the SECOND populator of the shared
/// `trust_briefing` field (D-EXPLAIN-TRUST-BRIEFING). The shared populate path must permit BOTH orient and
/// explain (NOT orient-only — the cross-slice correction RISK-E-G); reusing this function realizes that.
pub(crate) fn compute_trust_briefing(
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Option<serde_json::Value> {
    let snapshot = repo_state
        .storage
        .get_snapshot(snapshot_uid)
        .ok()
        .flatten()?;
    let trust = compute_trust_overlay_for_snapshot(
        &repo_state.storage,
        repo_uid,
        &snapshot,
        "CALLS+IMPORTS",
    )?;
    if trust.has_degradation() || !trust.caveats.is_empty() {
        serde_json::to_value(&trust).ok()
    } else {
        None
    }
}
