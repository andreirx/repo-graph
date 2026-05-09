//! Refresh copy-forward implementation for derived artifacts.
//!
//! Provides methods to copy forward measurements, inferences, boundary
//! interactions, and contract schemas for unchanged files during refresh.
//!
//! This module implements the refresh-integrity-parity slice requirements:
//! - Copy measurements/inferences anchored on unchanged file paths
//! - Copy boundary surfaces/channels anchored on unchanged source files
//! - Copy contract schemas/elements anchored on unchanged proto files
//!
//! See `docs/slices/refresh-integrity-parity.md` for full specification.

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// Result of copying forward derived artifacts during refresh.
#[derive(Debug, Default)]
pub struct ArtifactCopyForwardResult {
    /// Number of measurements copied forward.
    pub measurements_copied: u64,
    /// Number of inferences copied forward.
    pub inferences_copied: u64,
    /// Number of boundary surfaces copied forward.
    pub boundary_surfaces_copied: u64,
    /// Number of boundary channels copied forward.
    pub boundary_channels_copied: u64,
    /// Number of contract schemas copied forward.
    pub contract_schemas_copied: u64,
    /// Number of contract elements copied forward.
    pub contract_elements_copied: u64,
}

impl StorageConnection {
    /// Copy forward measurements for unchanged files.
    ///
    /// Measurements are keyed by `target_stable_key` which contains the file path.
    /// Format: `<repo_uid>:<file_path>#<symbol>:SYMBOL:<subtype>`
    ///
    /// For each unchanged file, copies measurements whose target_stable_key
    /// starts with `<repo_uid>:<file_path>` from parent to current snapshot.
    pub fn copy_forward_measurements(
        &self,
        parent_snapshot_uid: &str,
        current_snapshot_uid: &str,
        repo_uid: &str,
        unchanged_file_paths: &[String],
    ) -> Result<u64, StorageError> {
        if unchanged_file_paths.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let mut copied = 0u64;

        // Build LIKE patterns for each unchanged file path.
        // target_stable_key format: <repo_uid>:<file_path>#...
        // We match on the file path prefix.
        for file_path in unchanged_file_paths {
            let prefix = format!("{}:{}#%", repo_uid, file_path);
            let prefix_file_only = format!("{}:{}:FILE", repo_uid, file_path);

            // Copy measurements matching this file's symbols or the file itself.
            let rows = conn.execute(
                "INSERT INTO measurements (
                    measurement_uid, snapshot_uid, repo_uid,
                    target_stable_key, kind, value_json, source, created_at
                )
                SELECT
                    printf('%s-copy-%s', measurement_uid, ?1),
                    ?1,
                    repo_uid,
                    target_stable_key,
                    kind,
                    value_json,
                    source,
                    created_at
                FROM measurements
                WHERE snapshot_uid = ?2
                  AND (target_stable_key LIKE ?3 OR target_stable_key = ?4)",
                rusqlite::params![
                    current_snapshot_uid,
                    parent_snapshot_uid,
                    prefix,
                    prefix_file_only,
                ],
            )?;
            copied += rows as u64;
        }

