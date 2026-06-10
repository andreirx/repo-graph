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

use repo_graph_agent::AgentStorageRead;
use repo_graph_trust_model::{
    AnswerClass, AnswerEnvelope, DegradationReason, FreshnessState, Granularity, LanguageSupport,
    QueryCompleteness,
};

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
/// storage error so this stays decoupled from the concrete `AgentStorageError`.
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

/// orient CALLERS_SUMMARY (symbol focus) leaf decision — reuse the migrated `callers` surface (the Exact +
/// Fresh + TS-only ladder is the PRECONDITION) AND prove value-equivalence (the per-symbol no-loss key
/// compare vs SQLite `find_symbol_callers` is the SUFFICIENT proof). When referencing partitions are
/// non-resident the LiveGraph answer is non-Exact -> the ladder falls back BEFORE the compare (never an
/// Exact module-grouped summary from partition-only data).
pub(crate) fn orient_callers_outcome(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
) -> OrientLgOutcome {
    let (ladder, lg_keys) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            None => {
                return OrientLgOutcome::Fallback {
                    reason: FallbackReason::LiveGraphUnavailable,
                }
            }
            Some(lg) => {
                let env = lg.callers(target, Granularity::CallerDetail);
                let keys: std::collections::BTreeSet<String> = env
                    .data()
                    .map(|d| d.caller_identities.iter().map(|(_, k)| k.clone()).collect())
                    .unwrap_or_default();
                (orient_outcome_from_env(&env), keys)
            }
        }
    };
    gate_callgraph_no_loss(ladder, lg_keys, || {
        repo_state
            .storage
            .find_symbol_callers(snapshot_uid, target)
            .map(|rows| rows.into_iter().map(|r| r.stable_key).collect())
    })
}

