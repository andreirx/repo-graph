//! Query result envelope construction.

use repo_graph_storage::types::Snapshot;
use repo_graph_storage::StorageConnection;

/// Build a TS-compatible QueryResult JSON envelope.
///
/// Mirrors the TS `formatQueryResult` wrapper (json.ts:25-40).
/// `extra_fields` are merged into the envelope alongside the
/// standard metadata fields (e.g., `target` for callers/callees,
/// `kind_filter` for dead).
pub fn build_envelope(
    storage: &StorageConnection,
    command: &str,
    repo_uid: &str,
    snapshot: &Snapshot,
    results: serde_json::Value,
    count: usize,
    extra_fields: serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
    use repo_graph_storage::types::RepoRef;

    let repo_name = storage
        .get_repo(&RepoRef::Uid(repo_uid.to_string()))
        .ok()
        .flatten()
        .map(|r| r.name)
        .unwrap_or_else(|| repo_uid.to_string());

    let snapshot_scope = if snapshot.kind == "full" { "full" } else { "incremental" };

    let stale = storage
        .get_stale_files(&snapshot.snapshot_uid)
        .map(|files| !files.is_empty())
        .map_err(|e| format!("failed to compute stale state: {}", e))?;

    let mut envelope = serde_json::json!({
        "command": command,
        "repo": repo_name,
        "snapshot": snapshot.snapshot_uid,
        "snapshot_scope": snapshot_scope,
        "basis_commit": snapshot.basis_commit,
        "results": results,
        "count": count,
        "stale": stale,
    });

    // Merge command-specific fields into the envelope.
    if let serde_json::Value::Object(ref mut map) = envelope {
        for (k, v) in extra_fields {
            map.insert(k, v);
        }
    }

    Ok(envelope)
}

// ── Trust overlay helpers ────────────────────────────────────────

/// Compute a trust overlay summary for query surface envelopes.
///
/// Returns `None` if the trust report cannot be assembled (e.g.,
/// missing diagnostics). Errors are logged to stderr but do not
/// fail the command.
pub fn compute_trust_overlay_for_snapshot(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot: &Snapshot,
    graph_basis: &str,
) -> Option<repo_graph_trust::TrustOverlaySummary> {
    use repo_graph_trust::{assemble_trust_report, TrustOverlaySummary};

    let toolchain_json = snapshot.toolchain_json.as_deref();
    let basis_commit = snapshot.basis_commit.as_deref();

    match assemble_trust_report(
        storage,
        repo_uid,
        &snapshot.snapshot_uid,
        basis_commit,
        toolchain_json,
    ) {
        Ok(report) => Some(TrustOverlaySummary::from_report(&report, graph_basis)),
        Err(e) => {
            eprintln!("warning: failed to assemble trust overlay: {}", e);
            None
        }
    }
}
