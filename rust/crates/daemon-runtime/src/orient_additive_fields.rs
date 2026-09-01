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
//!   - MODULE-EDGES-1 §2.3 `top_module_edges` — the top-3 cross-module dependency
//!     edges headline where they exist (from `load_module_graph_facts` — the SAME
//!     graph `modules deps`/`modules list` serve), or unknown-with-reason on a failed
//!     read. Absent on repos with no cross-module edges (byte-identical).
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

    // MODULE-EDGES-1 §2.3: the top cross-module edges join the headline (the
    // first-60-seconds surface where agents look). READ ONLY — the SAME module
    // dependency graph `modules deps` / `modules list` serve
    // (`load_module_graph_facts`), no new fact class, no new computation — take the
    // top 3 by reference count (import_count DESC, then source/target ASC for
    // determinism). Attached ONLY when edges exist (byte-identical on repos without
    // them, e.g. leveldb's gold standard); a FAILED read is unknown-with-reason
    // (rendered at the detail tiers), NEVER a silent zero (standing honesty rule #1).
    inject_top_module_edges(output, storage, repo_uid, snapshot_uid);
}

/// MODULE-EDGES-1 §2.3: compute + inject the `top_module_edges` headline block.
/// The storage read stays here; the sort/cap/shape + degradation decision is the pure
/// [`top_module_edges_block`] (review-0 item 5: unit-testable without a DB).
fn inject_top_module_edges(
    output: &mut Value,
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) {
    let block = match repo_graph_module_queries::load_module_graph_facts(storage, snapshot_uid) {
        Ok(facts) => top_module_edges_block(Ok(&facts.edges)),
        Err(e) => top_module_edges_block(Err(e.to_string())),
    };
    if let Some(block) = block {
        inject_value_field(output, "top_module_edges", &block, repo_uid);
    }
}

/// MODULE-EDGES-1 §2.3 (pure): map a module-graph-facts LOAD OUTCOME to the optional
/// `top_module_edges` block. Storage-free so ordering, the top-3 cap, empty-omission,
/// and the read-failure reason are all unit-testable (review-0 item 5).
///
/// - `Ok(edges)`, non-empty → `Some({edges: [...]})`: sorted by reference count DESC
///   then (source, target) ASC — the SAME order `modules list` uses, so the two
///   surfaces agree — capped at the top 3.
/// - `Ok(edges)`, empty → `None`: nothing to inject (byte-identical on repos without
///   cross-module edges, e.g. leveldb's gold standard).
/// - `Err(reason)` → `Some({unavailable: reason})`: a FAILED read is unknown-with-reason,
///   NEVER a silent zero (standing honesty rule #1).
///
/// Abstraction record (crate-private pure fn):
///   - what: the top-3 edge projection + honest-degradation decision.
///   - concrete current users: `inject_top_module_edges` (sole caller) + this module's tests.
///   - axis of variation: the facts load succeeds-empty / succeeds-nonempty / fails.
///   - rejected simpler: inlining in `inject_top_module_edges` (couples the sort/cap to
///     a live `StorageConnection`, so the ordering/cap/failure cases can't be unit-tested).
fn top_module_edges_block(
    loaded: Result<&[repo_graph_classification::module_edges::ModuleDependencyEdge], String>,
) -> Option<Value> {
    match loaded {
        Ok([]) => None,
        Ok(edges) => {
            let mut sorted: Vec<&_> = edges.iter().collect();
            sorted.sort_by(|a, b| {
                b.import_count
                    .cmp(&a.import_count)
                    .then_with(|| a.source_canonical_path.cmp(&b.source_canonical_path))
                    .then_with(|| a.target_canonical_path.cmp(&b.target_canonical_path))
            });
            let top: Vec<Value> = sorted
                .iter()
                .take(3)
                .map(|e| {
                    serde_json::json!({
                        "source": e.source_canonical_path,
                        "target": e.target_canonical_path,
                        "import_count": e.import_count,
                    })
                })
                .collect();
            Some(serde_json::json!({ "edges": top }))
        }
        Err(reason) => Some(serde_json::json!({ "unavailable": reason })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_classification::module_edges::ModuleDependencyEdge;

    fn edge(source: &str, target: &str, import_count: u64) -> ModuleDependencyEdge {
        ModuleDependencyEdge {
            source_module_uid: format!("uid:{source}"),
            source_canonical_path: source.to_string(),
            target_module_uid: format!("uid:{target}"),
            target_canonical_path: target.to_string(),
            import_count,
            source_file_count: 1,
        }
    }

    /// review-0 item 5: reference-count-DESC ordering, with (source, target) ASC as the
    /// deterministic tie-break — the SAME order `modules list` uses.
    #[test]
    fn top_edges_sorted_by_refcount_then_name() {
        let edges = vec![
            edge("server", "lib", 9),
            edge("client", "lib", 14),
            edge("aaa", "lib", 9), // ties `server` on count → name ASC breaks it
        ];
        let block = top_module_edges_block(Ok(&edges)).expect("non-empty → Some");
        let rows = block["edges"].as_array().unwrap();
        assert_eq!(rows[0]["source"], "client"); // 14, heaviest first
        assert_eq!(rows[1]["source"], "aaa"); // 9, ties server, "aaa" < "server"
        assert_eq!(rows[2]["source"], "server");
        assert_eq!(rows[0]["import_count"], 14);
    }

    /// review-0 item 5: the headline caps at the top 3 by reference count.
    #[test]
    fn top_edges_capped_at_three() {
        let edges = vec![
            edge("a", "lib", 1),
            edge("b", "lib", 2),
            edge("c", "lib", 3),
            edge("d", "lib", 4),
            edge("e", "lib", 5),
        ];
        let block = top_module_edges_block(Ok(&edges)).expect("non-empty → Some");
        let rows = block["edges"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "capped at 3");
        // The three HEAVIEST (5, 4, 3) survive the cap, in DESC order.
        assert_eq!(rows[0]["source"], "e");
        assert_eq!(rows[1]["source"], "d");
        assert_eq!(rows[2]["source"], "c");
    }

    /// review-0 item 5: an empty graph injects NOTHING (byte-identical on repos with no
    /// cross-module edges).
    #[test]
    fn top_edges_empty_graph_omits_block() {
        assert!(top_module_edges_block(Ok(&[])).is_none());
    }

    /// review-0 item 5 + honesty rule #1: a FAILED graph read is unknown-WITH-REASON,
    /// never a silent zero.
    #[test]
    fn top_edges_read_failure_is_unavailable_with_reason() {
        let block = top_module_edges_block(Err("duplicate ownership on 2 file(s)".to_string()))
            .expect("failure → Some(unavailable)");
        assert_eq!(block["unavailable"], "duplicate ownership on 2 file(s)");
        assert!(
            block.get("edges").is_none(),
            "a failed read must not carry an edge list"
        );
    }
}
