//! TRUST-LIVEGRAPH-IMPL: assemble trust's `CoherenceEnvelope<CoherentTrustReport>` (the ratified hybrid).
//!
//! The IMPURE adapter (Clean Architecture: mechanism), mirroring `orient_coherence.rs` /
//! `check_coherence.rs` / `explain_coherence.rs`. It reads the daemon `RepoState` (the in-memory LiveGraph +
//! SQLite) and supplies the pure [`repo_graph_trust::trust_to_coherent`] with:
//!   - the AUTHORITATIVE stale-index flag (`get_stale_files`) for the Half-B snapshot freshness, and
//!   - the Half-A current-state posture leaf, GENUINELY SERVED from the LiveGraph (REAL serving, operator
//!     2026-06-08 — NOT a re-labelled SQLite trust result).
//!
//! Half A is a PROJECTION of EXISTING LiveGraph runtime state (D-TRUST-2, the anti-Option-B guard — it does
//! NOT recompute v1 reliability levels and introduces NO new producer):
//!   - residency / per-partition freshness / language / producer fingerprint from `live_partitions()`;
//!   - the leaf's POSTURE (class / completeness / freshness / contributing-language union) projected VERBATIM
//!     from the repo-wide current-state `module_stats()` `AnswerEnvelope` (a real read over the in-memory IR
//!     — the same surface the migrated `stats` answer serves), via `TrustPosture::from_answer`;
//!   - `producer_available` = no `ProducerUnavailable` degradation on that answer;
//!   - `migrated_answer_capability` = that answer is `Exact` + `Fresh` (the LiveGraph can serve exact
//!     structural answers right now).
//!
//! A cold / non-resident LiveGraph yields the `Unavailable` posture leaf (F3 — unknown, NOT a Fresh
//! known-zero); Half B is still fully served and labelled. The root folds by MEET over both halves.
//!
//! A SEPARATE focused module (not appended to the ~6900-line `dispatch.rs`) per the structural guardrail;
//! `handle_trust` just calls [`build_trust_envelope`]. The `livegraph` source on the posture leaf is a real
//! current-state read, never a relabel of a SQLite-built value.

use repo_graph_coherence::{
    AnswerClass, CoherenceEnvelope, DegradationReason, FreshnessState, TrustPosture,
};
use repo_graph_trust::types::TrustReport;
use repo_graph_trust::{
    trust_to_coherent, CoherentTrustReport, LiveGraphPartitionPosture, LiveGraphPosture,
};

use crate::state::RepoState;

/// Build trust's coherence-wrapped response from the assembled v1 [`TrustReport`].
///
/// `report` already has its `display_name` set by the handler. The returned envelope is what the daemon
/// serializes for `rmap trust`.
pub(crate) fn build_trust_envelope(
    repo_state: &RepoState,
    report: TrustReport,
) -> CoherenceEnvelope<CoherentTrustReport> {
    // `stale` = the backing index is stale. AUTHORITATIVE source: a direct `get_stale_files` read — the
    // SAME budget-/ranking-independent condition orient/check/explain use, so the Half-B freshness label is
    // faithful. CONSERVATIVE on read error -> STALE (a failed read cannot vouch for freshness). The snapshot
    // gate in `handle_trust` guarantees a ready snapshot before this point, so the uid is non-empty.
    let stale = match repo_state.storage.get_stale_files(&report.snapshot_uid) {
        Ok(files) => !files.is_empty(),
        Err(_) => true,
    };

    let posture = build_posture_leaf(repo_state);
    trust_to_coherent(report, posture, stale)
}