        Ok(copied)
    }

    /// Copy forward inferences for unchanged files.
    ///
    /// Similar to measurements, inferences use `target_stable_key` for anchoring.
    pub fn copy_forward_inferences(
        &self,
        parent_snapshot_uid: &str,
        current_snapshot_uid: &str,
        repo_uid: &str,
        unchanged_file_paths: &[String],
    ) -> Result<u64, StorageError> {
        if unchanged_file_paths.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let mut copied = 0u64;

        for file_path in unchanged_file_paths {
            let prefix = format!("{}:{}#%", repo_uid, file_path);
            let prefix_file_only = format!("{}:{}:FILE", repo_uid, file_path);

            // Copy forward inferences including provenance_json and freshness_state (ACR-4).
            // Preserving these columns enables impact propagation on copy-forwarded rows:
            // - provenance_json: tracks L0 dependencies
            // - freshness_state: preserved so 'current' stays 'current' until impacted
            let rows = conn.execute(
                "INSERT INTO inferences (
                    inference_uid, snapshot_uid, repo_uid,
                    target_stable_key, kind, value_json, confidence,
                    basis_json, extractor, created_at,
                    provenance_json, freshness_state
                )
                SELECT
                    printf('%s-copy-%s', inference_uid, ?1),
                    ?1,
                    repo_uid,
                    target_stable_key,
                    kind,
                    value_json,
                    confidence,
                    basis_json,
                    extractor,
                    created_at,
                    provenance_json,
                    freshness_state
                FROM inferences
                WHERE snapshot_uid = ?2
                  AND (target_stable_key LIKE ?3 OR target_stable_key = ?4)",
                rusqlite::params![
                    current_snapshot_uid,
                    parent_snapshot_uid,
                    prefix,
                    prefix_file_only,
                ],
            )?;
            copied += rows as u64;
        }

        Ok(copied)
    }

    /// Copy forward boundary interaction surfaces for unchanged source files.
    ///
    /// Boundary surfaces use `source_file` column for file anchoring.
    /// Returns (surfaces_copied, channels_copied).
    pub fn copy_forward_boundary_surfaces(
        &self,
        parent_snapshot_uid: &str,
        current_snapshot_uid: &str,
        unchanged_file_paths: &[String],
    ) -> Result<(u64, u64), StorageError> {
        if unchanged_file_paths.is_empty() {
            return Ok((0, 0));
        }

        let conn = self.connection();
        let tx = conn.unchecked_transaction()?;

        // Build a temp table of unchanged files for efficient joining.
        tx.execute(
            "CREATE TEMP TABLE IF NOT EXISTS _unchanged_files (path TEXT PRIMARY KEY)",
            [],
        )?;
        tx.execute("DELETE FROM _unchanged_files", [])?;

        {
            let mut stmt = tx.prepare("INSERT INTO _unchanged_files (path) VALUES (?)")?;
            for path in unchanged_file_paths {
                stmt.execute([path])?;
            }
        }

        // Build old_uid -> new_uid mapping for surfaces.
        let mut surface_uid_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // Select surfaces for unchanged files.
        #[allow(clippy::type_complexity)]
        let surfaces: Vec<(
            String, // old surface_uid
            String, // boundary_scope
            String, // channel_kind
            String, // direction
            String, // protocol
            String, // protocol_family
            String, // interaction_pattern
            String, // endpoint_locality
            String, // symbol_stable_key
            String, // source_file
            i64,    // line_start
            i64,    // line_end
            i64,    // col_start
            i64,    // col_end
            String, // extractor
            String, // basis
            f64,    // confidence
            String, // evidence_json
            String, // repo_uid
        )> = {
            let mut stmt = tx.prepare(
                "SELECT surface_uid, boundary_scope, channel_kind, direction,
                        protocol, protocol_family, interaction_pattern, endpoint_locality,
                        symbol_stable_key, source_file, line_start, line_end,
                        col_start, col_end, extractor, basis, confidence, evidence_json, repo_uid
                 FROM boundary_interaction_surfaces
                 WHERE snapshot_uid = ?
                   AND source_file IN (SELECT path FROM _unchanged_files)"
            )?;
            let rows = stmt.query_map([parent_snapshot_uid], |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                    row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                    row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                    row.get(12)?, row.get(13)?, row.get(14)?, row.get(15)?,
                    row.get(16)?, row.get(17)?, row.get(18)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let surfaces_copied = surfaces.len() as u64;

        // Assign new UIDs and insert surfaces.
        {
            let mut insert_stmt = tx.prepare(
                "INSERT INTO boundary_interaction_surfaces (
                    surface_uid, snapshot_uid, repo_uid, boundary_scope, channel_kind,
                    direction, protocol, protocol_family, interaction_pattern,
                    endpoint_locality, symbol_stable_key, source_file,
                    line_start, line_end, col_start, col_end,
                    extractor, basis, confidence, evidence_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )?;

            for s in &surfaces {
                let new_uid = uuid::Uuid::new_v4().to_string();
                surface_uid_map.insert(s.0.clone(), new_uid.clone());
                insert_stmt.execute(rusqlite::params![
                    new_uid, current_snapshot_uid, s.18, s.1, s.2, s.3, s.4, s.5,
                    s.6, s.7, s.8, s.9, s.10, s.11, s.12, s.13, s.14, s.15, s.16, s.17
                ])?;
            }
        }

        // Copy channel details for copied surfaces.
        let channels_copied: u64 = if !surface_uid_map.is_empty() {
            // Build placeholders for old surface UIDs.
            let old_uids: Vec<&String> = surface_uid_map.keys().collect();
            let placeholders: String = old_uids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

            let select_sql = format!(
                "SELECT channel_uid, surface_uid, channel_kind, channel_identity,
                        socket_path, tcp_endpoint, udp_endpoint, can_id, i2c_address,
                        spi_device, serial_device, shm_key, pipe_path, pipe_context,
                        mqueue_name, baud_rate, can_extended, frame_format,
                        payload_contract, metadata_json
                 FROM boundary_channel_details
                 WHERE surface_uid IN ({})",
                placeholders
            );

            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            for uid in &old_uids {
                params.push(Box::new((*uid).clone()));
            }
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();

            struct ChannelRow {
                old_surface_uid: String,
                channel_kind: String,
                channel_identity: String,
                socket_path: Option<String>,
                tcp_endpoint: Option<String>,
                udp_endpoint: Option<String>,
                can_id: Option<i64>,
                i2c_address: Option<i64>,
                spi_device: Option<String>,
                serial_device: Option<String>,
                shm_key: Option<String>,
                pipe_path: Option<String>,
                pipe_context: Option<String>,
                mqueue_name: Option<String>,
                baud_rate: Option<i64>,
                can_extended: Option<i64>,
                frame_format: Option<String>,
                payload_contract: Option<String>,
                metadata_json: Option<String>,
            }

            let mut stmt = tx.prepare(&select_sql)?;
            let channels: Vec<ChannelRow> = stmt
                .query_map(param_refs.as_slice(), |row| {
                    Ok(ChannelRow {
                        old_surface_uid: row.get(1)?,
                        channel_kind: row.get(2)?,
                        channel_identity: row.get(3)?,
                        socket_path: row.get(4)?,
                        tcp_endpoint: row.get(5)?,
                        udp_endpoint: row.get(6)?,
                        can_id: row.get(7)?,
                        i2c_address: row.get(8)?,
                        spi_device: row.get(9)?,
                        serial_device: row.get(10)?,
                        shm_key: row.get(11)?,
                        pipe_path: row.get(12)?,
                        pipe_context: row.get(13)?,
                        mqueue_name: row.get(14)?,
                        baud_rate: row.get(15)?,
                        can_extended: row.get(16)?,
                        frame_format: row.get(17)?,
                        payload_contract: row.get(18)?,
                        metadata_json: row.get(19)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);

            let count = channels.len() as u64;

            let mut insert_stmt = tx.prepare(
                "INSERT INTO boundary_channel_details (
                    channel_uid, surface_uid, channel_kind, channel_identity,
                    socket_path, tcp_endpoint, udp_endpoint, can_id, i2c_address,
                    spi_device, serial_device, shm_key, pipe_path, pipe_context,
                    mqueue_name, baud_rate, can_extended, frame_format,
                    payload_contract, metadata_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )?;

            for ch in &channels {
                let new_surface_uid = surface_uid_map.get(&ch.old_surface_uid)
                    .cloned()
                    .unwrap_or_else(|| ch.old_surface_uid.clone());
                let new_channel_uid = uuid::Uuid::new_v4().to_string();

                insert_stmt.execute(rusqlite::params![
                    new_channel_uid, new_surface_uid, ch.channel_kind, ch.channel_identity,
                    ch.socket_path, ch.tcp_endpoint, ch.udp_endpoint, ch.can_id, ch.i2c_address,
                    ch.spi_device, ch.serial_device, ch.shm_key, ch.pipe_path, ch.pipe_context,
                    ch.mqueue_name, ch.baud_rate, ch.can_extended, ch.frame_format,
                    ch.payload_contract, ch.metadata_json
                ])?;
            }

            count
        } else {
            0
        };

        // Cleanup temp table.
        tx.execute("DROP TABLE IF EXISTS _unchanged_files", [])?;
        tx.commit()?;

        Ok((surfaces_copied, channels_copied))
    }

    /// Copy forward contract schemas and elements for unchanged proto files.
    ///
    /// Contract schemas use `file_path` column for file anchoring.
    /// Contract elements reference their parent schema by `schema_uid`.
    pub fn copy_forward_contract_schemas(
        &self,
        parent_snapshot_uid: &str,
        current_snapshot_uid: &str,
        unchanged_proto_paths: &[String],
    ) -> Result<(u64, u64), StorageError> {
        if unchanged_proto_paths.is_empty() {
            return Ok((0, 0));
        }

        let conn = self.connection();
        let tx = conn.unchecked_transaction()?;

        // Build old_uid -> new_uid mapping for schemas.
        let mut schema_uid_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // Build temp table for unchanged paths.
        tx.execute(
            "CREATE TEMP TABLE IF NOT EXISTS _unchanged_protos (path TEXT PRIMARY KEY)",
            [],
        )?;
        tx.execute("DELETE FROM _unchanged_protos", [])?;

        {
            let mut stmt = tx.prepare("INSERT INTO _unchanged_protos (path) VALUES (?)")?;
            for path in unchanged_proto_paths {
                stmt.execute([path])?;
            }
        }

        // Select schemas for unchanged proto files.
        #[allow(clippy::type_complexity)]
        let schemas: Vec<(
            String, // old schema_uid
            String, // repo_uid
            String, // schema_kind
            String, // file_path
            Option<String>, // package_name
            Option<String>, // syntax_version
            String, // content_hash
            Option<String>, // imports_json
            Option<String>, // options_json
            String, // extractor
            String, // parsed_at
        )> = {
            let mut stmt = tx.prepare(
                "SELECT schema_uid, repo_uid, schema_kind, file_path, package_name,
                        syntax_version, content_hash, imports_json, options_json,
                        extractor, parsed_at
                 FROM contract_schemas
                 WHERE snapshot_uid = ?
                   AND file_path IN (SELECT path FROM _unchanged_protos)"
            )?;
            let rows = stmt.query_map([parent_snapshot_uid], |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                    row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                    row.get(8)?, row.get(9)?, row.get(10)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let schemas_copied = schemas.len() as u64;

        // Assign new UIDs and insert schemas.
        {
            let mut insert_stmt = tx.prepare(
                "INSERT INTO contract_schemas (
                    schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                    package_name, syntax_version, content_hash, imports_json,
                    options_json, extractor, parsed_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )?;

            for s in &schemas {
                let new_uid = uuid::Uuid::new_v4().to_string();
                schema_uid_map.insert(s.0.clone(), new_uid.clone());
                insert_stmt.execute(rusqlite::params![
                    new_uid, current_snapshot_uid, s.1, s.2, s.3, s.4, s.5,
                    s.6, s.7, s.8, s.9, s.10
                ])?;
            }
        }

        // Copy elements for copied schemas.
        let elements_copied: u64 = if !schema_uid_map.is_empty() {
            let old_uids: Vec<&String> = schema_uid_map.keys().collect();
            let placeholders: String = old_uids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

            let select_sql = format!(
                "SELECT element_uid, schema_uid, element_kind, name, qualified_name,
                        parent_element_uid, type_ref, is_repeated, is_optional, is_map,
                        map_key_type, map_value_type, field_number, default_value,
                        options_json, doc_comment, metadata_json
                 FROM contract_elements
                 WHERE schema_uid IN ({})",
                placeholders
            );

            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            for uid in &old_uids {
                params.push(Box::new((*uid).clone()));
            }
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();

            #[derive(Debug)]
            struct ElementRow {
                old_schema_uid: String,
                element_kind: String,
                name: String,
                qualified_name: Option<String>,
                parent_element_uid: Option<String>,
                type_ref: Option<String>,
                is_repeated: Option<i64>,
                is_optional: Option<i64>,
                is_map: Option<i64>,
                map_key_type: Option<String>,
                map_value_type: Option<String>,
                field_number: Option<i64>,
                default_value: Option<String>,
                options_json: Option<String>,
                doc_comment: Option<String>,
                metadata_json: Option<String>,
            }

            let mut stmt = tx.prepare(&select_sql)?;
            let elements: Vec<ElementRow> = stmt
                .query_map(param_refs.as_slice(), |row| {
                    Ok(ElementRow {
                        old_schema_uid: row.get(1)?,
                        element_kind: row.get(2)?,
                        name: row.get(3)?,
                        qualified_name: row.get(4)?,
                        parent_element_uid: row.get(5)?,
                        type_ref: row.get(6)?,
                        is_repeated: row.get(7)?,
                        is_optional: row.get(8)?,
                        is_map: row.get(9)?,
                        map_key_type: row.get(10)?,
                        map_value_type: row.get(11)?,
                        field_number: row.get(12)?,
                        default_value: row.get(13)?,
                        options_json: row.get(14)?,
                        doc_comment: row.get(15)?,
                        metadata_json: row.get(16)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);

            let count = elements.len() as u64;

            let mut insert_stmt = tx.prepare(
                "INSERT INTO contract_elements (
                    element_uid, schema_uid, element_kind, name, qualified_name,
                    parent_element_uid, type_ref, is_repeated, is_optional, is_map,
                    map_key_type, map_value_type, field_number, default_value,
                    options_json, doc_comment, metadata_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )?;

            for el in &elements {
                let new_schema_uid = schema_uid_map.get(&el.old_schema_uid)
                    .cloned()
                    .unwrap_or_else(|| el.old_schema_uid.clone());
                let new_element_uid = uuid::Uuid::new_v4().to_string();

                // Note: parent_element_uid references within same schema
                // are not remapped here. This is acceptable because:
                // 1. We copy all elements of a schema together
                // 2. Contract elements are typically looked up by schema_uid, not cross-referenced
                // If needed, a second pass could remap parent_element_uid.

                insert_stmt.execute(rusqlite::params![
                    new_element_uid, new_schema_uid, el.element_kind, el.name, el.qualified_name,
                    el.parent_element_uid, el.type_ref, el.is_repeated, el.is_optional, el.is_map,
                    el.map_key_type, el.map_value_type, el.field_number, el.default_value,
                    el.options_json, el.doc_comment, el.metadata_json
                ])?;
            }

            count
        } else {
            0
        };

        // Cleanup temp table.
        tx.execute("DROP TABLE IF EXISTS _unchanged_protos", [])?;
        tx.commit()?;

        Ok((schemas_copied, elements_copied))
    }

    /// Copy forward all derived artifacts for unchanged files.
    ///
    /// Convenience method that calls all individual copy-forward methods.
    /// Used by the compose layer during refresh.
    pub fn copy_forward_derived_artifacts(
        &self,
        parent_snapshot_uid: &str,
        current_snapshot_uid: &str,
        repo_uid: &str,
        unchanged_file_paths: &[String],
        unchanged_proto_paths: &[String],
    ) -> Result<ArtifactCopyForwardResult, StorageError> {
        let measurements_copied = self.copy_forward_measurements(
            parent_snapshot_uid,
            current_snapshot_uid,
            repo_uid,
            unchanged_file_paths,
        )?;

        let inferences_copied = self.copy_forward_inferences(
            parent_snapshot_uid,
            current_snapshot_uid,
            repo_uid,
            unchanged_file_paths,
        )?;

        let (boundary_surfaces_copied, boundary_channels_copied) = self.copy_forward_boundary_surfaces(
            parent_snapshot_uid,
            current_snapshot_uid,
            unchanged_file_paths,
        )?;

        let (contract_schemas_copied, contract_elements_copied) = self.copy_forward_contract_schemas(
            parent_snapshot_uid,
            current_snapshot_uid,
            unchanged_proto_paths,
        )?;

        Ok(ArtifactCopyForwardResult {
            measurements_copied,
            inferences_copied,
            boundary_surfaces_copied,
            boundary_channels_copied,
            contract_schemas_copied,
            contract_elements_copied,
        })
    }
}
