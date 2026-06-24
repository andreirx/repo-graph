//! ORIENT-LIVEGRAPH-IMPL: orient's per-leaf LiveGraph DECISIONS + the orient-exclusive complexity cert.
//!
//! Extracted from `livegraph_feed.rs` (review-7 pt2 / structural guardrail): the orient leaf-decision
//! machinery and the complexity no-loss cert are a DISTINCT responsibility (orient's coherence sourcing),
//! so they live in their own focused module instead of expanding the already-oversized shared feed file.
//! `livegraph_feed.rs` keeps the SHARED LiveGraph feed/fastpath infrastructure (the `Auto` ladder, the
//! engine responses, the import/cycles/stats certs, the SQLite-free fingerprint); this module REUSES that
//! infrastructure (`ts_only`, `import_cert_fingerprint`, `build_and_store_cycles_cert`, `FallbackReason`,
//! `CycleNoLossCert`) to decide orient's FOUR LG-first leaves (D-ORIENT-1).
//!
//! This is the IMPURE half (Clean Architecture: mechanism). It reads the daemon `RepoState` (the in-memory
//! LiveGraph + the cycles/complexity no-loss certs + SQLite) and returns a POSTURE-bearing
//! [`OrientLgOutcome`] per leaf. orient is unusual among the migrated commands in that the agent already
//! computed the SQLite VALUE, so these helpers do NOT re-serve a value — they only decide the per-leaf
//! LABEL (livegraph posture vs labelled SQLite fallback). `orient_coherence::build_orient_envelope` maps
//! [`OrientLgOutcome`] -> the agent's `OrientLeafLabel`. Every `livegraph` label is gated by a daemon-side
//! NO-LOSS proof — never a bare relabel of a SQLite-built value. The shared callers/callees/cycles fastpath
//! functions in `livegraph_feed.rs` are UNCHANGED (no regression).
//!
//! STRUCTURE (review-3 item 3): this file is the per-leaf decision core — the `OrientLgOutcome` ladder, the
//! cycles leaf decision, and the callgraph (callers/callees) leaf decisions including the serve-decision
//! gate. To honour the 500-line guardrail the bulk that bloated the file is extracted to siblings: the
//! ~1045 lines of TESTS to [`tests`] (pure unit) + [`served_e2e`] (daemon-half integration), and the
//! self-contained orient-exclusive COMPLEXITY no-loss cert to [`complexity_cert`] (a sibling cert module,
//! like `focus_resolution_cert` / `callgraph_cert`), re-exported so its `crate::orient_lg_decisions::` path
//! is unchanged.

use repo_graph_agent::AgentStorageRead;
use repo_graph_trust_model::{
    AnswerClass, AnswerEnvelope, DegradationReason, FreshnessState, Granularity, LanguageSupport,
    QueryCompleteness,
};

use crate::callgraph_cert::callgraph_cached_green;
use crate::livegraph_feed::{
    build_and_store_cycles_cert, import_cert_fingerprint, ts_only, FallbackReason,
};
use crate::state::RepoState;

// ── orient per-leaf LiveGraph leaf-decision ladder (the module doc covers the rationale + reuse) ──

/// The per-leaf LiveGraph outcome for one of orient's FOUR LG-first signals (D-ORIENT-1): either the
/// LiveGraph served (project the answer posture VERBATIM) or a labelled SQLite fallback (the no-loss-proof
/// ladder reason). Produced by `orient_cycles_outcome` (cycles cert), `orient_complexity_outcome`
/// (complexity cert), and `orient_callers_outcome` / `orient_callees_outcome` (the `Auto` ladder + the
/// per-symbol no-loss key compare).
pub(crate) enum OrientLgOutcome {
    /// The LiveGraph answer is servable (Exact + Fresh + TS-only, and — for cycles — the no-loss cert is
    /// GREEN). The posture axes are projected verbatim from the answer.
    Livegraph {
        class: AnswerClass,
        completeness: QueryCompleteness,
        freshness: FreshnessState,
        degradation_reasons: Vec<DegradationReason>,
        contributing_languages: std::collections::BTreeSet<LanguageSupport>,
    },
    /// The LiveGraph cannot serve this leaf -> the orient leaf falls back to the proven SQLite primary,
    /// labelled with this reason.
    Fallback { reason: FallbackReason },
}