/// Build the Half-A current-state posture leaf from REAL LiveGraph runtime state (D-TRUST-2). Cold /
/// non-resident -> the `Unavailable` posture leaf (F3). Resident -> the posture PROJECTED from the repo-wide
/// `module_stats()` answer + the per-partition `live_partitions()` rows. NO new producer: read-only.
fn build_posture_leaf(repo_state: &RepoState) -> CoherenceEnvelope<LiveGraphPosture> {
    let guard = repo_state.livegraph.read();
    let Some(lg) = guard.as_ref() else {
        return LiveGraphPosture::unavailable_leaf();
    };

    let partitions = lg.live_partitions();
    if partitions.is_empty() {
        // A LiveGraph with zero resident partitions is, for the posture, indistinguishable from cold.
        return LiveGraphPosture::unavailable_leaf();
    }

    // The repo-wide current-state structural answer — a REAL read over the in-memory IR (the same surface the
    // migrated `stats` answer serves). Its trust axes ARE the current per-answer reliability posture.
    let env = lg.module_stats();
    let posture_trust = TrustPosture::from_answer(&env);
    let posture_freshness = env.freshness();

    let producer_available = !env
        .degradation_reasons()
        .contains(&DegradationReason::ProducerUnavailable);
    let migrated_answer_capability =
        env.class() == AnswerClass::Exact && env.freshness() == FreshnessState::Fresh;

    let partition_rows: Vec<LiveGraphPartitionPosture> = partitions
        .iter()
        .map(|p| LiveGraphPartitionPosture {
            partition_id: p.id.clone(),
            // The partition snapshot exposes a coarse `fresh` bool; project it Fresh/Stale (the leaf-level
            // MEET freshness, from the repo-wide answer above, is the authoritative wrapper-sibling value).
            freshness: if p.fresh {
                FreshnessState::Fresh
            } else {
                FreshnessState::Stale
            },
            typescript_primary: p.ts,
            producer_fingerprint: p.producer_fingerprint.clone(),
        })
        .collect();

    LiveGraphPosture {
        resident: true,
        partitions: partition_rows,
        producer_available,
        migrated_answer_capability,
    }
    .into_leaf(posture_trust, posture_freshness)
}

#[cfg(test)]
mod tests {
    use super::build_trust_envelope;
    use crate::state::RepoState;
    use repo_graph_coherence::{AnswerClass, FreshnessState, Source};
    use repo_graph_livegraph::LiveGraph;
    use repo_graph_livegraph_feed::feed_partition;
    use repo_graph_scip_ingest::{decode_index, ingest_partition, IngestOutcome};
    use repo_graph_storage::types::{CreateSnapshotInput, Repo, UpdateSnapshotStatusInput};
    use repo_graph_storage::StorageConnection;
    use repo_graph_trust::types::{
        DowngradeTrigger, ReliabilityAxisScore, ReliabilityLevel, TrustDowngrades,
        TrustReliability, TrustReport, TrustSummary,
    };
    use repo_graph_trust_model::LanguageSupport;
    use std::collections::BTreeSet;
    use std::path::Path;
    use tempfile::tempdir;

    const REPO: &str = "repo_trust_e2e";

