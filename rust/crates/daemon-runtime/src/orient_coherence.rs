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
    orient_callees_outcome_served, orient_callers_outcome_served, orient_complexity_outcome,
    orient_cycles_outcome, OrientLgOutcome,
};
use crate::state::RepoState;
use crate::util::compute_trust_overlay_for_snapshot;

/// Build orient's coherence-wrapped response from the agent's bare [`OrientResult`].
///
/// `repo_uid` is the resolved repo uid; `display_name` is already set on `result` by the handler.
/// `serve_from_lg` is `handle_orient`'s bounded-cert serve decision (`orient_bounded_cert_is_green`): the
/// SINGLE authority for whether the (b) leaves were LiveGraph-served THIS call. It gates the CALLERS/CALLEES
/// callgraph leaf LABEL (review-3 item 1): on `false` the daemon ran orient over BARE SQLite, so those leaves
/// are SQLite-LABELLED, NEVER re-certified `livegraph` from the callgraph cert state alone (the provenance
/// follows the ACTUAL serve, not a cert peek). The cycles + complexity leaves are NOT (b) serve leaves — their
/// VALUE is always SQLite (CYCLES-A; the decorator delegates), and their hybrid `livegraph` LABEL is the
/// SHIPPED corroboration-cert behavior (their OWN cycles/complexity cert), independent of `serve_from_lg`.
///
/// The returned envelope is what the daemon serializes for `rmap orient`.
pub(crate) fn build_orient_envelope(
    repo_state: &RepoState,
    repo_uid: &str,
    result: OrientResult,
    serve_from_lg: bool,
    // EC-M2-LEAF-SERVE-1: true iff the decorator ACTUALLY served the MODULE_SUMMARY structural
    // counts from the LiveGraph this request (module-summary cert GREEN at the captured witness
    // fingerprint — review-0 #1: INDEPENDENT of `serve_from_lg`/the bounded fold — ∧ the epoch
    // still resident post-use-case). The leaf label follows the actual serve — on `false` the
    // decision is ABSENT and the leaf renders exactly as before M-2 (the plain sqlite leaf; RED
    // path byte-identical).
    module_summary_served: bool,
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
    // D-S = S-A: open a fresh per-operation connection (the orient read guard keeps it
    // snapshot-consistent). Open failure cannot vouch for freshness -> conservative STALE.
    let stale = match repo_state.storage() {
        Ok(conn) => match conn.get_stale_files(&snapshot_uid) {
            Ok(files) => !files.is_empty(),
            Err(_) => true,
        },
        Err(_) => true,
    };

    // Which LG-first signals are present? Only decide those (avoid needless LiveGraph reads + cert builds).
    let mut present_cycles = false;
    let mut present_complexity = false;
    let mut present_callers = false;
    let mut present_callees = false;
    let mut present_module_summary = false;
    for s in &result.signals {
        match s.code() {
            SignalCode::ImportCycles => present_cycles = true,
            SignalCode::HighComplexity => present_complexity = true,
            SignalCode::CallersSummary => present_callers = true,
            SignalCode::CalleesSummary => present_callees = true,
            SignalCode::ModuleSummary => present_module_summary = true,
            _ => {}
        }
    }

    let mut decisions = OrientLgDecisions::default();

    // EC-M2-LEAF-SERVE-1: the MODULE_SUMMARY leaf decision EXISTS only when the decorator actually
    // served the structural counts from the LiveGraph this request (provenance follows the ACTUAL
    // serve, the review-3 discipline). When absent, `to_coherent` keeps the pre-M-2 fixed sqlite
    // leaf — byte-identical on every non-served path, including bounded-GREEN with a RED
    // module-summary cert (an attempted-but-declined serve is still the proven SQLite primary).
    if present_module_summary && module_summary_served {
        decisions.module_summary = Some(map_outcome(
            crate::module_summary_cert::module_summary_served_outcome(),
        ));
    }

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

    // CALLERS_SUMMARY / CALLEES_SUMMARY are symbol-focus only; the focus carries the symbol stable key. The
    // SERVED-PATH variants gate the leaf LABEL on `serve_from_lg` (review-3 item 1): when dispatch SERVED
    // (bounded cert GREEN) the cert-gated label peeks the GREEN callgraph cert -> `livegraph` with ZERO
    // per-call SQLite read (matching the decorator's VALUE serve, review-1 item 1); when dispatch FELL BACK
    // to bare SQLite (`serve_from_lg == false`) the value is SQLite-sourced, so the leaf is SQLite-LABELLED
    // `LiveGraphBoundedServeDeclined` — NEVER re-certified `livegraph` from the callgraph cert state alone.
    if let Some(target) = symbol_target(&result.focus) {
        if present_callers {
            decisions.callers_summary = Some(map_outcome(orient_callers_outcome_served(
                repo_state,
                target,
                &snapshot_uid,
                serve_from_lg,
            )));
        }
        if present_callees {
            decisions.callees_summary = Some(map_outcome(orient_callees_outcome_served(
                repo_state,
                target,
                &snapshot_uid,
                serve_from_lg,
            )));
        }
    }

    let (trust_briefing, relationship_next_action) =
        compute_briefing_and_remedy(repo_state, repo_uid, &snapshot_uid);

    let mut envelope = to_coherent(result, &decisions, trust_briefing, stale);
    // HONEST-DEGRADATION-IMPL-2 (D5): place the daemon-computed next-action onto the value (populated
    // post-fold, like `display_name`). It rides the value as a reader hint, NOT a coherence leaf, so it
    // does not participate in the MEET fold.
    envelope.value.relationship_next_action = relationship_next_action;

    // METRIC-LANG-COVERAGE-1 (part A): when orient emits a complexity ranking (HIGH_COMPLEXITY present),
    // attach the per-language measurement-coverage block so the ranking never reads as repo-wide while a
    // whole language is unmeasured. Same post-fold reader-hint placement as above (NOT a coherence leaf: it
    // is a straight snapshot count, not an LG/SQLite-provenance value). Data-driven caveat (no hardcoded
    // language list) that disappears by itself once every significant language is measured. Gated on
    // `present_complexity` — no complexity surface, nothing to caveat. The block is ALWAYS PRESENT once
    // there IS a complexity surface (review-6 item 2): a query failure yields an explicit `unavailable`
    // block inside `measurement_coverage_json`, and a storage-OPEN failure yields one here — never a silent
    // gap, which a consumer would read as complete coverage. One fresh per-operation connection (the orient
    // read guard keeps it snapshot-consistent).
    if present_complexity {
        let block = match repo_state.storage() {
            Ok(storage) => crate::util::measurement_coverage_json(&storage, &snapshot_uid),
            Err(_) => crate::util::measurement_coverage_unavailable_json(),
        };
        envelope.value.measurement_coverage = Some(block);
    }

    envelope
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
        FallbackReason::LiveGraphBoundedServeDeclined => {
            CoherenceFallbackReason::LiveGraphBoundedServeDeclined
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
    // D-S = S-A: one fresh per-operation connection for this briefing (open failure -> None = no briefing).
    let storage = repo_state.storage().ok()?;
    let snapshot = storage.get_snapshot(snapshot_uid).ok().flatten()?;
    let trust = compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "CALLS+IMPORTS")?;
    briefing_json(&trust)
}