/// Reduce a LiveGraph `AnswerEnvelope` to an orient leaf outcome via the SAME ladder the migrated
/// callers/callees `Auto` decision uses (`auto_outcome`): freshness BEFORE class (a Stale answer reports
/// `LiveGraphStale`, not `LiveGraphPartial`), then class Exact, then TS-only. On success the posture axes
/// are projected verbatim so the orient leaf's trust mirrors the LiveGraph answer.
fn orient_outcome_from_env<T>(env: &AnswerEnvelope<T>) -> OrientLgOutcome {
    if env.freshness() != FreshnessState::Fresh {
        return OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphStale,
        };
    }
    if env.class() != AnswerClass::Exact {
        return OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphPartial,
        };
    }
    if !ts_only(env.contributing_languages()) {
        return OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphUnsupportedLanguage,
        };
    }
    OrientLgOutcome::Livegraph {
        class: env.class(),
        completeness: env.completeness(),
        freshness: env.freshness(),
        degradation_reasons: env.degradation_reasons().to_vec(),
        contributing_languages: env.contributing_languages().clone(),
    }
}

/// Gate an orient callers/callees `Livegraph` ladder outcome behind a per-symbol NO-LOSS key compare: the
/// LiveGraph key set (`lg_keys`) must equal the SQLite key set; else a labelled `LiveGraphCallgraphDivergence`
/// fallback. This is the VALUE-EQUIVALENCE PROOF that lets orient label its (SQLite-built) module summary
/// `livegraph` without a bare relabel (review feedback). A ladder that already fell back is returned
/// unchanged (the SQLite read is SKIPPED). A SQLite read error -> `LiveGraphError` (cannot prove
/// equivalence -> safe SQLite fallback). The compare is set-based (order-independent). Generic over the
/// storage error so this stays decoupled from the concrete `AgentStorageError`. It is the NOT-green
/// per-symbol fallback under [`gate_callgraph_label`] — a GREEN repo-wide callgraph cert skips this per-call read.
fn gate_callgraph_no_loss<E>(
    ladder: OrientLgOutcome,
    lg_keys: std::collections::BTreeSet<String>,
    sqlite_keys: impl FnOnce() -> Result<std::collections::BTreeSet<String>, E>,
) -> OrientLgOutcome {
    match &ladder {
        OrientLgOutcome::Fallback { .. } => return ladder,
        OrientLgOutcome::Livegraph { .. } => {}
    }
    match sqlite_keys() {
        Ok(sql) if sql == lg_keys => ladder,
        Ok(_) => OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphCallgraphDivergence,
        },
        Err(_) => OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphError,
        },
    }
}

/// Route an orient callers/callees `Livegraph` ladder LABEL through the repo-wide CALLGRAPH NO-LOSS cert —
/// ZERO per-call SQLite read on GREEN (review-1 item 1). A CACHED-GREEN cert ([`callgraph_cached_green`])
/// proves field-exact MULTISET parity (LiveGraph == SQLite) for EVERY corpus symbol; that SUBSUMES the
/// per-symbol key-set compare ([`gate_callgraph_no_loss`]) — an equal multiset of full rows projects onto an
/// equal stable_key set — so it licenses the `livegraph` label WITHOUT re-reading SQLite.
///
/// CRITICAL — this PEEKS the cert, it does NOT build it. The bounded-cert PRECHECK in `handle_orient`
/// (`orient_bounded_cert_is_green`) owns the (build-time, corpus-wide `callers`/`callees`-querying) cert
/// construction; on the served-green path it has already built the cert GREEN, so this peek hits and the leaf
/// labels `livegraph` zero-read. The label path must NEVER trigger the build itself: when the bounded cert is
/// RED (e.g. focus-resolution RED -> the precheck short-circuits and the callgraph cert is never built) the
/// daemon serves the (b) leaves from SQLite, and a label-path build would (a) do corpus-wide producer work on
/// a path we already chose to fall back on, and (b) surface a producer `callers`/`callees` panic on a path
/// that otherwise falls back cleanly. So a peek MISS -> the per-symbol `gate_callgraph_no_loss` compare (the
/// shipped granularity; one `find_symbol_*` read), byte-identical to the prior per-call path. An
/// already-fallback ladder is returned unchanged (no cert read, no SQLite read).
fn gate_callgraph_label<E>(
    repo_state: &RepoState,
    snapshot_uid: &str,
    ladder: OrientLgOutcome,
    lg_keys: std::collections::BTreeSet<String>,
    sqlite_keys: impl FnOnce() -> Result<std::collections::BTreeSet<String>, E>,
) -> OrientLgOutcome {
    match &ladder {
        OrientLgOutcome::Fallback { .. } => return ladder,
        OrientLgOutcome::Livegraph { .. } => {}
    }
    // Zero-read fast path: an ALREADY-CACHED GREEN callgraph cert subsumes the per-symbol compare. PEEK
    // only — never build (the precheck owns the build; see the doc above).
    if callgraph_cached_green(repo_state, snapshot_uid) {
        return ladder;
    }
    // Peek miss -> the per-symbol value-equivalence compare (the shipped granularity; reads SQLite once).
    gate_callgraph_no_loss(ladder, lg_keys, sqlite_keys)
}

