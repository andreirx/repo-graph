//! CHECK-LIVEGRAPH-IMPL: assemble check's `CoherenceEnvelope<CoherentOrientResult>` response.
//!
//! PURE policy (Clean Architecture: high-level policy, no I/O). Mirrors orient's
//! [`crate::dto::coherent::to_coherent`] but for check's STRUCTURALLY SIMPLER shape. The differences are
//! load-bearing and verified first-hand in `docs/slices/check-livegraph-1.md`:
//!
//!   - **ZERO LiveGraph-first leaves.** check touches none of the migrated SQLite-free surfaces, so it
//!     builds NO cert and runs NO fastpath (D-CHECK-4). There is no `OrientLgDecisions` analogue; every
//!     leaf is SQLite / Authority. `provenance.fallback_reason` is therefore ALWAYS `None` and
//!     `missing_partitions` ALWAYS empty (no LiveGraph read can fail or be partial).
//!   - **NO trust briefing.** `handle_check` injects no `trust` overlay, so `trust_briefing` is ALWAYS
//!     `None` on the shared container (D-CHECK-2 / orient W3 sibling-non-crossing).
//!   - **Always ≥ 1 signal.** check ALWAYS emits the verdict signal (even no-snapshot emits exactly
//!     `CHECK_INCOMPLETE`), so the zero-signal resolution-only carve-out orient needs does NOT arise here.
//!
//! check's WHOLE coherence contribution is the honest 2-axis labelling (spec §3c):
//!   - the verdict is ONE MULTI-SOURCE composite leaf (D-CHECK-1 = Option A): its `value` (the `CHECK_*`
//!     `Signal`, conditions nested in its evidence) stays PRISTINE; its `provenance.source` is the honest
//!     contributing-source SET (D-CHECK-5 / contract D8) — `{sqlite, declaration}` snapshot-present (the
//!     gate ALWAYS reads the `declarations` Authority table, even when NotConfigured — spec §1b
//!     PROVENANCE NOTE), `{sqlite}` on no-snapshot (the gate is never evaluated).
//!   - the optional `SNAPSHOT_INFO` leaf is single-source `{sqlite}`.
//!   - freshness is the SNAPSHOT freshness, computed INDEPENDENTLY of the Pass/Fail/Incomplete verdict
//!     (the 2-axis model): `Fresh` (snapshot, no stale files) | `Stale` (stale files) | `Unavailable`
//!     (no snapshot). "A PASS over a Stale snapshot is a Stale PASS, never a Fresh PASS."
//!
//! The shared `repo-graph-coherence` MEET folds the root; confidence is the MEET-derived band capped at
//! the legacy `derive_repo_confidence` value (D-CHECK-3). check's verdict logic (`evaluate`/`reduce`) is
//! UNTOUCHED — this layer WRAPS the answer, it does not re-judge it.

use std::collections::BTreeSet;

use repo_graph_coherence::{
    fold_parts, AnswerClass, CoherenceEnvelope, FreshnessState, Provenance, QueryCompleteness,
    Source, TrustPosture,
};

use crate::dto::coherent::{confidence_from_posture, min_confidence, CoherentOrientResult};
use crate::dto::envelope::OrientResult;
use crate::dto::signal::{Signal, SignalCode};

