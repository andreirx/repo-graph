//! ORIENT-LIVEGRAPH-IMPL: orient's repo-level COMPLEXITY no-loss cert (extracted from
//! `orient_lg_decisions.rs` per the structural guardrail, review-3 item 3). A sibling cert module — exactly
//! like `focus_resolution_cert` and `callgraph_cert` — holding `ComplexityNoLossCert` + its field-exact
//! compare/build + the `orient_complexity_outcome` leaf decision. The parent re-exports
//! `orient_complexity_outcome` + `ComplexityNoLossCert`, so consumers (`orient_coherence`, `state`,
//! `served_e2e`) reach them at the SAME `crate::orient_lg_decisions::` path as before. `super` is
//! `orient_lg_decisions`, so the private `orient_outcome_from_env` ladder + the `OrientLgOutcome` enum
//! resolve unchanged.

use repo_graph_agent::AgentStorageRead;

use super::{orient_outcome_from_env, OrientLgOutcome};
use crate::livegraph_feed::{import_cert_fingerprint, FallbackReason};
use crate::state::RepoState;

// ════════════════════════════════════════════════════════════════════════════════════════════════
// ORIENT-LIVEGRAPH-IMPL: the repo-level COMPLEXITY no-loss cert. Mirrors the cycles/stats certs EXACTLY:
// a field-exact compare of the LiveGraph repo-wide `high_complexity` `(symbol_key, complexity)` SET vs the
// SQLite `measurements` high-complexity SET (keyed by the SHARED SQLite-free fingerprint) gates INCLUDING
// `livegraph` in orient's HIGH_COMPLEXITY leaf source set. The orient agent already computed the SQLite
// signal VALUE; this cert is the SET-equivalence PROOF that the LiveGraph corroborates the SAME repo-wide
// complexity facts (current-state), so the `livegraph` member is never minted over un-corroborated
// structure (contract F1/F3). It is NOT a whole-VALUE proof: the rendered `HighComplexityEvidence` carries
// a top-N sample (display names, file paths, ordering) the cert does not compare and `high_complexity` does
// not even expose, so that evidence stays SQLite-built → the leaf is MULTI-source `{livegraph, sqlite}`
// (review-9 gap 2; see `coherent::livegraph_served_is_multi_source`). RED/stale/missing/precondition-unmet
// -> labelled SQLite fallback (no `livegraph` member at all). The cyclomatic facts are the SAME VALUE-JOIN-1
// facts `value_facts(symbol)` exposes per-symbol, read repo-wide — NO new producer, NO new extraction.
// ════════════════════════════════════════════════════════════════════════════════════════════════

/// ORIENT-LIVEGRAPH-IMPL: the in-memory repo-level COMPLEXITY NO-LOSS certificate (mirrors
/// [`CycleNoLossCert`] / [`StatsNoLossCert`]). `verdict == GREEN` iff the LiveGraph repo-wide
/// high-complexity set is field-exact equal to the SQLite `measurements` high-complexity set; `fingerprint`
/// is the SHARED SQLite-free fingerprint it was built at (the invalidation key). NOT durable (rebuilt on
/// restart).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexityNoLossCert {
    /// The repo-wide field-exact compare verdict (`GREEN` / `RED`).
    pub verdict: String,
    /// The SQLite-free fingerprint this verdict was computed at (the invalidation key).
    pub fingerprint: String,
}