/// orient CALLEES_SUMMARY (symbol focus) leaf decision — reuse the migrated `callees` surface + the
/// per-symbol no-loss key compare vs SQLite `find_symbol_callees`. The summary-callees residency asymmetry
/// (a non-resident defining partition) surfaces as a non-Exact answer -> the ladder falls back BEFORE the
/// compare (the ratified common callees path until the deferral lifts; never Exact-empty).
pub(crate) fn orient_callees_outcome(
    repo_state: &RepoState,
    target: &str,
    snapshot_uid: &str,
) -> OrientLgOutcome {
    let (ladder, lg_keys) = {
        let guard = repo_state.livegraph.read();
        match guard.as_ref() {
            None => {
                return OrientLgOutcome::Fallback {
                    reason: FallbackReason::LiveGraphUnavailable,
                }
            }
            Some(lg) => {
                let env = lg.callees(target, Granularity::CallerDetail);
                let keys: std::collections::BTreeSet<String> = env
                    .data()
                    .map(|d| d.callee_identities.iter().map(|(k, _)| k.clone()).collect())
                    .unwrap_or_default();
                (orient_outcome_from_env(&env), keys)
            }
        }
    };
    gate_callgraph_no_loss(ladder, lg_keys, || {
        repo_state
            .storage
            .find_symbol_callees(snapshot_uid, target)
            .map(|rows| rows.into_iter().map(|r| r.stable_key).collect())
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
    let count = repo_state
        .storage
        .count_high_complexity_symbols(snapshot_uid, threshold)
        .ok()?;
    let sqlite_rows = repo_state
        .storage
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── ORIENT-LIVEGRAPH-IMPL: the orient LG-leaf decision ladder (orient_outcome_from_env). ──
    // Pure over an AnswerEnvelope (no RepoState): proves the Fresh -> Exact -> TS-only gating and the
    // posture projection that the orient cycles/callers/callees leaf decisions rely on.

    fn ts_set() -> std::collections::BTreeSet<LanguageSupport> {
        std::collections::BTreeSet::from([LanguageSupport::TypeScriptPrimary])
    }

    #[test]
    fn orient_outcome_exact_fresh_ts_is_livegraph() {
        let env = AnswerEnvelope::exact(
            1u32,
            QueryCompleteness::Complete,
            FreshnessState::Fresh,
            vec![],
            ts_set(),
        )
        .unwrap();
        match orient_outcome_from_env(&env) {
            OrientLgOutcome::Livegraph {
                class,
                completeness,
                freshness,
                contributing_languages,
                ..
            } => {
                assert_eq!(class, AnswerClass::Exact);
                assert_eq!(completeness, QueryCompleteness::Complete);
                assert_eq!(freshness, FreshnessState::Fresh);
                assert_eq!(contributing_languages, ts_set());
            }
            OrientLgOutcome::Fallback { .. } => panic!("expected Livegraph for Exact+Fresh+TS"),
        }
    }

    #[test]
    fn orient_outcome_partial_falls_back_partial() {
        let env = AnswerEnvelope::partial(
            Some(1u32),
            vec![DegradationReason::ScipFallbackIdentity],
            vec![],
            FreshnessState::Fresh,
            vec![],
            ts_set(),
        )
        .unwrap();
        assert!(matches!(
            orient_outcome_from_env(&env),
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphPartial
            }
        ));
    }

    #[test]
    fn orient_outcome_stale_falls_back_stale_before_class() {
        // Freshness is checked BEFORE class, so a Stale answer reports LiveGraphStale (not Partial).
        let env = AnswerEnvelope::stale(
            1u32,
            FreshnessState::Stale,
            vec![],
            vec![],
            vec![],
            ts_set(),
        )
        .unwrap();
        assert!(matches!(
            orient_outcome_from_env(&env),
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphStale
            }
        ));
    }

    #[test]
    fn orient_outcome_non_ts_falls_back_unsupported_language() {
        let langs = std::collections::BTreeSet::from([LanguageSupport::RustPartialBeta]);
        let env = AnswerEnvelope::exact(
            1u32,
            QueryCompleteness::Complete,
            FreshnessState::Fresh,
            vec![],
            langs,
        )
        .unwrap();
        assert!(matches!(
            orient_outcome_from_env(&env),
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphUnsupportedLanguage
            }
        ));
    }

    // ── ORIENT-LIVEGRAPH-IMPL: the callers/callees VALUE-EQUIVALENCE proof (gate_callgraph_no_loss). ──
    // PURE over a ladder outcome + the LG key set + a SQLite-keys closure. A panicking closure proves the
    // already-fallback path SKIPS the SQLite read; set-equality is order-independent; any divergence or
    // read error NEVER yields a `livegraph` label.

    fn lg_ladder() -> OrientLgOutcome {
        OrientLgOutcome::Livegraph {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            freshness: FreshnessState::Fresh,
            degradation_reasons: vec![],
            contributing_languages: ts_set(),
        }
    }

    fn keyset(keys: &[&str]) -> std::collections::BTreeSet<String> {
        keys.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn callgraph_no_loss_equal_sets_keeps_livegraph() {
        // LG key set == SQLite key set (order-independent) -> the proven `livegraph` label is kept.
        let out = gate_callgraph_no_loss(lg_ladder(), keyset(&["a", "b"]), || {
            Ok::<_, ()>(keyset(&["b", "a"]))
        });
        assert!(matches!(out, OrientLgOutcome::Livegraph { .. }));
    }

    #[test]
    fn callgraph_no_loss_divergent_sets_falls_back_callgraph_divergence() {
        // A divergence (SQLite has `c`, LiveGraph has `b`) -> labelled SQLite fallback, never `livegraph`.
        let out = gate_callgraph_no_loss(lg_ladder(), keyset(&["a", "b"]), || {
            Ok::<_, ()>(keyset(&["a", "c"]))
        });
        assert!(matches!(
            out,
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphCallgraphDivergence
            }
        ));
    }

    #[test]
    fn callgraph_no_loss_storage_error_falls_back_error() {
        // Cannot prove equivalence (SQLite read errored) -> safe SQLite fallback, never `livegraph`.
        let out = gate_callgraph_no_loss(lg_ladder(), keyset(&["a"]), || {
            Err::<std::collections::BTreeSet<String>, _>(())
        });
        assert!(matches!(
            out,
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphError
            }
        ));
    }

    #[test]
    fn callgraph_no_loss_already_fallback_skips_sqlite_read() {
        // The ladder already fell back (non-resident partition) -> the SQLite compare read is SKIPPED.
        let ladder = OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphStale,
        };
        let out = gate_callgraph_no_loss(
            ladder,
            keyset(&["a"]),
            || -> Result<std::collections::BTreeSet<String>, ()> {
                panic!(
                    "SQLite caller/callee keys must NOT be read when the ladder already fell back"
                )
            },
        );
        assert!(matches!(
            out,
            OrientLgOutcome::Fallback {
                reason: FallbackReason::LiveGraphStale
            }
        ));
    }
}

