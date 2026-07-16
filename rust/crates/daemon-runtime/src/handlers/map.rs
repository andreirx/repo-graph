//! `map` handler — flat extracted facts for the deterministic MAP.md renderer.
//!
//! MAP-FROM-INDEX-1. `rmap map` renders MAP.md files from the current READY
//! snapshot with NO model call anywhere in the path (VISION commitment #1:
//! facts computed from source, reproducible, never model output). This handler
//! is the READ half: it gathers already-extracted Layer-0/1 facts for a subtree
//! and returns them FLAT. All grouping, ordering, coverage-honesty, the
//! dependency sketch, and the markdown rendering happen in the `rgr`
//! presentation layer — a pure `MapFacts -> files` function that is unit-tested
//! without a daemon (byte-determinism, stable ordering, unmapped honesty,
//! golden). Keeping this handler a thin fact-emitter (no domain logic) is why
//! the renderer can be pure and the daemon stays the read authority (rule #8).
//!
//! Read-only: no write path, no schema change, no extractor change. It follows
//! the direct-storage-read pattern (`handle_stats`/`handle_imports`), consuming
//! the additive `map_*_in_path` reads plus the existing `query_complexity_by_file`,
//! `list_manifest_roots`, and the shared `measurement_coverage_json` block.

use repo_graph_agent::AgentStorageRead;
use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::types::RepoRef;

use crate::handlers::support::{get_optional_string_param, resolve_and_load_repo};
use crate::state::DaemonState;

/// Resolve the repo's stored (DB-relative) `root_path` to an absolute,
/// canonicalized path — the directory the CLI writes MAP.md files under. Mirrors
/// `handlers::quality::support::resolve_root_path` exactly (the same rule the
/// churn/coverage/hotspots surfaces use so a daemon running at `cwd=/` still
/// resolves the repo root). Kept local to avoid a general-handler →
/// quality-internal dependency edge; consolidate into `handlers::support` if a
/// third handler group needs it.
fn resolve_repo_root_abs(db_path: &std::path::Path, root_path: &str) -> std::path::PathBuf {
    let db_dir = db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/"));
    let resolved = db_dir.join(root_path);
    resolved.canonicalize().unwrap_or(resolved)
}

