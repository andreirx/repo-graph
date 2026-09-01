//! `dead_causes` handler — DEAD-CAUSES-1.
//!
//! Serves the FACTS from which `rmap dead`'s refusal derives its "Root causes"
//! section, read from the reader's OWN current snapshot. The disable decision and
//! exit code 2 are frozen elsewhere (`rgr/src/commands/dead.rs`); this arm exists so
//! the refusal stops transcribing the stale 2026-04 "Missing framework detectors
//! (Spring, React, …)" text and instead states what the snapshot proves.
//!
//! # What it reads (all verified from code, not names)
//!
//! - **Framework liveness** — `list_inferences_for_snapshot` per-kind counts, run
//!   through the SAME detector catalog `inferences_list` uses
//!   (`crate::inferences_serve::build_detectors` / `empty_state`). One source of truth
//!   for "which framework detectors ship on this build + do they apply to these
//!   languages" — so this line can never re-rot independently of `inferences`.
//! - **Coverage evidence** — presence of `line_coverage` measurements for the snapshot.
//! - **Entrypoint declarations** — active `entrypoint` declarations for the repo
//!   (`TrustStorageRead::count_active_declarations`). This fact class EXISTS, so the
//!   line is DERIVED present/absent — not the spec's capability-statement fallback.
//!
//! # Honesty
//!
//! Every read is fallible and its result is RENDERED, so NONE is defaulted: a read
//! error returns a `DispatchResult::error` (the client then prints the explicitly
//! LABELLED generic causes with that reason — spec §2.2). `Ok(empty)` / `Ok(0)` is a
//! measured absence (known-zero), reported as `present: false` — never a swallowed
//! error. No `unwrap_or`, no `.ok()` on any rendered read.

use std::collections::BTreeMap;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_trust::TrustStorageRead;

use super::support::resolve_and_load_repo;
use crate::state::DaemonState;

/// Serve the derived dead-cause facts for the repo resolved from `params.repo`.
///
/// Request: `{"method": "dead_causes", "params": {"repo": "<path_or_alias>"}}`
pub fn handle_dead_causes(state: &DaemonState, request: &Request) -> DispatchResult {
    // REG-1: resolve repo from path/alias and auto-load. A not-indexed repo returns a
    // RepoNotFound error whose Display carries the not-indexed context (spec §2.2:
    // "no indexed snapshot → same labeling with the not-indexed reason").
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Pin the snapshot for the whole request under the read guard (epoch coherence).
    let _read_guard = repo_state.coordinator.acquire_read();
    let storage = match state.open_repo_storage_for_request(&repo_state) {
        Ok(s) => s,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // READY snapshot (get_latest_snapshot is READY-only). No READY snapshot → honest F2
    // message (partial state named, both next actions), never "index the repo first".
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::SnapshotNotFound,
                    crate::snapshot_facts::no_ready_snapshot_message(
                        &storage,
                        repo_state.db_path(),
                        &repo_uid,
                    ),
                ),
            );
        }
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };
    let snapshot_uid = snapshot.snapshot_uid;

    // ── Framework liveness: per-kind inference counts (UNFILTERED) ──────────────
    let inferences = match storage.list_inferences_for_snapshot(&snapshot_uid, None) {
        Ok(i) => i,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("read inferences: {e}")),
            );
        }
    };
    let mut per_kind: BTreeMap<String, u64> = BTreeMap::new();
    for i in &inferences {
        *per_kind.entry(i.kind.clone()).or_insert(0) += 1;
    }
    let total_inferences = inferences.len() as u64;

    // Snapshot language mix drives detector applicability + the honest zero-state line.
    // RENDERED, so a read failure is surfaced, never treated as "no languages".
    let languages: std::collections::BTreeSet<String> =
        match storage.distinct_file_languages_for_snapshot(&snapshot_uid) {
            Ok(v) => v.into_iter().map(|l| l.to_lowercase()).collect(),
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("read snapshot languages: {e}"),
                    ),
                );
            }
        };
    // Reuse the SAME catalog + honesty logic as `inferences_list` (one source of truth).
    let detectors = crate::inferences_serve::build_detectors(&languages, &per_kind);
    let framework_empty = if total_inferences == 0 {
        crate::inferences_serve::empty_state(&languages)
    } else {
        serde_json::Value::Null
    };
    // Mixed-language gap (DEAD-CAUSES-1 review #1): a snapshot can hold inferences for
    // one family (e.g. Spring/Java) AND carry materially-present languages that NO
    // detector covers (e.g. C/C++). `empty` states that only in the zero-inference case;
    // so for the total>0 case we surface the no-detector gap SEPARATELY, derived from the
    // SAME catalog. `uncovered_note` and `empty` are mutually exclusive by construction
    // (empty ⟺ total==0; uncovered_note ⟺ total>0 with a real gap) so the client never
    // double-reports the same fact. Uncovered set comes from the catalog, never a name.
    let uncovered_note = if total_inferences > 0 {
        let uncovered = crate::inferences_serve::uncovered_languages(&languages);
        crate::inferences_serve::no_detector_note(&uncovered)
    } else {
        None
    };

    // ── Coverage evidence: presence of line_coverage measurements ───────────────
    let coverage_count = match storage.query_measurements_by_kind(&snapshot_uid, "line_coverage") {
        Ok(rows) => rows.len() as u64,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("read coverage: {e}")),
            );
        }
    };

    // ── Entrypoint declarations: active `entrypoint` declarations for the repo ──
    let entrypoint_count = match storage.count_active_declarations(&repo_uid, "entrypoint") {
        Ok(n) => n as u64,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("read entrypoint declarations: {e}"),
                ),
            );
        }
    };

    let response = serde_json::json!({
        "command": "dead causes",
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "languages": languages.iter().cloned().collect::<Vec<_>>(),
        "framework": {
            "detectors": detectors,
            "total_inferences": total_inferences,
            "empty": framework_empty,
            "uncovered_note": uncovered_note,
        },
        "coverage": {
            "present": coverage_count > 0,
            "count": coverage_count,
        },
        "entrypoints": {
            "present": entrypoint_count > 0,
            "count": entrypoint_count,
        },
    });

    DispatchResult::success(&request.id, response)
}
