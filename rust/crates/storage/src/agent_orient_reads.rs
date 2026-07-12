//! Orient-supporting discovery reads (ORIENT-DENSITY-1).
//!
//! Extracted from `agent_impl.rs` per the >500-line structural guardrail
//! (review-1 #3): that file is a single, contiguous `impl AgentStorageRead for
//! StorageConnection` (a trait impl cannot be split across modules), so the
//! method *bodies* for the two dense-orient discovery reads live here as free
//! functions and the trait methods delegate with one-liners. This keeps the new
//! responsibility — the module-size + filesystem doc-inventory projections that
//! back the dense `orient` headline — out of the oversized adapter file.
//!
//! Both functions take a `&rusqlite::Connection` (the adapter's own backing
//! store) and the shared `map_err` from `agent_impl`, so error mapping is
//! identical to every other `AgentStorageRead` method.

use std::path::{Path, PathBuf};

use repo_graph_agent::{
    AgentDirectoryGroup, AgentDocEntry, AgentModuleSize, AgentStorageError, ManifestKind,
    ManifestRoot,
};
use rusqlite::Connection;

use crate::agent_impl::map_err;

/// Discover the live documentation inventory for a repo, resolving the stored
/// `root_path` the SAME way the daemon's `handle_docs_list` does.
///
/// ORIENT-DENSITY-1 review-1 #1 — the bug this fixes: `root_path` is stored
/// RELATIVE to the DB file's parent directory (the daemon `storage_root_path`
/// convention). The previous in-`agent_impl` read treated that stored value as
/// an absolute path (`Path::new(&root_path)`), so `is_dir()` failed and it
/// silently returned an EMPTY inventory — `orient` showed NO docs on a real
/// repo (e.g. nginx) even though `docs list`, which DOES resolve relative to the
/// DB parent, listed README.md / CONTRIBUTING.md. Confirmed first-hand on an
/// isolated nginx index: `documentation: null` in the orient envelope vs 4 docs
/// from `docs list`.
///
/// The fix mirrors `handle_docs_list`: join `root_path` onto the DB FILE's
/// parent dir (`Connection::path()` — the adapter's own backing file). `Path::
/// join` with an *absolute* `root_path` replaces, so this is correct for both
/// relative and absolute stored paths; an in-memory DB has no path → fall back
/// to the stored value as-is (preserves the in-memory test fakes' behavior).
/// Empty inventory is a valid result (repos with zero docs).
pub(crate) fn doc_inventory(
    conn: &Connection,
    repo_uid: &str,
) -> Result<Vec<AgentDocEntry>, AgentStorageError> {
    // 1. Look up the stored (DB-parent-relative) root_path.
    let raw_root: Option<String> = conn
        .query_row(
            "SELECT root_path FROM repos WHERE repo_uid = ?",
            rusqlite::params![repo_uid],
            |row| row.get(0),
        )
        .map_err(map_err("get_doc_inventory"))?;

    let raw_root = match raw_root {
        Some(p) => p,
        None => return Ok(Vec::new()), // No repo row / NULL root_path → empty.
    };

    // 2. Resolve relative to the DB file's parent (mirrors handle_docs_list).
    let resolved: PathBuf = match conn.path().and_then(|db| Path::new(db).parent()) {
        Some(parent) => parent.join(&raw_root),
        None => PathBuf::from(&raw_root), // in-memory / no parent → as-is.
    };

    if !resolved.is_dir() {
        return Ok(Vec::new()); // Path not a directory → graceful empty.
    }

    // 3. Live filesystem discovery (doc-facts crate); map to agent DTOs.
    let result = match repo_graph_doc_facts::discover_doc_inventory(&resolved, false) {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };

    Ok(result
        .entries
        .into_iter()
        .map(|e| AgentDocEntry {
            path: e.path,
            kind: e.kind,
            generated: e.generated,
        })
        .collect())
}

