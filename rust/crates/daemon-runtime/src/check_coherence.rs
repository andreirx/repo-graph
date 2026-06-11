//! CHECK-LIVEGRAPH-IMPL: assemble check's `CoherenceEnvelope<CoherentOrientResult>` response.
//!
//! The IMPURE adapter (Clean Architecture: mechanism). It reads the daemon `RepoState`'s SQLite storage
//! for the ONE coherence input the pure agent conversion needs — the AUTHORITATIVE stale-index flag — and
//! hands check's bare [`OrientResult`] to the pure [`repo_graph_agent::check_to_coherent`].
//!
//! Unlike orient (`orient_coherence.rs`), check has, by the ratified contract (coherence-layer-1.md check
//! row; verified first-hand in `docs/slices/check-livegraph-1.md`):
//!   - NO LiveGraph read, NO cert, NO fastpath → there is NO `OrientLgDecisions` analogue here.
//!   - NO trust briefing → `check_to_coherent` always sets `trust_briefing = None` (D-CHECK-2).
//!
//! So this adapter is a THIN stale-read + delegate. It lives in its own focused module (NOT appended to
//! the large `dispatch.rs`, ~6700 lines) per the structural guardrail, mirroring `orient_coherence.rs`;
//! `handle_check` just calls [`build_check_envelope`].

use repo_graph_agent::{check_to_coherent, CoherentOrientResult, OrientResult};
use repo_graph_coherence::CoherenceEnvelope;

use crate::state::RepoState;

/// Build check's coherence-wrapped response from the agent's bare [`OrientResult`].
///
/// `result` already has its `display_name` set by the handler. The returned envelope is what the daemon
/// serializes for `rmap check`.
pub(crate) fn build_check_envelope(
    repo_state: &RepoState,
    result: OrientResult,
) -> CoherenceEnvelope<CoherentOrientResult> {
    // `stale` = the backing index is stale. AUTHORITATIVE source: a direct `get_stale_files` read — the
    // SAME budget-/ranking-independent condition orient uses (`orient_coherence.rs`), so the freshness
    // label is faithful regardless of which signals survived ranking. The honesty requirement forbids
    // deriving staleness from a post-budget/truncated signal list.
    //
    // On NO-SNAPSHOT the snapshot uid is empty (check's `run_check` sets it so): there is no index to be
    // stale, and `check_to_coherent` labels the verdict `Unavailable` regardless, so `stale` is irrelevant
    // → pass `false` WITHOUT a storage read (an empty snapshot_uid has no stale-files row).
    let snapshot_uid = result.snapshot.clone();
    let stale = if snapshot_uid.is_empty() {
        false
    } else {
        match repo_state.storage.get_stale_files(&snapshot_uid) {
            Ok(files) => !files.is_empty(),
            // CONSERVATIVE on read error → STALE: a failed stale-files read cannot vouch for freshness, so
            // it degrades rather than minting a false `Fresh` (the codebase Unknown→Stale discipline;
            // `orient_coherence.rs` does the same).
            Err(_) => true,
        }
    };

    check_to_coherent(result, stale)
}

#[cfg(test)]
mod tests {
    use super::build_check_envelope;
    use crate::state::RepoState;
    use repo_graph_agent::{
        CheckConditionEvidence, CheckFailEvidence, CheckIncompleteEvidence, CheckPassEvidence,
        CoherentOrientResult, Confidence, Focus, OrientResult, Signal, SignalCode,
        SnapshotInfoEvidence, CHECK_COMMAND, ORIENT_SCHEMA,
    };
    use repo_graph_coherence::{AnswerClass, CoherenceEnvelope, FreshnessState, Source};
    use repo_graph_storage::types::{
        CreateSnapshotInput, FileVersion, Repo, TrackedFile, UpdateSnapshotStatusInput,
    };
    use repo_graph_storage::StorageConnection;
    use std::collections::BTreeSet;
    use std::path::Path;
    use tempfile::tempdir;

    const REPO: &str = "repo_check_e2e";