/// Build the orient CALLER ladder + LiveGraph caller key set from the migrated `callers` surface (reads the
/// LiveGraph ONLY — NO SQLite). Shared by the per-symbol-gated [`orient_callers_outcome`] (the EXPLAIN reuse)
/// and the served-path [`orient_callers_outcome_served`] (orient's serve-gated, zero-read-on-green path). A
/// non-resident referencing partition makes the answer non-Exact -> a `Fallback` ladder (empty key set).
fn callers_ladder(
    repo_state: &RepoState,
    target: &str,
) -> (OrientLgOutcome, std::collections::BTreeSet<String>) {
    let guard = repo_state.livegraph.read();
    match guard.as_ref() {
        None => (
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphUnavailable,
            },
            std::collections::BTreeSet::new(),
        ),
        Some(lg) => {
            let env = lg.callers(target, Granularity::CallerDetail);
            let keys: std::collections::BTreeSet<String> = env
                .data()
                .map(|d| d.caller_identities.iter().map(|(_, k)| k.clone()).collect())
                .unwrap_or_default();
            (orient_outcome_from_env(&env), keys)
        }
    }
}

/// Build the orient CALLEE ladder + LiveGraph callee key set from the migrated `callees` surface (LiveGraph
/// only — NO SQLite). The dual of [`callers_ladder`]; the callees residency asymmetry (a non-resident defining
/// partition) surfaces as a non-Exact answer -> a `Fallback` ladder.
fn callees_ladder(
    repo_state: &RepoState,
    target: &str,
) -> (OrientLgOutcome, std::collections::BTreeSet<String>) {
    let guard = repo_state.livegraph.read();
    match guard.as_ref() {
        None => (
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphUnavailable,
            },
            std::collections::BTreeSet::new(),
        ),
        Some(lg) => {
            let env = lg.callees(target, Granularity::CallerDetail);
            let keys: std::collections::BTreeSet<String> = env
                .data()
                .map(|d| d.callee_identities.iter().map(|(k, _)| k.clone()).collect())
                .unwrap_or_default();
            (orient_outcome_from_env(&env), keys)
        }
    }
}

/// The SQLite CALLER key set for `target` (`find_symbol_callers` -> stable_key set) — the per-symbol no-loss
/// compare read. The NOT-green fallback read for both callers outcome variants.
fn sqlite_caller_keys(
    repo_state: &RepoState,
    snapshot_uid: &str,
    target: &str,
) -> Result<std::collections::BTreeSet<String>, repo_graph_agent::AgentStorageError> {
    let conn = repo_state
        .storage()
        .map_err(|e| repo_graph_agent::AgentStorageError::new("find_symbol_callers", e))?;
    conn.find_symbol_callers(snapshot_uid, target)
        .map(|rows| rows.into_iter().map(|r| r.stable_key).collect())
}