/// The degraded-only briefing gate, extracted so orient (via [`compute_briefing_and_remedy`]) and explain
/// (via [`compute_trust_briefing`]) apply the SAME rule: `Some(overlay-as-json)` iff any axis is non-HIGH
/// or a caveat is present; else `None` (absent on the wire).
fn briefing_json(overlay: &repo_graph_trust::TrustOverlaySummary) -> Option<serde_json::Value> {
    if overlay.has_degradation() || !overlay.caveats.is_empty() {
        serde_json::to_value(overlay).ok()
    } else {
        None
    }
}

/// HONEST-DEGRADATION-IMPL-2 (D5): compute orient's degraded-state briefing AND its toolchain-aware
/// next-action line from ONE overlay — orient assembles the trust overlay only ONCE (no double assembly on
/// the primary surface). The next-action uses the SAME dispatch helper (`relationship_next_action_line`,
/// keyed on `configured_resolver_languages_from_env`) `stats` uses, so the two surfaces render ONE
/// coherent line for the same repo. `None`/`None` on any read failure (honest silence).
fn compute_briefing_and_remedy(
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
) -> (Option<serde_json::Value>, Option<String>) {
    let Ok(storage) = repo_state.storage() else {
        return (None, None);
    };
    let Some(snapshot) = storage.get_snapshot(snapshot_uid).ok().flatten() else {
        return (None, None);
    };
    let Some(overlay) =
        compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "CALLS+IMPORTS")
    else {
        return (None, None);
    };
    let briefing = briefing_json(&overlay);
    // Only fetch the repo languages when a relationship axis is actually LOW — this guards the
    // repo-summary COUNT off the healthy-repo hot path. (The line helper re-checks LOW for safety.)
    let remedy = if crate::dispatch::relationship_reliability_is_low(&overlay.reliability) {
        let languages =
            repo_graph_agent::AgentStorageRead::compute_repo_summary(&storage, snapshot_uid)
                .map(|s| s.languages)
                .unwrap_or_default();
        crate::dispatch::relationship_next_action_line(
            &overlay.reliability,
            &languages,
            &crate::dispatch::configured_resolver_languages_from_env(),
        )
    } else {
        None
    };
    (briefing, remedy)
}
