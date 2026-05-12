//! Project surface CRUD methods.
//!
//! Read and write operations for the `project_surfaces` and
//! `project_surface_evidence` tables.
//!
//! **Producers:**
//! - TS indexer (legacy path, most surface types)
//! - Rust framework detection (FD-SUPPORT-1, framework-detected surfaces)
//!
//! **Consumers:**
//! - `rmap surfaces list/show` CLI commands
//!
//! Key operations:
//! - `get_project_surfaces_for_snapshot` — read all surfaces with filtering
//! - `get_project_surface_by_ref` — resolve single surface by multiple criteria
//! - `get_project_surface_evidence` — evidence items for one surface
//! - `insert_project_surface` — create a new surface (FD-SUPPORT-1)
//! - `insert_project_surface_evidence` — create evidence for a surface

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::{
    CreateProjectSurfaceEvidenceInput, CreateProjectSurfaceInput, ProjectSurface,
    ProjectSurfaceEvidence,
};

/// Filter options for `get_project_surfaces_for_snapshot`.
#[derive(Debug, Default, Clone)]
pub struct SurfaceFilter {
    /// Filter by surface_kind (exact match).
    pub kind: Option<String>,
    /// Filter by runtime_kind (exact match).
    pub runtime: Option<String>,
    /// Filter by source_type (exact match). Null-safe: rows with NULL
    /// source_type are excluded when this filter is set.
    pub source: Option<String>,
    /// Filter by module (canonical_root_path or module_candidate_uid).
    pub module: Option<String>,
}

impl StorageConnection {
    /// Read all project surfaces for a snapshot, with optional filtering.
    ///
    /// Returns an empty vector if the snapshot has no surfaces.
    ///
    /// Order is deterministic:
    /// 1. root_path ASC
    /// 2. surface_kind ASC
    /// 3. display_name ASC (NULL last)
    /// 4. project_surface_uid ASC
    pub fn get_project_surfaces_for_snapshot(
        &self,
        snapshot_uid: &str,
        filter: &SurfaceFilter,
    ) -> Result<Vec<ProjectSurface>, StorageError> {
        // Build query with optional filter clauses.
        let mut sql = String::from(
            "SELECT ps.project_surface_uid, ps.snapshot_uid, ps.repo_uid,
			        ps.module_candidate_uid, ps.surface_kind, ps.display_name,
			        ps.root_path, ps.entrypoint_path, ps.build_system,
			        ps.runtime_kind, ps.confidence, ps.metadata_json,
			        ps.source_type, ps.source_specific_id, ps.stable_surface_key
			 FROM project_surfaces ps
			 WHERE ps.snapshot_uid = ?",
        );

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(snapshot_uid.to_string())];

        if let Some(kind) = &filter.kind {
            sql.push_str(" AND ps.surface_kind = ?");
            params.push(Box::new(kind.clone()));
        }

        if let Some(runtime) = &filter.runtime {
            sql.push_str(" AND ps.runtime_kind = ?");
            params.push(Box::new(runtime.clone()));
        }

        if let Some(source) = &filter.source {
            sql.push_str(" AND ps.source_type = ?");
            params.push(Box::new(source.clone()));
        }