    /// Build a minimal SQLite db carrying ONLY what `build_check_envelope` reads: a repo + a ready
    /// snapshot, and (when `with_stale`) one file with a STALE file-version so `get_stale_files` returns
    /// non-empty. No nodes/edges — check's coherence wrapper never reads the call graph. Returns the
    /// snapshot_uid.
    fn build_check_db(dir: &Path, with_stale: bool) -> String {
        let db_path = dir.join("repo.db");
        let mut conn = StorageConnection::open(&db_path).expect("open storage");
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

        if with_stale {
            conn.upsert_files(&[TrackedFile {
                file_uid: "f_stale".to_string(),
                repo_uid: REPO.to_string(),
                path: "src/stale.ts".to_string(),
                language: Some("typescript".to_string()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            }])
            .expect("upsert files");
            conn.upsert_file_versions(&[FileVersion {
                snapshot_uid: snapshot_uid.clone(),
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

        conn.update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: snapshot_uid.clone(),
            status: "ready".to_string(),
            completed_at: None,
        })
        .expect("ready snapshot");

        snapshot_uid
    }

    fn condition(code: &str, status: &str, summary: &str) -> CheckConditionEvidence {
        CheckConditionEvidence {
            code: code.to_string(),
            status: status.to_string(),
            summary: summary.to_string(),
        }
    }

    fn snapshot_info(uid: &str) -> Signal {
        Signal::snapshot_info(SnapshotInfoEvidence {
            snapshot_uid: uid.to_string(),
            scope: "repo".to_string(),
            basis_commit: None,
            created_at: "2026-06-10T00:00:00Z".to_string(),
        })
    }

    /// A snapshot-present check result mirroring `run_check`'s Phase 3 output (verdict + SNAPSHOT_INFO).
    fn check_result(snapshot_uid: &str, verdict: Signal) -> OrientResult {
        OrientResult {
            schema: ORIENT_SCHEMA,
            command: CHECK_COMMAND,
            repo: REPO.to_string(),
            display_name: Some(REPO.to_string()),
            snapshot: snapshot_uid.to_string(),
            focus: Focus::repo(),
            confidence: Confidence::High,
            documentation: None,
            signals: vec![verdict, snapshot_info(snapshot_uid)],
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

    fn pass_signal() -> Signal {
        Signal::check_pass(CheckPassEvidence {
            conditions: vec![condition(
                "GATE_STATUS",
                "pass",
                "No gate policy configured.",
            )],
        })
    }

    fn verdict_leaf(env: &CoherenceEnvelope<CoherentOrientResult>) -> &CoherenceEnvelope<Signal> {
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

    // ── A FRESH index → the daemon reads get_stale_files (empty) → Fresh, multi-source verdict ──

    #[test]
    fn fresh_index_yields_fresh_multi_source_verdict() {
        let dir = tempdir().unwrap();
        let snapshot_uid = build_check_db(dir.path(), false);
        let state = RepoState::open(&dir.path().join("repo.db"), REPO).expect("open repo state");

        let env = build_check_envelope(&state, check_result(&snapshot_uid, pass_signal()));

        assert_eq!(env.freshness, FreshnessState::Fresh);
        assert_eq!(env.trust.class, AnswerClass::Exact);
        let verdict = verdict_leaf(&env);
        assert_eq!(verdict.freshness, FreshnessState::Fresh);
        assert_eq!(
            verdict.provenance.source,
            BTreeSet::from([Source::Sqlite, Source::Declaration]),
            "snapshot-present verdict is multi-source {{sqlite, declaration}}"
        );
        // No LiveGraph read can fail or be partial.
        assert!(env.provenance.fallback_reason.is_none());
        assert!(env.provenance.missing_partitions.is_empty());
        // check never carries a trust briefing.
        assert!(env.value.trust_briefing.is_none());
    }

    // ── A STALE index → the daemon's authoritative get_stale_files read drives Stale freshness ──

    #[test]
    fn stale_index_yields_stale_freshness_via_authoritative_read() {
        let dir = tempdir().unwrap();
        let snapshot_uid = build_check_db(dir.path(), true);
        let state = RepoState::open(&dir.path().join("repo.db"), REPO).expect("open repo state");

        // A FAIL verdict (stale files would fail STALE_FILES in reality); the daemon labels its freshness
        // from the AUTHORITATIVE stale read, independent of the verdict.
        let fail = Signal::check_fail(CheckFailEvidence {
            fail_conditions: vec![condition(
                "STALE_FILES",
                "fail",
                "1 stale files recorded in storage.",
            )],
            passing: vec![],
        });
        let env = build_check_envelope(&state, check_result(&snapshot_uid, fail));

        assert_eq!(
            env.freshness,
            FreshnessState::Stale,
            "the daemon read get_stale_files (non-empty) and labelled the verdict Stale"
        );
        assert_ne!(env.trust.class, AnswerClass::Exact);
        let verdict = verdict_leaf(&env);
        assert_eq!(verdict.freshness, FreshnessState::Stale);
    }

    // ── NO snapshot → no storage read; Unavailable, single-source {sqlite}, no SNAPSHOT_INFO leaf ──

    #[test]
    fn no_snapshot_yields_unavailable_single_source_without_storage_read() {
        let dir = tempdir().unwrap();
        // A db with NO snapshot at all — proves the daemon does not require a stale read on no-snapshot.
        let db_path = dir.path().join("repo.db");
        {
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
        }
        let state = RepoState::open(&db_path, REPO).expect("open repo state");

        // A no-snapshot check result: empty snapshot, ONLY the verdict (no SNAPSHOT_INFO).
        let incomplete = Signal::check_incomplete(CheckIncompleteEvidence {
            incomplete_conditions: vec![condition(
                "SNAPSHOT_EXISTS",
                "incomplete",
                "No READY snapshot. Index the repo first.",
            )],
            fail_conditions: vec![],
            passing: vec![],
        });
        let result = OrientResult {
            schema: ORIENT_SCHEMA,
            command: CHECK_COMMAND,
            repo: REPO.to_string(),
            display_name: Some(REPO.to_string()),
            snapshot: String::new(),
            focus: Focus::repo(),
            confidence: Confidence::Low,
            documentation: None,
            signals: vec![incomplete],
            signals_truncated: None,
            signals_omitted_count: None,
            limits: Vec::new(),
            limits_truncated: None,
            limits_omitted_count: None,
            next: Vec::new(),
            next_truncated: None,
            next_omitted_count: None,
            truncated: false,
        };
        let env = build_check_envelope(&state, result);

        assert_eq!(env.freshness, FreshnessState::Unavailable);
        assert_eq!(env.trust.class, AnswerClass::Unavailable);
        assert_eq!(env.value.confidence, Confidence::Low);
        assert_eq!(
            env.value.signals.len(),
            1,
            "no SNAPSHOT_INFO leaf on no-snapshot"
        );
        let verdict = verdict_leaf(&env);
        assert_eq!(verdict.provenance.source, BTreeSet::from([Source::Sqlite]));
    }
}
