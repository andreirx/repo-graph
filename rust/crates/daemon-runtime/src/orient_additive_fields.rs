//! ORIENT — daemon-injected ADDITIVE `value` fields for the orient envelope.
//!
//! The single owner of every post-serialize field the daemon attaches onto the orient
//! envelope's `value` (via `inject_value_field`), keeping this wiring OUT of the
//! 9k-line `dispatch.rs` (guardrail: dispatch.rs net-neutral). It owns the two
//! pre-existing INDEX-BASIS-1 fields AND the ORIENT-SEGMENT-2 additions, in a fixed
//! order; each is present ONLY when it has something to say, so a repo that trips none
//! of the seg2 features is byte-identical to today:
//!   - `index_drift` (INDEX-BASIS-1) — the query-time working-tree drift (computed by
//!     the caller, which needs `&self`; passed in).
//!   - `parse_status` (INDEX-BASIS-1) — the honest parse axis from `get_stale_files`.
//!   - §2.1 `directory_group_fallback` — the promoted directory-group fan-in view,
//!     ONLY on package-group collapse (detected off `output`; from `orient_topology_fallback`).
//!   - §2.5 `http_surfaces` — the HTTP architecture headline where surfaces > 0
//!     (from the HSC-1 unified read), or unknown-with-reason on a failed read.
//!
//! Abstraction record (crate-private module, pre-ratified guardrail carve-out):
//!   - what: the orient additive-`value`-field injection orchestrator.
//!   - concrete current users: `dispatch::handle_orient` (sole caller).
//!   - axis of variation: which additive orient fields attach this request.
//!   - rejected alternative: inlining ~50 lines into `dispatch.rs` (violates the
//!     dispatch.rs net-neutral guardrail).

use repo_graph_daemon_transport::ProgressEmitter;
use repo_graph_storage::StorageConnection;
use serde::Serialize;
use serde_json::Value;

use crate::dispatch::{compute_parse_status, inject_value_field};
use crate::state::RepoState;

/// Attach every additive orient `value` field applicable to this request, in a fixed
/// order. `index_drift` is computed by the caller (it needs `&self`); everything else
/// is derived here. Collapse (§2.1) is detected off the already-serialized `output`,
/// so no `OrientResult` need be held past the envelope move.
#[allow(clippy::too_many_arguments)]
pub(crate) fn inject<D: Serialize>(
    output: &mut Value,
    index_drift: &D,
    repo_state: &RepoState,
    emitter: &mut dyn ProgressEmitter,
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) {
    // INDEX-BASIS-1: the query-time working-tree drift (git basis + how far the tree
    // has moved), computed by the caller. rgr renders it as the "index basis / drift"
    // footer line.
    inject_value_field(output, "index_drift", index_drift, repo_uid);

    // INDEX-BASIS-1 (review-0 fix #2): the parse axis is its OWN honest value (from
    // get_stale_files), DISTINCT from the coherence-envelope freshness meet. A FAILED
    // read is `Unknown` WITH reason, never `Ok`/zero.
    let parse_status = compute_parse_status(storage, snapshot_uid);
    inject_value_field(output, "parse_status", &parse_status, repo_uid);

    // §2.1: on package-group collapse, promote the directory-group fan-in view
    // `stats` already computes. Injected ONLY when collapsed — a non-collapsed orient
    // carries no `directory_group_fallback` key, so leveldb's gold standard is
    // byte-identical by construction.
    if crate::orient_topology_fallback::detect_collapse(output) {
        let fallback = crate::orient_topology_fallback::build(repo_state, emitter, snapshot_uid);
        inject_value_field(output, "directory_group_fallback", &fallback, repo_uid);
    }

    // §2.5: the HTTP surface count joins the headline where > 0, from the HSC-1
    // unified read (READ only — the SAME union `surfaces list` / `boundaries summary`
    // consume, so the three cannot disagree). A clean-zero read attaches nothing
    // (byte-identical on non-HTTP repos); a FAILED read is unknown-with-reason
    // (rendered at large/--full), never a silent zero (standing honesty rule #1).
    match crate::http_boundary_read::unified_http_surfaces_json(storage, repo_uid, snapshot_uid) {
        Ok((rows, providers, consumers)) if !rows.is_empty() => {
            let block = serde_json::json!({
                "total": rows.len(),
                "providers": providers,
                "consumers": consumers,
            });
            inject_value_field(output, "http_surfaces", &block, repo_uid);
        }
        Ok(_) => {}
        Err(reason) => {
            let block = serde_json::json!({ "unavailable": reason });
            inject_value_field(output, "http_surfaces", &block, repo_uid);
        }
    }
}
