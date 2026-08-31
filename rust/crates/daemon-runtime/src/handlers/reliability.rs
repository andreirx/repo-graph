//! `reliability` handler — the per-language / per-module call-resolution breakdown
//! (RESOLUTION-BREAKDOWN-CLI-1).
//!
//! This is the READ/RENDER surface that closes the DB-spelunking gap: the
//! per-language call-resolution split the operator hand-queried (`edges` CALLS vs
//! `unresolved_edges`, joined through `nodes → files.language`) is now a documented
//! CLI command. It computes NO new metric — it composes three existing pieces:
//!
//!   1. the grouping READS `storage.query_call_resolution_{total,by_language,
//!      by_module}` (the SAME `edges`/`unresolved_edges` populations, and the SAME
//!      CALLS-family + `classification` split, the aggregate uses);
//!   2. the SAME band rule `repo_graph_trust::rules::compute_call_graph_reliability`
//!      the aggregate `trust`/`check` surfaces score with — computed here per scope
//!      because `repo_graph_agent` (which owns the projection) deliberately does not
//!      depend on `repo_graph_trust`, so the daemon (which bridges both) injects it;
//!   3. the SAME reader-frame projection `repo_graph_agent::reliability_breakdown`
//!      that reuses `CallReliabilityView` for the rate, wording, UNKNOWN handling,
//!      and conservative caveat.
//!
//! It is a thin fact-composer (the direct-storage-read pattern of `handle_map` /
//! `handle_stats`): all reliability policy lives in the two crates above, so the
//! handler holds no rate/threshold/wording of its own.

use repo_graph_agent::check::{enrichment_state_summary, enrichment_state_token};
use repo_graph_agent::reliability_breakdown::{
    build_breakdown, CallResolutionCounts, ResolutionBreakdown, ScopeCountRow, ScopeCounts,
};
use repo_graph_agent::storage_port::{AgentReliabilityLevel, AgentStorageRead, EnrichmentState};
use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_trust::compute_call_graph_reliability;
use repo_graph_trust::types::ReliabilityLevel;

use crate::handlers::support::resolve_and_load_repo;
use crate::state::DaemonState;

/// Score one scope's reliability band with the SHARED trust rule and map it into
/// the agent's independent band enum. `compute_call_graph_reliability` returns a
/// vacuous `HIGH` for a zero in-scope denominator; the agent projection suppresses
/// the band in exactly that case (`resolution.is_none()`), so scoring every scope
/// uniformly here is safe and matches the aggregate surfaces.
fn band_for(counts: &CallResolutionCounts) -> Option<AgentReliabilityLevel> {
    let score = compute_call_graph_reliability(counts.resolved, counts.internal_like());
    Some(match score.level {
        ReliabilityLevel::HIGH => AgentReliabilityLevel::High,
        ReliabilityLevel::MEDIUM => AgentReliabilityLevel::Medium,
        ReliabilityLevel::LOW => AgentReliabilityLevel::Low,
    })
}

/// Attach the trust-scored band to a raw grouped read, yielding the agent
/// projection's per-scope input. Carries the storage read's `is_test` partition
/// (review-0 F4) through as `Some(..)`.
fn to_scope(row: ScopeCountRow) -> ScopeCounts {
    let band = band_for(&row.counts);
    ScopeCounts {
        key: row.key,
        is_test: Some(row.is_test),
        counts: row.counts,
        band,
    }
}

/// The whole-snapshot total scope, keyed by a sentinel the presentation renders as
/// "Overall". `is_test` is `None` — the total spans BOTH partitions (honest null).
fn total_scope(counts: CallResolutionCounts) -> ScopeCounts {
    ScopeCounts {
        key: "(total)".to_string(),
        is_test: None,
        band: band_for(&counts),
        counts,
    }
}