/// List discovered modules with owned-file counts, ordered by size (total,
/// source-independent), capped at `limit` rows (ORIENT-DENSITY-1 §5).
///
/// Reads the same Layer-1 surface as `get_module_summary`
/// (`module_candidates` ⋈ `module_file_ownership`), projecting per-module
/// NAMES + sizes instead of kind-counts — the data the dense structure headline
/// needs. Only modules owning ≥1 file are returned. The order (`file_count`
/// DESC, then `canonical_root_path`, then `module_candidate_uid`) is TOTAL, so
/// the budget cut the agent applies on top is a pure function of the SET, not of
/// row order (the DR-EXPLAIN-CALLER-ORDER discipline).
///
/// `limit` is the budget-derived cap (review-1 #2): `large`/`--full` pass
/// `usize::MAX` for the COMPLETE list. rusqlite binds `LIMIT` as `i64`, so
/// `usize::MAX` is clamped to `i64::MAX` — no snapshot has 9.2e18 modules, so
/// the clamp returns the full set while staying bind-safe (mirrors the
/// complexity read's `FETCH_ALL` note). `discovered_module_count` still reports
/// the true total, so a bounded cap never overclaims completeness.
pub(crate) fn module_sizes(
    conn: &Connection,
    snapshot_uid: &str,
    limit: usize,
) -> Result<Vec<AgentModuleSize>, AgentStorageError> {
    let limit_i64 = limit.min(i64::MAX as usize) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT mc.canonical_root_path AS path, COUNT(o.file_uid) AS file_count \
             FROM module_candidates mc \
             JOIN module_file_ownership o \
               ON o.module_candidate_uid = mc.module_candidate_uid \
               AND o.snapshot_uid = mc.snapshot_uid \
             WHERE mc.snapshot_uid = ?1 \
             GROUP BY mc.module_candidate_uid, mc.canonical_root_path \
             HAVING file_count > 0 \
             ORDER BY file_count DESC, mc.canonical_root_path ASC, mc.module_candidate_uid ASC \
             LIMIT ?2",
        )
        .map_err(map_err("list_module_sizes"))?;

    let rows = stmt
        .query_map(rusqlite::params![snapshot_uid, limit_i64], |row| {
            Ok(AgentModuleSize {
                path: row.get::<_, String>(0)?,
                file_count: row.get::<_, i64>(1)? as u64,
            })
        })
        .map_err(map_err("list_module_sizes"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_err("list_module_sizes"))
}

