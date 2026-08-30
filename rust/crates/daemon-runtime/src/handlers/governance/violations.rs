//! Violations handler — architectural violation analysis.
//!
//! LEGACY-CONTRACT-MIGRATION-1C: Migrated from legacy CLI contract.
//!
//! Combines two violation sources:
//! 1. Declared boundary violations (legacy FORBIDS declarations)
//! 2. Discovered module violations (from module graph analysis)
//!
//! This is a READ operation.

use std::collections::HashMap;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};

use crate::handlers::support::resolve_and_load_repo;
use crate::state::DaemonState;

/// Evaluate architectural violations (declared + discovered).
///
/// Request: `{"method": "violations", "params": {"repo": "<path>"}}`
///
/// - `repo` (required): path or alias
///
/// This is a READ operation combining:
/// 1. Declared boundary violations (FORBIDS rules)
/// 2. Discovered module boundary violations
pub fn handle_violations(state: &DaemonState, request: &Request) -> DispatchResult {
    use repo_graph_classification::boundary_evaluator::StaleSide;
    use repo_graph_module_queries::{evaluate_violations_from_facts, load_module_graph_facts};
    use repo_graph_storage::queries::BoundaryViolation;

    // REG-1: resolve repo
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Acquire read lock
    let _read_guard = repo_state.coordinator.acquire_read();

    // D-S = S-A (DAEMON-CONCURRENCY-IMPL-1): open one fresh per-operation connection for this
    // handler's SQLite reads. The coordinator guard above keeps it snapshot-consistent for the request.
    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // Get latest snapshot
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) if snap.status == "ready" => snap,
        // DAEMON-VISIBILITY-1 (F2): no READY snapshot on a READY-requiring surface — NAME any existing
        // partial (state, when, on-disk size) + BOTH next actions via the shared helper, never the bare
        // day-2 gaslighting string. `get_latest_snapshot` is READY-only, so the non-ready `Ok(Some(_))`
        // is unreachable today; folded in so a future non-READY leak is honest too.
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

    // ── Section 1: Declared boundary violations (legacy) ─────────────────

    // Load active boundary declarations (directory-level MODULE targets)
    let boundaries = match storage.get_active_boundary_declarations(&repo_uid) {
        Ok(b) => b,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };

    // Deduplicate rules by (boundary_module, forbids)
    let mut rule_map: HashMap<(String, String), (String, String, Option<String>)> = HashMap::new();
    for decl in &boundaries {
        let key = (decl.boundary_module.clone(), decl.forbids.clone());
        rule_map.entry(key).or_insert_with(|| {
            (
                decl.boundary_module.clone(),
                decl.forbids.clone(),
                decl.reason.clone(),
            )
        });
    }

    // For each unique rule, find violating IMPORTS edges
    let mut declared_violations: Vec<BoundaryViolation> = Vec::new();

    // Sort rules for deterministic output
    let mut rules: Vec<_> = rule_map.into_values().collect();
    rules.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));

    for (boundary_module, forbids, reason) in &rules {
        let edges = match storage.find_imports_between_paths(
            &snapshot.snapshot_uid,
            boundary_module,
            forbids,
        ) {
            Ok(e) => e,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        for edge in &edges {
            declared_violations.push(BoundaryViolation {
                boundary_module: boundary_module.clone(),
                forbidden_module: forbids.clone(),
                reason: reason.clone(),
                source_file: edge.source_file.clone(),
                target_file: edge.target_file.clone(),
                line: edge.line,
            });
        }
    }

    // ── Section 2: Discovered module boundary violations ─────────────────

    // Load module graph facts once
    let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
        Ok(f) => f,
        Err(e) => {
            // MODULE-OWNERSHIP-DUPLICATE-1: duplicate ownership → labeled
            // degradation, not a bare InternalError.
            return crate::module_degradation::module_facts_error_result(
                &request.id,
                "violations",
                e,
            );
        }
    };

    // Evaluate using preloaded facts
    let discovered_result = match evaluate_violations_from_facts(&storage, &repo_uid, &facts) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("failed to evaluate violations: {}", e),
                ),
            );
        }
    };

    // ── Build output ─────────────────────────────────────────────────────

    // Declared violations JSON
    let declared_violations_json: Vec<serde_json::Value> = declared_violations
        .iter()
        .map(|v| {
            serde_json::json!({
                "boundary_module": v.boundary_module,
                "forbidden_module": v.forbidden_module,
                "reason": v.reason,
                "source_file": v.source_file,
                "target_file": v.target_file,
                "line": v.line,
            })
        })
        .collect();

    // Discovered violations JSON
    let discovered_violations_json: Vec<serde_json::Value> = discovered_result
        .evaluation
        .violations
        .iter()
        .map(|v| {
            serde_json::json!({
                "declaration_uid": v.declaration_uid,
                "source": v.source_canonical_path,
                "target": v.target_canonical_path,
                "import_count": v.import_count,
                "source_file_count": v.source_file_count,
                "reason": v.reason,
            })
        })
        .collect();

    // Stale declarations JSON
    let stale_declarations_json: Vec<serde_json::Value> = discovered_result
        .evaluation
        .stale_declarations
        .iter()
        .map(|s| {
            serde_json::json!({
                "declaration_uid": s.declaration_uid,
                "stale_side": match s.stale_side {
                    StaleSide::Source => "source",
                    StaleSide::Target => "target",
                    StaleSide::Both => "both",
                },
                "missing_paths": s.missing_paths,
            })
        })
        .collect();

    let declared_count = declared_violations.len();
    let discovered_count = discovered_result.evaluation.violations.len();
    let stale_count = discovered_result.evaluation.stale_declarations.len();
    let total_count = declared_count + discovered_count;

    // Build envelope
    let toolchain: serde_json::Value = snapshot
        .toolchain_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);

    // GOV-ARMED-1: the surface is "armed" iff the repo has any active boundary
    // declaration. Both the legacy declared-boundary path and the discovered-
    // module path read the SAME `declarations` rows (kind='boundary',
    // is_active=1), so `declarations_evaluated` from the discovered result is
    // the single honest configuration-presence count for this surface. This is
    // a config fact, NOT an inference from `count == 0`.
    let declarations_evaluated = discovered_result.declarations_evaluated;
    let armed = declarations_evaluated > 0;

    let response = serde_json::json!({
        "command": "arch violations",
        "repo": repo_uid,
        "snapshot": snapshot.snapshot_uid,
        "toolchain": toolchain,
        "count": total_count,
        "declared_boundary_count": declared_count,
        "discovered_module_count": discovered_count,
        "stale_count": stale_count,
        // GOV-ARMED-1: additive configuration-presence facts.
        "armed": armed,
        "declarations_checked": declarations_evaluated,
        "results": {
            "declared_boundary_violations": declared_violations_json,
            "discovered_module_violations": discovered_violations_json,
        },
        "stale_declarations": stale_declarations_json,
    });

    DispatchResult::success(&request.id, response)
}