/// Handle `reliability`.
///
/// Request: `{"method":"reliability","params":{"repo":"<path>"}}`.
/// Response: the serialized [`ResolutionBreakdown`] (`total`/`by_language`/
/// `by_module`) plus `command`/`repo`/`snapshot`. Both breakdown axes are ALWAYS
/// present — the `--json` surface is the complete protocol contract; the `rgr`
/// `--by-language`/`--by-module` flags narrow only the HUMAN view.
pub fn handle_reliability(state: &DaemonState, request: &Request) -> DispatchResult {
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    let _read_guard = repo_state.coordinator.acquire_read();

    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // READY snapshot only (get_latest_snapshot excludes BUILDING/STALE/FAILED) —
    // NAME any existing partial via the shared helper, never a bare error string.
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) if snap.status == "ready" => snap,
        Ok(Some(_)) | Ok(None) => {
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
    let snapshot_uid = snapshot.snapshot_uid.clone();

    macro_rules! read_or_error {
        ($e:expr, $what:expr) => {
            match $e {
                Ok(v) => v,
                Err(err) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, format!("{}: {}", $what, err)),
                    )
                }
            }
        };
    }

    let total = read_or_error!(
        storage.query_call_resolution_total(&snapshot_uid),
        "call-resolution total"
    );
    let by_language = read_or_error!(
        storage.query_call_resolution_by_language(&snapshot_uid),
        "call-resolution by language"
    );
    let by_module = read_or_error!(
        storage.query_call_resolution_by_module(&snapshot_uid),
        "call-resolution by module"
    );

    // The SHARED enrichment state — the SAME value `check`/`trust` render (review-0
    // F1), read via the one source (`get_trust_summary` → the trust assembly). This
    // is why `reliability` declares the trust aggregate's fact classes (FC5, FC8) in
    // the witness manifest.
    // ORIENT-FACT-COHERENCE-1: overlay the daemon's repo-scoped in-flight fact (AUTO pass OR explicit
    // `rmap enrich` — the composed `DaemonState` predicate, review-1 F1) onto the persisted enrichment
    // state, so the reliability breakdown renders the SAME in-flight truth orient/check do (one snapshot,
    // one story) rather than a stale "did not run" while a pass runs. When NOT in flight the persisted
    // enrichment state is read from the trust summary; that read is fallible and its result is RENDERED
    // (`enrichment_state` / `enrichment_summary`), so a failure is surfaced as the established handler
    // error (the SAME `read_or_error!` the three grouped reads above use) — NEVER `.ok()`-collapsed to a
    // silent "unavailable" (STANDING HONESTY RULE 1, review-1 F2). `get_trust_summary` returns the
    // summary or an error (not an Option), so there is no "absent" case to distinguish here.
    let enrich_in_flight = state.enrichment_in_flight_for_db(repo_state.db_path());
    let enrichment_state: Option<EnrichmentState> = if enrich_in_flight {
        Some(EnrichmentState::InFlight)
    } else {
        let summary = read_or_error!(
            storage.get_trust_summary(&repo_uid, &snapshot_uid),
            "enrichment state (trust summary)"
        );
        Some(summary.enrichment_state)
    };

    let breakdown: ResolutionBreakdown = build_breakdown(
        total_scope(total),
        by_language.into_iter().map(to_scope).collect(),
        by_module.into_iter().map(to_scope).collect(),
    );

    // Serialize the shared DTO (plain numeric/string data — cannot yield NaN/Inf),
    // then attach identity. A serialize failure degrades to an explicit error,
    // never a fabricated/partial breakdown.
    let mut response = match serde_json::to_value(&breakdown) {
        Ok(v) => v,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("failed to serialize reliability breakdown: {e}"),
                ),
            )
        }
    };
    if let Some(obj) = response.as_object_mut() {
        obj.insert("command".to_string(), serde_json::json!("reliability"));
        obj.insert("repo".to_string(), serde_json::json!(repo_uid));
        obj.insert("snapshot".to_string(), serde_json::json!(snapshot_uid));
        // Shared enrichment state (review-0 F1): the machine token (`ran`/`not_run`/
        // `not_applicable`/null) an agent parses, plus check's EXACT reader summary so
        // the human view need not re-type the wording. Both from the ONE shared source.
        obj.insert(
            "enrichment_state".to_string(),
            serde_json::json!(enrichment_state_token(enrichment_state)),
        );
        obj.insert(
            "enrichment_summary".to_string(),
            serde_json::json!(enrichment_state_summary(enrichment_state)),
        );
    }

    DispatchResult::success(&request.id, response)
}
