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
        // Find the class node's uid AND name. The name drives the language-agnostic method
        // association below (Rust methods link to their type by qualified_name, not parent link).
        let conn = self.connection();

        let class_row: Option<(String, String)> = conn
            .query_row(
                "SELECT node_uid, name FROM nodes WHERE snapshot_uid = ?1 AND stable_key = ?2",
                rusqlite::params![snapshot_uid, class_stable_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    ))
                },
            )
            .ok();

        let (class_node_uid, class_name) = match class_row {
            Some(pair) => pair,
            None => return Ok(Vec::new()),
        };

        // Find this type's method-like symbols. Two association shapes, because extractors parent
        // methods differently by language:
        //   - parent_node_uid = <class node>            → TS/JS class members (the extractor sets
        //     the parent link when the method is lexically inside the class body).
        //   - qualified_name = "<TypeName>.<method>"    → Rust `impl` methods, which the
        //     rust-extractor emits with parent_node_uid = NULL ("No parent node for impl methods
        //     in v1" — impl blocks live apart from the type definition, often in another file, so
        //     no node_uid link is available at extraction time). The owning type is carried in the
        //     method's qualified_name instead.
        //
        // Without the second branch, promotion Gate 6 ("method maps to exactly one METHOD on the
        // class") finds ZERO methods for every Rust receiver, so an auto-enrich pass resolves
        // receiver types but promotes nothing (the ENRICH-LIFECYCLE-1 `promoted=0` defect). The
        // `?3 <> ''` guard prevents a name-less class from matching ".<method>". Precision holds: an
        // EXACT "<name>.<method>" equality (not a prefix) can at worst pull in a homonym type's
        // method, which the gate's own uniqueness check then rejects as ambiguous — never a
        // mis-promotion (VISION: precision matters for the call graph).
        let sql = r#"
            SELECT
                node_uid,
                stable_key,
                name,
                qualified_name,
                subtype
            FROM nodes
            WHERE snapshot_uid = ?1
              AND kind = 'SYMBOL'
              AND subtype IN ('METHOD', 'GETTER', 'SETTER', 'FUNCTION')
              AND (
                  parent_node_uid = ?2
                  OR (?3 <> '' AND qualified_name = ?3 || '.' || name)
              )
            ORDER BY name
        "#;

        let mut stmt = conn.prepare(sql).map_err(StorageError::from)?;

        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, class_node_uid, class_name],
                |row| {
                    Ok(RawMethodInfo {
                        node_uid: row.get(0)?,
                        stable_key: row.get(1)?,
                        name: row.get(2)?,
                        qualified_name: row.get(3)?,
                        subtype: row.get(4)?,
                    })
                },
            )
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

    fn apply_promotion(
        &self,
        snapshot_uid: &str,
        promoted: &[PromotedEdge],
    ) -> Result<usize, EnrichmentStorageError> {
        if promoted.is_empty() {
            // Nothing to delete (the idempotency delete targets exactly the
            // promoted uids) and nothing to insert → net delta 0; skip the
            // transaction entirely.
            return Ok(0);
        }

        let conn = self.connection();
        // ONE transaction for delete + insert + aggregate adjustments
        // (review-0 item 2; extended by EC-1 M-3a): the trust core and the
        // dead-liveness/map readers PREFER the persisted aggregates, so
        // they must move with the rows or not at all. Any hard error below
        // propagates with `?`; dropping the uncommitted transaction rolls
        // EVERYTHING back — rows, the g1 count, the g2 degrees and the g3
        // pairs revert together, and each aggregate still exactly
        // describes the stored state. `unchecked_transaction` because this
        // port takes `&self` (same pattern as retention/prune.rs).
        let tx = conn.unchecked_transaction().map_err(StorageError::from)?;

        // M-3a never-seed gates, read INSIDE the transaction: a
        // pre-migration snapshot (NULL/malformed marker) gets NO family
        // deltas — seeding would require a row-derived base, the banned
        // accounting; its labeled live-derived fallback keeps serving.
        // (The g1 equivalent is NULL-propagating SQL arithmetic.)
        use crate::crud::call_aggregates::CallAggregateFamily;
        let g2_present = crate::crud::call_aggregates::family_marker(
            &tx,
            snapshot_uid,
            CallAggregateFamily::SymbolCallDegrees,
        )
        .map_err(EnrichmentStorageError::from)?
        .is_some();
        let g3_present = crate::crud::call_aggregates::family_marker(
            &tx,
            snapshot_uid,
            CallAggregateFamily::ResolvedCallFilePairs,
        )
        .map_err(EnrichmentStorageError::from)?
        .is_some();

        // 1. Idempotency delete of the promoted uids, capturing the
        //    ENDPOINTS of the CALLS rows actually removed (exact
        //    accounting for the g1/g2/g3 deltas — the list's length is
        //    the M-3b deleted count, its endpoints feed the M-3a deltas;
        //    consistent because the SELECT and DELETE share the
        //    transaction). Chunked to avoid SQL length limits (same bound
        //    as the pre-M-3b implementation).
        let uids: Vec<&str> = promoted.iter().map(|e| e.edge_uid.as_str()).collect();
        let mut deleted_call_endpoints: Vec<(String, String)> = Vec::new();
        for chunk in uids.chunks(100) {
            let placeholders: Vec<String> = chunk
                .iter()
                .map(|uid| format!("'{}'", uid.replace('\'', "''")))
                .collect();
            let list = placeholders.join(", ");

            let mut stmt = tx
                .prepare(&format!(
                    "SELECT source_node_uid, target_node_uid FROM edges \
                     WHERE edge_uid IN ({list}) AND type = 'CALLS'"
                ))
                .map_err(StorageError::from)?;
            let chunk_endpoints = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(StorageError::from)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(StorageError::from)?;
            deleted_call_endpoints.extend(chunk_endpoints);

            tx.execute(&format!("DELETE FROM edges WHERE edge_uid IN ({list})"), [])
                .map_err(StorageError::from)?;
        }
        let deleted_calls = deleted_call_endpoints.len() as i64;

        // 2. Insert the newly promoted edges. Per-edge failures tolerated
        //    and logged (pre-existing promotion semantics — partial success
        //    within the set is acceptable); the aggregate delta counts ONLY
        //    the CALLS rows that actually landed, so it stays exact even
        //    when some inserts are skipped.
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

        let mut inserted: usize = 0;
        let mut inserted_call_endpoints: Vec<(String, String)> = Vec::new();
        {
            let mut stmt = tx.prepare(sql).map_err(StorageError::from)?;

            for edge in promoted {
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
                    Ok(_) => {
                        inserted += 1;
                        if edge.edge_type == "CALLS" {
                            // Deltas count ONLY rows that actually landed,
                            // so they stay exact under partial success.
                            inserted_call_endpoints
                                .push((edge.source_node_uid.clone(), edge.target_node_uid.clone()));
                        }
                    }
                    Err(e) => {
                        // Log but continue - partial success acceptable
                        eprintln!(
                            "warning: failed to insert promoted edge {}: {}",
                            edge.edge_uid, e
                        );
                    }
                }
            }
        } // stmt dropped before commit (it borrows the transaction)
        let inserted_calls = inserted_call_endpoints.len() as i64;

        // 3. Adjust the persisted g1 aggregate by the net CALLS delta
        //    INSIDE the same transaction (crud/snapshots.rs holds its
        //    write census). NULL-propagating: a pre-migration snapshot
        //    keeps NULL (never seeded — the labeled live-COUNT fallback
        //    applies).
        crate::crud::snapshots::adjust_resolved_call_aggregate(
            &tx,
            snapshot_uid,
            inserted_calls - deleted_calls,
        )?;

        // 4. EC-1 M-3a: adjust the g2 degree family by the same per-edge
        //    accounting (fan-in on targets, fan-out on sources; +1 per
        //    landed insert, −1 per deleted row — an idempotent
        //    re-promotion nets 0), marker-gated per the never-seed rule.
        //    crud/call_aggregates.rs holds the M-3a write census.
        if g2_present {
            let mut degree_deltas: std::collections::BTreeMap<String, (i64, i64)> =
                std::collections::BTreeMap::new();
            for (source, target) in &inserted_call_endpoints {
                degree_deltas.entry(target.clone()).or_default().0 += 1;
                degree_deltas.entry(source.clone()).or_default().1 += 1;
            }
            for (source, target) in &deleted_call_endpoints {
                degree_deltas.entry(target.clone()).or_default().0 -= 1;
                degree_deltas.entry(source.clone()).or_default().1 -= 1;
            }
            let deltas: Vec<(String, i64, i64)> = degree_deltas
                .into_iter()
                .map(|(uid, (d_in, d_out))| (uid, d_in, d_out))
                .collect();
            crate::crud::call_aggregates::adjust_symbol_call_degrees(&tx, snapshot_uid, &deltas)
                .map_err(EnrichmentStorageError::from)?;
        }

        // 5. EC-1 M-3a: adjust the g3 file-pair family. Endpoint symbols
        //    are mapped to their file paths INSIDE the transaction (via
        //    nodes⋈files — the FC1 skeleton, not the M-6-filtered edge
        //    rows); self-file pairs and file-less endpoints are excluded,
        //    mirroring the pipeline producer and the live join it
        //    replaces.
        if g3_present {
            let mut endpoint_uids: std::collections::BTreeSet<&str> =
                std::collections::BTreeSet::new();
            for (source, target) in inserted_call_endpoints
                .iter()
                .chain(deleted_call_endpoints.iter())
            {
                endpoint_uids.insert(source.as_str());
                endpoint_uids.insert(target.as_str());
            }
            let mut path_by_uid: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let uid_list: Vec<&str> = endpoint_uids.into_iter().collect();
            for chunk in uid_list.chunks(100) {
                let placeholders: Vec<String> = chunk
                    .iter()
                    .map(|uid| format!("'{}'", uid.replace('\'', "''")))
                    .collect();
                let list = placeholders.join(", ");
                let mut stmt = tx
                    .prepare(&format!(
                        "SELECT n.node_uid, f.path FROM nodes n \
                         JOIN files f ON n.file_uid = f.file_uid \
                         WHERE n.node_uid IN ({list})"
                    ))
                    .map_err(StorageError::from)?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(StorageError::from)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(StorageError::from)?;
                path_by_uid.extend(rows);
            }

            let mut pair_deltas: std::collections::BTreeMap<(String, String), i64> =
                std::collections::BTreeMap::new();
            let mut apply = |endpoints: &[(String, String)], sign: i64| {
                for (source, target) in endpoints {
                    if let (Some(sp), Some(tp)) = (path_by_uid.get(source), path_by_uid.get(target))
                    {
                        if sp != tp {
                            *pair_deltas.entry((sp.clone(), tp.clone())).or_default() += sign;
                        }
                    }
                }
            };
            apply(&inserted_call_endpoints, 1);
            apply(&deleted_call_endpoints, -1);
            let deltas: Vec<(String, String, i64)> = pair_deltas
                .into_iter()
                .map(|((source, target), d)| (source, target, d))
                .collect();
            crate::crud::call_aggregates::adjust_resolved_call_file_pairs(
                &tx,
                snapshot_uid,
                &deltas,
            )
            .map_err(EnrichmentStorageError::from)?;
        }

        tx.commit().map_err(StorageError::from)?;
        Ok(inserted)
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
    fn test_apply_promotion_empty() {
        let conn = setup_test_db();

        // Empty promotion set: nothing deleted, nothing inserted, no
        // transaction opened, aggregate untouched.
        let count = EnrichmentStoragePort::apply_promotion(&conn, "snap-1", &[]).unwrap();

        assert_eq!(count, 0);
    }

    // ── ENRICH-LIFECYCLE-1 promotion: Rust `impl` methods (unparented) ─────────────────────────
    //
    // The rust-extractor emits `impl` methods with parent_node_uid = NULL (methods live in a
    // separate impl block, often another file). The type↔method link is carried in the method's
    // `qualified_name` ("Type.method"). These tests pin that `load_class_methods` finds those
    // methods, so promotion Gate 6 sees them and a Rust receiver actually promotes. Before the fix,
    // `load_class_methods` matched only by parent_node_uid → zero methods → `promoted=0`.

    use crate::types::{CreateSnapshotInput, GraphNode, Repo, UpdateSnapshotStatusInput};

    /// A struct/method node the way the rust-extractor writes it: uppercase subtype, methods
    /// unparented (`parent_node_uid = None`) with the type in `qualified_name`.
    fn rust_node(
        node_uid: &str,
        stable_key: &str,
        subtype: &str,
        name: &str,
        qualified_name: &str,
        snap: &str,
    ) -> GraphNode {
        GraphNode {
            node_uid: node_uid.to_string(),
            snapshot_uid: snap.to_string(),
            repo_uid: "r1".to_string(),
            stable_key: stable_key.to_string(),
            kind: "SYMBOL".to_string(),
            subtype: Some(subtype.to_string()),
            name: name.to_string(),
            qualified_name: Some(qualified_name.to_string()),
            file_uid: None,
            parent_node_uid: None, // the rust-extractor's "no parent for impl methods" shape
            location: None,
            signature: None,
            visibility: Some("export".to_string()),
            doc_comment: None,
            metadata_json: None,
        }
    }

    /// Seed a ready snapshot with an `Engine` struct + its unparented `Engine.run` impl method,
    /// plus a decoy `Other.run` on a different type. Returns (storage, snapshot_uid, engine_key).
    fn seed_rust_type_with_impl_method() -> (StorageConnection, String, String) {
        let mut storage = setup_test_db();
        storage
            .add_repo(&Repo {
                repo_uid: "r1".to_string(),
                name: "repo".to_string(),
                root_path: "/tmp/r1".to_string(),
                default_branch: None,
                created_at: "2026-07-06T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                parent_snapshot_uid: None,
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid;
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: snap.clone(),
                status: "ready".to_string(),
                completed_at: Some("2026-07-06T00:01:00Z".to_string()),
            })
            .unwrap();

        let engine_key = "r1:src/engine.rs#Engine:SYMBOL:CLASS".to_string();
        storage
            .insert_nodes(&[
                // Rust struct → NodeSubtype::Class → "CLASS" (extractor.rs: "Closest mapping").
                rust_node("n-engine", &engine_key, "CLASS", "Engine", "Engine", &snap),
                // The impl method we WANT to find (unparented; qualified_name "Engine.run").
                rust_node(
                    "n-engine-run",
                    "r1:src/engine.rs#Engine.run:SYMBOL:METHOD",
                    "METHOD",
                    "run",
                    "Engine.run",
                    &snap,
                ),
                // A homonym method on a DIFFERENT type — must NOT be attributed to Engine.
                rust_node(
                    "n-other-run",
                    "r1:src/other.rs#Other.run:SYMBOL:METHOD",
                    "METHOD",
                    "run",
                    "Other.run",
                    &snap,
                ),
            ])
            .unwrap();

        (storage, snap, engine_key)
    }

    #[test]
    fn load_class_methods_finds_unparented_rust_impl_method_by_qualified_name() {
        let (storage, snap, engine_key) = seed_rust_type_with_impl_method();

        let methods =
            EnrichmentStoragePort::load_class_methods(&storage, &snap, &engine_key).unwrap();

        // Exactly the Engine.run method — NOT the homonym Other.run (exact "<type>.<method>" match).
        assert_eq!(
            methods.len(),
            1,
            "found the unparented impl method by qualified_name, and only Engine's: {methods:?}"
        );
        assert_eq!(methods[0].0, "run");
        assert_eq!(methods[0].1.node_uid, "n-engine-run");
    }

    #[test]
    fn rust_impl_method_receiver_promotes_end_to_end() {
        // Faithfully reconstructs EnrichmentPipeline::run_promotion's context build (load symbols →
        // for each CLASS, load its methods), then runs the real 8-gate filter. This is the actual
        // ENRICH-LIFECYCLE-1 defect: a resolved Rust receiver must PROMOTE (bank a resolved CALLS
        // edge), not just resolve. Before the load_class_methods fix, Gate 6 found no methods and
        // promoted.len() was 0.
        use enrichment::{promote_edges, PromotionContext};

        let (storage, snap, _engine_key) = seed_rust_type_with_impl_method();

        // Build the promotion context exactly as the pipeline does.
        let mut ctx = PromotionContext::new();
        let symbols =
            EnrichmentStoragePort::load_symbols_by_names(&storage, &snap, &["Engine".to_string()])
                .unwrap();
        for sym in symbols {
            let is_class = sym.subtype == SymbolSubtype::Class;
            let key = sym.stable_key.clone();
            ctx.add_symbol(sym);
            if is_class {
                for (mname, minfo) in
                    EnrichmentStoragePort::load_class_methods(&storage, &snap, &key).unwrap()
                {
                    ctx.add_class_method(&key, &mname, minfo);
                }
            }
        }

        // A resolved `engine.run()` receiver (what rust-analyzer produces: internal type "Engine").
        let candidate = PromotionCandidate {
            edge_uid: "e1".to_string(),
            snapshot_uid: snap.clone(),
            repo_uid: "r1".to_string(),
            source_node_uid: "n-caller".to_string(),
            target_key: "engine.run".to_string(),
            line_start: Some(10),
            col_start: Some(4),
            line_end: Some(10),
            col_end: Some(20),
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            enrichment: EnrichmentMetadata {
                receiver_type: Some("Engine".to_string()),
                type_display_name: Some("Engine".to_string()),
                is_external_type: false,
                origin: ReceiverTypeOrigin::Compiler,
                failure_reason: None,
            },
        };

        let result = promote_edges(&[candidate], &ctx);

        assert_eq!(
            result.promoted.len(),
            1,
            "a resolved Rust impl-method receiver now promotes; skipped: {:?}",
            result.skipped_reasons
        );
        assert_eq!(
            result.promoted[0].target_node_uid, "n-engine-run",
            "the promoted CALLS edge targets the resolved impl method"
        );
    }

    // ── EC-1 M-3b: apply_promotion keeps rows + aggregate coherent ─────────────
    //
    // Promotion mutates the resolved CALLS row set AFTER index finalization, so
    // `run_promotion` persists its result through the atomic `apply_promotion`:
    // delete-by-uid + insert + aggregate delta in ONE transaction. These tests
    // drive the REAL SQL through the adapter and assert persisted-vs-live parity
    // on the success path, across an idempotent re-promotion, and across a
    // forced mid-transaction failure (rollback — never a stale aggregate).
    // (The full pipeline→tail path needs a live resolver toolchain and is not
    // drivable in unit tests; the call site is a direct `?`-propagated port
    // call in `run_promotion`.)

    /// Baseline + one promoted edge, shared by the promotion tests:
    /// one pipeline-resolved CALLS row and the finalize-time aggregate
    /// write (what run_pipeline Phase 5 does — supplied stream count 1).
    fn seed_promotion_baseline() -> (StorageConnection, String) {
        let (mut storage, snap, _engine_key) = seed_rust_type_with_impl_method();
        storage
            .insert_edges(&[crate::types::GraphEdge {
                edge_uid: "e-base".to_string(),
                snapshot_uid: snap.clone(),
                repo_uid: "r1".to_string(),
                source_node_uid: "n-engine-run".to_string(),
                target_node_uid: "n-other-run".to_string(),
                edge_type: "CALLS".to_string(),
                resolution: "static".to_string(),
                extractor: "test:0.0.1".to_string(),
                location: None,
                metadata_json: None,
            }])
            .unwrap();
        storage.persist_resolved_call_aggregate(&snap, 1).unwrap();
        (storage, snap)
    }

    fn promoted_calls_edge(uid: &str, snap: &str) -> enrichment::PromotedEdge {
        enrichment::PromotedEdge {
            edge_uid: uid.to_string(),
            snapshot_uid: snap.to_string(),
            repo_uid: "r1".to_string(),
            source_node_uid: "n-engine".to_string(),
            target_node_uid: "n-engine-run".to_string(),
            edge_type: "CALLS",
            resolution: "enriched",
            extractor: "enrichment:0.1.0".to_string(),
            location: None,
            metadata_json: "{}".to_string(),
        }
    }

    fn assert_promotion_parity(storage: &StorageConnection, snap: &str, expected: u64, ctx: &str) {
        use repo_graph_trust::TrustStorageRead;
        let live = TrustStorageRead::count_edges_by_type(storage, snap, "CALLS").unwrap();
        let aggregate = TrustStorageRead::get_resolved_call_aggregate(storage, snap)
            .unwrap()
            .expect("aggregate persisted");
        assert_eq!(live, expected, "{ctx}: live CALLS rows");
        assert_eq!(
            aggregate.count, live,
            "{ctx}: persisted aggregate == live COUNT (parity window)"
        );
        assert_eq!(
            aggregate.provenance,
            crate::crud::snapshots::RESOLVED_CALL_PROVENANCE_PIPELINE,
            "{ctx}: provenance label survives"
        );
    }

    #[test]
    fn apply_promotion_success_keeps_aggregate_parity_and_is_idempotent() {
        let (storage, snap) = seed_promotion_baseline();
        assert_promotion_parity(&storage, &snap, 1, "baseline");

        // Promotion lands a new resolved CALLS row: net +1, atomic.
        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            &[promoted_calls_edge("promoted:e1", &snap)],
        )
        .unwrap();
        assert_eq!(inserted, 1);
        assert_promotion_parity(&storage, &snap, 2, "after promotion");

        // Idempotent re-promotion of the SAME uid: delete 1 + insert 1 →
        // net 0. The delete-side counting keeps the arithmetic exact.
        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            &[promoted_calls_edge("promoted:e1", &snap)],
        )
        .unwrap();
        assert_eq!(inserted, 1);
        assert_promotion_parity(&storage, &snap, 2, "after re-promotion");
    }

    /// review-0 item 2 (failure path): a hard failure AFTER rows were
    /// already mutated inside the promotion transaction must roll back
    /// EVERYTHING — rows and aggregate revert together; the aggregate is
    /// never stale relative to a partial mutation.
    ///
    /// Mechanism: 101 previously-promoted rows force TWO delete chunks
    /// (chunk size 100). A SQLite trigger aborts the DELETE that touches
    /// the 101st uid, so chunk 1's 100 deletions have already executed
    /// inside the transaction when the hard error fires — the exact
    /// "batched deletion partially mutates rows before returning an error"
    /// scenario from the review.
    #[test]
    fn apply_promotion_hard_failure_rolls_back_rows_and_aggregate() {
        let (storage, snap) = seed_promotion_baseline();

        // 101 previously-promoted CALLS rows (a prior promotion pass).
        let prior: Vec<enrichment::PromotedEdge> = (0..101)
            .map(|i| promoted_calls_edge(&format!("promoted:e{i:03}"), &snap))
            .collect();
        let inserted = EnrichmentStoragePort::apply_promotion(&storage, &snap, &prior).unwrap();
        assert_eq!(inserted, 101);
        assert_promotion_parity(&storage, &snap, 102, "after prior promotion");

        // Injected hard failure on the SECOND delete chunk (uid index 100).
        storage
            .connection()
            .execute_batch(
                "CREATE TRIGGER fail_on_e100 BEFORE DELETE ON edges \
                 WHEN OLD.edge_uid = 'promoted:e100' \
                 BEGIN SELECT RAISE(ABORT, 'injected mid-promotion failure'); END",
            )
            .unwrap();

        // Re-promotion of the same 101 uids now dies mid-delete.
        let result = EnrichmentStoragePort::apply_promotion(&storage, &snap, &prior);
        assert!(
            result.is_err(),
            "the injected trigger must surface as a hard error"
        );

        // Rollback proof: chunk 1's 100 deletions were undone with the
        // transaction — rows AND aggregate exactly as before the call.
        assert_promotion_parity(&storage, &snap, 102, "after failed promotion");

        // Recovery: drop the trigger; the same promotion applies cleanly
        // (delete 101 + insert 101 → net 0).
        storage
            .connection()
            .execute_batch("DROP TRIGGER fail_on_e100")
            .unwrap();
        let inserted = EnrichmentStoragePort::apply_promotion(&storage, &snap, &prior).unwrap();
        assert_eq!(inserted, 101);
        assert_promotion_parity(&storage, &snap, 102, "after recovery");
    }

    // ── EC-1 M-3a: apply_promotion keeps the g2/g3 families coherent ──
    //
    // Value-level pins for the family delta arithmetic (the integration
    // suite `tests/call_aggregate_families.rs` covers what the SERVED
    // read paths observe; these tests inspect the raw rows).

    use crate::crud::call_aggregates::{ResolvedCallFilePairRow, SymbolCallDegreeRow};
    use crate::types::TrackedFile;

    /// Repo + READY snapshot + three FILE-MAPPED symbols (a/b/c.ts) +
    /// one pipeline CALLS row a→b + parity-true families + g1 aggregate.
    fn seed_m3a_promotion_fixture() -> (StorageConnection, String) {
        let mut storage = setup_test_db();
        storage
            .add_repo(&Repo {
                repo_uid: "r1".to_string(),
                name: "repo".to_string(),
                root_path: "/tmp/r1".to_string(),
                default_branch: None,
                created_at: "2026-07-17T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                parent_snapshot_uid: None,
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid;
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: snap.clone(),
                status: "ready".to_string(),
                completed_at: Some("2026-07-17T00:01:00Z".to_string()),
            })
            .unwrap();

        let file = |uid: &str, path: &str| TrackedFile {
            file_uid: uid.to_string(),
            repo_uid: "r1".to_string(),
            path: path.to_string(),
            language: Some("typescript".to_string()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        };
        storage
            .upsert_files(&[
                file("r1:src/a.ts", "src/a.ts"),
                file("r1:src/b.ts", "src/b.ts"),
                file("r1:src/c.ts", "src/c.ts"),
            ])
            .unwrap();

        let node = |uid: &str, name: &str, file_uid: &str| GraphNode {
            node_uid: uid.to_string(),
            snapshot_uid: snap.clone(),
            repo_uid: "r1".to_string(),
            stable_key: format!("r1:{name}:SYMBOL:FUNCTION"),
            kind: "SYMBOL".to_string(),
            subtype: Some("FUNCTION".to_string()),
            name: name.to_string(),
            qualified_name: Some(name.to_string()),
            file_uid: Some(file_uid.to_string()),
            parent_node_uid: None,
            location: None,
            signature: None,
            visibility: Some("export".to_string()),
            doc_comment: None,
            metadata_json: None,
        };
        storage
            .insert_nodes(&[
                node("n-a", "fnA", "r1:src/a.ts"),
                node("n-b", "fnB", "r1:src/b.ts"),
                node("n-c", "fnC", "r1:src/c.ts"),
            ])
            .unwrap();
        storage
            .insert_edges(&[crate::types::GraphEdge {
                edge_uid: "e-pipe".to_string(),
                snapshot_uid: snap.clone(),
                repo_uid: "r1".to_string(),
                source_node_uid: "n-a".to_string(),
                target_node_uid: "n-b".to_string(),
                edge_type: "CALLS".to_string(),
                resolution: "static".to_string(),
                extractor: "test:0.0.1".to_string(),
                location: None,
                metadata_json: None,
            }])
            .unwrap();

        // Parity-true families + g1 (what Phase-5 finalization supplies).
        storage
            .persist_symbol_call_degrees(
                &snap,
                &[
                    SymbolCallDegreeRow {
                        node_uid: "n-a".into(),
                        call_fan_in: 0,
                        call_fan_out: 1,
                    },
                    SymbolCallDegreeRow {
                        node_uid: "n-b".into(),
                        call_fan_in: 1,
                        call_fan_out: 0,
                    },
                ],
            )
            .unwrap();
        storage
            .persist_resolved_call_file_pairs(
                &snap,
                &[ResolvedCallFilePairRow {
                    source_file: "src/a.ts".into(),
                    target_file: "src/b.ts".into(),
                    call_edge_count: 1,
                }],
            )
            .unwrap();
        storage.persist_resolved_call_aggregate(&snap, 1).unwrap();

        (storage, snap)
    }

    fn m3a_promoted_edge(uid: &str, snap: &str) -> enrichment::PromotedEdge {
        // b→c: a NEW cross-file call (fan-out for n-b, fan-in for n-c,
        // pair src/b.ts → src/c.ts).
        enrichment::PromotedEdge {
            edge_uid: uid.to_string(),
            snapshot_uid: snap.to_string(),
            repo_uid: "r1".to_string(),
            source_node_uid: "n-b".to_string(),
            target_node_uid: "n-c".to_string(),
            edge_type: "CALLS",
            resolution: "enriched",
            extractor: "enrichment:0.1.0".to_string(),
            location: None,
            metadata_json: "{}".to_string(),
        }
    }

    fn degree(storage: &StorageConnection, snap: &str, node: &str) -> Option<(i64, i64)> {
        storage
            .connection()
            .query_row(
                "SELECT call_fan_in, call_fan_out FROM symbol_call_degrees \
                 WHERE snapshot_uid = ?1 AND node_uid = ?2",
                rusqlite::params![snap, node],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok()
    }

    fn pair_count(storage: &StorageConnection, snap: &str, src: &str, tgt: &str) -> Option<i64> {
        storage
            .connection()
            .query_row(
                "SELECT call_edge_count FROM resolved_call_file_pairs \
                 WHERE snapshot_uid = ?1 AND source_file = ?2 AND target_file = ?3",
                rusqlite::params![snap, src, tgt],
                |row| row.get(0),
            )
            .ok()
    }

    /// Parity window at the row level: persisted degrees/pairs == what a
    /// live derivation over the current CALLS rows would say.
    fn assert_m3a_row_parity(storage: &StorageConnection, snap: &str, ctx: &str) {
        // Degrees: every node's persisted fan-in/out vs live counts.
        for node in ["n-a", "n-b", "n-c"] {
            let live_in: i64 = storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM edges WHERE snapshot_uid = ?1 \
                     AND type = 'CALLS' AND target_node_uid = ?2",
                    rusqlite::params![snap, node],
                    |row| row.get(0),
                )
                .unwrap();
            let live_out: i64 = storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM edges WHERE snapshot_uid = ?1 \
                     AND type = 'CALLS' AND source_node_uid = ?2",
                    rusqlite::params![snap, node],
                    |row| row.get(0),
                )
                .unwrap();
            let (fan_in, fan_out) = degree(storage, snap, node).unwrap_or((0, 0));
            assert_eq!(
                (fan_in, fan_out),
                (live_in, live_out),
                "{ctx}: node {node} persisted degrees == live row-derived degrees"
            );
        }
        // Pairs: persisted visible pair set == live DISTINCT pair set.
        let live_pairs: Vec<(String, String, i64)> = {
            let conn = storage.connection();
            let mut stmt = conn
                .prepare(
                    "SELECT src_f.path, tgt_f.path, COUNT(*) FROM edges e \
                     JOIN nodes sn ON e.source_node_uid = sn.node_uid \
                     JOIN files src_f ON sn.file_uid = src_f.file_uid \
                     JOIN nodes tn ON e.target_node_uid = tn.node_uid \
                     JOIN files tgt_f ON tn.file_uid = tgt_f.file_uid \
                     WHERE e.snapshot_uid = ?1 AND e.type = 'CALLS' \
                       AND src_f.path <> tgt_f.path \
                     GROUP BY src_f.path, tgt_f.path \
                     ORDER BY src_f.path, tgt_f.path",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![snap], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows
        };
        let persisted_pairs: Vec<(String, String, i64)> = {
            let conn = storage.connection();
            let mut stmt = conn
                .prepare(
                    "SELECT source_file, target_file, call_edge_count \
                     FROM resolved_call_file_pairs \
                     WHERE snapshot_uid = ?1 AND call_edge_count > 0 \
                     ORDER BY source_file, target_file",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![snap], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows
        };
        assert_eq!(
            persisted_pairs, live_pairs,
            "{ctx}: persisted visible pairs == live row-derived pairs"
        );
    }

    #[test]
    fn apply_promotion_adjusts_family_values_exactly_and_idempotently() {
        let (storage, snap) = seed_m3a_promotion_fixture();
        assert_m3a_row_parity(&storage, &snap, "baseline");

        // Promotion lands b→c: fan-out upsert on existing n-b row,
        // fan-in insert for previously-absent n-c, new pair row.
        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            &[m3a_promoted_edge("promoted:e1", &snap)],
        )
        .unwrap();
        assert_eq!(inserted, 1);
        assert_eq!(degree(&storage, &snap, "n-b"), Some((1, 1)));
        assert_eq!(degree(&storage, &snap, "n-c"), Some((1, 0)));
        assert_eq!(pair_count(&storage, &snap, "src/b.ts", "src/c.ts"), Some(1));
        assert_m3a_row_parity(&storage, &snap, "after promotion");

        // Idempotent re-promotion: delete+insert of the same uid nets 0
        // in every family.
        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            &[m3a_promoted_edge("promoted:e1", &snap)],
        )
        .unwrap();
        assert_eq!(inserted, 1);
        assert_eq!(degree(&storage, &snap, "n-b"), Some((1, 1)));
        assert_eq!(degree(&storage, &snap, "n-c"), Some((1, 0)));
        assert_eq!(pair_count(&storage, &snap, "src/b.ts", "src/c.ts"), Some(1));
        assert_m3a_row_parity(&storage, &snap, "after re-promotion");
    }

    /// A hard failure mid-transaction must roll back the family deltas
    /// WITH the row mutations (same guarantee the M-3b test pins for the
    /// g1 aggregate — here for g2/g3).
    #[test]
    fn apply_promotion_hard_failure_rolls_back_family_deltas() {
        let (storage, snap) = seed_m3a_promotion_fixture();
        let promoted = m3a_promoted_edge("promoted:e1", &snap);
        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            std::slice::from_ref(&promoted),
        )
        .unwrap();
        assert_eq!(inserted, 1);
        assert_m3a_row_parity(&storage, &snap, "after first promotion");

        // Abort the re-promotion INSIDE the transaction, after its delete
        // has already mutated rows (BEFORE INSERT trigger).
        storage
            .connection()
            .execute_batch(
                "CREATE TRIGGER fail_reinsert BEFORE INSERT ON edges \
                 WHEN NEW.edge_uid = 'promoted:e1' \
                 BEGIN SELECT RAISE(ABORT, 'injected mid-promotion failure'); END",
            )
            .unwrap();
        // The insert failure is TOLERATED per-edge (partial success), so
        // the transaction commits with delete-without-reinsert: families
        // must follow the rows DOWN (net −1) — coherent, not stale.
        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            std::slice::from_ref(&promoted),
        )
        .unwrap();
        assert_eq!(inserted, 0, "insert was blocked by the trigger");
        assert_eq!(degree(&storage, &snap, "n-b"), Some((1, 0)));
        assert_eq!(degree(&storage, &snap, "n-c"), Some((0, 0)));
        assert_eq!(pair_count(&storage, &snap, "src/b.ts", "src/c.ts"), Some(0));
        assert_m3a_row_parity(&storage, &snap, "after blocked re-insert");

        // Now abort the DELETE instead: a hard error → whole-transaction
        // rollback, families and rows revert together.
        storage
            .connection()
            .execute_batch("DROP TRIGGER fail_reinsert")
            .unwrap();
        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            std::slice::from_ref(&promoted),
        )
        .unwrap();
        assert_eq!(inserted, 1, "recovery promotion lands");
        assert_m3a_row_parity(&storage, &snap, "after recovery");
        storage
            .connection()
            .execute_batch(
                "CREATE TRIGGER fail_delete BEFORE DELETE ON edges \
                 WHEN OLD.edge_uid = 'promoted:e1' \
                 BEGIN SELECT RAISE(ABORT, 'injected delete failure'); END",
            )
            .unwrap();
        let result = EnrichmentStoragePort::apply_promotion(&storage, &snap, &[promoted]);
        assert!(result.is_err(), "delete abort must surface as a hard error");
        assert_eq!(
            degree(&storage, &snap, "n-b"),
            Some((1, 1)),
            "rollback: degrees revert with the rows"
        );
        assert_eq!(
            pair_count(&storage, &snap, "src/b.ts", "src/c.ts"),
            Some(1),
            "rollback: pair counts revert with the rows"
        );
        assert_m3a_row_parity(&storage, &snap, "after rolled-back promotion");
    }

    /// A snapshot with NO persisted families (pre-migration shape) gains
    /// NO family rows through a real promotion — never seeded; the
    /// labeled live-derived fallback stays in force.
    #[test]
    fn apply_promotion_never_seeds_missing_families() {
        let (storage, snap, _engine_key) = seed_rust_type_with_impl_method();

        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            &[promoted_calls_edge("promoted:e1", &snap)],
        )
        .unwrap();
        assert_eq!(inserted, 1, "the promotion itself lands");

        for table in ["symbol_call_degrees", "resolved_call_file_pairs"] {
            let rows: i64 = storage
                .connection()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE snapshot_uid = ?1"),
                    rusqlite::params![snap],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(rows, 0, "{table} must never be seeded by promotion");
        }
    }

    /// A snapshot with NO persisted aggregate (pre-migration shape) keeps
    /// NULL through a real promotion: the delta is NULL-propagating and the
    /// aggregate is never seeded from row counts — the labeled live-COUNT
    /// fallback stays in force (unknown is never fabricated).
    #[test]
    fn apply_promotion_never_seeds_a_missing_aggregate() {
        use repo_graph_trust::TrustStorageRead;

        let (storage, snap, _engine_key) = seed_rust_type_with_impl_method();
        assert_eq!(
            TrustStorageRead::get_resolved_call_aggregate(&storage, &snap).unwrap(),
            None,
            "precondition: pre-migration shape (no aggregate)"
        );

        let inserted = EnrichmentStoragePort::apply_promotion(
            &storage,
            &snap,
            &[promoted_calls_edge("promoted:e1", &snap)],
        )
        .unwrap();
        assert_eq!(inserted, 1, "the promotion itself lands");

        assert_eq!(
            TrustStorageRead::get_resolved_call_aggregate(&storage, &snap).unwrap(),
            None,
            "aggregate stays explicitly unavailable — fallback applies; never seeded"
        );
    }

    // ── ENRICH-YIELD-2 EY1-D: Rust enum receiver promotes end-to-end ───────────────────────────
    //
    // The full boundary: the extractor writes a Rust enum as subtype `"ENUM"` and its `impl` method
    // unparented (type carried in `qualified_name`). This proves, through the REAL SQLite adapter,
    // that (1) `load_symbols_by_names` returns the enum with subtype parsed to `Enum` (not collapsed
    // to `Other`), (2) `is_usable_receiver_type()` accepts it so its methods load, (3)
    // `load_class_methods` finds the enum's method by qualified_name, and (4) the 8-gate filter
    // promotes the call. Before EY1-D the subtype collapsed to `Other` and gate 5 rejected the enum
    // as `type_not_a_class` — a real single-answer type lost to a Class-only predicate.
    #[test]
    fn rust_enum_impl_method_receiver_promotes_end_to_end() {
        use enrichment::{promote_edges, PromotionContext};

        let mut storage = setup_test_db();
        storage
            .add_repo(&Repo {
                repo_uid: "r1".to_string(),
                name: "repo".to_string(),
                root_path: "/tmp/r1".to_string(),
                default_branch: None,
                created_at: "2026-07-12T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                parent_snapshot_uid: None,
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid;
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: snap.clone(),
                status: "ready".to_string(),
                completed_at: Some("2026-07-12T00:01:00Z".to_string()),
            })
            .unwrap();

        let status_key = "r1:src/status.rs#Status:SYMBOL:ENUM".to_string();
        storage
            .insert_nodes(&[
                // Rust enum → NodeSubtype::Enum → the extractor writes "ENUM".
                rust_node("n-status", &status_key, "ENUM", "Status", "Status", &snap),
                // Its `impl Status { fn is_active(&self) }` method — unparented, type in
                // qualified_name, exactly like a struct impl method.
                rust_node(
                    "n-status-active",
                    "r1:src/status.rs#Status.is_active:SYMBOL:METHOD",
                    "METHOD",
                    "is_active",
                    "Status.is_active",
                    &snap,
                ),
            ])
            .unwrap();

        // The adapter returns the enum with its subtype preserved as `Enum` (EY1-D boundary).
        let symbols =
            EnrichmentStoragePort::load_symbols_by_names(&storage, &snap, &["Status".to_string()])
                .unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(
            symbols[0].subtype,
            SymbolSubtype::Enum,
            "the enum subtype survives the storage read (not collapsed to Other)"
        );

        // Build the promotion context with the SAME predicate the pipeline uses (Class|Enum), so the
        // enum's methods load into the context.
        let mut ctx = PromotionContext::new();
        for sym in symbols {
            let usable = sym.subtype.is_usable_receiver_type();
            let key = sym.stable_key.clone();
            ctx.add_symbol(sym);
            if usable {
                for (mname, minfo) in
                    EnrichmentStoragePort::load_class_methods(&storage, &snap, &key).unwrap()
                {
                    ctx.add_class_method(&key, &mname, minfo);
                }
            }
        }

        // A resolved `status.is_active()` receiver (rust-analyzer resolves the enum type "Status").
        let candidate = PromotionCandidate {
            edge_uid: "e-enum-1".to_string(),
            snapshot_uid: snap.clone(),
            repo_uid: "r1".to_string(),
            source_node_uid: "n-caller".to_string(),
            target_key: "status.is_active".to_string(),
            line_start: Some(7),
            col_start: Some(4),
            line_end: Some(7),
            col_end: Some(22),
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            enrichment: EnrichmentMetadata {
                receiver_type: Some("Status".to_string()),
                type_display_name: Some("Status".to_string()),
                is_external_type: false,
                origin: ReceiverTypeOrigin::Compiler,
                failure_reason: None,
            },
        };

        let result = promote_edges(&[candidate], &ctx);

        assert_eq!(
            result.promoted.len(),
            1,
            "a resolved Rust ENUM method receiver promotes; skipped: {:?}",
            result.skipped_reasons
        );
        assert_eq!(
            result.promoted[0].target_node_uid, "n-status-active",
            "the promoted CALLS edge targets the enum's impl method"
        );
    }
}