    /// Build a minimal SQLite db: a repo + a ready snapshot (no nodes/edges — the posture reads the
    /// LiveGraph, not SQLite). Returns the snapshot_uid.
    fn build_db(dir: &Path) -> String {
        let db_path = dir.join("repo.db");
        let conn = StorageConnection::open(&db_path).expect("open storage");
        conn.add_repo(&Repo {
            repo_uid: REPO.to_string(),
            name: REPO.to_string(),
            root_path: ".".to_string(),
            default_branch: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            metadata_json: None,
        })
        .expect("add repo");
        let snap = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: REPO.to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot");
        let snapshot_uid = snap.snapshot_uid;
        conn.update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: snapshot_uid.clone(),
            status: "ready".to_string(),
            completed_at: None,
        })
        .expect("ready snapshot");
        snapshot_uid
    }

    fn axis() -> ReliabilityAxisScore {
        ReliabilityAxisScore {
            level: ReliabilityLevel::HIGH,
            reasons: vec![],
        }
    }

    fn downgrade() -> DowngradeTrigger {
        DowngradeTrigger {
            triggered: false,
            reasons: vec![],
        }
    }

    /// A minimal v1 report (the handler would assemble this from SQLite; here we hand it in to exercise the
    /// adapter's posture + labelling).
    fn minimal_report(snapshot_uid: &str) -> TrustReport {
        TrustReport {
            snapshot_uid: snapshot_uid.to_string(),
            display_name: Some(REPO.to_string()),
            basis_commit: None,
            toolchain: None,
            diagnostics_version: Some(1),
            summary: TrustSummary {
                edges_total: 10,
                edges_resolved: 10,
                unresolved_total: 2,
                resolved_calls: 5,
                unresolved_calls: 1,
                unresolved_calls_external: 0,
                unresolved_calls_internal_like: 1,
                call_resolution_rate: 0.83,
                reliability: TrustReliability {
                    import_graph: axis(),
                    call_graph: axis(),
                    dead_code: axis(),
                    change_impact: axis(),
                },
                triggered_downgrades: TrustDowngrades {
                    framework_heavy_suspicion: downgrade(),
                    registry_pattern_suspicion: downgrade(),
                    missing_entrypoint_declarations: downgrade(),
                    alias_resolution_suspicion: downgrade(),
                },
            },
            categories: vec![],
            classifications: vec![],
            unknown_calls_blast_radius: None,
            enrichment_status: None,
            modules: vec![],
            caveats: vec![],
            diagnostics_available: true,
            enrichment_eligible_count: 0,
        }
    }

    /// Ingest the committed synthetic SCIP fixture (producer-free; the SAME fixture orient/explain e2e use).
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

    // ── COLD LiveGraph: Half-A posture Unavailable; Half B still served; root degraded (D-T1/D-T6) ──

    #[test]
    fn cold_livegraph_yields_unavailable_posture_and_served_half_b() {
        let dir = tempdir().unwrap();
        let snapshot_uid = build_db(dir.path());
        let state = RepoState::open(&dir.path().join("repo.db"), REPO).expect("open repo state");
        // No preload -> livegraph is None.

        let env = build_trust_envelope(&state, minimal_report(&snapshot_uid));

        // Half A: Unavailable, livegraph-sourced, resident=false (F3 — not a Fresh known-zero).
        let posture = &env.value.current_state_posture;
        assert_eq!(
            posture.provenance.source,
            BTreeSet::from([Source::Livegraph])
        );
        assert_eq!(posture.trust.class, AnswerClass::Unavailable);
        assert_eq!(posture.freshness, FreshnessState::Unavailable);
        assert!(!posture.value.resident);

        // Half B fully served + Fresh + sqlite-labelled (the v1 report is available, honestly labelled).
        assert_eq!(env.value.reliability.freshness, FreshnessState::Fresh);
        assert_eq!(
            env.value.reliability.provenance.source,
            BTreeSet::from([Source::Sqlite])
        );
        // The downgrades leaf is multi-source {sqlite, declaration} (D-TRUST-4).
        assert_eq!(
            env.value.triggered_downgrades.provenance.source,
            BTreeSet::from([Source::Sqlite, Source::Declaration])
        );

        // Root MEET: a cold LiveGraph degrades the overall posture even over a Fresh snapshot (D-T6).
        assert_eq!(env.freshness, FreshnessState::Unavailable);
        assert_eq!(env.trust.class, AnswerClass::Unavailable);
    }

    // ── WARM LiveGraph: Half-A posture is GENUINELY projected from the resident synthetic partition ──

    #[test]
    fn warm_livegraph_projects_a_real_current_state_posture() {
        let outcome = synthetic_outcome();
        let mut lg = LiveGraph::new();
        feed_partition(
            &mut lg,
            "synthetic",
            outcome,
            LanguageSupport::TypeScriptPrimary,
        );

        let dir = tempdir().unwrap();
        let snapshot_uid = build_db(dir.path());
        let state = RepoState::open(&dir.path().join("repo.db"), REPO).expect("open repo state");
        *state.livegraph.write() = Some(lg);

        let env = build_trust_envelope(&state, minimal_report(&snapshot_uid));

        let posture = &env.value.current_state_posture;
        // The posture is livegraph-sourced and resident, with the real synthetic TS partition.
        assert_eq!(
            posture.provenance.source,
            BTreeSet::from([Source::Livegraph])
        );
        assert!(posture.value.resident);
        assert!(
            !posture.value.partitions.is_empty(),
            "the resident synthetic partition must appear in the posture"
        );
        assert!(
            posture
                .value
                .partitions
                .iter()
                .any(|p| p.typescript_primary),
            "the synthetic partition is TypeScript-primary"
        );
        assert!(
            posture
                .value
                .partitions
                .iter()
                .all(|p| !p.producer_fingerprint.is_empty()),
            "a resident partition carries its producer fingerprint"
        );
        // A single resident, fresh, TS partition -> Exact/Fresh current-state posture -> capable, producer OK.
        assert_eq!(posture.trust.class, AnswerClass::Exact);
        assert_eq!(posture.freshness, FreshnessState::Fresh);
        assert!(posture.value.producer_available);
        assert!(posture.value.migrated_answer_capability);

        // Root: a Fresh LiveGraph posture + a Fresh snapshot -> Exact/Fresh root; provenance UNION carries
        // all three sources (livegraph + sqlite + declaration).
        assert_eq!(env.trust.class, AnswerClass::Exact);
        assert_eq!(env.freshness, FreshnessState::Fresh);
        assert_eq!(
            env.provenance.source,
            BTreeSet::from([Source::Livegraph, Source::Sqlite, Source::Declaration])
        );
    }
}