/// Convert check's bare [`OrientResult`] into the coherence wrapper
/// `CoherenceEnvelope<CoherentOrientResult>`.
///
/// `stale` = whether the backing index is stale (`get_stale_files` non-empty). It is only meaningful when
/// a snapshot exists; on no-snapshot the verdict leaf is `Unavailable` regardless of `stale`. The daemon
/// supplies `stale` from an AUTHORITATIVE storage read (NOT a post-budget/truncated signal), so the
/// freshness label is faithful (the honesty requirement: never mint a false `Fresh`).
///
/// SNAPSHOT PRESENCE is derived from `result.snapshot.is_empty()`: check's `run_check` sets the snapshot
/// field to the uid when a READY snapshot exists and to `""` otherwise (both from the same `snapshot_opt`),
/// so the empty string is the authoritative in-band no-snapshot marker. A real snapshot uid is never empty.
pub fn check_to_coherent(
    result: OrientResult,
    stale: bool,
) -> CoherenceEnvelope<CoherentOrientResult> {
    let snapshot_present = !result.snapshot.is_empty();

    // The snapshot freshness is the SINGLE freshness unit in check's SQLite-only world: every input
    // (snapshot identity, stale-files, trust-core, gate) is computed over the SAME snapshot_uid. It is
    // INDEPENDENT of the verdict (the 2-axis model, spec §3c) so the label stays honest if the verdict
    // logic or input sources ever change.
    let snapshot_freshness = if !snapshot_present {
        FreshnessState::Unavailable
    } else if stale {
        FreshnessState::Stale
    } else {
        FreshnessState::Fresh
    };

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

    // ── Wrap each signal as a leaf. ──────────────────────────────
    let mut leaves: Vec<CoherenceEnvelope<Signal>> = Vec::with_capacity(signals.len());
    for signal in signals {
        let leaf = match signal.code() {
            // The verdict: ONE multi-source composite leaf (D-CHECK-1).
            SignalCode::CheckPass | SignalCode::CheckFail | SignalCode::CheckIncomplete => {
                verdict_leaf(signal, snapshot_present, snapshot_freshness)
            }
            // SNAPSHOT_INFO is emitted ONLY when a snapshot exists; single-source SQLite, snapshot posture.
            SignalCode::SnapshotInfo => CoherenceEnvelope::sqlite_leaf(signal, stale),
            // Defensive: check emits no other signal codes today. Treat any future addition as the proven
            // SQLite primary rather than silently mislabelling it.
            _ => CoherenceEnvelope::sqlite_leaf(signal, stale),
        };
        leaves.push(leaf);
    }

    // ── Fold the root from the leaves (MEET; monotone — can only LOWER). ──
    let (provenance, trust, freshness) = fold_parts(&leaves);

    // ── Confidence = MEET-derived, capped ≤ the legacy value (D-CHECK-3 / E1). ──
    // The legacy `derive_repo_confidence` result (or the static `Low` on no-snapshot) becomes ONE input to
    // the MEET, never the sole source; the coherent confidence never exceeds the weakest contributor.
    let coherent_confidence = min_confidence(confidence, confidence_from_posture(&trust));

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
        // check has NO daemon trust overlay (D-CHECK-2): the field stays absent on the wire.
        trust_briefing: None,
        // D5 (IMPL-2) next-action is an orient/stats surface; check never renders it.
        relationship_next_action: None,
        // METRIC-LANG-COVERAGE-1 coverage is an orient complexity surface; check never renders it.
        measurement_coverage: None,
    };

    CoherenceEnvelope::new(value, provenance, trust, freshness)
}

/// Build the verdict leaf: ONE multi-source composite leaf (D-CHECK-1 = Option A). The inner `Signal`
/// payload (with its conditions nested in evidence) stays UN-widened; provenance/trust/freshness ride in
/// the wrapper siblings.
///
/// `provenance.source` is the honest contributing-source SET (D-CHECK-5 / contract D8). It keys off
/// SNAPSHOT PRESENCE, not the gate outcome: when a snapshot exists `gather_gate_outcome` ALWAYS reads the
/// `declarations` Authority table via `get_active_requirements` — even when it returns empty
/// (`NotConfigured`) — so `declaration` contributes to EVERY snapshot-present verdict (spec §1b PROVENANCE
/// NOTE). On no-snapshot the gate is never evaluated, so the only source is `sqlite`.
fn verdict_leaf(
    signal: Signal,
    snapshot_present: bool,
    freshness: FreshnessState,
) -> CoherenceEnvelope<Signal> {
    let provenance = if snapshot_present {
        Provenance::multi([Source::Sqlite, Source::Declaration])
    } else {
        Provenance::sqlite()
    };
    let trust = verdict_posture(signal.code(), freshness);
    CoherenceEnvelope::new(signal, provenance, trust, freshness)
}