/// The SQLite CALLEE key set for `target` (`find_symbol_callees` -> stable_key set) — the per-symbol no-loss
/// compare read. The NOT-green fallback read for both callees outcome variants.
fn sqlite_callee_keys(
    repo_state: &RepoState,
    snapshot_uid: &str,
    target: &str,
) -> Result<std::collections::BTreeSet<String>, repo_graph_agent::AgentStorageError> {
    let conn = repo_state
        .storage()
        .map_err(|e| repo_graph_agent::AgentStorageError::new("find_symbol_callees", e))?;
    conn.find_symbol_callees(snapshot_uid, target)
        .map(|rows| rows.into_iter().map(|r| r.stable_key).collect())
}

/// orient CALLERS_SUMMARY (symbol focus) leaf decision — the per-symbol no-loss compare variant. Reuses the
/// migrated `callers` surface (the Exact + Fresh + TS-only ladder is the PRECONDITION) AND proves value-
/// equivalence (the per-symbol no-loss key compare vs SQLite `find_symbol_callers` is the SUFFICIENT proof).
/// When referencing partitions are non-resident the answer is non-Exact -> the ladder falls back BEFORE the
/// compare (never an Exact module-grouped summary from partition-only data). REUSED UNCHANGED by EXPLAIN-
/// LIVEGRAPH-IMPL (`explain_lg_serve::serve_callers`) — explain keeps the per-symbol compare; the served-path
/// variant ([`orient_callers_outcome_served`]) is orient-only (this slice is orient's half of PREREQ-1).
pub(crate) fn orient_callers_outcome(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
) -> OrientLgOutcome {
    let (ladder, lg_keys) = callers_ladder(repo_state, target);
    gate_callgraph_no_loss(ladder, lg_keys, || {
        sqlite_caller_keys(repo_state, snapshot_uid, target)
    })
}

/// orient CALLERS_SUMMARY (symbol focus) leaf decision — the SERVED-PATH variant used by
/// `build_orient_envelope`. `serve_from_lg` is `handle_orient`'s bounded-cert verdict
/// (`orient_bounded_cert_is_green`) — the SINGLE authority for whether the (b) leaves were LiveGraph-served
/// THIS call:
/// - `serve_from_lg == false` (review-3 item 1): the bounded cert was RED for SOME contributor (e.g.
///   focus-resolution), so `handle_orient` ran the agent over BARE SQLite — the value is SQLite-sourced.
///   The leaf is SQLite-LABELLED `LiveGraphBoundedServeDeclined`, NEVER re-certified `livegraph` from the
///   callgraph cert state alone (the false-provenance fix: provenance follows the ACTUAL serve, not a cert
///   peek). The callgraph contributor may even be GREEN here — irrelevant, because dispatch fell back.
/// - `serve_from_lg == true`: the decorator served the value from the LiveGraph; the bounded cert is GREEN
///   (so the callgraph cert is GREEN), and the CERT-GATED label peeks it -> `livegraph` with ZERO per-call
///   SQLite read (the FULL served path matches the decorator's VALUE serve, review-1 item 1). On the GREEN
///   path the label is byte-identical to [`orient_callers_outcome`]; a cert PEEK miss (tests not running the
///   precheck) falls back to the per-symbol compare (same label, one read).
pub(crate) fn orient_callers_outcome_served(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
    serve_from_lg: bool,
) -> OrientLgOutcome {
    if !serve_from_lg {
        return OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphBoundedServeDeclined,
        };
    }
    let (ladder, lg_keys) = callers_ladder(repo_state, target);
    gate_callgraph_label(repo_state, snapshot_uid, ladder, lg_keys, || {
        sqlite_caller_keys(repo_state, snapshot_uid, target)
    })
}

/// orient CALLEES_SUMMARY (symbol focus) leaf decision — the per-symbol no-loss compare variant. Reuses the
/// migrated `callees` surface + the per-symbol no-loss key compare vs SQLite `find_symbol_callees`. The
/// summary-callees residency asymmetry (a non-resident defining partition) surfaces as a non-Exact answer ->
/// the ladder falls back BEFORE the compare (the ratified common callees path until the deferral lifts; never
/// Exact-empty). REUSED UNCHANGED by EXPLAIN-LIVEGRAPH-IMPL (`explain_lg_serve::serve_callees`).
pub(crate) fn orient_callees_outcome(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
) -> OrientLgOutcome {
    let (ladder, lg_keys) = callees_ladder(repo_state, target);
    gate_callgraph_no_loss(ladder, lg_keys, || {
        sqlite_callee_keys(repo_state, snapshot_uid, target)
    })
}