        if let Some(module) = &filter.module {
            // Match by canonical_root_path or module_candidate_uid.
            // Join to module_candidates to resolve by path.
            sql.push_str(
                " AND (ps.module_candidate_uid = ?
				       OR ps.module_candidate_uid IN (
				           SELECT mc.module_candidate_uid
				           FROM module_candidates mc
				           WHERE mc.snapshot_uid = ps.snapshot_uid
				             AND mc.canonical_root_path = ?
				       ))",
            );
            params.push(Box::new(module.clone()));
            params.push(Box::new(module.clone()));
        }

        // Deterministic ordering per spec.
        sql.push_str(
            " ORDER BY ps.root_path ASC, ps.surface_kind ASC,
			           CASE WHEN ps.display_name IS NULL THEN 1 ELSE 0 END,
			           ps.display_name ASC,
			           ps.project_surface_uid ASC",
        );

        let conn = self.connection();
        let mut stmt = conn.prepare(&sql)?;

        // Convert params to references for rusqlite.
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), ProjectSurface::from_row)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Resolve a project surface by reference within a snapshot.
    ///
    /// Resolution order:
    /// 1. Exact project_surface_uid match
    /// 2. project_surface_uid prefix match (unique)
    /// 3. Exact stable_surface_key match
    /// 4. stable_surface_key prefix match (unique)
    /// 5. Exact display_name match (unique within snapshot)
    ///
    /// Returns:
    /// - `Ok(Some(surface))` if exactly one match
    /// - `Ok(None)` if no match
    /// - `Err(...)` with ambiguity details if multiple matches
    pub fn get_project_surface_by_ref(
        &self,
        snapshot_uid: &str,
        surface_ref: &str,
    ) -> Result<Option<ProjectSurface>, StorageError> {
        let conn = self.connection();

        // 1. Exact project_surface_uid
        {
            let mut stmt = conn.prepare(
                "SELECT project_surface_uid, snapshot_uid, repo_uid,
				        module_candidate_uid, surface_kind, display_name,
				        root_path, entrypoint_path, build_system,
				        runtime_kind, confidence, metadata_json,
				        source_type, source_specific_id, stable_surface_key
				 FROM project_surfaces
				 WHERE snapshot_uid = ? AND project_surface_uid = ?",
            )?;
            let mut rows = stmt.query([snapshot_uid, surface_ref])?;
            if let Some(row) = rows.next()? {
                return Ok(Some(ProjectSurface::from_row(row)?));
            }
        }

        // 2. project_surface_uid prefix (unique)
        {
            let prefix_pattern = format!("{}%", surface_ref);
            let mut stmt = conn.prepare(
                "SELECT project_surface_uid, snapshot_uid, repo_uid,
				        module_candidate_uid, surface_kind, display_name,
				        root_path, entrypoint_path, build_system,
				        runtime_kind, confidence, metadata_json,
				        source_type, source_specific_id, stable_surface_key
				 FROM project_surfaces
				 WHERE snapshot_uid = ? AND project_surface_uid LIKE ?",
            )?;
            let matches: Vec<ProjectSurface> = stmt
                .query_map([snapshot_uid, &prefix_pattern], ProjectSurface::from_row)?
                .collect::<Result<Vec<_>, _>>()?;

            match matches.len() {
                1 => return Ok(Some(matches.into_iter().next().unwrap())),
                n if n > 1 => {
                    return Err(StorageError::Migration(format!(
                        "ambiguous surface ref '{}': {} matches by UID prefix",
                        surface_ref, n
                    )));
                }
                _ => {}
            }
        }

        // 3. Exact stable_surface_key
        {
            let mut stmt = conn.prepare(
                "SELECT project_surface_uid, snapshot_uid, repo_uid,
				        module_candidate_uid, surface_kind, display_name,
				        root_path, entrypoint_path, build_system,
				        runtime_kind, confidence, metadata_json,
				        source_type, source_specific_id, stable_surface_key
				 FROM project_surfaces
				 WHERE snapshot_uid = ? AND stable_surface_key = ?",
            )?;
            let mut rows = stmt.query([snapshot_uid, surface_ref])?;
            if let Some(row) = rows.next()? {
                return Ok(Some(ProjectSurface::from_row(row)?));
            }
        }

        // 4. stable_surface_key prefix (unique)
        {
            let prefix_pattern = format!("{}%", surface_ref);
            let mut stmt = conn.prepare(
                "SELECT project_surface_uid, snapshot_uid, repo_uid,
				        module_candidate_uid, surface_kind, display_name,
				        root_path, entrypoint_path, build_system,
				        runtime_kind, confidence, metadata_json,
				        source_type, source_specific_id, stable_surface_key
				 FROM project_surfaces
				 WHERE snapshot_uid = ? AND stable_surface_key LIKE ?",
            )?;
            let matches: Vec<ProjectSurface> = stmt
                .query_map([snapshot_uid, &prefix_pattern], ProjectSurface::from_row)?
                .collect::<Result<Vec<_>, _>>()?;

            match matches.len() {
                1 => return Ok(Some(matches.into_iter().next().unwrap())),
                n if n > 1 => {
                    return Err(StorageError::Migration(format!(
                        "ambiguous surface ref '{}': {} matches by stable_key prefix",
                        surface_ref, n
                    )));
                }
                _ => {}
            }
        }

        // 5. Exact display_name (unique within snapshot)
        {
            let mut stmt = conn.prepare(
                "SELECT project_surface_uid, snapshot_uid, repo_uid,
				        module_candidate_uid, surface_kind, display_name,
				        root_path, entrypoint_path, build_system,
				        runtime_kind, confidence, metadata_json,
				        source_type, source_specific_id, stable_surface_key
				 FROM project_surfaces
				 WHERE snapshot_uid = ? AND display_name = ?",
            )?;
            let matches: Vec<ProjectSurface> = stmt
                .query_map([snapshot_uid, surface_ref], ProjectSurface::from_row)?
                .collect::<Result<Vec<_>, _>>()?;

            match matches.len() {
                1 => return Ok(Some(matches.into_iter().next().unwrap())),
                n if n > 1 => {
                    return Err(StorageError::Migration(format!(
                        "ambiguous surface ref '{}': {} matches by display_name",
                        surface_ref, n
                    )));
                }
                _ => {}
            }
        }

        Ok(None)
    }

    /// Get evidence items for a project surface.
    ///
    /// Returns evidence ordered by:
    /// 1. source_type ASC
    /// 2. source_path ASC
    /// 3. evidence_kind ASC
    /// 4. project_surface_evidence_uid ASC
    pub fn get_project_surface_evidence(
        &self,
        project_surface_uid: &str,
    ) -> Result<Vec<ProjectSurfaceEvidence>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT project_surface_evidence_uid, project_surface_uid,
			        snapshot_uid, repo_uid, source_type, source_path,
			        evidence_kind, confidence, payload_json
			 FROM project_surface_evidence
			 WHERE project_surface_uid = ?
			 ORDER BY source_type ASC, source_path ASC, evidence_kind ASC,
			          project_surface_evidence_uid ASC",
        )?;

        let rows = stmt.query_map([project_surface_uid], ProjectSurfaceEvidence::from_row)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Count evidence items for all surfaces in a snapshot.
    ///
    /// Returns a map of project_surface_uid -> evidence_count.
    /// Surfaces with zero evidence are not included in the map.
    pub fn count_evidence_by_surface(
        &self,
        snapshot_uid: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT project_surface_uid, COUNT(*) as cnt
			 FROM project_surface_evidence
			 WHERE snapshot_uid = ?
			 GROUP BY project_surface_uid",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;

        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (uid, count) = row?;
            map.insert(uid, count);
        }
        Ok(map)
    }

    // ── Write operations (FD-SUPPORT-1) ───────────────────────────────

    /// Insert a project surface. Returns the generated UID.
    ///
    /// FD-SUPPORT-1: Enables Rust-side framework detection to persist
    /// surfaces. UID is generated as `ps-<uuid-prefix>`.
    ///
    /// The `stable_surface_key` must be unique within the snapshot
    /// (enforced by table constraint).
    pub fn insert_project_surface(
        &mut self,
        input: &CreateProjectSurfaceInput,
    ) -> Result<String, StorageError> {
        let uid = format!("ps-{}", &uuid::Uuid::new_v4().to_string()[..8]);

        let conn = self.connection();
        conn.execute(
            "INSERT INTO project_surfaces (
				project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
				surface_kind, display_name, root_path, entrypoint_path,
				build_system, runtime_kind, confidence, metadata_json,
				source_type, source_specific_id, stable_surface_key
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                uid,
                input.snapshot_uid,
                input.repo_uid,
                input.module_candidate_uid,
                input.surface_kind,
                input.display_name,
                input.root_path,
                input.entrypoint_path,
                input.build_system,
                input.runtime_kind,
                input.confidence,
                input.metadata_json,
                input.source_type,
                input.source_specific_id,
                input.stable_surface_key,
            ],
        )?;

        Ok(uid)
    }

    /// Insert project surface evidence. Returns the generated UID.
    ///
    /// FD-SUPPORT-1: Links detection evidence to a surface.
    /// UID is generated as `pse-<uuid-prefix>`.
    pub fn insert_project_surface_evidence(
        &mut self,
        input: &CreateProjectSurfaceEvidenceInput,
    ) -> Result<String, StorageError> {
        let uid = format!("pse-{}", &uuid::Uuid::new_v4().to_string()[..8]);

        let conn = self.connection();
        conn.execute(
            "INSERT INTO project_surface_evidence (
				project_surface_evidence_uid, project_surface_uid,
				snapshot_uid, repo_uid, source_type, source_path,
				evidence_kind, confidence, payload_json
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                uid,
                input.project_surface_uid,
                input.snapshot_uid,
                input.repo_uid,
                input.source_type,
                input.source_path,
                input.evidence_kind,
                input.confidence,
                input.payload_json,
            ],
        )?;

        Ok(uid)
    }

    /// Batch insert project surfaces (transaction-wrapped).
    ///
    /// Returns a vector of generated UIDs in the same order as inputs.
    /// All inserts succeed or all fail (atomic).
    pub fn insert_project_surfaces_batch(
        &mut self,
        inputs: &[CreateProjectSurfaceInput],
    ) -> Result<Vec<String>, StorageError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.connection_mut();
        let tx = conn.transaction()?;

        let mut uids = Vec::with_capacity(inputs.len());

        {
            let mut stmt = tx.prepare(
                "INSERT INTO project_surfaces (
					project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
					surface_kind, display_name, root_path, entrypoint_path,
					build_system, runtime_kind, confidence, metadata_json,
					source_type, source_specific_id, stable_surface_key
				) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )?;

            for input in inputs {
                let uid = format!("ps-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                stmt.execute(rusqlite::params![
                    uid,
                    input.snapshot_uid,
                    input.repo_uid,
                    input.module_candidate_uid,
                    input.surface_kind,
                    input.display_name,
                    input.root_path,
                    input.entrypoint_path,
                    input.build_system,
                    input.runtime_kind,
                    input.confidence,
                    input.metadata_json,
                    input.source_type,
                    input.source_specific_id,
                    input.stable_surface_key,
                ])?;
                uids.push(uid);
            }
        }

        tx.commit()?;
        Ok(uids)
    }

    /// Batch insert project surface evidence (transaction-wrapped).
    ///
    /// Returns a vector of generated UIDs in the same order as inputs.
    /// All inserts succeed or all fail (atomic).
    pub fn insert_project_surface_evidence_batch(
        &mut self,
        inputs: &[CreateProjectSurfaceEvidenceInput],
    ) -> Result<Vec<String>, StorageError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.connection_mut();
        let tx = conn.transaction()?;

        let mut uids = Vec::with_capacity(inputs.len());

        {
            let mut stmt = tx.prepare(
                "INSERT INTO project_surface_evidence (
					project_surface_evidence_uid, project_surface_uid,
					snapshot_uid, repo_uid, source_type, source_path,
					evidence_kind, confidence, payload_json
				) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )?;

            for input in inputs {
                let uid = format!("pse-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                stmt.execute(rusqlite::params![
                    uid,
                    input.project_surface_uid,
                    input.snapshot_uid,
                    input.repo_uid,
                    input.source_type,
                    input.source_path,
                    input.evidence_kind,
                    input.confidence,
                    input.payload_json,
                ])?;
                uids.push(uid);
            }
        }

        tx.commit()?;
        Ok(uids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::test_helpers::{fresh_storage, make_repo};
    use crate::types::CreateSnapshotInput;

    /// Insert a project surface directly for testing.
    #[allow(clippy::too_many_arguments)]
    fn insert_surface(
        conn: &StorageConnection,
        uid: &str,
        snapshot_uid: &str,
        repo_uid: &str,
        module_candidate_uid: &str,
        surface_kind: &str,
        root_path: &str,
        display_name: Option<&str>,
        runtime_kind: &str,
        source_type: Option<&str>,
        stable_surface_key: Option<&str>,
    ) {
        conn.connection()
            .execute(
                "INSERT INTO project_surfaces
				 (project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
				  surface_kind, display_name, root_path, build_system, runtime_kind,
				  confidence, source_type, source_specific_id, stable_surface_key)
				 VALUES (?, ?, ?, ?, ?, ?, ?, 'npm', ?, 1.0, ?, NULL, ?)",
                rusqlite::params![
                    uid,
                    snapshot_uid,
                    repo_uid,
                    module_candidate_uid,
                    surface_kind,
                    display_name,
                    root_path,
                    runtime_kind,
                    source_type,
                    stable_surface_key,
                ],
            )
            .expect("insert surface");
    }

    fn insert_module_candidate(
        conn: &StorageConnection,
        uid: &str,
        snapshot_uid: &str,
        repo_uid: &str,
        canonical_root_path: &str,
    ) {
        conn.connection()
            .execute(
                "INSERT INTO module_candidates
				 (module_candidate_uid, snapshot_uid, repo_uid, module_key,
				  module_kind, canonical_root_path, confidence)
				 VALUES (?, ?, ?, ?, 'npm_package', ?, 1.0)",
                rusqlite::params![
                    uid,
                    snapshot_uid,
                    repo_uid,
                    format!("npm:{}", uid),
                    canonical_root_path
                ],
            )
            .expect("insert module candidate");
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_evidence(
        conn: &StorageConnection,
        uid: &str,
        surface_uid: &str,
        snapshot_uid: &str,
        repo_uid: &str,
        source_type: &str,
        source_path: &str,
        evidence_kind: &str,
    ) {
        conn.connection()
            .execute(
                "INSERT INTO project_surface_evidence
				 (project_surface_evidence_uid, project_surface_uid, snapshot_uid,
				  repo_uid, source_type, source_path, evidence_kind, confidence)
				 VALUES (?, ?, ?, ?, ?, ?, ?, 1.0)",
                rusqlite::params![
                    uid,
                    surface_uid,
                    snapshot_uid,
                    repo_uid,
                    source_type,
                    source_path,
                    evidence_kind
                ],
            )
            .expect("insert evidence");
    }

    fn setup_test_snapshot(conn: &StorageConnection) -> (String, String) {
        let repo = make_repo("test-repo");
        conn.add_repo(&repo).expect("add repo");

        let snapshot = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.repo_uid.clone(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot");

        (repo.repo_uid, snapshot.snapshot_uid)
    }

    #[test]
    fn get_project_surfaces_returns_empty_for_empty_snapshot() {
        let conn = fresh_storage();
        let (_, snapshot_uid) = setup_test_snapshot(&conn);

        let filter = SurfaceFilter::default();
        let result = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn get_project_surfaces_returns_all_surfaces_sorted() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Insert module candidate.
        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");

        // Insert surfaces in non-sorted order.
        insert_surface(
            &conn,
            "ps-3",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "library",
            "packages/app",
            Some("zlib"),
            "node",
            None,
            None,
        );
        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("app-cli"),
            "node",
            None,
            None,
        );
        insert_surface(
            &conn,
            "ps-2",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            None,
            "node",
            None,
            None,
        );

        let filter = SurfaceFilter::default();
        let result = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");

        assert_eq!(result.len(), 3);
        // Sorted by root_path, then surface_kind, then display_name (NULL last), then uid
        assert_eq!(result[0].project_surface_uid, "ps-1"); // cli_tool, app-cli
        assert_eq!(result[1].project_surface_uid, "ps-2"); // cli_tool, NULL
        assert_eq!(result[2].project_surface_uid, "ps-3"); // library, zlib
    }

    #[test]
    fn get_project_surfaces_filters_by_kind() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");

        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None,
        );
        insert_surface(
            &conn,
            "ps-2",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "library",
            "packages/app",
            Some("lib"),
            "node",
            None,
            None,
        );

        let filter = SurfaceFilter {
            kind: Some("cli_tool".to_string()),
            ..Default::default()
        };
        let result = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface_kind, "cli_tool");
    }

    #[test]
    fn get_project_surfaces_filters_by_runtime() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");

        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None,
        );
        insert_surface(
            &conn,
            "ps-2",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli-rust"),
            "rust_native",
            None,
            None,
        );

        let filter = SurfaceFilter {
            runtime: Some("rust_native".to_string()),
            ..Default::default()
        };
        let result = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].runtime_kind, "rust_native");
    }

    #[test]
    fn get_project_surfaces_filters_by_source() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");

        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "backend_service",
            ".",
            Some("api"),
            "node",
            Some("dockerfile"),
            Some("surface:dockerfile:Dockerfile"),
        );
        insert_surface(
            &conn,
            "ps-2",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None, // legacy row, null source_type
        );

        let filter = SurfaceFilter {
            source: Some("dockerfile".to_string()),
            ..Default::default()
        };
        let result = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source_type, Some("dockerfile".to_string()));
    }

    #[test]
    fn get_project_surfaces_filters_by_module_path() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_module_candidate(&conn, "mc2", &snapshot_uid, &repo_uid, "packages/lib");

        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("app-cli"),
            "node",
            None,
            None,
        );
        insert_surface(
            &conn,
            "ps-2",
            &snapshot_uid,
            &repo_uid,
            "mc2",
            "library",
            "packages/lib",
            Some("lib"),
            "node",
            None,
            None,
        );

        let filter = SurfaceFilter {
            module: Some("packages/app".to_string()),
            ..Default::default()
        };
        let result = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].module_candidate_uid, "mc1");
    }

    #[test]
    fn get_project_surface_by_ref_exact_uid() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-exact-uid",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None,
        );

        let result = conn
            .get_project_surface_by_ref(&snapshot_uid, "ps-exact-uid")
            .expect("query");

        assert!(result.is_some());
        assert_eq!(result.unwrap().project_surface_uid, "ps-exact-uid");
    }

    #[test]
    fn get_project_surface_by_ref_uid_prefix() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-unique-abc123",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None,
        );

        let result = conn
            .get_project_surface_by_ref(&snapshot_uid, "ps-unique-abc")
            .expect("query");

        assert!(result.is_some());
        assert_eq!(result.unwrap().project_surface_uid, "ps-unique-abc123");
    }

    #[test]
    fn get_project_surface_by_ref_stable_key() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "backend_service",
            ".",
            Some("api"),
            "node",
            Some("dockerfile"),
            Some("surface:dockerfile:Dockerfile"),
        );

        let result = conn
            .get_project_surface_by_ref(&snapshot_uid, "surface:dockerfile:Dockerfile")
            .expect("query");

        assert!(result.is_some());
        assert_eq!(
            result.unwrap().stable_surface_key,
            Some("surface:dockerfile:Dockerfile".to_string())
        );
    }

    #[test]
    fn get_project_surface_by_ref_display_name() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("my-unique-cli"),
            "node",
            None,
            None,
        );

        let result = conn
            .get_project_surface_by_ref(&snapshot_uid, "my-unique-cli")
            .expect("query");

        assert!(result.is_some());
        assert_eq!(
            result.unwrap().display_name,
            Some("my-unique-cli".to_string())
        );
    }

    #[test]
    fn get_project_surface_by_ref_returns_none_for_no_match() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None,
        );

        let result = conn
            .get_project_surface_by_ref(&snapshot_uid, "nonexistent")
            .expect("query");

        assert!(result.is_none());
    }

    #[test]
    fn get_project_surface_by_ref_errors_on_ambiguous_uid_prefix() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-ambig-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli1"),
            "node",
            None,
            None,
        );
        insert_surface(
            &conn,
            "ps-ambig-2",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli2"),
            "node",
            None,
            None,
        );

        let result = conn.get_project_surface_by_ref(&snapshot_uid, "ps-ambig");

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("ambiguous"));
        assert!(err_msg.contains("2 matches"));
    }

    #[test]
    fn get_project_surface_evidence_returns_ordered() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None,
        );

        // Insert evidence in non-sorted order.
        insert_evidence(
            &conn,
            "ev-3",
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "package_json",
            "package.json",
            "bin_field",
        );
        insert_evidence(
            &conn,
            "ev-1",
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "dockerfile",
            "Dockerfile",
            "from_instruction",
        );
        insert_evidence(
            &conn,
            "ev-2",
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "dockerfile",
            "Dockerfile",
            "entrypoint",
        );

        let result = conn.get_project_surface_evidence("ps-1").expect("query");

        assert_eq!(result.len(), 3);
        // Sorted by source_type, source_path, evidence_kind, uid
        assert_eq!(result[0].project_surface_evidence_uid, "ev-2"); // dockerfile, Dockerfile, entrypoint
        assert_eq!(result[1].project_surface_evidence_uid, "ev-1"); // dockerfile, Dockerfile, from_instruction
        assert_eq!(result[2].project_surface_evidence_uid, "ev-3"); // package_json, package.json, bin_field
    }

    #[test]
    fn count_evidence_by_surface_returns_counts() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/app");
        insert_surface(
            &conn,
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "cli_tool",
            "packages/app",
            Some("cli"),
            "node",
            None,
            None,
        );
        insert_surface(
            &conn,
            "ps-2",
            &snapshot_uid,
            &repo_uid,
            "mc1",
            "library",
            "packages/app",
            Some("lib"),
            "node",
            None,
            None,
        );

        insert_evidence(
            &conn,
            "ev-1",
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "package_json",
            "package.json",
            "bin",
        );
        insert_evidence(
            &conn,
            "ev-2",
            "ps-1",
            &snapshot_uid,
            &repo_uid,
            "package_json",
            "package.json",
            "main",
        );
        insert_evidence(
            &conn,
            "ev-3",
            "ps-2",
            &snapshot_uid,
            &repo_uid,
            "package_json",
            "package.json",
            "exports",
        );

        let counts = conn
            .count_evidence_by_surface(&snapshot_uid)
            .expect("query");

        assert_eq!(counts.get("ps-1"), Some(&2));
        assert_eq!(counts.get("ps-2"), Some(&1));
    }

    // ── FD-SUPPORT-1: Insert method tests ─────────────────────────────

    #[test]
    fn insert_project_surface_returns_uid_and_persists() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/api");

        let input = CreateProjectSurfaceInput {
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: repo_uid.clone(),
            module_candidate_uid: "mc1".to_string(),
            surface_kind: "http_provider".to_string(),
            display_name: Some("GET /api/users".to_string()),
            root_path: "packages/api".to_string(),
            entrypoint_path: Some("packages/api/src/routes.ts".to_string()),
            build_system: "npm".to_string(),
            runtime_kind: "node".to_string(),
            confidence: 0.9,
            metadata_json: Some(r#"{"framework":"express"}"#.to_string()),
            source_type: "express_route".to_string(),
            source_specific_id: None,
            stable_surface_key: "surface:express_route:GET:/api/users".to_string(),
        };

        let uid = conn.insert_project_surface(&input).expect("insert");

        // UID should have correct prefix
        assert!(
            uid.starts_with("ps-"),
            "UID should start with ps-, got: {}",
            uid
        );

        // Verify round-trip via query
        let filter = SurfaceFilter::default();
        let surfaces = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");

        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0].project_surface_uid, uid);
        assert_eq!(surfaces[0].surface_kind, "http_provider");
        assert_eq!(surfaces[0].display_name, Some("GET /api/users".to_string()));
        assert_eq!(surfaces[0].source_type, Some("express_route".to_string()));
        assert_eq!(
            surfaces[0].stable_surface_key,
            Some("surface:express_route:GET:/api/users".to_string())
        );
    }

    #[test]
    fn insert_project_surface_evidence_returns_uid_and_persists() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/api");

        // First insert a surface
        let surface_input = CreateProjectSurfaceInput {
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: repo_uid.clone(),
            module_candidate_uid: "mc1".to_string(),
            surface_kind: "http_provider".to_string(),
            display_name: Some("GET /api/users".to_string()),
            root_path: "packages/api".to_string(),
            entrypoint_path: None,
            build_system: "npm".to_string(),
            runtime_kind: "node".to_string(),
            confidence: 0.9,
            metadata_json: None,
            source_type: "express_route".to_string(),
            source_specific_id: None,
            stable_surface_key: "surface:express_route:GET:/api/users".to_string(),
        };
        let surface_uid = conn
            .insert_project_surface(&surface_input)
            .expect("insert surface");

        // Now insert evidence
        let evidence_input = CreateProjectSurfaceEvidenceInput {
            project_surface_uid: surface_uid.clone(),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: repo_uid.clone(),
            source_type: "code_detection".to_string(),
            source_path: "packages/api/src/routes.ts".to_string(),
            evidence_kind: "route_registration".to_string(),
            confidence: 0.9,
            payload_json: Some(r#"{"method":"GET","path":"/api/users"}"#.to_string()),
        };

        let evidence_uid = conn
            .insert_project_surface_evidence(&evidence_input)
            .expect("insert evidence");

        // UID should have correct prefix
        assert!(
            evidence_uid.starts_with("pse-"),
            "UID should start with pse-, got: {}",
            evidence_uid
        );

        // Verify round-trip via query
        let evidence_list = conn
            .get_project_surface_evidence(&surface_uid)
            .expect("query");

        assert_eq!(evidence_list.len(), 1);
        assert_eq!(evidence_list[0].project_surface_evidence_uid, evidence_uid);
        assert_eq!(evidence_list[0].source_type, "code_detection");
        assert_eq!(evidence_list[0].evidence_kind, "route_registration");
        assert_eq!(
            evidence_list[0].payload_json,
            Some(r#"{"method":"GET","path":"/api/users"}"#.to_string())
        );
    }

    #[test]
    fn insert_project_surfaces_batch_is_atomic() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/api");

        let inputs = vec![
            CreateProjectSurfaceInput {
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: repo_uid.clone(),
                module_candidate_uid: "mc1".to_string(),
                surface_kind: "http_provider".to_string(),
                display_name: Some("GET /api/users".to_string()),
                root_path: ".".to_string(),
                entrypoint_path: None,
                build_system: "npm".to_string(),
                runtime_kind: "node".to_string(),
                confidence: 0.9,
                metadata_json: None,
                source_type: "express_route".to_string(),
                source_specific_id: None,
                stable_surface_key: "surface:express_route:GET:/api/users".to_string(),
            },
            CreateProjectSurfaceInput {
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: repo_uid.clone(),
                module_candidate_uid: "mc1".to_string(),
                surface_kind: "http_provider".to_string(),
                display_name: Some("POST /api/users".to_string()),
                root_path: ".".to_string(),
                entrypoint_path: None,
                build_system: "npm".to_string(),
                runtime_kind: "node".to_string(),
                confidence: 0.9,
                metadata_json: None,
                source_type: "express_route".to_string(),
                source_specific_id: None,
                stable_surface_key: "surface:express_route:POST:/api/users".to_string(),
            },
        ];

        let uids = conn
            .insert_project_surfaces_batch(&inputs)
            .expect("batch insert");

        assert_eq!(uids.len(), 2);
        assert!(uids[0].starts_with("ps-"));
        assert!(uids[1].starts_with("ps-"));
        assert_ne!(uids[0], uids[1], "UIDs must be unique");

        // Verify all inserted
        let filter = SurfaceFilter::default();
        let surfaces = conn
            .get_project_surfaces_for_snapshot(&snapshot_uid, &filter)
            .expect("query");
        assert_eq!(surfaces.len(), 2);
    }

    #[test]
    fn insert_project_surface_evidence_batch_is_atomic() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/api");

        // Insert a surface first
        let surface_input = CreateProjectSurfaceInput {
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: repo_uid.clone(),
            module_candidate_uid: "mc1".to_string(),
            surface_kind: "http_provider".to_string(),
            display_name: Some("GET /api/users".to_string()),
            root_path: ".".to_string(),
            entrypoint_path: None,
            build_system: "npm".to_string(),
            runtime_kind: "node".to_string(),
            confidence: 0.9,
            metadata_json: None,
            source_type: "express_route".to_string(),
            source_specific_id: None,
            stable_surface_key: "surface:express_route:GET:/api/users".to_string(),
        };
        let surface_uid = conn
            .insert_project_surface(&surface_input)
            .expect("insert surface");

        // Batch insert evidence
        let evidence_inputs = vec![
            CreateProjectSurfaceEvidenceInput {
                project_surface_uid: surface_uid.clone(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: repo_uid.clone(),
                source_type: "code_detection".to_string(),
                source_path: "src/routes.ts".to_string(),
                evidence_kind: "route_registration".to_string(),
                confidence: 0.9,
                payload_json: None,
            },
            CreateProjectSurfaceEvidenceInput {
                project_surface_uid: surface_uid.clone(),
                snapshot_uid: snapshot_uid.clone(),
                repo_uid: repo_uid.clone(),
                source_type: "code_detection".to_string(),
                source_path: "src/routes.ts".to_string(),
                evidence_kind: "handler_attribution".to_string(),
                confidence: 0.8,
                payload_json: Some(r#"{"handler":"getUsers"}"#.to_string()),
            },
        ];

        let evidence_uids = conn
            .insert_project_surface_evidence_batch(&evidence_inputs)
            .expect("batch insert");

        assert_eq!(evidence_uids.len(), 2);
        assert!(evidence_uids[0].starts_with("pse-"));
        assert!(evidence_uids[1].starts_with("pse-"));
        assert_ne!(evidence_uids[0], evidence_uids[1]);

        // Verify all inserted
        let evidence_list = conn
            .get_project_surface_evidence(&surface_uid)
            .expect("query");
        assert_eq!(evidence_list.len(), 2);
    }

    #[test]
    fn insert_project_surfaces_batch_empty_input_returns_empty() {
        let mut conn = fresh_storage();
        let inputs: Vec<CreateProjectSurfaceInput> = vec![];

        let uids = conn
            .insert_project_surfaces_batch(&inputs)
            .expect("batch insert");
        assert!(uids.is_empty());
    }

    #[test]
    fn insert_project_surface_duplicate_stable_key_fails() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc1", &snapshot_uid, &repo_uid, "packages/api");

        let input = CreateProjectSurfaceInput {
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: repo_uid.clone(),
            module_candidate_uid: "mc1".to_string(),
            surface_kind: "http_provider".to_string(),
            display_name: Some("GET /api/users".to_string()),
            root_path: ".".to_string(),
            entrypoint_path: None,
            build_system: "npm".to_string(),
            runtime_kind: "node".to_string(),
            confidence: 0.9,
            metadata_json: None,
            source_type: "express_route".to_string(),
            source_specific_id: None,
            stable_surface_key: "surface:express_route:GET:/api/users".to_string(),
        };

        // First insert should succeed
        let _uid1 = conn.insert_project_surface(&input).expect("first insert");

        // Second insert with same stable_surface_key should fail (UNIQUE constraint)
        let result = conn.insert_project_surface(&input);
        assert!(result.is_err(), "duplicate stable_surface_key should fail");
    }
}
