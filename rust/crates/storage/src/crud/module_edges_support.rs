//! Module edge derivation support — raw fact loading (RS-MG-2).
//!
//! Provides minimal CRUD methods to load the raw facts needed for
//! module edge derivation:
//! - File ownership (file → module)
//!
//! The partitioned import rows live in `crate::import_partition_reads`
//! (TEST-EDGE-SCOPE-1B: one read, every resolution class, the importer's test status).
//!
//! The derivation itself is pure policy and lives in the classification
//! crate. This module only loads the raw facts with minimal DTOs.

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// A file ownership assignment.
///
/// Minimal DTO mapping a file to its owning module candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOwnership {
    pub file_uid: String,
    pub module_candidate_uid: String,
}

impl StorageConnection {
    /// Read all file ownership assignments for a snapshot.
    ///
    /// Returns one row per (file, module) assignment from the
    /// module_file_ownership table. Order is deterministic: sorted by
    /// (file_uid, module_candidate_uid).
    ///
    /// The derivation layer is responsible for detecting duplicate
    /// ownership (multiple modules claiming the same file) and handling
    /// it as an error condition.
    pub fn get_file_ownership_for_snapshot(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<FileOwnership>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT file_uid, module_candidate_uid
			 FROM module_file_ownership
			 WHERE snapshot_uid = ?
			 ORDER BY file_uid ASC, module_candidate_uid ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            Ok(FileOwnership {
                file_uid: row.get("file_uid")?,
                module_candidate_uid: row.get("module_candidate_uid")?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Fallback: derive file ownership from OWNS edges.
    ///
    /// The Rust indexer creates OWNS edges (MODULE → FILE) in the edges
    /// table but does not populate `module_file_ownership`. This method
    /// provides a fallback path for rollup computation.
    ///
    /// Returns ownership facts derived from OWNS edges where:
    /// - source_node is kind = 'MODULE'
    /// - target_node is kind = 'FILE'
    pub fn get_file_ownership_from_owns_edges(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<FileOwnership>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT f.file_uid, m.node_uid AS module_uid
			 FROM edges e
			 JOIN nodes m ON e.source_node_uid = m.node_uid
			 JOIN nodes f ON e.target_node_uid = f.node_uid
			 WHERE e.snapshot_uid = ?
			   AND e.type = 'OWNS'
			   AND m.kind = 'MODULE'
			   AND f.kind = 'FILE'
			 ORDER BY f.file_uid ASC, m.node_uid ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            Ok(FileOwnership {
                file_uid: row.get("file_uid")?,
                module_candidate_uid: row.get("module_uid")?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Fallback: get owned files for rollup from OWNS edges.
    ///
    /// Similar to `get_owned_files_for_rollup` but queries OWNS edges
    /// instead of `module_file_ownership` table.
    ///
    /// Derives file path from FILE node's qualified_name and is_test
    /// from the files table via file_uid.
    pub fn get_owned_files_from_owns_edges(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<OwnedFileForRollup>, StorageError> {
        let conn = self.connection();
        // Join FILE node to files table via file_uid to get is_test.
        // FILE node's qualified_name is the repo-relative path.
        // FILE node's file_uid is the file identity (for modules files command).
        let mut stmt = conn.prepare(
            "SELECT f.file_uid, f.qualified_name AS path, m.node_uid AS module_uid, fi.is_test
			 FROM edges e
			 JOIN nodes m ON e.source_node_uid = m.node_uid
			 JOIN nodes f ON e.target_node_uid = f.node_uid
			 LEFT JOIN files fi ON f.file_uid = fi.file_uid
			 WHERE e.snapshot_uid = ?
			   AND e.type = 'OWNS'
			   AND m.kind = 'MODULE'
			   AND f.kind = 'FILE'
			   AND f.qualified_name IS NOT NULL
			 ORDER BY f.qualified_name ASC, m.node_uid ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            let is_test_int: Option<i64> = row.get("is_test").ok();
            Ok(OwnedFileForRollup {
                file_uid: row.get("file_uid")?,
                file_path: row.get("path")?,
                module_candidate_uid: row.get("module_uid")?,
                is_test: is_test_int.unwrap_or(0) == 1,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Read files owned by a specific module candidate.
    ///
    /// Joins module_file_ownership with files to return ownership
    /// details plus file metadata. Order is deterministic: sorted by
    /// file path.
    ///
    /// Returns empty Vec if the module has no owned files.
    pub fn get_files_for_module(
        &self,
        snapshot_uid: &str,
        module_candidate_uid: &str,
    ) -> Result<Vec<ModuleFileEntry>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT f.file_uid, f.path, f.language,
			        o.assignment_kind, o.confidence
			 FROM module_file_ownership o
			 JOIN files f ON o.file_uid = f.file_uid
			 WHERE o.snapshot_uid = ?
			   AND o.module_candidate_uid = ?
			 ORDER BY f.path ASC",
        )?;

        let rows = stmt.query_map(
            rusqlite::params![snapshot_uid, module_candidate_uid],
            |row| {
                Ok(ModuleFileEntry {
                    file_uid: row.get("file_uid")?,
                    path: row.get("path")?,
                    language: row.get("language")?,
                    assignment_kind: row.get("assignment_kind")?,
                    confidence: row.get("confidence")?,
                })
            },
        )?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Read all owned files for a snapshot with is_test flag.
    ///
    /// Joins module_file_ownership with files to return the fields
    /// needed for rollup computation: file_uid, file_path, module_candidate_uid, is_test.
    ///
    /// Order is deterministic: sorted by (file_path, module_candidate_uid).
    pub fn get_owned_files_for_rollup(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<OwnedFileForRollup>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT f.file_uid, f.path, o.module_candidate_uid, f.is_test
			 FROM module_file_ownership o
			 JOIN files f ON o.file_uid = f.file_uid
			 WHERE o.snapshot_uid = ?
			 ORDER BY f.path ASC, o.module_candidate_uid ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            let is_test_int: i64 = row.get("is_test")?;
            Ok(OwnedFileForRollup {
                file_uid: row.get("file_uid")?,
                file_path: row.get("path")?,
                module_candidate_uid: row.get("module_candidate_uid")?,
                is_test: is_test_int == 1,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ── DEP-1: Dependency reconciliation queries ──────────────────

    /// Read the observed package references `deps list` reconciles for a snapshot.
    ///
    /// Returns (1) unresolved edges classified as `external_library_candidate`
    /// with their specifier and source file UID (the source file UID is resolved
    /// via the source_node's file_uid), plus (2) TS-WORKSPACE-RESOLUTION-1 — the
    /// TS IMPORTS edges bound INFERRED to a workspace member's source entry, each
    /// checked against its own edge by `checked_workspace_import_sites` and
    /// returned as an import site of the package it spells. A carrier that fails
    /// that check makes this read fail with a named error.
    ///
    /// Order is deterministic: sorted by (source_file_uid, specifier).
    pub fn get_external_imports_for_snapshot(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ExternalImportFact>, StorageError> {
        let conn = self.connection();
        // DEPS-CLASSIFIER-1 §2.2: read the DOTTED specifier from `metadata_json.specifier` when the
        // edge carries one, else the raw `target_key`. This corrects ONLY Python IMPORTS edges
        // (whose target_key is the resolver's slash form); Rust (target_key == metadata specifier),
        // Java (no `specifier` key), TS and CALL edges (no `specifier` key) all fall back to
        // target_key unchanged. §2.3: `ue.type` distinguishes import sites from call sites.
        // DEPS-ECOSYSTEM-PARTITION-1 §2.1: also read the SOURCE file's repo-relative path (join
        // `files`, the same INNER JOIN `get_external_imports_with_locations` uses) so the query-time
        // ecosystem partition can gate an observed reference by the importing file's ecosystem — the
        // same `path_ecosystem` predicate the declared side already applies. Every node with a
        // non-null `file_uid` has a `files` row (the join is 1:1, as the two sibling readers rely on),
        // so the row set — and therefore `total_external_imports` — is byte-stable. ORDER BY unchanged.
        let mut stmt = conn.prepare(
            "SELECT n.file_uid AS source_file_uid,
			        f.path AS source_file_path,
			        COALESCE(json_extract(ue.metadata_json, '$.specifier'), ue.target_key) AS specifier,
			        ue.type AS edge_type
			 FROM unresolved_edges ue
			 JOIN nodes n ON ue.source_node_uid = n.node_uid
			 JOIN files f ON n.file_uid = f.file_uid
			 WHERE ue.snapshot_uid = ?
			   AND ue.classification = 'external_library_candidate'
			   AND n.file_uid IS NOT NULL
			 ORDER BY source_file_uid ASC, specifier ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            let edge_type: String = row.get("edge_type")?;
            Ok(ExternalImportFact {
                source_file_uid: row.get("source_file_uid")?,
                source_file_path: row.get("source_file_path")?,
                specifier: row.get("specifier")?,
                is_import_edge: edge_type == "IMPORTS",
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        // TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L07): an import the resolver bound INFERRED to a
        // workspace member's source entry left `unresolved_edges`; it is still an import site of its
        // package, by its spelled specifier — checked against its own edge first.
        for site in self.checked_workspace_import_sites(snapshot_uid, "deps imports")? {
            results.push(ExternalImportFact {
                source_file_uid: site.source_file_uid,
                source_file_path: site.source_file_path,
                specifier: site.specifier,
                is_import_edge: true,
            });
        }
        results.sort_by(|a, b| {
            (&a.source_file_uid, &a.specifier).cmp(&(&b.source_file_uid, &b.specifier))
        });
        Ok(results)
    }

    /// Read import bindings for identifier → specifier resolution.
    ///
    /// Returns all non-relative import bindings from file_signals. Used by
    /// DEP-1 to resolve callee identifiers (e.g., "useState") to their
    /// import specifiers (e.g., "react").
    ///
    /// Relative imports (./foo, ../bar) are excluded since they represent
    /// internal module imports, not external package dependencies.
    ///
    /// Order is deterministic: sorted by (file_uid, identifier).
    pub fn get_external_import_bindings_for_snapshot(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ImportBindingFact>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT file_uid, import_bindings_json
			 FROM file_signals
			 WHERE snapshot_uid = ?
			   AND import_bindings_json IS NOT NULL
			 ORDER BY file_uid ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            let file_uid: String = row.get("file_uid")?;
            let json_str: String = row.get("import_bindings_json")?;
            Ok((file_uid, json_str))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (file_uid, json_str) = row?;
            // Parse the JSON array of import bindings.
            // Format: [{"identifier": "React", "specifier": "react", "is_relative": false}, ...]
            match serde_json::from_str::<Vec<ImportBindingJson>>(&json_str) {
                Ok(bindings) => {
                    for b in bindings {
                        // Skip relative imports — they're internal, not external packages.
                        if b.is_relative {
                            continue;
                        }
                        results.push(ImportBindingFact {
                            file_uid: file_uid.clone(),
                            identifier: b.identifier,
                            specifier: b.specifier,
                            is_relative: b.is_relative,
                            is_type_only: b.is_type_only,
                        });
                    }
                }
                Err(_) => {
                    // Skip malformed JSON rather than failing the whole query.
                    continue;
                }
            }
        }
        Ok(results)
    }

    /// Read package dependency names from file_signals for a snapshot.
    ///
    /// Returns files that have a package_dependencies_json value, along
    /// with the parsed dependency names. Only files with non-null
    /// package dependencies are returned.
    ///
    /// Order is deterministic: sorted by file_path.
    pub fn get_package_dependencies_for_snapshot(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<FilePackageDependencies>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT fs.file_uid, f.path AS file_path, fs.package_dependencies_json
			 FROM file_signals fs
			 JOIN files f ON fs.file_uid = f.file_uid
			 WHERE fs.snapshot_uid = ?
			   AND fs.package_dependencies_json IS NOT NULL
			 ORDER BY f.path ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            let file_uid: String = row.get("file_uid")?;
            let file_path: String = row.get("file_path")?;
            let json_str: String = row.get("package_dependencies_json")?;
            Ok((file_uid, file_path, json_str))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (file_uid, file_path, json_str) = row?;
            // Parse the JSON to extract package names.
            // Format: {"names": ["react", "lodash", ...]}
            match serde_json::from_str::<PackageDependencySetJson>(&json_str) {
                Ok(parsed) => {
                    results.push(FilePackageDependencies {
                        file_uid,
                        file_path,
                        package_names: parsed.names,
                    });
                }
                Err(_) => {
                    // Skip malformed JSON rather than failing the whole query.
                    // This is a degradation signal but not a fatal error.
                    continue;
                }
            }
        }
        Ok(results)
    }

    /// Read external imports with file paths and locations for `deps why`.
    ///
    /// Returns unresolved edges classified as `external_library_candidate`
    /// enriched with file path and line/column information, plus the checked
    /// workspace-bound TS imports of `checked_workspace_import_sites`
    /// (TS-WORKSPACE-RESOLUTION-1), each at its own location; a carrier that
    /// fails the check makes this read fail with a named error. Used for
    /// sample import evidence in the CLI.
    ///
    /// Order is deterministic: sorted by (file_path, line_start, specifier).
    pub fn get_external_imports_with_locations(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ExternalImportWithLocation>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT f.file_uid,
			        f.path AS file_path,
			        ue.target_key AS specifier,
			        ue.line_start,
			        ue.col_start
			 FROM unresolved_edges ue
			 JOIN nodes n ON ue.source_node_uid = n.node_uid
			 JOIN files f ON n.file_uid = f.file_uid
			 WHERE ue.snapshot_uid = ?
			   AND ue.classification = 'external_library_candidate'
			   AND n.file_uid IS NOT NULL
			 ORDER BY file_path ASC, ue.line_start ASC, specifier ASC",
        )?;

        let rows = stmt.query_map([snapshot_uid], |row| {
            Ok(ExternalImportWithLocation {
                file_uid: row.get("file_uid")?,
                file_path: row.get("file_path")?,
                specifier: row.get("specifier")?,
                line_start: row.get("line_start")?,
                col_start: row.get("col_start")?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        // TS-WORKSPACE-RESOLUTION-1: the checked workspace-bound imports, each at its own location.
        for site in self.checked_workspace_import_sites(snapshot_uid, "deps import locations")? {
            results.push(ExternalImportWithLocation {
                file_uid: site.source_file_uid,
                file_path: site.source_file_path,
                specifier: site.specifier,
                line_start: site.line_start,
                col_start: site.col_start,
            });
        }
        results.sort_by(|a, b| {
            (&a.file_path, a.line_start, &a.specifier).cmp(&(
                &b.file_path,
                b.line_start,
                &b.specifier,
            ))
        });
        Ok(results)
    }

    /// TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L07, RG-REQ-002-L04): every IMPORTS edge the resolver
    /// bound INFERRED for the TS extractor (only the workspace import stage writes one) as an
    /// import site of its package — after checking its carrier against its own edge, in Rust (never
    /// with SQL JSON functions, so no unrelated carrier can fail the query):
    ///
    /// the carrier is a JSON object; `basis` is `workspace_source_entry`; `rawPath` is a non-empty
    /// bare package specifier (no leading `.` or `/`, no `:`); `candidates` holds two or more
    /// strings whose first is the edge's target stable key; the target node is a FILE; the
    /// importing node has a file.
    ///
    /// A candidate edge that fails any clause is a NAMED error (the importing file, the line and
    /// the failed clause): the deps answers then fail by name — the import is never dropped (a false
    /// `no static import`) and never counted on unchecked evidence. Inferred edges of every other
    /// producer are not candidates.
    fn checked_workspace_import_sites(
        &self,
        snapshot_uid: &str,
        reader: &str,
    ) -> Result<Vec<WorkspaceImportSite>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT sn.stable_key AS source_key,
			        sn.file_uid AS source_file_uid,
			        f.path AS source_file_path,
			        e.line_start,
			        e.col_start,
			        e.metadata_json,
			        tn.stable_key AS target_key,
			        tn.kind AS target_kind
			 FROM edges e
			 JOIN nodes sn ON sn.node_uid = e.source_node_uid
			 LEFT JOIN files f ON f.file_uid = sn.file_uid
			 LEFT JOIN nodes tn ON tn.node_uid = e.target_node_uid
			 WHERE e.snapshot_uid = ?
			   AND e.type = 'IMPORTS'
			   AND e.resolution = 'inferred'
			   AND substr(e.extractor, 1, 8) = 'ts-core:'
			 ORDER BY source_file_path ASC, e.line_start ASC, e.col_start ASC",
        )?;
        let rows = stmt.query_map([snapshot_uid], |row| {
            Ok(StoredWorkspaceImport {
                source_key: row.get("source_key")?,
                source_file_uid: row.get("source_file_uid")?,
                source_file_path: row.get("source_file_path")?,
                line_start: row.get("line_start")?,
                col_start: row.get("col_start")?,
                metadata_json: row.get("metadata_json")?,
                target_key: row.get("target_key")?,
                target_kind: row.get("target_kind")?,
            })
        })?;
        let mut sites = Vec::new();
        for row in rows {
            sites.push(check_workspace_import(reader, snapshot_uid, row?)?);
        }
        Ok(sites)
    }
}

/// One stored TS inferred IMPORTS edge, as read for the deps check.
struct StoredWorkspaceImport {
    source_key: String,
    source_file_uid: Option<String>,
    source_file_path: Option<String>,
    line_start: Option<i64>,
    col_start: Option<i64>,
    metadata_json: Option<String>,
    target_key: Option<String>,
    target_kind: Option<String>,
}

/// A checked workspace-bound import: one import site of the package it spells.
struct WorkspaceImportSite {
    source_file_uid: String,
    source_file_path: String,
    specifier: String,
    line_start: Option<i64>,
    col_start: Option<i64>,
}

/// Check one stored TS inferred import against its own edge (see
/// `checked_workspace_import_sites`); a failed clause is a named error.
fn check_workspace_import(
    reader: &str,
    snapshot_uid: &str,
    row: StoredWorkspaceImport,
) -> Result<WorkspaceImportSite, StorageError> {
    let at = format!(
        "{} line {}",
        row.source_file_path.as_deref().unwrap_or(&row.source_key),
        row.line_start
            .map(|l| l.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    let fail = |clause: String| {
        StorageError::SerializationError(format!(
            "{reader}: the inferred workspace import at {at} of snapshot {snapshot_uid} is \
             unreadable: {clause} — it can be neither counted nor dropped; run \
             `rmap repo rebuild <path>`"
        ))
    };
    let raw = row
        .metadata_json
        .as_deref()
        .ok_or_else(|| fail("no metadata_json carrier".to_string()))?;
    let carrier = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Object(obj)) => obj,
        Ok(_) | Err(_) => return Err(fail("the carrier is not a JSON object".to_string())),
    };
    match carrier.get("basis") {
        Some(serde_json::Value::String(b)) if b == "workspace_source_entry" => {}
        Some(other) => return Err(fail(format!("basis {other} is not workspace_source_entry"))),
        None => return Err(fail("the carrier has no basis".to_string())),
    }
    let specifier = match carrier.get("rawPath") {
        Some(serde_json::Value::String(p))
            if !p.is_empty() && !p.starts_with('.') && !p.starts_with('/') && !p.contains(':') =>
        {
            p.clone()
        }
        Some(other) => {
            return Err(fail(format!(
                "rawPath {other} is not a bare package specifier"
            )))
        }
        None => return Err(fail("the carrier has no rawPath".to_string())),
    };
    let candidates = match carrier.get("candidates") {
        Some(serde_json::Value::Array(items)) if items.len() >= 2 => items
            .iter()
            .map(|c| c.as_str())
            .collect::<Option<Vec<&str>>>()
            .ok_or_else(|| fail("candidates holds a non-string".to_string()))?,
        Some(_) => {
            return Err(fail(
                "candidates is not an array of two or more entries".to_string(),
            ))
        }
        None => return Err(fail("the carrier has no candidates".to_string())),
    };
    if Some(candidates[0]) != row.target_key.as_deref() {
        return Err(fail(format!(
            "the first candidate {} is not the edge's target",
            candidates[0]
        )));
    }
    if row.target_kind.as_deref() != Some("FILE") {
        return Err(fail(format!(
            "the target is {}, not a FILE",
            row.target_kind.as_deref().unwrap_or("missing")
        )));
    }
    let (Some(source_file_uid), Some(source_file_path)) =
        (row.source_file_uid, row.source_file_path)
    else {
        return Err(fail("the importing node has no file".to_string()));
    };
    Ok(WorkspaceImportSite {
        source_file_uid,
        source_file_path,
        specifier,
        line_start: row.line_start,
        col_start: row.col_start,
    })
}

/// An external import fact from unresolved_edges.
///
/// Used by DEP-1 dependency reconciliation to identify imports
/// classified as external library candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalImportFact {
    /// The file UID of the source file containing the import.
    pub source_file_uid: String,
    /// DEPS-ECOSYSTEM-PARTITION-1 §2.1: the repo-relative path of the source file that CONTAINS this
    /// import (`n.file_uid → files.path`). The query-time ecosystem partition maps this through
    /// `path_ecosystem` to gate an observed reference by the importing file's ecosystem — so a
    /// Python-file import is not reconciled into an npm view (RC-6). Read from the store, not stored
    /// anew.
    pub source_file_path: String,
    /// The import specifier (e.g., "react/jsx-runtime", "tokio::spawn").
    ///
    /// DEPS-CLASSIFIER-1 §2.2: for a Python IMPORTS edge this is the DOTTED specifier read from
    /// `metadata_json.specifier` (`asgiref.sync`), not the slash-form `target_key` the extractor
    /// writes for the resolver (`asgiref/sync`) — so query-time Python head reduction reaches it.
    /// Byte-stable for Rust (target_key == metadata specifier), Java (no `specifier` key → NULL →
    /// falls back to target_key), TS and CALL edges (same fallback).
    pub specifier: String,
    /// DEPS-CLASSIFIER-1 §2.3: whether this evidence is an IMPORTS edge (`type = 'IMPORTS'`) — an
    /// "import site" — versus a CALL edge (a "call site"). Lets the per-package basis report
    /// `used (N import sites, M call sites)` instead of one conflated count.
    pub is_import_edge: bool,
}

/// An external import with file path and location evidence.
///
/// Used by `deps why` to show sample import locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalImportWithLocation {
    /// The file UID.
    pub file_uid: String,
    /// The file path (relative to repo root).
    pub file_path: String,
    /// The import specifier.
    pub specifier: String,
    /// Line number where the import appears (1-indexed, may be None).
    pub line_start: Option<i64>,
    /// Column number where the import starts (0-indexed, may be None).
    pub col_start: Option<i64>,
}

/// Package dependencies declared for a file (from its nearest manifest).
///
/// Used by DEP-1 dependency reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePackageDependencies {
    /// The file UID.
    pub file_uid: String,
    /// The file path (relative to repo root). This is the manifest path.
    pub file_path: String,
    /// Package names declared as dependencies in the nearest manifest.
    pub package_names: Vec<String>,
}

/// JSON structure for package_dependencies_json column.
#[derive(Debug, serde::Deserialize)]
struct PackageDependencySetJson {
    names: Vec<String>,
}

/// JSON structure for import_bindings_json column.
///
/// The persisted array is the camelCase serialization of
/// `repo_graph_classification::types::ImportBinding` (`#[serde(rename_all = "camelCase")]`), so the
/// JSON keys are `isRelative` / `isTypeOnly`. DEPS-CLASSIFIER-1B §2.2 item 3: `is_type_only` reads
/// `isTypeOnly` explicitly so the type-only disposition SURVIVES the read (it was silently dropped
/// before — the struct only projected identifier/specifier/is_relative).
///
/// NOTE (surfaced, out-of-scope): `is_relative` here has NO rename, so it never matches the
/// persisted `isRelative` key and always decodes to its `false` default — a pre-existing casing
/// defect that makes `get_external_import_bindings_for_snapshot`'s relative-skip inert. Left
/// unchanged deliberately: fixing it would alter the reconcile input set (a deps-output /
/// STOP-trigger surface) and belongs in its own slice. The type-only path below reduces via the
/// ecosystem normalizer, which drops relative specifiers regardless of this flag.
#[derive(Debug, serde::Deserialize)]
struct ImportBindingJson {
    identifier: String,
    specifier: String,
    #[serde(default)]
    is_relative: bool,
    #[serde(default, rename = "isTypeOnly")]
    is_type_only: bool,
}

/// An import binding for identifier → specifier resolution.
///
/// Used by DEP-1 to resolve callee identifiers (e.g., "useState") to
/// their import specifiers (e.g., "react").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBindingFact {
    /// The file UID containing this import.
    pub file_uid: String,
    /// The local identifier bound by the import (e.g., "useState", "React").
    pub identifier: String,
    /// The import specifier (e.g., "react", "./utils").
    pub specifier: String,
    /// Whether this is a relative import (./foo, ../bar).
    pub is_relative: bool,
    /// DEPS-CLASSIFIER-1B §2.2 item 3: whether the enclosing statement was `import type …` (or
    /// `export type … from`). The deps assembly reduces a type-only-ONLY package to the
    /// `type_only_import` category — usage that is a compile-time type reference, not a runtime
    /// dependency. Reads the persisted `isTypeOnly` JSON key (survives the storage read).
    pub is_type_only: bool,
}

/// A file ownership fact for rollup computation.
///
/// Minimal DTO combining file identity (path, is_test, file_uid) with module
/// ownership. Used by `get_owned_files_for_rollup` and fallback queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedFileForRollup {
    pub file_uid: String,
    pub file_path: String,
    pub module_candidate_uid: String,
    pub is_test: bool,
}

/// A file owned by a module with ownership metadata.
///
/// Combines data from the `files` table (path, language) with
/// ownership metadata from `module_file_ownership` (assignment_kind, confidence).
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleFileEntry {
    pub file_uid: String,
    pub path: String,
    pub language: Option<String>,
    pub assignment_kind: String,
    pub confidence: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::test_helpers::{fresh_storage, make_file, make_node, make_repo};
    use crate::types::CreateSnapshotInput;

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

    fn insert_file_ownership(
        conn: &StorageConnection,
        snapshot_uid: &str,
        repo_uid: &str,
        file_uid: &str,
        module_candidate_uid: &str,
    ) {
        conn.connection()
            .execute(
                "INSERT INTO module_file_ownership
				 (snapshot_uid, repo_uid, file_uid, module_candidate_uid,
				  assignment_kind, confidence, basis_json)
				 VALUES (?, ?, ?, ?, 'manifest', 1.0, NULL)",
                rusqlite::params![snapshot_uid, repo_uid, file_uid, module_candidate_uid],
            )
            .expect("insert file ownership");
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
				  module_kind, canonical_root_path, confidence, display_name, metadata_json)
				 VALUES (?, ?, ?, ?, 'npm_package', ?, 1.0, NULL, NULL)",
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

    // ── Import bindings tests ──────────────────────────────────────

    /// DEPS-CLASSIFIER-1B §2.2 item 3: the persisted `import_bindings_json` is the camelCase
    /// serialization of `ImportBinding` (`isRelative`, `isTypeOnly`); the read must preserve
    /// `is_type_only`. Before this slice `ImportBindingJson` projected only identifier/specifier/
    /// is_relative, so the flag was silently dropped. Round-trip the exact on-disk shape.
    #[test]
    fn get_external_import_bindings_preserves_is_type_only() {
        let conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);
        let file_uid = format!("{repo_uid}:src/Button.stories.ts");
        // The exact serialized shape produced by `serde_json::to_string(&Vec<ImportBinding>)`
        // (camelCase keys). One value import, one `import type` — both bare packages.
        let json = r#"[
            {"identifier":"React","specifier":"react","isRelative":false,"location":null,"isTypeOnly":false},
            {"identifier":"Meta","specifier":"@storybook/react","isRelative":false,"location":null,"isTypeOnly":true}
        ]"#;
        conn.connection()
            .execute(
                "INSERT INTO file_signals (snapshot_uid, file_uid, import_bindings_json) VALUES (?, ?, ?)",
                rusqlite::params![snapshot_uid, file_uid, json],
            )
            .expect("insert file_signals");

        let bindings = conn
            .get_external_import_bindings_for_snapshot(&snapshot_uid)
            .expect("query bindings");
        let react = bindings
            .iter()
            .find(|b| b.identifier == "React")
            .expect("react binding");
        assert!(!react.is_type_only, "value import is not type-only");
        let meta = bindings
            .iter()
            .find(|b| b.identifier == "Meta")
            .expect("Meta binding");
        assert!(
            meta.is_type_only,
            "the `import type` binding's is_type_only survives the storage read"
        );
    }

    // ── External imports tests ─────────────────────────────────────

    #[test]
    fn external_import_facts_carry_the_source_file_path() {
        // DEPS-ECOSYSTEM-PARTITION-1 §2.1 (DEP-C02): `get_external_imports_for_snapshot` joins
        // `files` and returns the SOURCE file's repo-relative path, so the query-time ecosystem
        // partition can gate an observed reference by the importing file's ecosystem. No schema
        // change, no new persisted column — the path is read from the existing `files` row.
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);
        let file = make_file(&repo_uid, "django/tasks/base.py");
        conn.upsert_files(std::slice::from_ref(&file))
            .expect("upsert file");
        let node = make_node("n1", &snapshot_uid, &repo_uid, "k1", &file.file_uid, "base");
        conn.insert_nodes(&[node]).expect("insert node");
        conn.connection()
            .execute(
                "INSERT INTO unresolved_edges \
                 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, \
                  resolution, extractor, category, classification, classifier_version, \
                  basis_code, observed_at) \
                 VALUES ('u1', ?, ?, 'n1', 'asgiref.sync', 'IMPORTS', 'unresolved', 't', \
                  'imports_file_not_found', 'external_library_candidate', 1, \
                  'no_supporting_signal', '2024-01-01T00:00:00Z')",
                rusqlite::params![snapshot_uid, repo_uid],
            )
            .expect("insert unresolved edge");

        let facts = conn
            .get_external_imports_for_snapshot(&snapshot_uid)
            .expect("query external imports");
        assert_eq!(facts.len(), 1, "one external import fact expected");
        assert_eq!(
            facts[0].source_file_path, "django/tasks/base.py",
            "fact carries the source file's repo-relative path"
        );
        assert_eq!(facts[0].specifier, "asgiref.sync");
        assert!(facts[0].is_import_edge, "an IMPORTS edge is an import site");
    }

    // ── TS-WORKSPACE-RESOLUTION-1: the deps readers keep a workspace-bound import ──

    /// A TS FILE node for `path` (stable key `<repo>:<path>:FILE`, node uid `n:<path>`) with its
    /// `files` row.
    fn ts_file_node(conn: &StorageConnection, snap: &str, repo: &str, path: &str, kind: &str) {
        conn.connection()
            .execute(
                "INSERT OR IGNORE INTO files (file_uid, repo_uid, path) VALUES (?, ?, ?)",
                rusqlite::params![format!("{repo}:{path}"), repo, path],
            )
            .expect("insert file");
        conn.connection()
            .execute(
                "INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, name, kind, file_uid) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    format!("n:{path}"),
                    snap,
                    repo,
                    format!("{repo}:{path}:{kind}"),
                    path,
                    kind,
                    format!("{repo}:{path}")
                ],
            )
            .expect("insert node");
    }

    /// One IMPORTS edge from `n:<from>` to `n:<to>` at `line`/`col`.
    #[allow(clippy::too_many_arguments)]
    fn import_edge(
        conn: &StorageConnection,
        snap: &str,
        repo: &str,
        uid: &str,
        from: &str,
        to: &str,
        resolution: &str,
        extractor: &str,
        line: i64,
        carrier: Option<&str>,
    ) {
        conn.connection()
            .execute(
                "INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, \
                 type, resolution, extractor, line_start, col_start, metadata_json) \
                 VALUES (?, ?, ?, ?, ?, 'IMPORTS', ?, ?, ?, 0, ?)",
                rusqlite::params![
                    uid,
                    snap,
                    repo,
                    format!("n:{from}"),
                    format!("n:{to}"),
                    resolution,
                    extractor,
                    line,
                    carrier
                ],
            )
            .expect("insert edge");
    }

    /// One unresolved external-library IMPORTS row from `n:<from>`.
    fn external_row(
        conn: &StorageConnection,
        snap: &str,
        repo: &str,
        uid: &str,
        from: &str,
        spec: &str,
        line: i64,
    ) {
        conn.connection()
            .execute(
                "INSERT INTO unresolved_edges \
                 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, \
                  resolution, extractor, line_start, col_start, category, classification, \
                  classifier_version, basis_code, observed_at) \
                 VALUES (?, ?, ?, ?, ?, 'IMPORTS', 'static', 'ts-core:0.2.0', ?, 0, \
                  'imports_file_not_found', 'external_library_candidate', 1, \
                  'specifier_matches_package_dependency', '2024-01-01T00:00:00Z')",
                rusqlite::params![uid, snap, repo, format!("n:{from}"), spec, line],
            )
            .expect("insert unresolved edge");
    }

    const SERVER: &str = "packages/api/src/server.ts";
    const ENGINE_SRC: &str = "packages/engine/src/index.ts";

    /// FRAKTAG's shape: server.ts:6 imports `@fraktag/engine` (bound INFERRED to the engine's
    /// source entry) and server.ts:3 imports `express` (an unresolved external row).
    fn fraktag_store(
        carrier: Option<&str>,
        target_kind: &str,
    ) -> (StorageConnection, String, String) {
        let conn = fresh_storage();
        let (repo, snap) = setup_test_snapshot(&conn);
        ts_file_node(&conn, &snap, &repo, SERVER, "FILE");
        ts_file_node(&conn, &snap, &repo, ENGINE_SRC, target_kind);
        import_edge(
            &conn,
            &snap,
            &repo,
            "e-ws",
            SERVER,
            ENGINE_SRC,
            "inferred",
            "ts-core:0.2.0",
            6,
            carrier,
        );
        external_row(&conn, &snap, &repo, "u-express", SERVER, "express", 3);
        (conn, repo, snap)
    }

    fn workspace_carrier(repo: &str) -> String {
        serde_json::json!({
            "rawPath": "@fraktag/engine",
            "isTypeOnly": false,
            "basis": "workspace_source_entry",
            "candidates": [
                format!("{repo}:{ENGINE_SRC}:FILE"),
                format!("{repo}:packages/engine/dist/index.js:FILE"),
            ],
        })
        .to_string()
    }

    #[test]
    fn external_import_facts_include_checked_workspace_inferred_imports_by_their_spelled_specifier()
    {
        let conn = fresh_storage();
        let (repo, snap) = setup_test_snapshot(&conn);
        ts_file_node(&conn, &snap, &repo, SERVER, "FILE");
        ts_file_node(&conn, &snap, &repo, ENGINE_SRC, "FILE");
        ts_file_node(&conn, &snap, &repo, "packages/api/src/routes.ts", "FILE");
        let carrier = workspace_carrier(&repo);
        import_edge(
            &conn,
            &snap,
            &repo,
            "e-ws",
            SERVER,
            ENGINE_SRC,
            "inferred",
            "ts-core:0.2.0",
            6,
            Some(&carrier),
        );
        // A certain TS import is not an external import.
        import_edge(
            &conn,
            &snap,
            &repo,
            "e-rel",
            SERVER,
            "packages/api/src/routes.ts",
            "static",
            "ts-core:0.2.0",
            2,
            Some(
                r#"{"rawPath":"./routes","resolvedPath":"packages/api/src/routes.ts","isTypeOnly":false}"#,
            ),
        );
        external_row(&conn, &snap, &repo, "u-express", SERVER, "express", 3);

        let facts = conn
            .get_external_imports_for_snapshot(&snap)
            .expect("query external imports");
        let file_uid = format!("{repo}:{SERVER}");
        assert_eq!(
            facts,
            vec![
                ExternalImportFact {
                    source_file_uid: file_uid.clone(),
                    source_file_path: SERVER.to_string(),
                    specifier: "@fraktag/engine".to_string(),
                    is_import_edge: true,
                },
                ExternalImportFact {
                    source_file_uid: file_uid,
                    source_file_path: SERVER.to_string(),
                    specifier: "express".to_string(),
                    is_import_edge: true,
                },
            ],
            "the workspace import is one import site of its package, by its spelled specifier, \
             in the reader's order"
        );
    }

    #[test]
    fn external_imports_with_locations_include_checked_workspace_inferred_imports() {
        let (conn, repo, snap) = fraktag_store(None, "FILE");
        // Replace the carrier-less edge with a checked one.
        conn.connection()
            .execute(
                "UPDATE edges SET metadata_json = ? WHERE edge_uid = 'e-ws'",
                [workspace_carrier(&repo)],
            )
            .unwrap();
        let rows = conn
            .get_external_imports_with_locations(&snap)
            .expect("query locations");
        let file_uid = format!("{repo}:{SERVER}");
        assert_eq!(
            rows,
            vec![
                ExternalImportWithLocation {
                    file_uid: file_uid.clone(),
                    file_path: SERVER.to_string(),
                    specifier: "express".to_string(),
                    line_start: Some(3),
                    col_start: Some(0),
                },
                ExternalImportWithLocation {
                    file_uid,
                    file_path: SERVER.to_string(),
                    specifier: "@fraktag/engine".to_string(),
                    line_start: Some(6),
                    col_start: Some(0),
                },
            ],
            "the workspace import keeps its own line and column, ordered by file, line, specifier"
        );
    }

    #[test]
    fn inferred_imports_of_another_producer_never_join_the_external_import_facts() {
        let conn = fresh_storage();
        let (repo, snap) = setup_test_snapshot(&conn);
        ts_file_node(&conn, &snap, &repo, "src/event/ngx_event.c", "FILE");
        ts_file_node(&conn, &snap, &repo, "src/core/ngx_core.h", "FILE");
        ts_file_node(&conn, &snap, &repo, "pkg/app.py", "FILE");
        ts_file_node(&conn, &snap, &repo, "pkg/sub.py", "FILE");
        import_edge(
            &conn,
            &snap,
            &repo,
            "e-c",
            "src/event/ngx_event.c",
            "src/core/ngx_core.h",
            "inferred",
            "c-core:0.1.0",
            1,
            Some(&format!(
                r#"{{"isTypeOnly":false,"basis":"unique_basename","candidates":["{repo}:src/core/ngx_core.h:FILE"]}}"#
            )),
        );
        import_edge(
            &conn,
            &snap,
            &repo,
            "e-py",
            "pkg/app.py",
            "pkg/sub.py",
            "inferred",
            "python-core:0.2.0",
            1,
            Some(&format!(
                r#"{{"importedName":"sub","alternateTarget":"{repo}:pkg/__init__.py:FILE","basis":"python_submodule"}}"#
            )),
        );
        // Even a malformed carrier of another producer never fails or joins the reads.
        import_edge(
            &conn,
            &snap,
            &repo,
            "e-py2",
            "pkg/app.py",
            "pkg/sub.py",
            "inferred",
            "python-core:0.2.0",
            2,
            Some("not json"),
        );
        assert_eq!(
            conn.get_external_imports_for_snapshot(&snap).unwrap(),
            vec![]
        );
        assert_eq!(
            conn.get_external_imports_with_locations(&snap).unwrap(),
            vec![]
        );
    }

    #[test]
    fn a_malformed_workspace_carrier_fails_both_deps_reads_by_name_never_dropped() {
        let src = |repo: &str| format!("{repo}:{ENGINE_SRC}:FILE");
        let dec = |repo: &str| format!("{repo}:packages/engine/dist/index.js:FILE");
        // (carrier builder, target kind, the clause the error names)
        type Case = (
            Box<dyn Fn(&str) -> Option<String>>,
            &'static str,
            &'static str,
        );
        let cases: Vec<Case> = vec![
            (Box::new(|_| None), "FILE", "no metadata_json carrier"),
            (
                Box::new(|_| Some("not json".into())),
                "FILE",
                "not a JSON object",
            ),
            (
                Box::new(|_| Some("[1]".into())),
                "FILE",
                "not a JSON object",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "@fraktag/engine", "candidates": [src(r), dec(r)]}).to_string())
                }),
                "FILE",
                "basis",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "@fraktag/engine", "basis": "unique_basename", "candidates": [src(r), dec(r)]}).to_string())
                }),
                "FILE",
                "basis",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"basis": "workspace_source_entry", "candidates": [src(r), dec(r)]}).to_string())
                }),
                "FILE",
                "rawPath",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "./engine", "basis": "workspace_source_entry", "candidates": [src(r), dec(r)]}).to_string())
                }),
                "FILE",
                "rawPath",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "npm:@fraktag/engine", "basis": "workspace_source_entry", "candidates": [src(r), dec(r)]}).to_string())
                }),
                "FILE",
                "rawPath",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "@fraktag/engine", "basis": "workspace_source_entry", "candidates": [dec(r), src(r)]}).to_string())
                }),
                "FILE",
                "first candidate",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "@fraktag/engine", "basis": "workspace_source_entry", "candidates": [src(r)]}).to_string())
                }),
                "FILE",
                "candidates",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "@fraktag/engine", "basis": "workspace_source_entry", "candidates": [src(r), 7]}).to_string())
                }),
                "FILE",
                "candidates",
            ),
            (
                Box::new(move |r| {
                    Some(serde_json::json!({"rawPath": "@fraktag/engine", "basis": "workspace_source_entry", "candidates": [format!("{r}:{ENGINE_SRC}:SYMBOL"), dec(r)]}).to_string())
                }),
                "SYMBOL",
                "not a FILE",
            ),
        ];
        for (carrier, kind, clause) in &cases {
            let conn = fresh_storage();
            let (repo, snap) = setup_test_snapshot(&conn);
            ts_file_node(&conn, &snap, &repo, SERVER, "FILE");
            ts_file_node(&conn, &snap, &repo, ENGINE_SRC, kind);
            let c = carrier(&repo);
            import_edge(
                &conn,
                &snap,
                &repo,
                "e-ws",
                SERVER,
                ENGINE_SRC,
                "inferred",
                "ts-core:0.2.0",
                6,
                c.as_deref(),
            );
            external_row(&conn, &snap, &repo, "u-express", SERVER, "express", 3);
            for (reader, err) in [
                (
                    "get_external_imports_for_snapshot",
                    conn.get_external_imports_for_snapshot(&snap).err(),
                ),
                (
                    "get_external_imports_with_locations",
                    conn.get_external_imports_with_locations(&snap).err(),
                ),
            ] {
                let err =
                    err.unwrap_or_else(|| panic!("{reader} {c:?}: a named error, never a count"));
                let msg = err.to_string();
                assert!(
                    msg.contains(SERVER) && msg.contains("line 6") && msg.contains(clause),
                    "{reader} {c:?}: the error names the file, the line and the clause `{clause}`: {msg}"
                );
            }
        }
    }

    // ── File ownership tests ───────────────────────────────────────

    #[test]
    fn get_file_ownership_returns_empty_for_empty_snapshot() {
        let conn = fresh_storage();
        let (_, snapshot_uid) = setup_test_snapshot(&conn);

        let result = conn
            .get_file_ownership_for_snapshot(&snapshot_uid)
            .expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn get_file_ownership_returns_all_assignments() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create module candidates
        insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");
        insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

        // Create files
        let file_a = make_file(&repo_uid, "packages/app/index.ts");
        let file_b = make_file(&repo_uid, "packages/core/lib.ts");
        conn.upsert_files(&[file_a.clone(), file_b.clone()])
            .expect("upsert files");

        // Create ownership
        insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_a.file_uid, "mc-app");
        insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_b.file_uid, "mc-core");

        let result = conn
            .get_file_ownership_for_snapshot(&snapshot_uid)
            .expect("query");

        assert_eq!(result.len(), 2);
        // Sorted by file_uid
        let file_a_ownership = result
            .iter()
            .find(|o| o.file_uid == file_a.file_uid)
            .expect("find file_a");
        assert_eq!(file_a_ownership.module_candidate_uid, "mc-app");
    }

    #[test]
    fn get_file_ownership_returns_duplicate_assignments() {
        // The CRUD method should return ALL assignments, including duplicates.
        // The derivation layer is responsible for detecting and handling them.
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create two module candidates
        insert_module_candidate(&conn, "mc-1", &snapshot_uid, &repo_uid, "packages/mod1");
        insert_module_candidate(&conn, "mc-2", &snapshot_uid, &repo_uid, "packages/mod2");

        // Create one file
        let file = make_file(&repo_uid, "shared/utils.ts");
        conn.upsert_files(std::slice::from_ref(&file))
            .expect("upsert file");

        // Assign the same file to BOTH modules (duplicate ownership)
        insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file.file_uid, "mc-1");
        insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file.file_uid, "mc-2");

        let result = conn
            .get_file_ownership_for_snapshot(&snapshot_uid)
            .expect("query");

        // Should return both assignments — derivation layer handles the error
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn get_file_ownership_is_scoped_to_snapshot() {
        let mut conn = fresh_storage();
        let repo = make_repo("test-repo");
        conn.add_repo(&repo).expect("add repo");

        // Create two snapshots
        let snap1 = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.repo_uid.clone(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot 1");

        let snap2 = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.repo_uid.clone(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot 2");

        // Set up module and file in snapshot 1 only
        insert_module_candidate(&conn, "mc-1", &snap1.snapshot_uid, &repo.repo_uid, "pkg");
        let file = make_file(&repo.repo_uid, "pkg/index.ts");
        conn.upsert_files(std::slice::from_ref(&file))
            .expect("upsert file");
        insert_file_ownership(
            &conn,
            &snap1.snapshot_uid,
            &repo.repo_uid,
            &file.file_uid,
            "mc-1",
        );

        // Snapshot 1 has ownership
        let result1 = conn
            .get_file_ownership_for_snapshot(&snap1.snapshot_uid)
            .expect("query");
        assert_eq!(result1.len(), 1);

        // Snapshot 2 has no ownership
        let result2 = conn
            .get_file_ownership_for_snapshot(&snap2.snapshot_uid)
            .expect("query");
        assert!(result2.is_empty());
    }

    // ── get_files_for_module tests ─────────────────────────────────

    #[test]
    fn get_files_for_module_returns_empty_for_empty_module() {
        let conn = fresh_storage();
        let (_, snapshot_uid) = setup_test_snapshot(&conn);

        let result = conn
            .get_files_for_module(&snapshot_uid, "nonexistent-module")
            .expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn get_files_for_module_returns_owned_files() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create module
        insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");

        // Create files
        let file_a = make_file(&repo_uid, "packages/app/index.ts");
        let file_b = make_file(&repo_uid, "packages/app/utils.ts");
        conn.upsert_files(&[file_a.clone(), file_b.clone()])
            .expect("upsert files");

        // Create ownership
        insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_a.file_uid, "mc-app");
        insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file_b.file_uid, "mc-app");

        let result = conn
            .get_files_for_module(&snapshot_uid, "mc-app")
            .expect("query");

        assert_eq!(result.len(), 2);
        // Sorted by path
        assert_eq!(result[0].path, "packages/app/index.ts");
        assert_eq!(result[1].path, "packages/app/utils.ts");
    }

    #[test]
    fn get_files_for_module_includes_all_fields() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create module
        insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

        // Create file with language
        let mut file = make_file(&repo_uid, "packages/core/lib.ts");
        file.language = Some("typescript".to_string());
        conn.upsert_files(std::slice::from_ref(&file))
            .expect("upsert file");

        // Create ownership
        insert_file_ownership(&conn, &snapshot_uid, &repo_uid, &file.file_uid, "mc-core");

        let result = conn
            .get_files_for_module(&snapshot_uid, "mc-core")
            .expect("query");

        assert_eq!(result.len(), 1);
        let entry = &result[0];
        assert_eq!(entry.file_uid, file.file_uid);
        assert_eq!(entry.path, "packages/core/lib.ts");
        assert_eq!(entry.language, Some("typescript".to_string()));
        assert_eq!(entry.assignment_kind, "manifest");
        assert!((entry.confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn get_files_for_module_only_returns_files_for_specified_module() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create two modules
        insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");
        insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

        // Create files
        let file_app = make_file(&repo_uid, "packages/app/index.ts");
        let file_core = make_file(&repo_uid, "packages/core/lib.ts");
        conn.upsert_files(&[file_app.clone(), file_core.clone()])
            .expect("upsert files");

        // Assign to different modules
        insert_file_ownership(
            &conn,
            &snapshot_uid,
            &repo_uid,
            &file_app.file_uid,
            "mc-app",
        );
        insert_file_ownership(
            &conn,
            &snapshot_uid,
            &repo_uid,
            &file_core.file_uid,
            "mc-core",
        );

        // Query for app module only
        let result = conn
            .get_files_for_module(&snapshot_uid, "mc-app")
            .expect("query");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "packages/app/index.ts");
    }

    #[test]
    fn get_files_for_module_is_scoped_to_snapshot() {
        let mut conn = fresh_storage();
        let repo = make_repo("test-repo");
        conn.add_repo(&repo).expect("add repo");

        // Create two snapshots
        let snap1 = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.repo_uid.clone(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot 1");

        let snap2 = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.repo_uid.clone(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot 2");

        // Set up module and file in snapshot 1 only
        insert_module_candidate(&conn, "mc-1", &snap1.snapshot_uid, &repo.repo_uid, "pkg");
        let file = make_file(&repo.repo_uid, "pkg/index.ts");
        conn.upsert_files(std::slice::from_ref(&file))
            .expect("upsert file");
        insert_file_ownership(
            &conn,
            &snap1.snapshot_uid,
            &repo.repo_uid,
            &file.file_uid,
            "mc-1",
        );

        // Snapshot 1 has files for module
        let result1 = conn
            .get_files_for_module(&snap1.snapshot_uid, "mc-1")
            .expect("query");
        assert_eq!(result1.len(), 1);

        // Snapshot 2 has no files for this module
        let result2 = conn
            .get_files_for_module(&snap2.snapshot_uid, "mc-1")
            .expect("query");
        assert!(result2.is_empty());
    }

    // ── get_owned_files_for_rollup tests ───────────────────────────

    #[test]
    fn get_owned_files_for_rollup_returns_empty_for_empty_snapshot() {
        let conn = fresh_storage();
        let (_, snapshot_uid) = setup_test_snapshot(&conn);

        let result = conn
            .get_owned_files_for_rollup(&snapshot_uid)
            .expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn get_owned_files_for_rollup_returns_all_owned_files() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create modules
        insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");
        insert_module_candidate(&conn, "mc-core", &snapshot_uid, &repo_uid, "packages/core");

        // Create files
        let file_app = make_file(&repo_uid, "packages/app/index.ts");
        let file_core = make_file(&repo_uid, "packages/core/lib.ts");
        conn.upsert_files(&[file_app.clone(), file_core.clone()])
            .expect("upsert files");

        // Create ownership
        insert_file_ownership(
            &conn,
            &snapshot_uid,
            &repo_uid,
            &file_app.file_uid,
            "mc-app",
        );
        insert_file_ownership(
            &conn,
            &snapshot_uid,
            &repo_uid,
            &file_core.file_uid,
            "mc-core",
        );

        let result = conn
            .get_owned_files_for_rollup(&snapshot_uid)
            .expect("query");

        assert_eq!(result.len(), 2);
        // Sorted by path
        assert_eq!(result[0].file_path, "packages/app/index.ts");
        assert_eq!(result[0].module_candidate_uid, "mc-app");
        assert_eq!(result[1].file_path, "packages/core/lib.ts");
        assert_eq!(result[1].module_candidate_uid, "mc-core");
    }

    #[test]
    fn get_owned_files_for_rollup_includes_is_test_flag() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        insert_module_candidate(&conn, "mc-app", &snapshot_uid, &repo_uid, "packages/app");

        // Create non-test and test files
        let mut file_src = make_file(&repo_uid, "packages/app/service.ts");
        file_src.is_test = false;
        let mut file_test = make_file(&repo_uid, "packages/app/service.test.ts");
        file_test.is_test = true;
        conn.upsert_files(&[file_src.clone(), file_test.clone()])
            .expect("upsert files");

        insert_file_ownership(
            &conn,
            &snapshot_uid,
            &repo_uid,
            &file_src.file_uid,
            "mc-app",
        );
        insert_file_ownership(
            &conn,
            &snapshot_uid,
            &repo_uid,
            &file_test.file_uid,
            "mc-app",
        );

        let result = conn
            .get_owned_files_for_rollup(&snapshot_uid)
            .expect("query");

        assert_eq!(result.len(), 2);
        // Sorted by path
        let src = result
            .iter()
            .find(|r| r.file_path.contains("service.ts") && !r.file_path.contains(".test."))
            .unwrap();
        let test = result
            .iter()
            .find(|r| r.file_path.contains(".test."))
            .unwrap();

        assert!(!src.is_test);
        assert!(test.is_test);
    }

    // ── OWNS edge fallback tests ───────────────────────────────────

    use crate::types::{GraphEdge, GraphNode};

    fn make_module_node(
        node_uid: &str,
        snapshot_uid: &str,
        repo_uid: &str,
        module_key: &str,
        canonical_path: &str,
    ) -> GraphNode {
        GraphNode {
            node_uid: node_uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            stable_key: format!("{}:{}:MODULE", repo_uid, canonical_path),
            kind: "MODULE".to_string(),
            subtype: None,
            name: module_key.to_string(),
            qualified_name: Some(canonical_path.to_string()),
            file_uid: None,
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: None,
        }
    }

    fn make_file_node(
        node_uid: &str,
        snapshot_uid: &str,
        repo_uid: &str,
        file_path: &str,
        file_uid: &str,
    ) -> GraphNode {
        GraphNode {
            node_uid: node_uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            stable_key: format!("{}:{}:FILE", repo_uid, file_path),
            kind: "FILE".to_string(),
            subtype: None,
            name: file_path
                .rsplit('/')
                .next()
                .unwrap_or(file_path)
                .to_string(),
            qualified_name: Some(file_path.to_string()),
            file_uid: Some(file_uid.to_string()),
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: None,
            doc_comment: None,
            metadata_json: None,
        }
    }

    fn make_owns_edge(
        edge_uid: &str,
        snapshot_uid: &str,
        repo_uid: &str,
        module_node_uid: &str,
        file_node_uid: &str,
    ) -> GraphEdge {
        GraphEdge {
            edge_uid: edge_uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            source_node_uid: module_node_uid.to_string(),
            target_node_uid: file_node_uid.to_string(),
            edge_type: "OWNS".to_string(),
            resolution: "static".to_string(),
            extractor: "test".to_string(),
            location: None,
            metadata_json: None,
        }
    }

    #[test]
    fn get_file_ownership_from_owns_edges_returns_empty_for_empty_snapshot() {
        let conn = fresh_storage();
        let (_, snapshot_uid) = setup_test_snapshot(&conn);

        let result = conn
            .get_file_ownership_from_owns_edges(&snapshot_uid)
            .expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn get_file_ownership_from_owns_edges_returns_ownership() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create MODULE node
        let module_node =
            make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

        // Create file record (required for FK constraint on nodes.file_uid)
        let file_record = make_file(&repo_uid, "packages/app/index.ts");
        conn.upsert_files(std::slice::from_ref(&file_record))
            .expect("upsert file");

        // Create FILE node with matching file_uid
        let file = make_file_node(
            "file-1",
            &snapshot_uid,
            &repo_uid,
            "packages/app/index.ts",
            &file_record.file_uid,
        );

        conn.insert_nodes(&[module_node, file])
            .expect("insert nodes");

        // Create OWNS edge: module -> file
        let owns = make_owns_edge("owns-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
        conn.insert_edges(&[owns]).expect("insert edge");

        let result = conn
            .get_file_ownership_from_owns_edges(&snapshot_uid)
            .expect("query");

        assert_eq!(result.len(), 1);
        // The result.file_uid is the FILE node's file_uid field (reference to files table)
        assert_eq!(result[0].file_uid, file_record.file_uid);
        assert_eq!(result[0].module_candidate_uid, "mod-1");
    }

    #[test]
    fn get_file_ownership_from_owns_edges_excludes_non_owns_edges() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        let module_node =
            make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

        // Create file record (required for FK constraint on nodes.file_uid)
        let file_record = make_file(&repo_uid, "packages/app/index.ts");
        conn.upsert_files(std::slice::from_ref(&file_record))
            .expect("upsert file");

        let file = make_file_node(
            "file-1",
            &snapshot_uid,
            &repo_uid,
            "packages/app/index.ts",
            &file_record.file_uid,
        );
        conn.insert_nodes(&[module_node, file])
            .expect("insert nodes");

        // Create IMPORTS edge (not OWNS)
        let mut edge = make_owns_edge("edge-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
        edge.edge_type = "IMPORTS".to_string();
        conn.insert_edges(&[edge]).expect("insert edge");

        let result = conn
            .get_file_ownership_from_owns_edges(&snapshot_uid)
            .expect("query");

        assert!(result.is_empty());
    }

    #[test]
    fn get_owned_files_from_owns_edges_returns_empty_for_empty_snapshot() {
        let conn = fresh_storage();
        let (_, snapshot_uid) = setup_test_snapshot(&conn);

        let result = conn
            .get_owned_files_from_owns_edges(&snapshot_uid)
            .expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn get_owned_files_from_owns_edges_returns_all_fields() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        // Create MODULE node
        let module_node =
            make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

        // Create file in files table (for is_test lookup)
        let file_record = make_file(&repo_uid, "packages/app/service.ts");
        conn.upsert_files(std::slice::from_ref(&file_record))
            .expect("upsert file");

        // Create FILE node with matching file_uid
        let file_node = make_file_node(
            "file-1",
            &snapshot_uid,
            &repo_uid,
            "packages/app/service.ts",
            &file_record.file_uid,
        );

        conn.insert_nodes(&[module_node, file_node])
            .expect("insert nodes");

        // Create OWNS edge
        let owns = make_owns_edge("owns-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
        conn.insert_edges(&[owns]).expect("insert edge");

        let result = conn
            .get_owned_files_from_owns_edges(&snapshot_uid)
            .expect("query");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_path, "packages/app/service.ts");
        assert_eq!(result[0].module_candidate_uid, "mod-1");
        assert!(!result[0].is_test);
    }

    #[test]
    fn get_owned_files_from_owns_edges_preserves_is_test() {
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);

        let module_node =
            make_module_node("mod-1", &snapshot_uid, &repo_uid, "app", "packages/app");

        // Create test file
        let mut file_record = make_file(&repo_uid, "packages/app/service.test.ts");
        file_record.is_test = true;
        conn.upsert_files(std::slice::from_ref(&file_record))
            .expect("upsert file");

        let file_node = make_file_node(
            "file-1",
            &snapshot_uid,
            &repo_uid,
            "packages/app/service.test.ts",
            &file_record.file_uid,
        );

        conn.insert_nodes(&[module_node, file_node])
            .expect("insert nodes");

        let owns = make_owns_edge("owns-1", &snapshot_uid, &repo_uid, "mod-1", "file-1");
        conn.insert_edges(&[owns]).expect("insert edge");

        let result = conn
            .get_owned_files_from_owns_edges(&snapshot_uid)
            .expect("query");

        assert_eq!(result.len(), 1);
        assert!(result[0].is_test);
    }
}
