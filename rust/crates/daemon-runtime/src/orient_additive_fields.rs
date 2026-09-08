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
    // COHERENCE-3 (§2.2): the headline counts EXCLUDE test-fixture surfaces (via the SHARED
    // `counts_partitioned`, the SAME `is_test` partition the `surfaces` command and `boundaries
    // summary` apply) so the three cannot state a different provider/consumer count; the excluded +
    // unknown tallies ride additively for the disclosure clause. `total`/`providers`/`consumers`
    // are the PRODUCTION (non-fixture) figures — byte-identical on a repo with no test surfaces.
    match crate::http_boundary_read::unified_http_surfaces(storage, repo_uid, snapshot_uid) {
        Ok(rows) if !rows.is_empty() => {
            let part = crate::http_surface_union::counts_partitioned(&rows);
            let block = serde_json::json!({
                "total": part.providers + part.consumers,
                "providers": part.providers,
                "consumers": part.consumers,
                "test_fixture_excluded": part.test_fixture_excluded,
                "test_status_unknown": part.test_status_unknown,
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

    // MODULES-METHOD-1 §2.1 + §2.2: the per-repo method description and orientation-doc
    // recommendation. Computed from the SAME `load_module_graph_facts` (frozen: no new
    // discovery) and the doc inventory. Always injected (the fields are ADDITIVE; the
    // presenter skips rendering when absent — an older daemon's orient is byte-identical).
    inject_modules_method(output, storage, repo_uid, snapshot_uid);
}

/// MODULES-METHOD-1: compute + inject the `modules_method` and `orientation_docs`
/// blocks into the orient envelope. Storage reads stay here; the pure computation
/// lives in `crate::modules_method`.
fn inject_modules_method(
    output: &mut Value,
    storage: &StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) {
    // §2.1: method line from module_graph_facts (the SAME load `top_module_edges` above
    // may already have done — but orient calls are rare and the read is cheap; sharing
    // the load result would couple the two injectors, which is worse than a second read).
    match repo_graph_module_queries::load_module_graph_facts(storage, snapshot_uid) {
        Ok(facts) => {
            // review-2 #4: orient carries the SAME diagnostics as `modules list` — both
            // the Maven presence AND the Gradle projectDir count — so orient's method
            // line is not diagnostic-blind. Both are Result: a FAILED read renders a
            // named degradation, never a silent absence (standing honesty rule #1).
            let diagnostics = crate::modules_method::MethodDiagnostics {
                gradle_projectdir_unhandled: read_gradle_projectdir_unhandled(
                    storage,
                    snapshot_uid,
                ),
                maven_manifests_present: read_maven_manifests_present(storage, snapshot_uid),
            };
            // review-2 #1: `build_modules_method_json` propagates a FAILED evidence read
            // as `{unavailable}` — a named method is NEVER derived from evidence that
            // could not be read (no `.ok()`/`.and_then` swallowing).
            let modules: Vec<(&str, &str)> = facts
                .context
                .modules
                .iter()
                .map(|m| (m.module_candidate_uid.as_str(), m.module_kind.as_str()))
                .collect();
            let block = build_modules_method_json(storage, &modules, &diagnostics);
            inject_value_field(output, "modules_method", &block, repo_uid);
        }
        Err(e) => {
            // A failed read is unknown-with-reason, NEVER a silent absence.
            let block = serde_json::json!({ "unavailable": e.to_string() });
            inject_value_field(output, "modules_method", &block, repo_uid);
        }
    }

    // §2.2: orientation docs from the doc inventory.
    // STANDING HONESTY RULE #1: a FAILED read is unknown-with-reason, never
    // `unwrap_or_default()` which would turn a failure into "no docs found" (false).
    //
    // review-1 fix #1: apply the vendored-path check to match `docs list`'s classified
    // facts. `get_doc_inventory` uses `discover_doc_inventory(..., false)` and does NOT
    // apply the vendored overlay, so vendored docs would be classified by their content
    // kind (readme, license) rather than demoted to "vendored". The is_vendored_path
    // check ensures vendored docs are excluded from orientation recommendations.
    let orientation_result =
        match repo_graph_agent::AgentStorageRead::get_doc_inventory(storage, repo_uid) {
            Ok(doc_inventory) => {
                let orientation_inputs: Vec<crate::modules_method::OrientationDocInput> =
                    doc_inventory
                        .iter()
                        .map(|d| {
                            // review-1 fix #1: demote vendored paths to kind "vendored"
                            // so is_orientation_doc filters them out, matching docs list.
                            let kind =
                                if crate::handlers::quality::support::is_vendored_path(&d.path) {
                                    "vendored"
                                } else {
                                    d.kind.as_str()
                                };
                            crate::modules_method::OrientationDocInput {
                                path: d.path.as_str(),
                                kind,
                                generated: d.generated,
                            }
                        })
                        .collect();
                let paths = crate::modules_method::select_orientation_docs(&orientation_inputs);
                crate::modules_method::OrientationDocsResult::Ok {
                    paths: paths.into_iter().map(|s| s.to_string()).collect(),
                }
            }
            Err(e) => crate::modules_method::OrientationDocsResult::Unavailable {
                reason: e.to_string(),
            },
        };
    let block = crate::modules_method::orientation_docs_to_json(&orientation_result);
    inject_value_field(output, "orientation_docs", &block, repo_uid);
}

/// MODULES-METHOD-1 (review-2 #1): build the `modules_method` JSON block for either
/// surface (`modules list` / `orient`) from the modules `(candidate_uid, module_kind)`
/// pairs + the already-read diagnostics.
///
/// The single owner of the evidence→family derivation, shared by the two callers
/// (dispatch's `handle_modules_list` and this module's `inject_modules_method`) so the
/// honesty contract is enforced in ONE place: a FAILED `module_candidate_evidence`
/// read degrades the WHOLE block to `{unavailable}` — a named method is NEVER derived
/// from evidence that could not be read (standing honesty rule #1). The old code
/// `.ok().and_then(..)` per module, silently conflating a read failure with genuinely
/// absent evidence and then rendering a family from it (review-2 #1).
///
/// Abstraction record (crate-private fn):
///   - what: the evidence-read + method-family derivation for the modules surface.
///   - concrete current users: `dispatch::handle_modules_list`, `inject_modules_method`.
///   - axis of variation: the evidence read succeeds / fails per module.
///   - rejected simpler: inlining in each caller (that IS what review-2 #1 flagged —
///     two copies of the `.ok()` swallow that diverged from the honest contract).
pub(crate) fn build_modules_method_json(
    storage: &StorageConnection,
    modules: &[(&str, &str)],
    diagnostics: &crate::modules_method::MethodDiagnostics,
) -> Value {
    let uids: Vec<&str> = modules.iter().map(|(uid, _)| *uid).collect();
    match load_module_source_types(storage, &uids) {
        Ok(source_types) => {
            let inputs: Vec<crate::modules_method::ModuleMethodInput> = modules
                .iter()
                .zip(source_types.iter())
                .map(|((_, kind), st)| crate::modules_method::ModuleMethodInput {
                    module_kind: kind,
                    source_type: st.as_deref(),
                })
                .collect();
            let entries = crate::modules_method::compute_method(&inputs);
            crate::modules_method::method_to_json(&entries, diagnostics)
        }
        Err(reason) => serde_json::json!({ "unavailable": reason }),
    }
}

/// Load each module's manifest `source_type` from `module_candidate_evidence`,
/// PROPAGATING read failures (review-2 #1 — never `.ok()`). One FAILED read aborts the
/// whole load with the reason, so `build_modules_method_json` degrades the block rather
/// than fabricating a family from unread evidence.
///
/// Deterministic multi-row rule (review-2 #1): the evidence query has no `ORDER BY`, so
/// `.next()` had no defined contract. When a module has multiple evidence rows we take
/// the lexicographically smallest `source_type` — a total, stable choice independent of
/// row order. `None` = the read succeeded with ZERO rows (genuinely absent evidence — a
/// legacy MODULE-node fallback), which is honest, distinct from a failed read.
fn load_module_source_types(
    storage: &StorageConnection,
    module_uids: &[&str],
) -> Result<Vec<Option<String>>, String> {
    let mut out = Vec::with_capacity(module_uids.len());
    for uid in module_uids {
        match storage.get_module_candidate_evidence(uid) {
            Ok(evs) => out.push(evs.into_iter().map(|e| e.source_type).min()),
            Err(e) => return Err(format!("module evidence read failed for {uid}: {e}")),
        }
    }
    Ok(out)
}

/// Read the Gradle `projectDir`-relocation count from the extraction diagnostics blob
/// (review-2 #4). Mirrors `read_maven_manifests_present`; reuses the SAME parser
/// `handle_modules_list` uses (`dispatch::parse_gradle_projectdir_unhandled`) so orient
/// and modules-list cannot disagree. `Ok(Some(n))` = n relocations; `Ok(None)` = none;
/// `Err(reason)` = the blob was unreadable/malformed — rendered as a named degradation,
/// never a silent zero.
pub(crate) fn read_gradle_projectdir_unhandled(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<Option<u64>, String> {
    use repo_graph_trust::storage_port::TrustStorageRead;
    match TrustStorageRead::get_snapshot_extraction_diagnostics(storage, snapshot_uid) {
        Ok(blob) => crate::dispatch::parse_gradle_projectdir_unhandled(blob.as_deref()),
        Err(e) => Err(format!("extraction diagnostics unreadable ({e})")),
    }
}

/// Read Maven manifest presence count from the extraction diagnostics blob.
///
/// review-1 fix #2: returns `Result<Option<u64>, String>` — distinguishes:
/// - `Ok(Some(n))` where `n > 0`: Maven manifests present but not parsed
/// - `Ok(None)`: key absent or zero (not applicable)
/// - `Err(reason)`: the blob could not be read or parsed — carries the reason
///   honestly, never collapsed to `None` via `.ok()`/`.flatten()` (standing
///   honesty rule #1).
pub(crate) fn read_maven_manifests_present(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<Option<u64>, String> {
    use repo_graph_trust::storage_port::TrustStorageRead;
    let blob = match TrustStorageRead::get_snapshot_extraction_diagnostics(storage, snapshot_uid) {
        Ok(Some(b)) => b,
        Ok(None) => return Ok(None), // No diagnostics blob → not applicable
        Err(e) => return Err(format!("extraction diagnostics read failed: {e}")),
    };
    let parsed: serde_json::Value = serde_json::from_str(&blob)
        .map_err(|e| format!("extraction diagnostics blob not valid JSON: {e}"))?;
    let maven = parsed
        .get("deps_manifests_present")
        .and_then(|v| v.get("maven"))
        .and_then(|v| v.as_u64());
    match maven {
        Some(n) if n > 0 => Ok(Some(n)),
        _ => Ok(None),
    }
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
