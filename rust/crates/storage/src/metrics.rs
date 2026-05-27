//! PERF-OBS-1: Storage metrics queries for performance observability.
//!
//! This module provides one-shot measurement queries for the `rmap perf`
//! command. These are diagnostic queries, not production read paths.
//!
//! ## Metrics Categories
//!
//! - **Volume**: Row counts and size estimates per table
//! - **Tier**: Grouped by storage tier (A: authority, B: cache)
//! - **Layer**: Grouped by fact layer (0-1: extracted, 2: derived, 3: hints)
//! - **Retention**: Snapshot count and age analysis
//!
//! See `docs/slices/perf-obs-1.md` for the full observability spec.

use serde::{Deserialize, Serialize};

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// Per-table metrics row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetrics {
    pub name: String,
    pub row_count: i64,
    /// Estimated size in bytes (from dbstat if available, else 0)
    pub size_bytes: i64,
    /// Tier classification: "A" (authority), "B" (cache), "unknown", or "varies"
    pub tier: String,
    /// Fact layer: "0-1" (extracted), "2" (derived), "3" (hints), "N/A", "unknown", or "varies"
    pub layer: String,
}

/// Database-level metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseMetrics {
    /// Total database file size in bytes (from PRAGMA page_count * page_size)
    pub total_size_bytes: i64,
    /// Page size in bytes
    pub page_size: i64,
    /// Total page count
    pub page_count: i64,
    /// Per-table metrics
    pub tables: Vec<TableMetrics>,
    /// Tier A (authority) row count total
    pub tier_a_rows: i64,
    /// Tier B (cache) row count total
    pub tier_b_rows: i64,
    /// Layer 0-1 (extracted) row count total
    pub layer_01_rows: i64,
    /// Layer 2 (derived) row count total
    pub layer_2_rows: i64,
    /// Layer 3 (hints) row count total
    pub layer_3_rows: i64,
    /// Classification coverage report
    pub classification: ClassificationCoverage,
}

/// Classification coverage report.
///
/// Shows how much of the data is in known vs unknown/varies classifications.
/// Tier/layer percentages are only trustworthy if unknown_rows is low.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationCoverage {
    /// Total rows across all tables
    pub total_rows: i64,
    /// Rows in tables with known tier (A or B)
    pub classified_tier_rows: i64,
    /// Rows in tables with tier="unknown" or "varies"
    pub unclassified_tier_rows: i64,
    /// Rows in tables with known layer (0-1, 2, 3, N/A)
    pub classified_layer_rows: i64,
    /// Rows in tables with layer="unknown" or "varies"
    pub unclassified_layer_rows: i64,
    /// Tables with unknown tier
    pub unknown_tier_tables: Vec<String>,
    /// Tables with unknown layer
    pub unknown_layer_tables: Vec<String>,
}

/// Snapshot retention metrics for a repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRetentionMetrics {
    pub total_snapshots: i64,
    pub ready_snapshots: i64,
    pub failed_snapshots: i64,
    /// Oldest snapshot timestamp (ISO 8601)
    pub oldest_snapshot: Option<String>,
    /// Newest snapshot timestamp (ISO 8601)
    pub newest_snapshot: Option<String>,
}