/// The verdict leaf's trust posture — the internal MEET of its conditions projected onto the coherence
/// axes (spec §3a). A Pass/Fail verdict is fully EVALUABLE → `Exact` under a Fresh snapshot (capped by
/// freshness otherwise). An INCOMPLETE verdict (a required condition could not be evaluated) is NEVER
/// `Exact`: it is `Partial` (Fresh) / `Stale` / `Unavailable` with `Degraded`/`Unknown` completeness.
///
/// This is a HAND-BUILT [`TrustPosture`]: the `repo-graph-coherence` crate documents `TrustPosture` as a
/// non-invariant-constructed projection for a non-LiveGraph leaf (cf. `TrustPosture::resolution_only`,
/// which is likewise a hand-built `Partial` with empty reasons justified by context). Two deliberate,
/// honesty-driven choices:
///   - `degradation_reasons` is EMPTY. The [`repo_graph_coherence::DegradationReason`] enum is the
///     SCIP/extraction-substrate vocabulary (AnonymousStructuralMember, ScipFallbackIdentity, …); NONE of
///     its variants honestly describes a check READINESS verdict, which touches no SCIP state. The SPECIFIC
///     incompleteness reason is carried in the PRISTINE verdict signal's nested conditions (e.g. "Gate
///     incomplete: missing evidence", "No READY snapshot. Index the repo first."). Mislabelling check with
///     an extraction reason would be a WORSE honesty violation than the empty set — and no false
///     completeness is possible regardless, since `Exact` requires `Complete` AND `Fresh`, which an
///     INCOMPLETE verdict never has.
///   - `contributing_languages` is EMPTY. check is not language-partition-scoped — its verdict folds
///     SQLite/Authority facts, not a LiveGraph language partition (cf. `snapshot_exact`/`snapshot_stale`).
fn verdict_posture(code: SignalCode, freshness: FreshnessState) -> TrustPosture {
    // A Pass/Fail verdict evaluated every applicable condition; an INCOMPLETE could not (reduce.rs:
    // Incomplete iff ≥ 1 condition Incomplete).
    let evaluable = matches!(code, SignalCode::CheckPass | SignalCode::CheckFail);

    // The class ceiling comes from freshness; an evaluable verdict reaches it, an INCOMPLETE drops below.
    let class = match freshness {
        FreshnessState::Fresh => {
            if evaluable {
                AnswerClass::Exact
            } else {
                AnswerClass::Partial
            }
        }
        // Defensive: check has no PrecisionPending source (it never reads the SCIP-dependent LiveGraph),
        // so this arm is unreachable today; cap conservatively at Partial (never Exact-under-PP).
        FreshnessState::PrecisionPending => AnswerClass::Partial,
        FreshnessState::Stale | FreshnessState::RefreshFailed => AnswerClass::Stale,
        FreshnessState::Unavailable => AnswerClass::Unavailable,
    };

    let completeness = match class {
        AnswerClass::Exact => QueryCompleteness::Complete,
        AnswerClass::Partial | AnswerClass::Stale => QueryCompleteness::Degraded,
        AnswerClass::Unavailable => QueryCompleteness::Unknown,
    };

    TrustPosture {
        class,
        completeness,
        degradation_reasons: Vec::new(),
        contributing_languages: BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::envelope::{Confidence, Focus, CHECK_COMMAND, ORIENT_SCHEMA};
    use crate::dto::signal::{
        CheckConditionEvidence, CheckFailEvidence, CheckIncompleteEvidence, CheckPassEvidence,
        SnapshotInfoEvidence,
    };

    // ── Builders ───────────────────────────────────────────────

    fn condition(code: &str, status: &str, summary: &str) -> CheckConditionEvidence {
        CheckConditionEvidence {
            code: code.to_string(),
            status: status.to_string(),
            summary: summary.to_string(),
        }
    }

    fn pass_signal() -> Signal {
        Signal::check_pass(CheckPassEvidence {
            conditions: vec![
                condition("SNAPSHOT_EXISTS", "pass", "READY snapshot available."),
                condition("GATE_STATUS", "pass", "No gate policy configured."),
            ],
        })
    }

    fn fail_signal(fail_code: &str, fail_summary: &str) -> Signal {
        Signal::check_fail(CheckFailEvidence {
            fail_conditions: vec![condition(fail_code, "fail", fail_summary)],
            passing: vec![condition(
                "SNAPSHOT_EXISTS",
                "pass",
                "READY snapshot available.",
            )],
        })
    }

    fn incomplete_signal(inc_code: &str, inc_summary: &str) -> Signal {
        Signal::check_incomplete(CheckIncompleteEvidence {
            incomplete_conditions: vec![condition(inc_code, "incomplete", inc_summary)],
            fail_conditions: vec![],
            passing: vec![],
        })
    }

    fn snapshot_info() -> Signal {
        Signal::snapshot_info(SnapshotInfoEvidence {
            snapshot_uid: "snap-1".to_string(),
            scope: "repo".to_string(),
            basis_commit: None,
            created_at: "2026-06-10T00:00:00Z".to_string(),
        })
    }

    /// A snapshot-present check `OrientResult` (verdict + SNAPSHOT_INFO), mirroring `run_check`'s Phase 3.
    fn snapshot_present_result(verdict: Signal, confidence: Confidence) -> OrientResult {
        base_result(
            "snap-1".to_string(),
            vec![verdict, snapshot_info()],
            confidence,
        )
    }

    /// A no-snapshot check `OrientResult`: empty snapshot, ONLY the verdict signal, static `Low`.
    fn no_snapshot_result() -> OrientResult {
        base_result(
            String::new(),
            vec![incomplete_signal(
                "SNAPSHOT_EXISTS",
                "No READY snapshot. Index the repo first.",
            )],
            Confidence::Low,
        )
    }

    fn base_result(snapshot: String, signals: Vec<Signal>, confidence: Confidence) -> OrientResult {
        OrientResult {
            schema: ORIENT_SCHEMA,
            command: CHECK_COMMAND,
            repo: "demo".to_string(),
            display_name: Some("demo".to_string()),
            snapshot,
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

    fn verdict_leaf_of(
        env: &CoherenceEnvelope<CoherentOrientResult>,
    ) -> &CoherenceEnvelope<Signal> {
        env.value
            .signals
            .iter()
            .find(|l| {
                matches!(
                    l.value.code(),
                    SignalCode::CheckPass | SignalCode::CheckFail | SignalCode::CheckIncomplete
                )
            })
            .expect("verdict leaf present")
    }

    // ── D-V1 / D-V1b: PASS over a FRESH snapshot — multi-source verdict, Fresh, Exact ──

    #[test]
    fn pass_fresh_verdict_leaf_is_multi_source_and_fresh() {
        let env = check_to_coherent(
            snapshot_present_result(pass_signal(), Confidence::High),
            false,
        );

        // Exactly two leaves: the verdict + SNAPSHOT_INFO.
        assert_eq!(env.value.signals.len(), 2);

        let verdict = verdict_leaf_of(&env);
        // D-CHECK-5 / D8: the verdict leaf carries the honest multi-source set {sqlite, declaration}.
        assert_eq!(
            verdict.provenance.source,
            BTreeSet::from([Source::Sqlite, Source::Declaration]),
            "snapshot-present verdict folds sqlite-operational + sqlite-trust-core + declaration-authority"
        );
        assert_eq!(verdict.freshness, FreshnessState::Fresh);
        assert_eq!(verdict.trust.class, AnswerClass::Exact);
        // E3: no LiveGraph read => never a fallback reason, never a missing partition.
        assert!(verdict.provenance.fallback_reason.is_none());
        assert!(verdict.provenance.missing_partitions.is_empty());

        // The SNAPSHOT_INFO leaf is single-source {sqlite}.
        let snap = env
            .value
            .signals
            .iter()
            .find(|l| l.value.code() == SignalCode::SnapshotInfo)
            .expect("snapshot-info leaf");
        assert_eq!(snap.provenance.source, BTreeSet::from([Source::Sqlite]));

        // Root: Exact / Fresh; provenance UNION = {sqlite, declaration}; confidence unchanged High.
        assert_eq!(env.trust.class, AnswerClass::Exact);
        assert_eq!(env.freshness, FreshnessState::Fresh);
        assert_eq!(
            env.provenance.source,
            BTreeSet::from([Source::Sqlite, Source::Declaration])
        );
        assert_eq!(env.value.confidence, Confidence::High);
        // E5: check never carries a trust briefing.
        assert!(env.value.trust_briefing.is_none());
    }

    #[test]
    fn pass_with_not_configured_gate_still_carries_declaration_source() {
        // D-V1b: a PASS whose gate is NotConfigured STILL reads the declarations table (get_active_
        // requirements) — so the source set is {sqlite, declaration}, NOT {sqlite}. The provenance keys off
        // snapshot presence, not the gate outcome.
        let pass = Signal::check_pass(CheckPassEvidence {
            conditions: vec![condition(
                "GATE_STATUS",
                "pass",
                "No gate policy configured.",
            )],
        });
        let env = check_to_coherent(snapshot_present_result(pass, Confidence::High), false);
        let verdict = verdict_leaf_of(&env);
        assert_eq!(
            verdict.provenance.source,
            BTreeSet::from([Source::Sqlite, Source::Declaration])
        );
    }

    // ── D-V2: FAIL caused by stale files — Stale freshness ──

    #[test]
    fn fail_from_stale_files_is_fail_at_stale() {
        let env = check_to_coherent(
            snapshot_present_result(
                fail_signal("STALE_FILES", "2 stale files recorded in storage."),
                Confidence::Medium,
            ),
            true, // stale index
        );
        let verdict = verdict_leaf_of(&env);
        assert_eq!(verdict.freshness, FreshnessState::Stale);
        assert_eq!(verdict.trust.class, AnswerClass::Stale);
        // Root MEET is Stale; confidence capped below High.
        assert_eq!(env.freshness, FreshnessState::Stale);
        assert_ne!(env.trust.class, AnswerClass::Exact);
        assert_ne!(env.value.confidence, Confidence::High);
    }

    // ── D-V3: FAIL from a gate violation over a FRESH snapshot — freshness independent of verdict ──

    #[test]
    fn gate_only_fail_over_fresh_snapshot_is_fail_at_fresh() {
        let env = check_to_coherent(
            snapshot_present_result(fail_signal("GATE_STATUS", "Gate fails."), Confidence::High),
            false, // no stale files
        );
        let verdict = verdict_leaf_of(&env);
        // The freshness axis is INDEPENDENT of the verdict: a gate-only FAIL over a fresh snapshot is
        // FAIL@Fresh (distinct from the stale-files FAIL@Stale of D-V2).
        assert_eq!(verdict.freshness, FreshnessState::Fresh);
        assert_eq!(env.freshness, FreshnessState::Fresh);
        // A FAIL evaluated every condition => the verdict leaf is Exact at Fresh (the verdict VALUE says
        // FAIL; the trust posture says the verdict itself is a complete, current answer).
        assert_eq!(verdict.trust.class, AnswerClass::Exact);
    }

    // ── D-V4: INCOMPLETE from no snapshot — single-source, Unavailable, static Low, no SNAPSHOT_INFO ──

    #[test]
    fn no_snapshot_is_incomplete_unavailable_single_source() {
        let env = check_to_coherent(no_snapshot_result(), false);

        // Exactly ONE leaf (the verdict); NO SNAPSHOT_INFO leaf.
        assert_eq!(env.value.signals.len(), 1);
        assert!(env
            .value
            .signals
            .iter()
            .all(|l| l.value.code() != SignalCode::SnapshotInfo));

        let verdict = verdict_leaf_of(&env);
        assert_eq!(verdict.value.code(), SignalCode::CheckIncomplete);
        // {sqlite} only — the gate was NOT evaluated (no snapshot), so no declaration source.
        assert_eq!(verdict.provenance.source, BTreeSet::from([Source::Sqlite]));
        assert_eq!(verdict.freshness, FreshnessState::Unavailable);
        assert_ne!(verdict.trust.class, AnswerClass::Exact);

        // Root: Unavailable (≠ empty — a reasoned INCOMPLETE@Unavailable, not an empty PASS); static Low.
        assert_eq!(env.freshness, FreshnessState::Unavailable);
        assert_eq!(env.trust.class, AnswerClass::Unavailable);
        assert_eq!(env.value.confidence, Confidence::Low);
        assert_eq!(env.provenance.source, BTreeSet::from([Source::Sqlite]));
    }

    // ── D-V5: INCOMPLETE over an EXISTING snapshot (empty index) — freshness reflects the snapshot ──

    #[test]
    fn incomplete_over_existing_snapshot_keeps_snapshot_freshness() {
        let env = check_to_coherent(
            snapshot_present_result(
                incomplete_signal("INDEX_NOT_EMPTY", "Snapshot has zero indexed files."),
                Confidence::Low,
            ),
            false, // snapshot exists and is fresh
        );
        let verdict = verdict_leaf_of(&env);
        // The snapshot EXISTS, so freshness is Fresh (NOT Unavailable — that is the no-snapshot case).
        assert_eq!(verdict.freshness, FreshnessState::Fresh);
        // INCOMPLETE is never Exact: Partial / Degraded over the fresh snapshot.
        assert_eq!(verdict.trust.class, AnswerClass::Partial);
        assert_eq!(verdict.trust.completeness, QueryCompleteness::Degraded);
        // The snapshot-present INCOMPLETE verdict STILL reads declarations => multi-source.
        assert_eq!(
            verdict.provenance.source,
            BTreeSet::from([Source::Sqlite, Source::Declaration])
        );
    }

    // ── E1: confidence never exceeds the legacy value, and the MEET is monotone ──

    #[test]
    fn confidence_never_exceeds_legacy() {
        // Legacy Medium + a Fresh/Exact MEET → coherent stays Medium (capped at legacy).
        let env = check_to_coherent(
            snapshot_present_result(pass_signal(), Confidence::Medium),
            false,
        );
        assert_eq!(env.value.confidence, Confidence::Medium);
    }

    #[test]
    fn stale_meet_caps_confidence_below_legacy() {
        // Legacy High but a Stale MEET → coherent drops to Low (the MEET lowers it).
        let env = check_to_coherent(
            snapshot_present_result(
                fail_signal("STALE_FILES", "1 stale file recorded in storage."),
                Confidence::High,
            ),
            true,
        );
        assert_eq!(env.value.confidence, Confidence::Low);
    }

    // ── E2/E3: no fold manufactures Exact; provenance never claims a LiveGraph fallback ──

    #[test]
    fn no_leaf_or_root_ever_claims_a_livegraph_source_or_fallback() {
        for (result, stale) in [
            (
                snapshot_present_result(pass_signal(), Confidence::High),
                false,
            ),
            (
                snapshot_present_result(fail_signal("STALE_FILES", "stale"), Confidence::Medium),
                true,
            ),
            (no_snapshot_result(), false),
        ] {
            let env = check_to_coherent(result, stale);
            assert!(!env.provenance.source.contains(&Source::Livegraph));
            assert!(env.provenance.fallback_reason.is_none());
            assert!(env.provenance.missing_partitions.is_empty());
            for leaf in &env.value.signals {
                assert!(!leaf.provenance.source.contains(&Source::Livegraph));
                assert!(leaf.provenance.fallback_reason.is_none());
                assert!(leaf.trust.contributing_languages.is_empty());
            }
        }
    }

    // ── P1: the inner Signal VALUE payloads stay byte-identical (pristine) ──

    #[test]
    fn inner_signal_value_is_pristine() {
        let verdict = pass_signal();
        let before = serde_json::to_value(&verdict).unwrap();
        let env = check_to_coherent(snapshot_present_result(verdict, Confidence::High), false);
        let leaf = verdict_leaf_of(&env);
        let after = serde_json::to_value(&leaf.value).unwrap();
        assert_eq!(
            before, after,
            "the wrapper must not widen the Signal payload"
        );
    }

    // ── E5: trust_briefing is ALWAYS absent from check's wire shape ──

    #[test]
    fn trust_briefing_absent_in_serialized_wire_shape() {
        let env = check_to_coherent(
            snapshot_present_result(pass_signal(), Confidence::High),
            false,
        );
        let json = serde_json::to_value(&env).unwrap();
        assert!(
            json["value"].get("trust_briefing").is_none(),
            "check never emits a trust_briefing key (skip_serializing_if None)"
        );
    }
}
