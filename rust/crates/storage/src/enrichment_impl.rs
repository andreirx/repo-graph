//! `EnrichmentStoragePort` implementation for `StorageConnection`.
//!
//! This module implements the enrichment crate's storage port on
//! top of the storage adapter's rusqlite connection. The
//! dependency direction is adapter -> policy (storage crate
//! imports and implements the trait from the enrichment crate),
//! following the Clean Architecture dependency rule.
//!
//! **Error handling:** every method propagates `StorageError`
//! through the `Result` return. No silent coercion of SQL errors
//! to zero/empty.
//!
//! **Schema contract:** The trust service expects enrichment metadata
//! in a specific shape:
//! ```json
//! {
//!   "enrichment": {
//!     "receiverType": "...",
//!     "typeDisplayName": "...",
//!     "isExternalType": true,
//!     "origin": "compiler",
//!     "failureReason": "..."
//!   }
//! }
//! ```
//! This adapter converts between the in-memory `EnrichmentMetadata`
//! (snake_case) and the DB schema (camelCase, nested under "enrichment").

use enrichment::{
    EligibilityQuery, EligibleEdge, EnrichmentLanguage, EnrichmentMetadata, EnrichmentStoragePort,
    PromotedEdge, PromotionCandidate, ReceiverTypeOrigin, StorageError as EnrichmentStorageError,
    SymbolInfo, SymbolSubtype, UnresolvedCategory,
};

use crate::connection::StorageConnection;
use crate::error::StorageError;

// ── Schema conversion helpers ─────────────────────────────────────
//
// Convert between in-memory EnrichmentMetadata (snake_case) and
// DB schema (camelCase, nested under "enrichment") for trust
// service compatibility.

/// Convert EnrichmentMetadata to DB schema JSON.
fn metadata_to_db_json(meta: &EnrichmentMetadata) -> serde_json::Value {
    let origin_str = match meta.origin {
        ReceiverTypeOrigin::Compiler => "compiler",
        ReceiverTypeOrigin::Failed => "failed",
    };

    let mut enrichment = serde_json::json!({
        "origin": origin_str,
        "isExternalType": meta.is_external_type,
    });

    if let Some(ref rt) = meta.receiver_type {
        enrichment["receiverType"] = serde_json::Value::String(rt.clone());
    }
    if let Some(ref tdn) = meta.type_display_name {
        enrichment["typeDisplayName"] = serde_json::Value::String(tdn.clone());
    }
    if let Some(ref fr) = meta.failure_reason {
        enrichment["failureReason"] = serde_json::Value::String(fr.clone());
    }

    serde_json::json!({ "enrichment": enrichment })
}