/// List leaf directories that own ≥1 file, with their owned-file counts
/// (MODULE-MODEL-1 D2(i)).
///
/// Reads the per-directory TOPOLOGY — the indexer materializes a `nodes`
/// kind=MODULE node per directory (`qualified_name` = the dir path) and an OWNS
/// edge from a directory to each file it directly contains
/// (`orchestrator::create_module_nodes`). This is the SAME `(path, file_count)`
/// set `queries::compute_module_stats` derives for `stats` (OWNS-edge count per
/// MODULE node, kept only when > 0), projected without the Martin metrics — so
/// `orient` (which folds these into package groups) and `stats` cannot report
/// divergent topology numbers. A Layer-0/1 EXTRACTED fact, DISTINCT from the
/// declared/inferred `module_candidates` surface `module_sizes` reads.
///
/// Order is by path ASC (a total order); the caller folds + re-sorts.
pub(crate) fn directory_groups(
    conn: &Connection,
    snapshot_uid: &str,
) -> Result<Vec<AgentDirectoryGroup>, AgentStorageError> {
    let mut stmt = conn
        .prepare(
            "SELECT m.qualified_name AS path, COUNT(o.target_node_uid) AS file_count \
             FROM nodes m \
             JOIN edges o \
               ON o.source_node_uid = m.node_uid \
               AND o.snapshot_uid = ?1 \
               AND o.type = 'OWNS' \
             WHERE m.snapshot_uid = ?1 \
               AND m.kind = 'MODULE' \
               AND m.qualified_name IS NOT NULL \
             GROUP BY m.node_uid, m.qualified_name \
             HAVING file_count > 0 \
             ORDER BY m.qualified_name ASC",
        )
        .map_err(map_err("list_directory_groups"))?;

    let rows = stmt
        .query_map(rusqlite::params![snapshot_uid], |row| {
            Ok(AgentDirectoryGroup {
                path: row.get::<_, String>(0)?,
                file_count: row.get::<_, i64>(1)? as u64,
            })
        })
        .map_err(map_err("list_directory_groups"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_err("list_directory_groups"))
}

/// List the manifest-declared package boundaries (crate / workspace-package
/// roots) for a snapshot — the per-toolchain grouping facts the package-group
/// fold uses to name Rust crates and TS packages (MODULE-MODEL-2 §13 D4).
///
/// Reads the ALREADY-STORED `module_candidates` ⋈ `module_candidate_evidence`
/// surface: `canonical_root_path` (the crate/package root dir) filtered to the
/// manifest `source_type`s whose ecosystem D4 groups by boundary —
/// `cargo_toml` → Rust, `package_json` / `pnpm_workspace_yaml` → TS. It reads
/// `source_type` (the ecosystem marker), NOT `module_kind` (which is provenance:
/// `declared`/`inferred`/`directory`, identical across cargo/npm on the Rust
/// indexer path). No new scan, no new table.
///
/// `pyproject_toml` / `settings_gradle` are deliberately NOT surfaced: the
/// ratified D4 keeps Python/JVM/C/C++/manifest-less trees on the directory/JVM
/// heuristic. Rows ordered by `(canonical_root_path, source_type)` for a total,
/// deterministic order (the fold is order-independent, but a hybrid
/// same-root/two-manifest dir then resolves deterministically).
pub(crate) fn manifest_roots(
    conn: &Connection,
    snapshot_uid: &str,
) -> Result<Vec<ManifestRoot>, AgentStorageError> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT mc.canonical_root_path, e.source_type \
             FROM module_candidates mc \
             JOIN module_candidate_evidence e \
               ON e.module_candidate_uid = mc.module_candidate_uid \
               AND e.snapshot_uid = mc.snapshot_uid \
             WHERE mc.snapshot_uid = ?1 \
               AND e.source_type IN ('cargo_toml', 'package_json', 'pnpm_workspace_yaml') \
             ORDER BY mc.canonical_root_path ASC, e.source_type ASC",
        )
        .map_err(map_err("list_manifest_roots"))?;

    let rows = stmt
        .query_map(rusqlite::params![snapshot_uid], |row| {
            let path: String = row.get(0)?;
            let source_type: String = row.get(1)?;
            Ok((path, source_type))
        })
        .map_err(map_err("list_manifest_roots"))?;

    let mut out = Vec::new();
    for row in rows {
        let (path, source_type) = row.map_err(map_err("list_manifest_roots"))?;
        // The WHERE clause guarantees one of the three; map to the D4 ecosystem.
        let kind = match source_type.as_str() {
            "cargo_toml" => ManifestKind::RustCrate,
            "package_json" | "pnpm_workspace_yaml" => ManifestKind::TsPackage,
            _ => continue,
        };
        out.push(ManifestRoot { path, kind });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::StorageConnection;
    use crate::types::{CreateSnapshotInput, Repo};
    use repo_graph_agent::AgentStorageRead;
    use tempfile::tempdir;

    // "Fetch all" sentinel mirroring the agent-side budget cap for `--full`.
    const ALL: usize = usize::MAX;

    fn setup() -> StorageConnection {
        let storage = StorageConnection::open_in_memory().unwrap();
        storage
            .add_repo(&Repo {
                repo_uid: "r1".into(),
                name: "test".into(),
                root_path: "/tmp/test".into(),
                default_branch: Some("main".into()),
                created_at: "2025-01-01T00:00:00.000Z".into(),
                metadata_json: None,
            })
            .unwrap();
        storage
    }

    fn snapshot(storage: &StorageConnection) -> String {
        storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".into(),
                kind: "full".into(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid
    }

    fn seed_three_modules(storage: &StorageConnection, snap: &str) {
        // http (3 files), core (2), util (1) — inserted out of order.
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_candidates \
                 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
                  module_kind, canonical_root_path, confidence) VALUES \
                 ('mc_util', '{snap}', 'r1', 'dir:src/util', 'directory', 'src/util', 1.0), \
                 ('mc_http', '{snap}', 'r1', 'dir:src/http', 'directory', 'src/http', 1.0), \
                 ('mc_core', '{snap}', 'r1', 'dir:src/core', 'directory', 'src/core', 1.0)"
            ))
            .unwrap();
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_file_ownership \
                 (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) VALUES \
                 ('{snap}', 'r1', 'r1:src/http/a.c', 'mc_http', 'directory', 1.0), \
                 ('{snap}', 'r1', 'r1:src/http/b.c', 'mc_http', 'directory', 1.0), \
                 ('{snap}', 'r1', 'r1:src/http/c.c', 'mc_http', 'directory', 1.0), \
                 ('{snap}', 'r1', 'r1:src/core/x.c', 'mc_core', 'directory', 1.0), \
                 ('{snap}', 'r1', 'r1:src/core/y.c', 'mc_core', 'directory', 1.0), \
                 ('{snap}', 'r1', 'r1:src/util/z.c', 'mc_util', 'directory', 1.0)"
            ))
            .unwrap();
    }

    #[test]
    fn empty_when_no_modules() {
        let storage = setup();
        let snap = snapshot(&storage);
        assert_eq!(storage.list_module_sizes(&snap, ALL).unwrap(), vec![]);
    }

    #[test]
    fn returns_named_modules_ordered_by_size_desc() {
        let storage = setup();
        let snap = snapshot(&storage);
        seed_three_modules(&storage, &snap);

        let got = storage.list_module_sizes(&snap, ALL).unwrap();
        assert_eq!(
            got,
            vec![
                AgentModuleSize {
                    path: "src/http".into(),
                    file_count: 3
                },
                AgentModuleSize {
                    path: "src/core".into(),
                    file_count: 2
                },
                AgentModuleSize {
                    path: "src/util".into(),
                    file_count: 1
                },
            ]
        );
    }

    #[test]
    fn excludes_modules_without_owned_files() {
        let storage = setup();
        let snap = snapshot(&storage);
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_candidates \
                 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
                  module_kind, canonical_root_path, confidence) VALUES \
                 ('mc_empty', '{snap}', 'r1', 'dir:src/empty', 'directory', 'src/empty', 1.0)"
            ))
            .unwrap();
        assert_eq!(storage.list_module_sizes(&snap, ALL).unwrap(), vec![]);
    }

    #[test]
    fn limit_caps_the_returned_set_but_preserves_top_order() {
        // ORIENT-DENSITY-1 §5: a bounded budget limit returns the TOP-`limit`
        // by the (size DESC, …) order — the small/medium headline set — while
        // `--full` (ALL) returns every module. The cut is a prefix of the same
        // total order, so small ⊂ full.
        let storage = setup();
        let snap = snapshot(&storage);
        seed_three_modules(&storage, &snap);

        let top2 = storage.list_module_sizes(&snap, 2).unwrap();
        assert_eq!(
            top2,
            vec![
                AgentModuleSize {
                    path: "src/http".into(),
                    file_count: 3
                },
                AgentModuleSize {
                    path: "src/core".into(),
                    file_count: 2
                },
            ],
            "limit=2 returns the top-2 by size (prefix of the full order)"
        );
        let all = storage.list_module_sizes(&snap, ALL).unwrap();
        assert_eq!(all.len(), 3, "--full (ALL) returns every module");
        assert_eq!(all[..2], top2[..], "small ⊂ full: the cut is a prefix");
    }

    #[test]
    fn doc_inventory_resolves_db_relative_root_path() {
        // Regression for review-1 #1: orient showed NO docs on a real repo
        // because get_doc_inventory treated the DB-parent-RELATIVE root_path as
        // absolute, failed is_dir(), and returned empty. Set up a FILE-backed DB
        // (so conn.path() is real), store root_path relative to the DB parent,
        // drop a README.md in the resolved dir, and assert the inventory
        // resolves it — matching docs_list.
        let tmp = tempdir().unwrap();
        let base = tmp.path();
        let db_dir = base.join("databases");
        std::fs::create_dir_all(&db_dir).unwrap();
        let repo_dir = base.join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("README.md"), "# hi").unwrap();

        let storage = StorageConnection::open(db_dir.join("repo.db")).unwrap();
        storage
            .add_repo(&Repo {
                repo_uid: "r1".into(),
                name: "myrepo".into(),
                // RELATIVE to the DB parent (<base>/databases) — the convention
                // that the old absolute-path read could not resolve.
                root_path: "../myrepo".into(),
                default_branch: Some("main".into()),
                created_at: "2025-01-01T00:00:00.000Z".into(),
                metadata_json: None,
            })
            .unwrap();

        let docs = storage.get_doc_inventory("r1").unwrap();
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(
            paths.contains(&"README.md"),
            "db-relative root_path must resolve to live docs: {paths:?}"
        );
    }

    #[test]
    fn doc_inventory_empty_when_root_missing() {
        // A stored root_path that resolves to a non-directory → graceful empty.
        let tmp = tempdir().unwrap();
        let db_dir = tmp.path().join("databases");
        std::fs::create_dir_all(&db_dir).unwrap();
        let storage = StorageConnection::open(db_dir.join("repo.db")).unwrap();
        storage
            .add_repo(&Repo {
                repo_uid: "r1".into(),
                name: "ghost".into(),
                root_path: "../does-not-exist".into(),
                default_branch: Some("main".into()),
                created_at: "2025-01-01T00:00:00.000Z".into(),
                metadata_json: None,
            })
            .unwrap();
        assert_eq!(storage.get_doc_inventory("r1").unwrap(), vec![]);
    }

    // ── MODULE-MODEL-2 §13 D4: manifest-root read (source_type → toolchain) ────────

    #[test]
    fn manifest_roots_maps_source_type_to_kind_and_filters() {
        use repo_graph_agent::ManifestKind;
        let storage = setup();
        let snap = snapshot(&storage);
        // Four candidate roots; only the cargo + npm manifests are surfaced.
        // pyproject (Python) and a directory-inferred candidate (no manifest
        // evidence) are excluded — the ratified D4 keeps those on the directory
        // heuristic. `module_kind` is 'declared' for all three manifests (proving
        // the read keys on `source_type`, NOT the provenance `module_kind`).
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_candidates \
                 (module_candidate_uid, snapshot_uid, repo_uid, module_key, \
                  module_kind, canonical_root_path, confidence) VALUES \
                 ('mc_rust','{snap}','r1','crate:agent','declared','rust/crates/agent',1.0), \
                 ('mc_ts','{snap}','r1','pkg:api','declared','packages/api',1.0), \
                 ('mc_py','{snap}','r1','py:app','declared','app',1.0), \
                 ('mc_dir','{snap}','r1','dir:src/x','directory','src/x',1.0)"
            ))
            .unwrap();
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_candidate_evidence \
                 (evidence_uid, module_candidate_uid, snapshot_uid, repo_uid, \
                  source_type, source_path, evidence_kind, confidence) VALUES \
                 ('e_rust','mc_rust','{snap}','r1','cargo_toml','rust/crates/agent/Cargo.toml','manifest_declaration',1.0), \
                 ('e_ts','mc_ts','{snap}','r1','package_json','packages/api/package.json','manifest_declaration',1.0), \
                 ('e_py','mc_py','{snap}','r1','pyproject_toml','app/pyproject.toml','manifest_declaration',1.0)"
            ))
            .unwrap();

        let mut roots = storage.list_manifest_roots(&snap).unwrap();
        roots.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(
            roots.len(),
            2,
            "only Rust + TS manifests surface: {roots:?}"
        );
        assert_eq!(roots[0].path, "packages/api");
        assert_eq!(roots[0].kind, ManifestKind::TsPackage);
        assert_eq!(roots[1].path, "rust/crates/agent");
        assert_eq!(roots[1].kind, ManifestKind::RustCrate);
    }

    #[test]
    fn manifest_roots_empty_without_manifest_evidence() {
        // A directory-inferred snapshot (no manifest evidence) → no manifest roots
        // → the fold degrades to directory grouping (honest, no crate/package split).
        let storage = setup();
        let snap = snapshot(&storage);
        seed_three_modules(&storage, &snap); // all 'directory' kind, no evidence
        assert!(storage.list_manifest_roots(&snap).unwrap().is_empty());
    }
}