/// Table-to-tier classification.
///
/// Based on STORAGE-ARCH-1 (agent_docs/storage-architecture-v2.md).
fn classify_table(name: &str) -> (&'static str, &'static str) {
    match name {
        // Tier A: Authority
        "repos" => ("A", "N/A"),
        "declarations" => ("A", "N/A"),
        "schema_migrations" => ("A", "N/A"),
        "snapshots" => ("A", "N/A"), // Metadata only

        // Tier B, Layer 0-1: Extracted facts
        "nodes" => ("B", "0-1"),
        "edges" => ("B", "0-1"),
        "files" => ("B", "0-1"),
        "file_versions" => ("B", "0-1"),
        "measurements" => ("B", "0-1"),
        "unresolved_edges" => ("B", "0-1"),
        "extraction_edges" => ("B", "0-1"),
        "staged_edges" => ("B", "0-1"),
        "file_signals" => ("B", "0-1"),

        // Tier B, Layer 2: Derived/inferred
        "inferences" => ("B", "2"),
        "module_candidates" => ("B", "2"),
        "module_candidate_evidence" => ("B", "2"),
        "module_file_ownership" => ("B", "2"),
        "module_discovery_diagnostics" => ("B", "2"),
        "boundary_provider_facts" => ("B", "2"),
        "boundary_consumer_facts" => ("B", "2"),
        "boundary_links" => ("B", "2"),
        "boundary_interaction_surfaces" => ("B", "2"),
        "boundary_channel_details" => ("B", "2"),
        "boundary_contracts" => ("B", "2"),
        "boundary_interaction_links" => ("B", "2"),
        "contract_schemas" => ("B", "2"),
        "contract_elements" => ("B", "2"),
        "generated_code_mappings" => ("B", "2"),
        "semantic_facts" => ("B", "2"),
        "status_mappings" => ("B", "2"),
        "behavioral_markers" => ("B", "2"),
        "return_fates" => ("B", "2"),
        "annotations" => ("B", "2"),

        // Tier B, Layer 3: Hints
        "project_surfaces" => ("B", "3"),
        "project_surface_evidence" => ("B", "3"),
        "surface_entrypoints" => ("B", "3"),
        "surface_config_roots" => ("B", "3"),
        "surface_env_dependencies" => ("B", "3"),
        "surface_env_evidence" => ("B", "3"),
        "surface_fs_mutations" => ("B", "3"),
        "surface_fs_mutation_evidence" => ("B", "3"),
        "quality_assessments" => ("B", "3"),

        // Metadata/system tables
        "artifacts" => ("B", "varies"),
        "evidence_links" => ("B", "varies"),

        // Unknown
        _ => ("unknown", "unknown"),
    }
}

impl StorageConnection {
    /// Collect database-level metrics.
    ///
    /// Returns size estimates, per-table row counts, and tier/layer aggregates.
    /// This is a diagnostic query for `rmap perf`, not a production read path.
    pub fn collect_database_metrics(&self) -> Result<DatabaseMetrics, StorageError> {
        let conn = self.connection();

        // Get page size and count for size estimate
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let total_size_bytes = page_size * page_count;

        // Get list of tables
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master
             WHERE type='table'
             AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )?;

        let table_names: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Try to get per-table sizes via dbstat virtual table
        // This may not be available in all SQLite builds
        let table_sizes: std::collections::HashMap<String, i64> = Self::get_table_sizes(conn);

        // Count rows per table and aggregate
        let mut tables = Vec::new();
        let mut tier_a_rows = 0i64;
        let mut tier_b_rows = 0i64;
        let mut layer_01_rows = 0i64;
        let mut layer_2_rows = 0i64;
        let mut layer_3_rows = 0i64;
        let mut total_rows = 0i64;
        let mut classified_tier_rows = 0i64;
        let mut unclassified_tier_rows = 0i64;
        let mut classified_layer_rows = 0i64;
        let mut unclassified_layer_rows = 0i64;
        let mut unknown_tier_tables = Vec::new();
        let mut unknown_layer_tables = Vec::new();

