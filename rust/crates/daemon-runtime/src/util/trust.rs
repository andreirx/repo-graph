//! Trust overlay computation for daemon responses.

use repo_graph_storage::types::Snapshot;
use repo_graph_storage::StorageConnection;

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