/// orient CALLEES_SUMMARY (symbol focus) leaf decision — the SERVED-PATH variant (the dual of
/// [`orient_callers_outcome_served`]). `serve_from_lg == false` -> SQLite-LABELLED
/// `LiveGraphBoundedServeDeclined` (the value is the bare SQLite read; review-3 item 1); `serve_from_lg ==
/// true` -> the cert-gated label (ZERO per-call SQLite read on a GREEN repo-wide callgraph cert). Used by
/// `build_orient_envelope`. On the served-green path the LABEL is byte-identical to [`orient_callees_outcome`].
pub(crate) fn orient_callees_outcome_served(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
    serve_from_lg: bool,
) -> OrientLgOutcome {
    if !serve_from_lg {
        return OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphBoundedServeDeclined,
        };
    }
    let (ladder, lg_keys) = callees_ladder(repo_state, target);
    gate_callgraph_label(repo_state, snapshot_uid, ladder, lg_keys, || {
        sqlite_callee_keys(repo_state, snapshot_uid, target)
    })
}

/// orient IMPORT_CYCLES (repo / path focus + the symbol `ModuleContext` variant) leaf decision. Reuses
/// the repo-wide module-cycle no-loss cert: only labels `livegraph` when the answer is Exact + Fresh +
/// TS-only AND the cert is GREEN at the current fingerprint (proving LG == SQLite — RISK-E module-identity
/// divergence). Else a labelled SQLite fallback. The repo-wide cert is a CONSERVATIVE gate for the
/// symbol-focus module-context cycles variant (GREEN repo-wide implies the module subset matches too).
pub(crate) fn orient_cycles_outcome(repo_state: &RepoState, snapshot_uid: &str) -> OrientLgOutcome {
    // Capture the LiveGraph posture + fingerprint under the read lock, then DROP it before the cert build
    // (the cert build acquires storage + the cycles_cert write lock and re-reads the livegraph; never
    // hold the livegraph read across it — mirrors `cycles_auto_response`).
    let (served, current_fp) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            None => {
                return OrientLgOutcome::Fallback {
                    reason: FallbackReason::LiveGraphUnavailable,
                }
            }
            Some(lg) => {
                let env = lg.module_import_cycles();
                let fp = import_cert_fingerprint(&lg.live_partitions(), snapshot_uid);
                (orient_outcome_from_env(&env), fp)
            }
        }
    };
    // If the answer-class ladder already failed, that is the outcome (the cert is irrelevant).
    match &served {
        OrientLgOutcome::Fallback { .. } => return served,
        OrientLgOutcome::Livegraph { .. } => {}
    }
    // The no-loss cert gates the FINAL livegraph label: only claim livegraph when LG == SQLite at the
    // current fingerprint. Stale/missing -> (re)build once per fingerprint.
    let cert_green = {
        let cached = repo_state.cycles_cert.read();
        match cached.as_ref() {
            Some(c) if c.fingerprint == current_fp => Some(c.verdict == "GREEN"),
            _ => None,
        }
    };
    let green = match cert_green {
        Some(g) => g,
        None => {
            build_and_store_cycles_cert(repo_state, snapshot_uid, Some(current_fp)).unwrap_or(false)
        }
    };
    if green {
        served
    } else {
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphCycleDivergence,
        }
    }
}

/// The orient-exclusive COMPLEXITY no-loss cert (`ComplexityNoLossCert` + `orient_complexity_outcome`)
/// lives in its own sibling module — like `focus_resolution_cert` / `callgraph_cert` — and is re-exported
/// so consumers reach it at the unchanged `crate::orient_lg_decisions::` path (review-3 item 3).
mod complexity_cert;
pub(crate) use complexity_cert::{orient_complexity_outcome, ComplexityNoLossCert};

#[cfg(test)]
mod served_e2e;
#[cfg(test)]
mod tests;