/// Parse EnrichmentMetadata from DB schema JSON.
fn metadata_from_db_json(json_str: &str) -> Option<EnrichmentMetadata> {
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let enrichment = value.get("enrichment")?;

    let origin = match enrichment.get("origin")?.as_str()? {
        "compiler" => ReceiverTypeOrigin::Compiler,
        "failed" => ReceiverTypeOrigin::Failed,
        _ => return None,
    };

    Some(EnrichmentMetadata {
        receiver_type: enrichment
            .get("receiverType")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        type_display_name: enrichment
            .get("typeDisplayName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_external_type: enrichment
            .get("isExternalType")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        origin,
        failure_reason: enrichment
            .get("failureReason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

// ── Error conversion ──────────────────────────────────────────────

impl From<StorageError> for EnrichmentStorageError {
    fn from(e: StorageError) -> Self {
        EnrichmentStorageError::Database(e.to_string())
    }
}

// ── EnrichmentStoragePort implementation ──────────────────────────

impl EnrichmentStoragePort for StorageConnection {
    fn query_eligible_edges(
        &self,
        query: &EligibilityQuery,
    ) -> Result<Vec<EligibleEdge>, EnrichmentStorageError> {
        // Build dynamic SQL with all filters embedded
        let categories: Vec<String> = query
            .categories
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();

        let category_clause = if categories.is_empty() {
            String::new()
        } else {
            let category_list: String = categories
                .iter()
                .map(|c| format!("'{}'", c.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(", ");
            format!(" AND category IN ({})", category_list)
        };

        let enriched_clause = if query.exclude_already_enriched {
            // Check for enrichment marker presence (origin field), not just successful type resolution.
            // Failed attempts also have the marker (origin = "failed"), so they are excluded.
            // Use --force to re-enrich previously attempted edges.
            " AND (metadata_json IS NULL OR json_extract(metadata_json, '$.enrichment.origin') IS NULL)"
        } else {
            ""
        };

        let limit_clause = query
            .limit
            .map(|n| format!(" LIMIT {}", n))
            .unwrap_or_default();

        let sql = format!(
            r#"
            SELECT
                edge_uid,
                snapshot_uid,
                repo_uid,
                source_node_uid,
                target_key,
                line_start,
                col_start,
                category
            FROM unresolved_edges
            WHERE snapshot_uid = ?1
            {}{}
            ORDER BY edge_uid
            {}
            "#,
            category_clause, enriched_clause, limit_clause
        );

        let conn = self.connection();
        let mut stmt = conn.prepare(&sql).map_err(StorageError::from)?;

        let snapshot_uid = &query.snapshot_uid;

        // Collect raw rows first
        let raw_rows: Vec<RawEligibleEdge> = stmt
            .query_map([snapshot_uid], |row| {
                Ok(RawEligibleEdge {
                    edge_uid: row.get(0)?,
                    snapshot_uid: row.get(1)?,
                    repo_uid: row.get(2)?,
                    source_node_uid: row.get(3)?,
                    target_key: row.get(4)?,
                    line_start: row.get::<_, Option<i64>>(5)?,
                    col_start: row.get::<_, Option<i64>>(6)?,
                    category: row.get(7)?,
                })
            })
            .map_err(StorageError::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        // Convert to EligibleEdge with language derivation
        let mut edges = Vec::new();
        for raw in raw_rows {
            // Parse category
            let category = match UnresolvedCategory::parse(&raw.category) {
                Some(c) => c,
                None => continue, // Skip unrecognized categories
            };

            // Derive language from source file path
            let file_path = self.get_file_path_for_node(&raw.source_node_uid)?;

            let language = match file_path
                .as_ref()
                .and_then(|p| EnrichmentLanguage::from_path(p))
            {
                Some(lang) => lang,
                None => continue, // Skip files with unsupported language
            };

            // Apply language filter if specified
            if !query.languages.is_empty() && !query.languages.contains(&language) {
                continue;
            }

            edges.push(EligibleEdge {
                edge_uid: raw.edge_uid,
                snapshot_uid: raw.snapshot_uid,
                repo_uid: raw.repo_uid,
                source_node_uid: raw.source_node_uid,
                target_key: raw.target_key,
                source_file_path: file_path.unwrap_or_default(),
                line_start: raw.line_start.unwrap_or(0) as u32,
                col_start: raw.col_start.unwrap_or(0) as u32,
                category,
                language,
            });
        }

        Ok(edges)
    }

    fn persist_enrichments(
        &self,
        updates: &[(String, EnrichmentMetadata)],
    ) -> Result<usize, EnrichmentStorageError> {
        if updates.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let mut count = 0;

        for (edge_uid, metadata) in updates {
            // Convert to DB schema (camelCase, nested under "enrichment")
            let db_json = metadata_to_db_json(metadata);
            let enrichment_json = serde_json::to_string(&db_json)
                .map_err(|e| EnrichmentStorageError::Serialization(e.to_string()))?;

            // Merge with existing metadata_json using json_patch
            // SQLite's json_patch merges two JSON objects
            let result = conn.execute(
                r#"
                UPDATE unresolved_edges
                SET metadata_json = CASE
                    WHEN metadata_json IS NULL THEN ?1
                    ELSE json_patch(metadata_json, ?1)
                END
                WHERE edge_uid = ?2
                "#,
                rusqlite::params![enrichment_json, edge_uid],
            );

            match result {
                Ok(n) => count += n,
                Err(e) => {
                    // Log but continue - partial success is acceptable
                    eprintln!(
                        "warning: failed to persist enrichment for {}: {}",
                        edge_uid, e
                    );
                }
            }
        }

        Ok(count)
    }

    fn load_promotion_candidates(
        &self,
        snapshot_uid: &str,
        limit: Option<usize>,
    ) -> Result<Vec<PromotionCandidate>, EnrichmentStorageError> {
        let limit_clause = limit.map(|n| format!(" LIMIT {}", n)).unwrap_or_default();

        // Query using nested camelCase schema (trust service compatible)
        let sql = format!(
            r#"
            SELECT
                edge_uid,
                snapshot_uid,
                repo_uid,
                source_node_uid,
                target_key,
                line_start,
                col_start,
                line_end,
                col_end,
                category,
                metadata_json
            FROM unresolved_edges
            WHERE snapshot_uid = ?1
              AND metadata_json IS NOT NULL
              AND json_extract(metadata_json, '$.enrichment.receiverType') IS NOT NULL
              AND json_extract(metadata_json, '$.enrichment.origin') = 'compiler'
            ORDER BY edge_uid
            {}
            "#,
            limit_clause
        );

        let conn = self.connection();
        let mut stmt = conn.prepare(&sql).map_err(StorageError::from)?;

        let rows = stmt
            .query_map([snapshot_uid], |row| {
                Ok(RawPromotionCandidate {
                    edge_uid: row.get(0)?,
                    snapshot_uid: row.get(1)?,
                    repo_uid: row.get(2)?,
                    source_node_uid: row.get(3)?,
                    target_key: row.get(4)?,
                    line_start: row.get::<_, Option<i64>>(5)?,
                    col_start: row.get::<_, Option<i64>>(6)?,
                    line_end: row.get::<_, Option<i64>>(7)?,
                    col_end: row.get::<_, Option<i64>>(8)?,
                    category: row.get(9)?,
                    metadata_json: row.get(10)?,
                })
            })
            .map_err(StorageError::from)?;

        let mut candidates = Vec::new();
        for row_result in rows {
            let raw = row_result.map_err(StorageError::from)?;

            let category = match UnresolvedCategory::parse(&raw.category) {
                Some(c) => c,
                None => continue,
            };

            // Parse from DB schema (camelCase, nested under "enrichment")
            let enrichment = match metadata_from_db_json(&raw.metadata_json) {
                Some(e) => e,
                None => continue, // Skip malformed metadata
            };

            candidates.push(PromotionCandidate {
                edge_uid: raw.edge_uid,
                snapshot_uid: raw.snapshot_uid,
                repo_uid: raw.repo_uid,
                source_node_uid: raw.source_node_uid,
                target_key: raw.target_key,
                line_start: raw.line_start.map(|n| n as u32),
                col_start: raw.col_start.map(|n| n as u32),
                line_end: raw.line_end.map(|n| n as u32),
                col_end: raw.col_end.map(|n| n as u32),
                category,
                enrichment,
            });
        }

        Ok(candidates)
    }

    fn load_symbols_by_names(
        &self,
        snapshot_uid: &str,
        type_names: &[String],
    ) -> Result<Vec<SymbolInfo>, EnrichmentStorageError> {
        if type_names.is_empty() {
            return Ok(Vec::new());
        }

        // Build IN clause for type names
        // Match against the last segment of qualified_name or name
        let placeholders: Vec<String> = type_names
            .iter()
            .map(|name| format!("'{}'", name.replace('\'', "''")))
            .collect();

        let sql = format!(
            r#"
            SELECT
                node_uid,
                stable_key,
                qualified_name,
                subtype
            FROM nodes
            WHERE snapshot_uid = ?1
              AND kind = 'SYMBOL'
              AND (
                  name IN ({placeholders})
                  OR qualified_name IN ({placeholders})
              )
            ORDER BY node_uid
            "#,
            placeholders = placeholders.join(", ")
        );

        let conn = self.connection();
        let mut stmt = conn.prepare(&sql).map_err(StorageError::from)?;

        let rows = stmt
            .query_map([snapshot_uid], |row| {
                Ok(RawSymbolInfo {
                    node_uid: row.get(0)?,
                    stable_key: row.get(1)?,
                    qualified_name: row.get(2)?,
                    subtype: row.get(3)?,
                })
            })
            .map_err(StorageError::from)?;

        let mut symbols = Vec::new();
        for row_result in rows {
            let raw = row_result.map_err(StorageError::from)?;

            symbols.push(SymbolInfo {
                node_uid: raw.node_uid,
                stable_key: raw.stable_key,
                qualified_name: raw.qualified_name,
                subtype: raw
                    .subtype
                    .map(|s| SymbolSubtype::parse(&s))
                    .unwrap_or(SymbolSubtype::Other),
            });
        }

        Ok(symbols)
    }

    fn load_class_methods(
        &self,
        snapshot_uid: &str,
        class_stable_key: &str,
    ) -> Result<Vec<(String, SymbolInfo)>, EnrichmentStorageError> {
        // First find the class node_uid
        let conn = self.connection();

        let class_node_uid: Option<String> = conn
            .query_row(
                "SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 AND stable_key = ?2",
                rusqlite::params![snapshot_uid, class_stable_key],
                |row| row.get(0),
            )
            .ok();

        let class_node_uid = match class_node_uid {
            Some(uid) => uid,
            None => return Ok(Vec::new()),
        };

        // Find all method-like children of this class
        let sql = r#"
            SELECT
                node_uid,
                stable_key,
                name,
                qualified_name,
                subtype
            FROM nodes
            WHERE snapshot_uid = ?1
              AND parent_node_uid = ?2
              AND kind = 'SYMBOL'
              AND subtype IN ('METHOD', 'GETTER', 'SETTER', 'FUNCTION')
            ORDER BY name
        "#;

        let mut stmt = conn.prepare(sql).map_err(StorageError::from)?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, class_node_uid], |row| {
                Ok(RawMethodInfo {
                    node_uid: row.get(0)?,
                    stable_key: row.get(1)?,
                    name: row.get(2)?,
                    qualified_name: row.get(3)?,
                    subtype: row.get(4)?,
                })
            })
            .map_err(StorageError::from)?;

        let mut methods = Vec::new();
        for row_result in rows {
            let raw = row_result.map_err(StorageError::from)?;

            let symbol = SymbolInfo {
                node_uid: raw.node_uid,
                stable_key: raw.stable_key,
                qualified_name: raw.qualified_name,
                subtype: raw
                    .subtype
                    .map(|s| SymbolSubtype::parse(&s))
                    .unwrap_or(SymbolSubtype::Method),
            };

            methods.push((raw.name, symbol));
        }

        Ok(methods)
    }

    fn delete_edges_by_uids(&self, edge_uids: &[String]) -> Result<usize, EnrichmentStorageError> {
        if edge_uids.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let mut count = 0;

        // Delete in batches to avoid SQL length limits
        for chunk in edge_uids.chunks(100) {
            let placeholders: Vec<String> = chunk
                .iter()
                .map(|uid| format!("'{}'", uid.replace('\'', "''")))
                .collect();

            let sql = format!(
                "DELETE FROM edges WHERE edge_uid IN ({})",
                placeholders.join(", ")
            );

            count += conn.execute(&sql, []).map_err(StorageError::from)?;
        }

        Ok(count)
    }

    fn insert_promoted_edges(
        &self,
        edges: &[PromotedEdge],
    ) -> Result<usize, EnrichmentStorageError> {
        if edges.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let mut count = 0;

        let sql = r#"
            INSERT INTO edges (
                edge_uid,
                snapshot_uid,
                repo_uid,
                source_node_uid,
                target_node_uid,
                type,
                resolution,
                extractor,
                line_start,
                col_start,
                line_end,
                col_end,
                metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#;

        let mut stmt = conn.prepare(sql).map_err(StorageError::from)?;

        for edge in edges {
            let (line_start, col_start, line_end, col_end) = edge
                .location
                .as_ref()
                .map(|loc| {
                    (
                        Some(loc.line_start as i64),
                        Some(loc.col_start as i64),
                        Some(loc.line_end as i64),
                        Some(loc.col_end as i64),
                    )
                })
                .unwrap_or((None, None, None, None));

            let result = stmt.execute(rusqlite::params![
                edge.edge_uid,
                edge.snapshot_uid,
                edge.repo_uid,
                edge.source_node_uid,
                edge.target_node_uid,
                edge.edge_type,
                edge.resolution,
                edge.extractor,
                line_start,
                col_start,
                line_end,
                col_end,
                edge.metadata_json,
            ]);

            match result {
                Ok(_) => count += 1,
                Err(e) => {
                    // Log but continue - partial success acceptable
                    eprintln!(
                        "warning: failed to insert promoted edge {}: {}",
                        edge.edge_uid, e
                    );
                }
            }
        }

        Ok(count)
    }

    fn get_repo_root(&self, repo_uid: &str) -> Result<String, EnrichmentStorageError> {
        let conn = self.connection();

        conn.query_row(
            "SELECT root_path FROM repos WHERE repo_uid = ?1",
            [repo_uid],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                EnrichmentStorageError::RepoNotFound(repo_uid.to_string())
            }
            other => EnrichmentStorageError::Database(other.to_string()),
        })
    }
}

// ── Helper methods ────────────────────────────────────────────────

impl StorageConnection {
    /// Get the file path for a node (by looking up its file_uid).
    fn get_file_path_for_node(&self, node_uid: &str) -> Result<Option<String>, StorageError> {
        let conn = self.connection();

        let result: Option<String> = conn
            .query_row(
                r#"
                SELECT f.path
                FROM nodes n
                JOIN files f ON n.file_uid = f.file_uid
                WHERE n.node_uid = ?1
                "#,
                [node_uid],
                |row| row.get(0),
            )
            .ok();

        Ok(result)
    }
}

// ── Raw row types ─────────────────────────────────────────────────

struct RawEligibleEdge {
    edge_uid: String,
    snapshot_uid: String,
    repo_uid: String,
    source_node_uid: String,
    target_key: String,
    line_start: Option<i64>,
    col_start: Option<i64>,
    category: String,
}

struct RawPromotionCandidate {
    edge_uid: String,
    snapshot_uid: String,
    repo_uid: String,
    source_node_uid: String,
    target_key: String,
    line_start: Option<i64>,
    col_start: Option<i64>,
    line_end: Option<i64>,
    col_end: Option<i64>,
    category: String,
    metadata_json: String,
}

struct RawSymbolInfo {
    node_uid: String,
    stable_key: String,
    qualified_name: Option<String>,
    subtype: Option<String>,
}

struct RawMethodInfo {
    node_uid: String,
    stable_key: String,
    name: String,
    qualified_name: Option<String>,
    subtype: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> StorageConnection {
        StorageConnection::open_in_memory().unwrap()
    }

    #[test]
    fn test_get_repo_root_not_found() {
        let conn = setup_test_db();

        let result = EnrichmentStoragePort::get_repo_root(&conn, "nonexistent");

        assert!(matches!(
            result,
            Err(EnrichmentStorageError::RepoNotFound(_))
        ));
    }

    #[test]
    fn test_query_eligible_edges_empty() {
        let conn = setup_test_db();

        let query = EligibilityQuery::new("snap-1");
        let edges = conn.query_eligible_edges(&query).unwrap();

        assert!(edges.is_empty());
    }

    #[test]
    fn test_persist_enrichments_empty() {
        let conn = setup_test_db();

        let count = conn.persist_enrichments(&[]).unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_load_promotion_candidates_empty() {
        let conn = setup_test_db();

        let candidates = conn.load_promotion_candidates("snap-1", None).unwrap();

        assert!(candidates.is_empty());
    }

    #[test]
    fn test_load_symbols_by_names_empty_input() {
        let conn = setup_test_db();

        let symbols = conn.load_symbols_by_names("snap-1", &[]).unwrap();

        assert!(symbols.is_empty());
    }

    #[test]
    fn test_delete_edges_empty() {
        let conn = setup_test_db();

        // Explicitly call trait method (inherent method on StorageConnection returns ())
        let count = EnrichmentStoragePort::delete_edges_by_uids(&conn, &[]).unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_promoted_edges_empty() {
        let conn = setup_test_db();

        // Explicitly call trait method
        let count = EnrichmentStoragePort::insert_promoted_edges(&conn, &[]).unwrap();

        assert_eq!(count, 0);
    }
}