        for name in table_names {
            // COUNT(*) for row count
            let count_sql = format!("SELECT COUNT(*) FROM \"{}\"", name);
            let row_count: i64 = conn
                .query_row(&count_sql, [], |row| row.get(0))
                .unwrap_or(0);

            let size_bytes = table_sizes.get(&name).copied().unwrap_or(0);

            let (tier, layer) = classify_table(&name);

            total_rows += row_count;

            // Aggregate by tier
            match tier {
                "A" => {
                    tier_a_rows += row_count;
                    classified_tier_rows += row_count;
                }
                "B" => {
                    tier_b_rows += row_count;
                    classified_tier_rows += row_count;
                }
                _ => {
                    unclassified_tier_rows += row_count;
                    if row_count > 0 {
                        unknown_tier_tables.push(name.clone());
                    }
                }
            }

            // Aggregate by layer
            match layer {
                "0-1" => {
                    layer_01_rows += row_count;
                    classified_layer_rows += row_count;
                }
                "2" => {
                    layer_2_rows += row_count;
                    classified_layer_rows += row_count;
                }
                "3" => {
                    layer_3_rows += row_count;
                    classified_layer_rows += row_count;
                }
                "N/A" => {
                    // N/A is valid for Tier A tables
                    classified_layer_rows += row_count;
                }
                _ => {
                    unclassified_layer_rows += row_count;
                    if row_count > 0 && !unknown_tier_tables.contains(&name) {
                        unknown_layer_tables.push(name.clone());
                    }
                }
            }

            tables.push(TableMetrics {
                name,
                row_count,
                size_bytes,
                tier: tier.to_string(),
                layer: layer.to_string(),
            });
        }

        let classification = ClassificationCoverage {
            total_rows,
            classified_tier_rows,
            unclassified_tier_rows,
            classified_layer_rows,
            unclassified_layer_rows,
            unknown_tier_tables,
            unknown_layer_tables,
        };

        Ok(DatabaseMetrics {
            total_size_bytes,
            page_size,
            page_count,
            tables,
            tier_a_rows,
            tier_b_rows,
            layer_01_rows,
            layer_2_rows,
            layer_3_rows,
            classification,
        })
    }

    /// Get per-table sizes using dbstat virtual table.
    ///
    /// Returns empty map if dbstat is not available.
    fn get_table_sizes(conn: &rusqlite::Connection) -> std::collections::HashMap<String, i64> {
        let mut sizes = std::collections::HashMap::new();

        // dbstat may not be compiled in; fail gracefully
        let result = conn.prepare(
            "SELECT name, SUM(pgsize) as size
             FROM dbstat
             GROUP BY name",
        );

        if let Ok(mut stmt) = result {
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let size: i64 = row.get(1)?;
                Ok((name, size))
            });

            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    sizes.insert(row.0, row.1);
                }
            }
        }

        sizes
    }

    /// Collect snapshot retention metrics for a repo.
    ///
    /// Returns snapshot counts by status and age range.
    pub fn collect_snapshot_retention_metrics(
        &self,
        repo_uid: &str,
    ) -> Result<SnapshotRetentionMetrics, StorageError> {
        let conn = self.connection();

        // Total snapshot count
        let total_snapshots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ?",
                rusqlite::params![repo_uid],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Ready snapshots
        let ready_snapshots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ? AND status = 'ready'",
                rusqlite::params![repo_uid],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Failed snapshots
        let failed_snapshots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ? AND status = 'failed'",
                rusqlite::params![repo_uid],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Oldest snapshot
        let oldest_snapshot: Option<String> = conn
            .query_row(
                "SELECT MIN(created_at) FROM snapshots WHERE repo_uid = ?",
                rusqlite::params![repo_uid],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        // Newest snapshot
        let newest_snapshot: Option<String> = conn
            .query_row(
                "SELECT MAX(created_at) FROM snapshots WHERE repo_uid = ?",
                rusqlite::params![repo_uid],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        Ok(SnapshotRetentionMetrics {
            total_snapshots,
            ready_snapshots,
            failed_snapshots,
            oldest_snapshot,
            newest_snapshot,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_table() {
        // Tier A
        assert_eq!(classify_table("repos"), ("A", "N/A"));
        assert_eq!(classify_table("declarations"), ("A", "N/A"));

        // Tier B, Layer 0-1
        assert_eq!(classify_table("nodes"), ("B", "0-1"));
        assert_eq!(classify_table("edges"), ("B", "0-1"));

        // Tier B, Layer 2
        assert_eq!(classify_table("inferences"), ("B", "2"));
        assert_eq!(classify_table("module_candidates"), ("B", "2"));

        // Tier B, Layer 3
        assert_eq!(classify_table("project_surfaces"), ("B", "3"));

        // Unknown
        assert_eq!(classify_table("unknown_table"), ("unknown", "unknown"));
    }
}