/// The shared COMPLEXITY compare: the SQLite `measurements` high-complexity set (key + value, at the agent's
/// default threshold) vs the LiveGraph `high_complexity` set. Returns `Some(true)` iff the two SETS are
/// equal (a `(key, value)`-pair compare catches a missing symbol, an extra symbol, AND a value mismatch).
/// `None` on any storage error -> the caller treats it as NOT GREEN (safe SQLite fallback). Reads SQLite
/// once + the LiveGraph once.
fn complexity_compare_is_exact(repo_state: &RepoState, snapshot_uid: &str) -> Option<bool> {
    use std::collections::BTreeSet;
    let threshold = repo_graph_agent::aggregators::complexity::DEFAULT_COMPLEXITY_THRESHOLD;
    // SQLite FULL high-complexity set: count first, then read the whole set (limit = count) — the compare is
    // over the WHOLE set, not the top-N sample the signal evidence carries.
    // D-S = S-A: one fresh per-operation connection; open failure -> None (safe SQLite fallback).
    let conn = repo_state.storage().ok()?;
    let count = conn
        .count_high_complexity_symbols(snapshot_uid, threshold)
        .ok()?;
    let sqlite_rows = conn
        .query_high_complexity_symbols(snapshot_uid, threshold, count as usize)
        .ok()?;
    let sqlite_set: BTreeSet<(String, u64)> = sqlite_rows
        .into_iter()
        .map(|m| (m.stable_key, m.complexity))
        .collect();
    let lg_set: BTreeSet<(String, u64)> = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            Some(lg) => lg
                .high_complexity(threshold as u32)
                .data()
                .map(|d| {
                    d.symbols
                        .iter()
                        .map(|f| (f.symbol.clone(), f.complexity as u64))
                        .collect()
                })
                .unwrap_or_default(),
            None => BTreeSet::new(),
        }
    };
    Some(sqlite_set == lg_set)
}

/// ORIENT-LIVEGRAPH-IMPL (build): run the field-exact complexity compare -> verdict, STORE the cert keyed
/// by `fingerprint`, return `Some(is_green)` (or `None` if no fingerprint / a storage error -> the caller
/// falls back to SQLite). Mirrors [`build_and_store_cycles_cert`] / [`build_and_store_stats_cert`].
fn build_and_store_complexity_cert(
    repo_state: &RepoState,
    snapshot_uid: &str,
    fingerprint: Option<String>,
) -> Option<bool> {
    let fingerprint = fingerprint?;
    let is_green = complexity_compare_is_exact(repo_state, snapshot_uid)?;
    let verdict = if is_green { "GREEN" } else { "RED" }.to_string();
    *repo_state.complexity_cert.write() = Some(ComplexityNoLossCert {
        verdict,
        fingerprint,
    });
    Some(is_green)
}

/// orient HIGH_COMPLEXITY (repo focus ONLY) leaf decision — mirrors [`orient_cycles_outcome`]. Reuses the
/// `high_complexity` read for the answer-class ladder (Exact + Fresh + TS-only) AND the repo-wide complexity
/// no-loss cert: only returns `Livegraph` (which makes `coherent` INCLUDE `livegraph` in the leaf's
/// multi-source `{livegraph, sqlite}` set) when the answer is servable AND the cert is GREEN at the current
/// fingerprint (proving the LiveGraph repo-wide `(key, complexity)` set == SQLite). Else a labelled SQLite
/// fallback (no `livegraph` member). The LiveGraph read guard is DROPPED before the cert build (which
/// re-reads the livegraph + write-locks the complexity_cert) — never held across it.
pub(crate) fn orient_complexity_outcome(
    repo_state: &RepoState,
    snapshot_uid: &str,
) -> OrientLgOutcome {
    let threshold = repo_graph_agent::aggregators::complexity::DEFAULT_COMPLEXITY_THRESHOLD as u32;
    let (served, current_fp) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            None => {
                return OrientLgOutcome::Fallback {
                    reason: FallbackReason::LiveGraphUnavailable,
                }
            }
            Some(lg) => {
                let env = lg.high_complexity(threshold);
                let fp = import_cert_fingerprint(&lg.live_partitions(), snapshot_uid);
                (orient_outcome_from_env(&env), fp)
            }
        }
    };
    // If the answer-class ladder already failed (non-Exact / stale / non-TS), that is the outcome.
    match &served {
        OrientLgOutcome::Fallback { .. } => return served,
        OrientLgOutcome::Livegraph { .. } => {}
    }
    // The no-loss cert gates the FINAL livegraph label. Stale/missing -> (re)build once per fingerprint.
    let cert_green = {
        let cached = repo_state.complexity_cert.read();
        match cached.as_ref() {
            Some(c) if c.fingerprint == current_fp => Some(c.verdict == "GREEN"),
            _ => None,
        }
    };
    let green = match cert_green {
        Some(g) => g,
        None => build_and_store_complexity_cert(repo_state, snapshot_uid, Some(current_fp))
            .unwrap_or(false),
    };
    if green {
        served
    } else {
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphComplexityDivergence,
        }
    }
}