// ════════════════════════════════════════════════════════════════════════════════════════════════
// ORIENT-LIVEGRAPH-IMPL (review-6 pt2): daemon-half LG-SERVED end-to-end proof.
//
// Builds an in-process daemon `RepoState` with a REAL LiveGraph (the committed `synthetic/index.scip`,
// ingested producer-FREE — no scip-typescript at test time) plus a SQLite that MIRRORS the LiveGraph
// caller/callee key sets, and proves that orient's per-leaf decision resolves to `Livegraph` for ALL FOUR
// LG-first signals — and that the assembled `CoherenceEnvelope<CoherentOrientResult>` carries the leaves
// as `livegraph` (cycles) / `{livegraph, sqlite}` (callers/callees, review-6 pt3). The producer being
// unavailable is exactly the case the reviewer named: the LiveGraph is hand-populated, the certs seeded.
//
// Split of proof:
//   - CALLERS_SUMMARY / CALLEES_SUMMARY: NO cert — the no-loss GATE reads SQLite `find_symbol_callers`/
//     `find_symbol_callees` directly, so the SQLite is mirrored from the LiveGraph answer → the gate is
//     GENUINELY GREEN (a real value-equivalence proof, not a stub).
//   - IMPORT_CYCLES / HIGH_COMPLEXITY: cert-gated — the cycles/complexity no-loss certs are SEEDED GREEN at
//     the live fingerprint to ISOLATE the orient label wiring (GREEN cert + Exact LG → livegraph). The cert
//     COMPARE itself (LG == SQLite → GREEN/RED) is unit-tested by the cycles fastpath + complexity cert
//     tests; here we prove the outcome function consumes a GREEN cert into a `livegraph` label.
// ════════════════════════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod orient_lg_served_e2e {
    use super::{
        orient_callees_outcome, orient_callers_outcome, orient_complexity_outcome,
        orient_cycles_outcome, ComplexityNoLossCert, OrientLgOutcome,
    };
    // `import_cert_fingerprint` + `CycleNoLossCert` stay in the shared feed module (review-7 pt2 refactor).
    use crate::livegraph_feed::{import_cert_fingerprint, CycleNoLossCert};
    use crate::state::RepoState;
    use repo_graph_agent::{
        CalleesSummaryEvidence, CallersSummaryEvidence, Confidence, Focus, HighComplexityEvidence,
        ImportCyclesEvidence, OrientResult, Signal, SignalCode, SnapshotInfoEvidence,
        ORIENT_COMMAND, ORIENT_SCHEMA,
    };
    use repo_graph_coherence::Source;
    use repo_graph_ir::EdgeType;
    use repo_graph_livegraph::LiveGraph;
    use repo_graph_livegraph_feed::feed_partition;
    use repo_graph_scip_ingest::{decode_index, ingest_partition, IngestOutcome};
    use repo_graph_storage::types::{
        CreateSnapshotInput, FileVersion, GraphEdge, GraphNode, Repo, TrackedFile,
        UpdateSnapshotStatusInput,
    };
    use repo_graph_storage::StorageConnection;
    use repo_graph_trust_model::{AnswerClass, FreshnessState, Granularity, LanguageSupport};
    use std::collections::{BTreeSet, HashMap};
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    const REPO: &str = "repo_orient_e2e";

    /// Ingest the committed synthetic SCIP fixture (producer-free; the SAME fixture `feed_real_index.rs`
    /// uses). NOT run through scip-typescript — the `.scip` is committed.
    fn synthetic_outcome() -> IngestOutcome {
        let root = format!(
            "{}/../repo-graph-scip-ingest/tests/fixtures/synthetic",
            env!("CARGO_MANIFEST_DIR")
        );
        let scip = std::fs::read(format!("{root}/index.scip")).expect("read committed index.scip");
        let index = decode_index(&scip).expect("decode scip");
        ingest_partition(
            &index,
            &root,
            "synthetic",
            "synthetic",
            "scip-typescript",
            "0.4.0",
            "h",
            "",
        )
    }

    /// The endpoints of the first real `Calls` edge: `(caller_key, callee_key)`.
    fn find_calls_edge(outcome: &IngestOutcome) -> (String, String) {
        outcome
            .ir
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Calls)
            .map(|e| (e.src.as_str().to_string(), e.dst.as_str().to_string()))
            .expect("at least one Calls edge in the synthetic fixture")
    }

    /// Build a SQLite db carrying ONLY the call-graph the no-loss gate reads: a repo + a ready snapshot +
    /// SYMBOL nodes + `CALLS` edges for `calls` (`(caller_key, callee_key)`). Returns `(db_path,
    /// snapshot_uid)`. Uses ONLY the public storage write API.
    fn build_db_with_calls(
        dir: &Path,
        repo_uid: &str,
        calls: &[(String, String)],
    ) -> (PathBuf, String) {
        let db_path = dir.join("repo.db");
        let mut conn = StorageConnection::open(&db_path).expect("open storage");
        conn.add_repo(&Repo {
            repo_uid: repo_uid.to_string(),
            name: repo_uid.to_string(),
            root_path: ".".to_string(),
            default_branch: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            metadata_json: None,
        })
        .expect("add repo");
        let snap = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo_uid.to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot");
        let snapshot_uid = snap.snapshot_uid;

        // Distinct node keys -> unique node_uids.
        let mut uid_of: HashMap<String, String> = HashMap::new();
        let mut nodes: Vec<GraphNode> = Vec::new();
        for (a, b) in calls {
            for k in [a, b] {
                if !uid_of.contains_key(k) {
                    let uid = format!("n{}", uid_of.len());
                    uid_of.insert(k.clone(), uid.clone());
                    nodes.push(GraphNode {
                        node_uid: uid,
                        snapshot_uid: snapshot_uid.clone(),
                        repo_uid: repo_uid.to_string(),
                        stable_key: k.clone(),
                        kind: "SYMBOL".to_string(),
                        subtype: Some("FUNCTION".to_string()),
                        name: k.clone(),
                        qualified_name: None,
                        file_uid: None,
                        parent_node_uid: None,
                        location: None,
                        signature: None,
                        visibility: None,
                        doc_comment: None,
                        metadata_json: None,
                    });
                }
            }
        }
        if !nodes.is_empty() {
            conn.insert_nodes(&nodes).expect("insert nodes");
        }

        // Distinct CALLS edges (source/target by node_uid).
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        let mut edges: Vec<GraphEdge> = Vec::new();
        for (a, b) in calls {
            if seen.insert((a.clone(), b.clone())) {
                edges.push(GraphEdge {
                    edge_uid: format!("e{}", edges.len()),
                    snapshot_uid: snapshot_uid.clone(),
                    repo_uid: repo_uid.to_string(),
                    source_node_uid: uid_of[a].clone(),
                    target_node_uid: uid_of[b].clone(),
                    edge_type: "CALLS".to_string(),
                    resolution: "resolved".to_string(),
                    extractor: "test".to_string(),
                    location: None,
                    metadata_json: None,
                });
            }
        }
        if !edges.is_empty() {
            conn.insert_edges(&edges).expect("insert edges");
        }

        conn.update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: snapshot_uid.clone(),
            status: "ready".to_string(),
            completed_at: None,
        })
        .expect("ready snapshot");

        (db_path, snapshot_uid)
    }

    /// A fully-wired fixture: a `RepoState` with the synthetic LiveGraph + a SQLite mirroring its
    /// caller/callee key sets. `_dir` is held to keep the db file alive for the test's lifetime.
    struct Fixture {
        _dir: tempfile::TempDir,
        state: RepoState,
        src: String,
        dst: String,
        ks_callers: Vec<String>,
        ks_callees: Vec<String>,
        snapshot_uid: String,
    }

    fn setup() -> Fixture {
        let outcome = synthetic_outcome();
        let (src, dst) = find_calls_edge(&outcome);
        let mut lg = LiveGraph::new();
        feed_partition(
            &mut lg,
            "synthetic",
            outcome,
            LanguageSupport::TypeScriptPrimary,
        );

        // The single resident TS partition makes intra-partition callers/callees Exact (precondition for
        // the orient ladder to reach the no-loss gate).
        let callers_env = lg.callers(&dst, Granularity::CallerDetail);
        assert_eq!(
            callers_env.class(),
            AnswerClass::Exact,
            "callers(dst) Exact precondition over the resident synthetic partition"
        );
        let ks_callers: Vec<String> = callers_env
            .data()
            .expect("callers data")
            .caller_identities
            .iter()
            .map(|(_, k)| k.clone())
            .collect();
        let callees_env = lg.callees(&src, Granularity::CallerDetail);
        assert_eq!(
            callees_env.class(),
            AnswerClass::Exact,
            "callees(src) Exact precondition over the resident synthetic partition"
        );
        let ks_callees: Vec<String> = callees_env
            .data()
            .expect("callees data")
            .callee_identities
            .iter()
            .map(|(k, _)| k.clone())
            .collect();

        // SQLite mirrors BOTH key sets: (k -> dst) for callers, (src -> k) for callees.
        let mut calls: Vec<(String, String)> = ks_callers
            .iter()
            .map(|k| (k.clone(), dst.clone()))
            .collect();
        for k in &ks_callees {
            calls.push((src.clone(), k.clone()));
        }
        let dir = tempdir().unwrap();
        let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &calls);
        let state = RepoState::open(&db_path, REPO).expect("open repo state");
        *state.livegraph.write() = Some(lg);

        Fixture {
            _dir: dir,
            state,
            src,
            dst,
            ks_callers,
            ks_callees,
            snapshot_uid,
        }
    }

    /// Seed the cycles + complexity no-loss certs GREEN at the LIVE fingerprint (isolates the cert-gated
    /// label wiring; the genuine compare is unit-tested elsewhere).
    fn seed_certs_green(state: &RepoState, snapshot_uid: &str) {
        let fp = {
            let guard = state.livegraph.read();
            let lg = guard.as_ref().expect("livegraph set");
            import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
        };
        *state.cycles_cert.write() = Some(CycleNoLossCert {
            verdict: "GREEN".to_string(),
            fingerprint: fp.clone(),
        });
        *state.complexity_cert.write() = Some(ComplexityNoLossCert {
            verdict: "GREEN".to_string(),
            fingerprint: fp,
        });
    }

    /// Seed ONLY the complexity no-loss cert at the LIVE fingerprint with the given verdict — for the
    /// cert-divergence label test (a RED cert at the matching fingerprint short-circuits the genuine compare,
    /// isolating the "RED -> labelled SQLite fallback" wiring; the real GREEN/RED compare is unit-tested by
    /// `complexity_compare_is_exact`).
    fn seed_complexity_cert(state: &RepoState, snapshot_uid: &str, verdict: &str) {
        let fp = {
            let guard = state.livegraph.read();
            let lg = guard.as_ref().expect("livegraph set");
            import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
        };
        *state.complexity_cert.write() = Some(ComplexityNoLossCert {
            verdict: verdict.to_string(),
            fingerprint: fp,
        });
    }

    /// A repo-focus HIGH_COMPLEXITY signal value (the agent-built SQLite evidence; the daemon decides only
    /// its LABEL). Field values are cosmetic for the label tests.
    fn high_complexity_signal() -> Signal {
        Signal::high_complexity(HighComplexityEvidence {
            high_complexity_count: 0,
            threshold: repo_graph_agent::aggregators::complexity::DEFAULT_COMPLEXITY_THRESHOLD,
            top_complex: Vec::new(),
        })
    }

    /// Insert a FILE with a STALE file-version for `snapshot_uid`, so `get_stale_files` returns non-empty —
    /// the AUTHORITATIVE stale condition `build_orient_envelope` reads from storage (review-9 gap 1),
    /// independent of which signals survived ranking/budget. Reopens the db the fixture already created.
    fn insert_stale_file(db_path: &Path, repo_uid: &str, snapshot_uid: &str) {
        let mut conn = StorageConnection::open(db_path).expect("reopen storage");
        conn.upsert_files(&[TrackedFile {
            file_uid: "f_stale".to_string(),
            repo_uid: repo_uid.to_string(),
            path: "src/stale.ts".to_string(),
            language: Some("typescript".to_string()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        }])
        .expect("upsert files");
        conn.upsert_file_versions(&[FileVersion {
            snapshot_uid: snapshot_uid.to_string(),
            file_uid: "f_stale".to_string(),
            content_hash: "h".to_string(),
            ast_hash: None,
            extractor: None,
            parse_status: "stale".to_string(),
            size_bytes: None,
            line_count: None,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
        }])
        .expect("upsert file versions");
    }

    fn orient_result(
        repo: &str,
        snapshot_uid: &str,
        focus: Focus,
        signals: Vec<Signal>,
    ) -> OrientResult {
        OrientResult {
            schema: ORIENT_SCHEMA,
            command: ORIENT_COMMAND,
            repo: repo.to_string(),
            display_name: None,
            snapshot: snapshot_uid.to_string(),
            focus,
            confidence: Confidence::High,
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

    // ── The four per-leaf DAEMON DECISIONS resolve to Livegraph ──

    #[test]
    fn orient_callers_outcome_serves_livegraph_when_sqlite_matches() {
        let f = setup();
        assert!(f.ks_callers.contains(&f.src), "src is a caller of dst");
        match orient_callers_outcome(&f.state, &f.dst, &f.snapshot_uid) {
            OrientLgOutcome::Livegraph {
                class,
                contributing_languages,
                ..
            } => {
                assert_eq!(class, AnswerClass::Exact);
                assert!(contributing_languages.contains(&LanguageSupport::TypeScriptPrimary));
            }
            OrientLgOutcome::Fallback { reason } => {
                panic!("expected Livegraph callers, got fallback {reason:?}")
            }
        }
    }

    #[test]
    fn orient_callees_outcome_serves_livegraph_when_sqlite_matches() {
        let f = setup();
        assert!(f.ks_callees.contains(&f.dst), "dst is a callee of src");
        match orient_callees_outcome(&f.state, &f.src, &f.snapshot_uid) {
            OrientLgOutcome::Livegraph { class, .. } => assert_eq!(class, AnswerClass::Exact),
            OrientLgOutcome::Fallback { reason } => {
                panic!("expected Livegraph callees, got fallback {reason:?}")
            }
        }
    }

    #[test]
    fn orient_cycles_outcome_serves_livegraph_with_green_cert() {
        let f = setup();
        {
            let guard = f.state.livegraph.read();
            let lg = guard.as_ref().unwrap();
            assert_eq!(
                lg.module_import_cycles().class(),
                AnswerClass::Exact,
                "module cycles Exact precondition over the resident partition"
            );
        }
        seed_certs_green(&f.state, &f.snapshot_uid);
        match orient_cycles_outcome(&f.state, &f.snapshot_uid) {
            OrientLgOutcome::Livegraph { .. } => {}
            OrientLgOutcome::Fallback { reason } => {
                panic!("expected Livegraph cycles, got fallback {reason:?}")
            }
        }
    }

    #[test]
    fn orient_complexity_outcome_serves_livegraph_with_green_cert() {
        let f = setup();
        let threshold =
            repo_graph_agent::aggregators::complexity::DEFAULT_COMPLEXITY_THRESHOLD as u32;
        {
            let guard = f.state.livegraph.read();
            let lg = guard.as_ref().unwrap();
            assert_eq!(
                lg.high_complexity(threshold).class(),
                AnswerClass::Exact,
                "high_complexity Exact precondition over the resident partition"
            );
        }
        seed_certs_green(&f.state, &f.snapshot_uid);
        match orient_complexity_outcome(&f.state, &f.snapshot_uid) {
            OrientLgOutcome::Livegraph { .. } => {}
            OrientLgOutcome::Fallback { reason } => {
                panic!("expected Livegraph complexity, got fallback {reason:?}")
            }
        }
    }

    // ── The assembled CoherenceEnvelope carries the leaves with the right SOURCE set ──

    #[test]
    fn build_orient_envelope_symbol_focus_callers_leaf_is_multi_source() {
        let f = setup();
        let result = orient_result(
            REPO,
            &f.snapshot_uid,
            Focus::symbol(&f.dst, &f.dst, None),
            vec![Signal::callers_summary(CallersSummaryEvidence {
                count: f.ks_callers.len() as u64,
                top_modules: Vec::new(),
            })],
        );
        let env = crate::orient_coherence::build_orient_envelope(&f.state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::CallersSummary)
            .expect("callers leaf present");
        assert_eq!(
            leaf.provenance.source,
            BTreeSet::from([Source::Livegraph, Source::Sqlite]),
            "assembled CALLERS_SUMMARY leaf is multi-source {{livegraph, sqlite}} (review-6 pt3)"
        );
        assert!(leaf.provenance.fallback_reason.is_none());

        // The serialized rmapd wire shape shows BOTH sources on the leaf.
        let json = serde_json::to_value(&env).unwrap();
        let leaf_json = json["value"]["signals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["value"]["code"] == "CALLERS_SUMMARY")
            .unwrap();
        let sources = leaf_json["provenance"]["source"].as_array().unwrap();
        assert!(sources.iter().any(|s| s == "livegraph"));
        assert!(sources.iter().any(|s| s == "sqlite"));
    }

    #[test]
    fn build_orient_envelope_symbol_focus_callees_leaf_is_multi_source() {
        let f = setup();
        let result = orient_result(
            REPO,
            &f.snapshot_uid,
            Focus::symbol(&f.src, &f.src, None),
            vec![Signal::callees_summary(CalleesSummaryEvidence {
                count: f.ks_callees.len() as u64,
                top_modules: Vec::new(),
            })],
        );
        let env = crate::orient_coherence::build_orient_envelope(&f.state, REPO, result);
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
    }

    #[test]
    fn build_orient_envelope_repo_focus_cycles_leaf_is_livegraph() {
        let f = setup();
        seed_certs_green(&f.state, &f.snapshot_uid);
        let result = orient_result(
            REPO,
            &f.snapshot_uid,
            Focus::repo(),
            vec![Signal::import_cycles(ImportCyclesEvidence {
                cycle_count: 0,
                cycles: Vec::new(),
            })],
        );
        let env = crate::orient_coherence::build_orient_envelope(&f.state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::ImportCycles)
            .expect("cycles leaf present");
        assert_eq!(
            leaf.provenance.source,
            BTreeSet::from([Source::Livegraph]),
            "assembled IMPORT_CYCLES leaf is single-source livegraph (field-exact cert)"
        );
        // The root provenance union now includes livegraph (the LG-served path is reached end-to-end).
        assert!(env.provenance.source.contains(&Source::Livegraph));
    }

    #[test]
    fn build_orient_envelope_emits_producer_unavailable_limit_without_livegraph() {
        // review-6 pt1 (E5), integration level: a repo-focus orient that emits an LG-first signal but has
        // NO populated LiveGraph -> the leaf falls back (LiveGraphUnavailable) AND the assembled envelope
        // gains the machine-discoverable PRODUCER_UNAVAILABLE limit (through the real build_orient_envelope).
        let dir = tempdir().unwrap();
        let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
        let state = RepoState::open(&db_path, REPO).expect("open repo state");
        // `state.livegraph` is None (never preloaded).
        let result = orient_result(
            REPO,
            &snapshot_uid,
            Focus::repo(),
            vec![Signal::import_cycles(ImportCyclesEvidence {
                cycle_count: 1,
                cycles: Vec::new(),
            })],
        );
        let env = crate::orient_coherence::build_orient_envelope(&state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::ImportCycles)
            .expect("cycles leaf present");
        assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert_eq!(
            leaf.provenance.fallback_reason,
            Some(repo_graph_coherence::CoherenceFallbackReason::LiveGraphUnavailable)
        );
        assert!(
            env.value
                .limits
                .iter()
                .any(|l| l.code == repo_graph_agent::LimitCode::ProducerUnavailable),
            "envelope gains PRODUCER_UNAVAILABLE when an LG-first leaf has no LiveGraph"
        );
    }

    // ── review-9 gap 2: HIGH_COMPLEXITY through build_orient_envelope (the reviewer's required coverage) ──

    #[test]
    fn build_orient_envelope_repo_focus_complexity_leaf_is_multi_source() {
        // Green LiveGraph path -> correct provenance WITHOUT a false single-source claim. With the complexity
        // no-loss cert GREEN at the live fingerprint and an Exact high_complexity answer, the assembled
        // HIGH_COMPLEXITY leaf is multi-source {livegraph, sqlite} (the cert corroborates the (key,
        // complexity) SET; the rendered top-N sample stays SQLite-built) — never single-source `livegraph`.
        let f = setup();
        seed_certs_green(&f.state, &f.snapshot_uid);
        let result = orient_result(
            REPO,
            &f.snapshot_uid,
            Focus::repo(),
            vec![high_complexity_signal()],
        );
        let env = crate::orient_coherence::build_orient_envelope(&f.state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::HighComplexity)
            .expect("complexity leaf present");
        assert_eq!(
            leaf.provenance.source,
            BTreeSet::from([Source::Livegraph, Source::Sqlite]),
            "assembled HIGH_COMPLEXITY leaf is multi-source {{livegraph, sqlite}} (review-9 gap 2)"
        );
        assert!(leaf.provenance.fallback_reason.is_none());
        // The root provenance union reaches livegraph end-to-end.
        assert!(env.provenance.source.contains(&Source::Livegraph));

        // The serialized rmapd wire shape shows BOTH sources on the leaf — never a bare `livegraph` claim.
        let json = serde_json::to_value(&env).unwrap();
        let leaf_json = json["value"]["signals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["value"]["code"] == "HIGH_COMPLEXITY")
            .unwrap();
        let sources = leaf_json["provenance"]["source"].as_array().unwrap();
        assert!(sources.iter().any(|s| s == "livegraph"));
        assert!(sources.iter().any(|s| s == "sqlite"));
    }

    #[test]
    fn build_orient_envelope_complexity_no_livegraph_falls_back_unavailable() {
        // No LiveGraph -> the SQLite primary, labelled LiveGraphUnavailable, PRODUCER_UNAVAILABLE surfaced.
        // NEVER a `livegraph` claim (the false-provenance risk review-9 flagged is impossible here).
        let dir = tempdir().unwrap();
        let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
        let state = RepoState::open(&db_path, REPO).expect("open repo state");
        // `state.livegraph` is None (never preloaded).
        let result = orient_result(
            REPO,
            &snapshot_uid,
            Focus::repo(),
            vec![high_complexity_signal()],
        );
        let env = crate::orient_coherence::build_orient_envelope(&state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::HighComplexity)
            .expect("complexity leaf present");
        assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert!(!leaf.provenance.source.contains(&Source::Livegraph));
        assert_eq!(
            leaf.provenance.fallback_reason,
            Some(repo_graph_coherence::CoherenceFallbackReason::LiveGraphUnavailable)
        );
        assert!(
            env.value
                .limits
                .iter()
                .any(|l| l.code == repo_graph_agent::LimitCode::ProducerUnavailable),
            "envelope gains PRODUCER_UNAVAILABLE when HIGH_COMPLEXITY has no LiveGraph"
        );
    }

    #[test]
    fn build_orient_envelope_complexity_cert_divergence_falls_back() {
        // Cert divergence -> labelled SQLite fallback. The high_complexity answer is Exact, but the complexity
        // no-loss cert is RED at the live fingerprint (LG set != SQLite) -> the SQLite primary, labelled
        // LiveGraphComplexityDivergence. NEVER a `livegraph` claim.
        let f = setup();
        seed_complexity_cert(&f.state, &f.snapshot_uid, "RED");
        let result = orient_result(
            REPO,
            &f.snapshot_uid,
            Focus::repo(),
            vec![high_complexity_signal()],
        );
        let env = crate::orient_coherence::build_orient_envelope(&f.state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::HighComplexity)
            .expect("complexity leaf present");
        assert_eq!(leaf.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert!(!leaf.provenance.source.contains(&Source::Livegraph));
        assert_eq!(
            leaf.provenance.fallback_reason,
            Some(repo_graph_coherence::CoherenceFallbackReason::LiveGraphComplexityDivergence)
        );
    }

    // ── review-9 gap 1: the authoritative stale flag (storage, not the emitted signal list) ──

    #[test]
    fn build_orient_envelope_stale_index_marks_leaves_stale_without_trust_signal() {
        // review-9 gap 1 regression: `stale` is derived from `get_stale_files` (storage), NOT from the
        // presence of TRUST_STALE_SNAPSHOT in the (ranked + budget-truncated) emitted signals. Here the index
        // IS stale but the result carries NO TRUST_STALE_SNAPSHOT signal (simulating truncation / a focus that
        // omits it). The SQLite leaf + the root MUST still be Stale and SQLITE_SNAPSHOT_STALE must fire —
        // proving a missing trust signal can no longer mint a false Fresh/Exact.
        let dir = tempdir().unwrap();
        let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
        insert_stale_file(&db_path, REPO, &snapshot_uid);
        let state = RepoState::open(&db_path, REPO).expect("open repo state");

        let result = orient_result(
            REPO,
            &snapshot_uid,
            Focus::repo(),
            vec![Signal::snapshot_info(SnapshotInfoEvidence {
                snapshot_uid: snapshot_uid.clone(),
                scope: "repo".to_string(),
                basis_commit: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            })],
        );
        // Fixture precondition: the emitted signals do NOT contain the (unreliable) TRUST_STALE_SNAPSHOT proxy.
        assert!(
            !result
                .signals
                .iter()
                .any(|s| s.code() == SignalCode::TrustStaleSnapshot),
            "fixture: no TRUST_STALE_SNAPSHOT signal is emitted"
        );

        let env = crate::orient_coherence::build_orient_envelope(&state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::SnapshotInfo)
            .expect("snapshot-info leaf present");
        assert_eq!(
            leaf.freshness,
            FreshnessState::Stale,
            "SQLite leaf is Stale from the storage-authoritative flag despite no TRUST_STALE_SNAPSHOT signal"
        );
        assert_eq!(
            env.freshness,
            FreshnessState::Stale,
            "root freshness is Stale"
        );
        assert_ne!(
            env.trust.class,
            AnswerClass::Exact,
            "root trust is never Exact over a stale index"
        );
        assert!(
            env.value
                .limits
                .iter()
                .any(|l| l.code == repo_graph_agent::LimitCode::SqliteSnapshotStale),
            "SQLITE_SNAPSHOT_STALE fires from the authoritative stale flag"
        );
    }

    #[test]
    fn build_orient_envelope_fresh_index_keeps_leaves_fresh() {
        // The complement: a non-stale index (no stale file-versions) keeps the SQLite leaf + root Fresh and
        // does NOT fire SQLITE_SNAPSHOT_STALE — proving the authoritative read does not over-report staleness.
        let dir = tempdir().unwrap();
        let (db_path, snapshot_uid) = build_db_with_calls(dir.path(), REPO, &[]);
        let state = RepoState::open(&db_path, REPO).expect("open repo state");
        let result = orient_result(
            REPO,
            &snapshot_uid,
            Focus::repo(),
            vec![Signal::snapshot_info(SnapshotInfoEvidence {
                snapshot_uid: snapshot_uid.clone(),
                scope: "repo".to_string(),
                basis_commit: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            })],
        );
        let env = crate::orient_coherence::build_orient_envelope(&state, REPO, result);
        let leaf = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::SnapshotInfo)
            .expect("snapshot-info leaf present");
        assert_eq!(leaf.freshness, FreshnessState::Fresh);
        assert_eq!(env.freshness, FreshnessState::Fresh);
        assert!(
            !env.value
                .limits
                .iter()
                .any(|l| l.code == repo_graph_agent::LimitCode::SqliteSnapshotStale),
            "SQLITE_SNAPSHOT_STALE must NOT fire on a fresh index"
        );
    }
}
