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
//! The store reads take a `&rusqlite::Connection` (the adapter's own backing
//! store) and the shared `map_err` from `agent_impl`, so error mapping is
//! identical to every other `AgentStorageRead` method. The doc inventory is
//! connection-free (`doc_inventory_at_root`, STATE-ROOT-RELATIVE-REPO-ROOT-1): it
//! takes the repository root the caller resolved from the daemon registry.

use std::path::Path;

use repo_graph_agent::{
    AgentDirectoryGroup, AgentDocEntry, AgentModuleSize, AgentStorageError, ManifestKind,
    ManifestRoot,
};
use rusqlite::Connection;

use crate::agent_impl::map_err;

/// Discover the live documentation inventory under `root` — the repository's working
/// tree as the caller resolved it (the daemon passes its registry `canonical_path`,
/// STATE-ROOT-RELATIVE-REPO-ROOT-1 / D-SRR-SCOPE-1). Connection-free: this function
/// never reads the store, so the stored `repos.root_path` (relative to the store
/// file's directory; retained for compatibility, not a resolution source —
/// D-SRR-ROOTPATH-1) can never change which tree is read.
///
/// Outcomes:
/// - `root` does not exist or is not a directory → `Err` whose message is
///   `repo root not found: <root>` — a named state, NEVER an empty inventory (an
///   empty list would render as "no docs found", a false absence).
/// - discovery fails while walking a directory that DOES exist (permissions, a
///   mid-walk failure) → `Err` with the discovery error (ORIENT-DENSITY-1 review-2 #3:
///   a failure is never collapsed to an empty inventory).
/// - otherwise `Ok(entries)`; an empty vector is a real, observed absence of docs.
///
/// `compute_hashes = true` (ORIENT-DENSITY-1 review-1 fix #1): the same content-based
/// classification `docs list` uses, so content-evidence kinds (`license`,
/// `release-notes`) fire; the hashes themselves are discarded (the DTO carries only
/// path / kind / generated).
pub fn doc_inventory_at_root(root: &Path) -> Result<Vec<AgentDocEntry>, AgentStorageError> {
    if !root.is_dir() {
        return Err(AgentStorageError::new(
            "get_doc_inventory",
            format!("repo root not found: {}", root.display()),
        ));
    }
    let result = repo_graph_doc_facts::discover_doc_inventory(root, true)
        .map_err(map_err("get_doc_inventory"))?;

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

/// The `AgentStorageRead::get_doc_inventory` answer of a bare store connection: a
/// NAMED error, never a list. A store connection holds no repository root — the
/// working tree is found from the daemon registry (D-SRR-SCOPE-1), and the stored
/// `repos.root_path` is not a resolution source (D-SRR-ROOTPATH-1) — so it cannot
/// read the inventory, and it must not answer with an empty list (a false absence).
/// Callers that hold the root call [`doc_inventory_at_root`].
pub(crate) fn doc_inventory_without_root(
    repo_uid: &str,
) -> Result<Vec<AgentDocEntry>, AgentStorageError> {
    Err(AgentStorageError::new(
        "get_doc_inventory",
        format!(
            "no repository root supplied for {repo_uid}: a store connection does not resolve \
             a working tree; read the inventory with doc_inventory_at_root(<registry root>)"
        ),
    ))
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
    // ORIENT-SEGMENT-2 §2.2: also project the DECLARED name (`display_name`) and the
    // `module_key` (whose source prefix names the owning manifest), so each row is
    // self-describing — orient can render `name [manifest]` on a collision/divergence
    // WITHOUT a path-keyed side map (two modules can share a `canonical_root_path`).
    let mut stmt = conn
        .prepare(
            "SELECT mc.canonical_root_path AS path, COUNT(o.file_uid) AS file_count, \
                    mc.display_name AS display_name, mc.module_key AS module_key \
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
            let display_name: Option<String> = row.get(2)?;
            let module_key: Option<String> = row.get(3)?;
            Ok(AgentModuleSize {
                path: row.get::<_, String>(0)?,
                file_count: row.get::<_, i64>(1)? as u64,
                name: display_name.filter(|n| !n.trim().is_empty()),
                manifest: module_key
                    .as_deref()
                    .and_then(manifest_for_module_key)
                    .map(str::to_string),
            })
        })
        .map_err(map_err("list_module_sizes"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_err("list_module_sizes"))
}

/// ORIENT-SEGMENT-2 §2.2 / MODULES-IDENTITY-2 §2.1: map a `module_key`
/// (`<source>:<repo_uid>:<root>`) to the owning manifest filename. The source
/// prefix is the indexer's own deterministic tag (each
/// `indexer::*::generate_module_key`), so this is a total, typo-proof mapping — an
/// unknown / inferred prefix returns `None` (no manifest), never a guessed file.
///
/// `pub` + re-exported at the crate root (`repo_graph_storage::manifest_for_module_key`)
/// so `modules list` disambiguates twin display names from the SAME derivation
/// `orient` uses (one implementation, never a second copy — MODULES-IDENTITY-2 §2.1).
/// It is consumed by `module_sizes` here (the orient data path) and by the daemon's
/// `handle_modules_list` (the modules-list data path).
pub fn manifest_for_module_key(module_key: &str) -> Option<&'static str> {
    match module_key.split(':').next() {
        Some("cargo") => Some("Cargo.toml"),
        Some("pyproject") => Some("pyproject.toml"),
        Some("npm") => Some("package.json"),
        Some("gradle") => Some("settings.gradle"),
        // "inferred" / "directory" / anything else -> no manifest declared it.
        _ => None,
    }
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
///
/// TEST-EDGE-SCOPE-1A (D-TESA-11): each group also carries `test_file_count` — its
/// OWNS-owned FILE nodes whose stored `files.is_test = 1` — so the package-group
/// `(N test)` count reads the stored fact, never a directory name. An owned FILE
/// node with no `files` row has an UNKNOWN test status: the read errors naming the
/// directory rather than counting it as non-test (RG-REQ-002-L04). `file_count` is
/// unchanged (the joins are to primary keys, so no OWNS row is duplicated or lost).
pub(crate) fn directory_groups(
    conn: &Connection,
    snapshot_uid: &str,
) -> Result<Vec<AgentDirectoryGroup>, AgentStorageError> {
    let mut stmt = conn
        .prepare(
            "SELECT m.qualified_name AS path, COUNT(o.target_node_uid) AS file_count, \
                    COALESCE(SUM(CASE WHEN f.is_test = 1 THEN 1 ELSE 0 END), 0) \
                      AS test_file_count, \
                    COALESCE(SUM(CASE WHEN t.kind = 'FILE' AND f.file_uid IS NULL \
                      THEN 1 ELSE 0 END), 0) AS unknown_test_status \
             FROM nodes m \
             JOIN edges o \
               ON o.source_node_uid = m.node_uid \
               AND o.snapshot_uid = ?1 \
               AND o.type = 'OWNS' \
             LEFT JOIN nodes t ON t.node_uid = o.target_node_uid \
             LEFT JOIN files f ON f.file_uid = t.file_uid \
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
            Ok((
                AgentDirectoryGroup {
                    path: row.get::<_, String>(0)?,
                    file_count: row.get::<_, i64>(1)? as u64,
                    test_file_count: row.get::<_, i64>(2)? as u64,
                },
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(map_err("list_directory_groups"))?;

    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_err("list_directory_groups"))?;
    let mut groups = Vec::with_capacity(rows.len());
    for (group, unknown_test_status) in rows {
        if unknown_test_status > 0 {
            return Err(AgentStorageError::new(
                "list_directory_groups",
                format!(
                    "directory {} owns {} FILE node(s) with no files row, so its stored \
                     test count (is_test) is unknown",
                    group.path, unknown_test_status
                ),
            ));
        }
        groups.push(group);
    }
    Ok(groups)
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
                    file_count: 3,
                    name: None,
                    manifest: None,
                },
                AgentModuleSize {
                    path: "src/core".into(),
                    file_count: 2,
                    name: None,
                    manifest: None,
                },
                AgentModuleSize {
                    path: "src/util".into(),
                    file_count: 1,
                    name: None,
                    manifest: None,
                },
            ]
        );
    }

    #[test]
    fn declared_module_carries_name_and_derived_manifest() {
        // ORIENT-SEGMENT-2 §2.2: a manifest-declared module surfaces its display_name
        // + the manifest derived from the module_key source prefix; an inferred module
        // carries neither (honest — no manifest declared it).
        let storage = setup();
        let snap = snapshot(&storage);
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO module_candidates \
                 (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, \
                  canonical_root_path, confidence, display_name) VALUES \
                 ('mc_py', '{snap}', 'r1', 'pyproject:r1:.', 'declared', '.', 1.0, 'Django'), \
                 ('mc_dir', '{snap}', 'r1', 'inferred:r1:src', 'inferred', 'src', 1.0, NULL); \
                 INSERT INTO module_file_ownership \
                 (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) VALUES \
                 ('{snap}', 'r1', 'r1:./a.py', 'mc_py', 'directory', 1.0), \
                 ('{snap}', 'r1', 'r1:src/b.py', 'mc_dir', 'directory', 1.0)"
            ))
            .unwrap();
        let got = storage.list_module_sizes(&snap, ALL).unwrap();
        let py = got.iter().find(|m| m.path == ".").unwrap();
        assert_eq!(py.name.as_deref(), Some("Django"));
        assert_eq!(py.manifest.as_deref(), Some("pyproject.toml"));
        let dir = got.iter().find(|m| m.path == "src").unwrap();
        assert_eq!(dir.name, None, "inferred module has no declared name");
        assert_eq!(dir.manifest, None, "inferred module has no manifest");
    }

    #[test]
    fn manifest_prefixes_map_to_files() {
        assert_eq!(manifest_for_module_key("cargo:r:."), Some("Cargo.toml"));
        assert_eq!(
            manifest_for_module_key("pyproject:django:."),
            Some("pyproject.toml")
        );
        assert_eq!(
            manifest_for_module_key("npm:r:packages/plugins"),
            Some("package.json")
        );
        assert_eq!(
            manifest_for_module_key("gradle:k:core"),
            Some("settings.gradle")
        );
        assert_eq!(manifest_for_module_key("inferred:r:src"), None);
        assert_eq!(manifest_for_module_key("weird"), None);
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
                    file_count: 3,
                    name: None,
                    manifest: None,
                },
                AgentModuleSize {
                    path: "src/core".into(),
                    file_count: 2,
                    name: None,
                    manifest: None,
                },
            ],
            "limit=2 returns the top-2 by size (prefix of the full order)"
        );
        let all = storage.list_module_sizes(&snap, ALL).unwrap();
        assert_eq!(all.len(), 3, "--full (ALL) returns every module");
        assert_eq!(all[..2], top2[..], "small ⊂ full: the cut is a prefix");
    }

    // ── STATE-ROOT-RELATIVE-REPO-ROOT-1: the doc inventory reads the root it is given ───────────

    /// A file-backed store under `<base>/databases/repo.db` whose repo row stores
    /// `stored_root_path` (relative to the store directory — the index-time convention).
    fn store_with_root_path(base: &std::path::Path, stored_root_path: &str) -> StorageConnection {
        let db_dir = base.join("databases");
        std::fs::create_dir_all(&db_dir).unwrap();
        let storage = StorageConnection::open(db_dir.join("repo.db")).unwrap();
        storage
            .add_repo(&Repo {
                repo_uid: "r1".into(),
                name: "myrepo".into(),
                root_path: stored_root_path.into(),
                default_branch: Some("main".into()),
                created_at: "2025-01-01T00:00:00.000Z".into(),
                metadata_json: None,
            })
            .unwrap();
        storage
    }

    fn paths_of(docs: &[AgentDocEntry]) -> Vec<&str> {
        docs.iter().map(|d| d.path.as_str()).collect()
    }

    #[test]
    fn doc_inventory_reads_the_root_it_is_given() {
        let tmp = tempdir().unwrap();
        let repo_dir = tmp.path().join("myrepo");
        std::fs::create_dir_all(repo_dir.join("docs")).unwrap();
        std::fs::write(repo_dir.join("README.md"), "# hi").unwrap();
        std::fs::write(repo_dir.join("CONTRIBUTING.md"), "# contributing").unwrap();

        let docs = doc_inventory_at_root(&repo_dir).unwrap();
        let paths = paths_of(&docs);
        assert!(paths.contains(&"README.md"), "{paths:?}");
        assert!(paths.contains(&"CONTRIBUTING.md"), "{paths:?}");
        let readme = docs.iter().find(|d| d.path == "README.md").unwrap();
        assert_eq!(readme.kind, "readme");
    }

    #[test]
    fn doc_inventory_names_a_missing_root() {
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let err = doc_inventory_at_root(&missing).unwrap_err();
        assert_eq!(
            err.message,
            format!("repo root not found: {}", missing.display()),
            "a missing root is a named state, never an empty inventory"
        );
    }

    #[test]
    fn doc_inventory_names_a_root_that_is_a_file() {
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("README.md");
        std::fs::write(&file, "# not a directory").unwrap();
        let err = doc_inventory_at_root(&file).unwrap_err();
        assert_eq!(
            err.message,
            format!("repo root not found: {}", file.display()),
            "a regular file at the root path is the same named state as a missing root"
        );
    }

    #[test]
    fn doc_inventory_ignores_a_wrong_stored_root_path() {
        // The store's `repos.root_path` points at an EXISTING different directory with its own
        // docs. The inventory of the given root lists only the given root's docs, and the store
        // connection's port read does not fall back to the stored field.
        let tmp = tempdir().unwrap();
        let base = tmp.path();
        let repo_dir = base.join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("README.md"), "# real").unwrap();
        let other = base.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("ARCHITECTURE.md"), "# wrong tree").unwrap();
        std::fs::write(other.join("NEWS"), "wrong tree").unwrap();
        let storage = store_with_root_path(base, "../other");

        let docs = doc_inventory_at_root(&repo_dir).unwrap();
        assert_eq!(paths_of(&docs), vec!["README.md"]);
        assert!(
            storage.get_doc_inventory("r1").is_err(),
            "the store's port read never resolves the stored root_path"
        );
    }

    #[test]
    fn port_doc_inventory_without_a_root_is_a_named_error() {
        // The bare store connection has no repository root: its port read is a NAMED error,
        // never an empty list (which would render as "no docs found"), whatever the stored
        // root_path says — here it names an existing directory holding a README.
        let tmp = tempdir().unwrap();
        let base = tmp.path();
        std::fs::create_dir_all(base.join("myrepo")).unwrap();
        std::fs::write(base.join("myrepo").join("README.md"), "# hi").unwrap();
        let storage = store_with_root_path(base, "../myrepo");

        let err = storage.get_doc_inventory("r1").unwrap_err();
        assert_eq!(err.operation, "get_doc_inventory");
        assert!(
            err.message.contains("no repository root supplied for r1"),
            "names the missing root: {}",
            err.message
        );
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