/// Gather the flat fact payload for `rmap map`.
///
/// Request: `{"method": "map", "params": {"repo": "<path>", "path": "<repo-relative dir>"}}`.
/// An omitted/empty `path` selects the whole repo subtree.
pub fn handle_map(state: &DaemonState, request: &Request) -> DispatchResult {
    // REG-1: resolve repo from cwd-derived path param.
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // The subtree to render; repo-root-relative. Empty => whole repo.
    let path = get_optional_string_param(&request.params, "path")
        .unwrap_or("")
        .to_string();

    let _read_guard = repo_state.coordinator.acquire_read();

    // D-S = S-A: one fresh per-operation connection; the read guard keeps it
    // snapshot-consistent for the request.
    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // READY snapshot only (get_latest_snapshot excludes BUILDING/STALE/FAILED).
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) if snap.status == "ready" => snap,
        // DAEMON-VISIBILITY-1 (F2): no READY snapshot on a READY-requiring surface —
        // NAME any existing partial via the shared helper, never a bare error string.
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

    // Repo record: display name for the header + `root_path` for the write path.
    let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };
    let repo_name = repo
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_else(|| repo_uid.clone());
    // Absolute repo root so the CLI writes each MAP.md at its repo-root-relative
    // location regardless of the caller's cwd (REVIEWER-4: default write must not
    // recreate whole-repo paths under a nested cwd). `repos.root_path` is stored
    // relative to the DB file; the shared resolver joins it to the DB dir and
    // canonicalizes (same helper the churn/coverage surfaces use). Empty only when
    // the repo record is absent — the CLI then falls back to cwd-relative writes.
    let repo_root = repo
        .as_ref()
        .map(|r| {
            resolve_repo_root_abs(repo_state.db_path(), &r.root_path)
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_default();

    // ── Gather flat facts. Each read is already ordered by a total key; the
    //    renderer re-imposes canonical order anyway, so producer order is not
    //    load-bearing for the rendered output. ─────────────────────────────
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

    let files = read_or_error!(storage.map_files_in_path(&snapshot_uid, &path), "map files");
    let symbols = read_or_error!(
        storage.map_symbols_in_path(&snapshot_uid, &path),
        "map symbols"
    );
    // Resolved intra-repo dependency edges (IMPORTS + CALLS) feed the per-file
    // import list (imports subset) and the per-directory dependency sketch.
    let dep_edges = read_or_error!(
        storage.map_resolved_dep_edges_in_path(&snapshot_uid, &path),
        "map dependency edges"
    );
    // Unresolved IMPORTS (external packages / missing paths) feed the per-file
    // "external / unresolved imports" list so a file's imports are never dropped.
    let unresolved_imports = read_or_error!(
        storage.map_unresolved_imports_in_path(&snapshot_uid, &path),
        "map unresolved imports"
    );
    // Whole-snapshot per-file complexity; the renderer looks it up by path over
    // the scoped file set (extra rows are harmless).
    let complexity = read_or_error!(
        storage.query_complexity_by_file(&snapshot_uid),
        "map complexity"
    );
    // Manifest-declared package boundaries (crate / TS package). Empty on a
    // manifest-less tree — the renderer then simply omits package identity.
    let manifest_roots = read_or_error!(
        storage.list_manifest_roots(&snapshot_uid),
        "map manifest roots"
    );

    let files_json: Vec<serde_json::Value> = files
        .into_iter()
        .map(|f| {
            serde_json::json!({
                "path": f.path,
                "language": f.language,
                "parse_status": f.parse_status,
                // Raw skip-cause discriminator: 'skipped:oversized' vs null (no
                // extractor) — the renderer turns it into a reader-frame reason.
                "extractor": f.extractor,
                "is_test": f.is_test,
                "is_generated": f.is_generated,
                "symbol_count": f.symbol_count,
            })
        })
        .collect();

    let symbols_json: Vec<serde_json::Value> = symbols
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "file": s.file_path,
                "name": s.name,
                "qualified_name": s.qualified_name,
                "subtype": s.subtype,
                "line_start": s.line_start,
                "signature": s.signature,
            })
        })
        .collect();

    let dependency_edges_json: Vec<serde_json::Value> = dep_edges
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "source": e.source_file,
                "target": e.target_file,
                "edge_type": e.edge_type,
            })
        })
        .collect();

    let unresolved_imports_json: Vec<serde_json::Value> = unresolved_imports
        .into_iter()
        .map(|u| serde_json::json!({ "source": u.source_file, "specifier": u.specifier }))
        .collect();

    let complexity_json: Vec<serde_json::Value> = complexity
        .into_iter()
        .map(|c| serde_json::json!({ "file": c.file_path, "sum_complexity": c.sum_complexity }))
        .collect();

    let manifest_roots_json: Vec<serde_json::Value> = manifest_roots
        .into_iter()
        .map(|r| {
            use repo_graph_agent::ManifestKind;
            // Reader-frame label, not the internal enum name.
            let kind = match r.kind {
                ManifestKind::RustCrate => "rust crate",
                ManifestKind::TsPackage => "ts package",
            };
            serde_json::json!({ "path": r.path, "kind": kind })
        })
        .collect();

    let response = serde_json::json!({
        "command": "map",
        "repo": repo_uid,
        "repo_name": repo_name,
        // Absolute repo root for the CLI write path (empty => CLI writes cwd-relative).
        "repo_root": repo_root,
        "snapshot": snapshot_uid,
        "path": path,
        "files": files_json,
        "symbols": symbols_json,
        // Resolved intra-repo dependency edges (IMPORTS + CALLS), each tagged with
        // its edge_type; the renderer splits the imports-only subset (a file's
        // import list) from the full import+call set (the directory sketch). Named
        // `dependency_edges`, NOT `imports`, because the collection is not
        // imports-only — the name must match the contract.
        "dependency_edges": dependency_edges_json,
        "unresolved_imports": unresolved_imports_json,
        "complexity": complexity_json,
        "manifest_roots": manifest_roots_json,
        // "coverage is part of the fact" (VISION): always-present per-language
        // complexity-coverage block — honest about which languages are measured.
        "measurement_coverage": crate::util::measurement_coverage_json(&storage, &snapshot_uid),
    });

    DispatchResult::success(&request.id, response)
}
