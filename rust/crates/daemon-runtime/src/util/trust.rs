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
    match try_trust_overlay_for_snapshot(storage, repo_uid, snapshot, graph_basis) {
        Ok(overlay) => Some(overlay),
        Err(e) => {
            eprintln!("warning: failed to assemble trust overlay: {}", e);
            None
        }
    }
}

/// Assemble the SAME trust overlay as [`compute_trust_overlay_for_snapshot`] but PRESERVE the
/// underlying failure reason instead of collapsing it to `None` (DEPS-LIST-REWRITE-1 review-5 item 3
/// / operator ruling 3 item 1). `deps list` needs the reason to render
/// `resolution-state unknown (overlay read failed: …)` rather than a generic "could not be
/// assembled" string — never silent certainty.
///
/// This is the one shared assembly path: `compute_trust_overlay_for_snapshot` delegates here, so
/// there is still a SINGLE `assemble_trust_report` call site (no recomputation, no divergence — the
/// ratified reuse from ruling 2). Kept `pub(crate)`: the only caller is the `deps list` dispatch arm
/// in this crate; it is not part of the crate's cross-crate surface.
pub(crate) fn try_trust_overlay_for_snapshot(
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot: &Snapshot,
    graph_basis: &str,
) -> Result<repo_graph_trust::TrustOverlaySummary, String> {
    use repo_graph_trust::{assemble_trust_report, TrustOverlaySummary};

    let toolchain_json = snapshot.toolchain_json.as_deref();
    let basis_commit = snapshot.basis_commit.as_deref();

    assemble_trust_report(
        storage,
        repo_uid,
        &snapshot.snapshot_uid,
        basis_commit,
        toolchain_json,
    )
    .map(|report| TrustOverlaySummary::from_report(&report, graph_basis))
    .map_err(|e| e.to_string())
}
