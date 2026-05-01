//! Migration 025 — contract schema and transport class extensions.
//!
//! Foundation for multi-track boundary detection (Track B: Schema-Backed RPC).
//! See docs/design/boundary-detection-multitrack.md for the full specification.
//!
//! Changes:
//! 1. Adds `transport_class`, `provenance`, `confidence_basis` columns to
//!    `boundary_interaction_surfaces`
//! 2. Creates `contract_schemas` table for protobuf/gRPC/eRPC IDL files
//! 3. Creates `contract_elements` table for messages/services/methods
//! 4. Creates `generated_code_mappings` table for schema-to-code provenance
//! 5. Creates `boundary_contracts` table for boundary-to-contract association
//! 6. Creates `boundary_interaction_links` table for provider/consumer linking

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    // ── Step 1: Add columns to boundary_interaction_surfaces ─────
    //
    // Check if each column exists before adding (idempotent pattern).
    let bis_cols = pragma_table_columns(conn, "boundary_interaction_surfaces")?;

    if !bis_cols.contains(&"transport_class".to_string()) {
        conn.execute(
            "ALTER TABLE boundary_interaction_surfaces ADD COLUMN transport_class TEXT",
            [],
        )?;
    }

    if !bis_cols.contains(&"provenance".to_string()) {
        conn.execute(
            "ALTER TABLE boundary_interaction_surfaces ADD COLUMN provenance TEXT",
            [],
        )?;
    }

    if !bis_cols.contains(&"confidence_basis".to_string()) {
        conn.execute(
            "ALTER TABLE boundary_interaction_surfaces ADD COLUMN confidence_basis TEXT",
            [],
        )?;
    }

    // ── Step 2: Create contract/IDL tables ───────────────────────
    conn.execute_batch(
        r#"
        -- Contract/schema storage (CS-1: Protobuf schema extraction)
        -- One row per .proto or other IDL file parsed.
        CREATE TABLE IF NOT EXISTS contract_schemas (
            schema_uid TEXT PRIMARY KEY,
            snapshot_uid TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            repo_uid TEXT NOT NULL REFERENCES repos(repo_uid),
            schema_kind TEXT NOT NULL,      -- 'protobuf', 'grpc', 'erpc', 'capn_proto', 'flatbuffers'
            file_path TEXT NOT NULL,        -- Repo-relative path to IDL file
            package_name TEXT,              -- Package namespace (e.g., 'api.v1')
            syntax_version TEXT,            -- 'proto2', 'proto3', etc.
            content_hash TEXT NOT NULL,     -- For cache invalidation
            imports_json TEXT,              -- JSON array of imported file paths
            options_json TEXT,              -- JSON object of file-level options
            extractor TEXT NOT NULL,        -- 'proto-parser:0.1.0'
            parsed_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_cs_snapshot_kind
            ON contract_schemas(snapshot_uid, schema_kind);
        CREATE INDEX IF NOT EXISTS idx_cs_snapshot_file
            ON contract_schemas(snapshot_uid, file_path);
        CREATE INDEX IF NOT EXISTS idx_cs_snapshot_package
            ON contract_schemas(snapshot_uid, package_name);

        -- Contract elements (messages, enums, services, methods)
        -- One row per named element within a schema file.
        CREATE TABLE IF NOT EXISTS contract_elements (
            element_uid TEXT PRIMARY KEY,
            schema_uid TEXT NOT NULL REFERENCES contract_schemas(schema_uid) ON DELETE CASCADE,
            element_kind TEXT NOT NULL,     -- 'message', 'enum', 'service', 'method', 'field'
            name TEXT NOT NULL,             -- Short name without package
            full_name TEXT NOT NULL,        -- Fully qualified: package.OuterMessage.InnerMessage
            parent_element_uid TEXT REFERENCES contract_elements(element_uid) ON DELETE CASCADE,
            line_start INTEGER,
            line_end INTEGER,
            metadata_json TEXT              -- Element-specific details (fields, values, options)
        );

        CREATE INDEX IF NOT EXISTS idx_ce_schema
            ON contract_elements(schema_uid);
        CREATE INDEX IF NOT EXISTS idx_ce_kind
            ON contract_elements(element_kind);
        CREATE INDEX IF NOT EXISTS idx_ce_full_name
            ON contract_elements(full_name);
        CREATE INDEX IF NOT EXISTS idx_ce_parent
            ON contract_elements(parent_element_uid);

        -- Generated code mappings (CS-2: Generated code provenance)
        -- Links schema elements to their generated code symbols.
        CREATE TABLE IF NOT EXISTS generated_code_mappings (
            mapping_uid TEXT PRIMARY KEY,
            snapshot_uid TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            schema_element_uid TEXT NOT NULL REFERENCES contract_elements(element_uid) ON DELETE CASCADE,
            generated_symbol_key TEXT NOT NULL,     -- Stable key of generated code symbol
            language TEXT NOT NULL,                  -- 'python', 'cpp', 'rust', 'java', 'typescript'
            generated_file TEXT NOT NULL,            -- Repo-relative path to generated file
            mapping_basis TEXT NOT NULL,             -- 'file_pattern', 'name_transform', 'import_trace', etc.
            confidence REAL NOT NULL,
            metadata_json TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_gcm_snapshot
            ON generated_code_mappings(snapshot_uid);
        CREATE INDEX IF NOT EXISTS idx_gcm_schema_element
            ON generated_code_mappings(schema_element_uid);
        CREATE INDEX IF NOT EXISTS idx_gcm_symbol
            ON generated_code_mappings(generated_symbol_key);
        CREATE INDEX IF NOT EXISTS idx_gcm_language
            ON generated_code_mappings(snapshot_uid, language);

        -- Boundary-to-contract association
        -- Links boundary interaction surfaces to the contracts they carry.
        CREATE TABLE IF NOT EXISTS boundary_contracts (
            association_uid TEXT PRIMARY KEY,
            surface_uid TEXT NOT NULL REFERENCES boundary_interaction_surfaces(surface_uid) ON DELETE CASCADE,
            contract_element_uid TEXT REFERENCES contract_elements(element_uid) ON DELETE SET NULL,
            contract_kind TEXT NOT NULL,             -- 'protobuf_message', 'grpc_method', 'erpc_method', 'shm_layout', 'none'
            association_basis TEXT NOT NULL,         -- 'schema_type', 'usage_site', 'config', 'inferred'
            confidence REAL NOT NULL,
            evidence_json TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_bc_surface
            ON boundary_contracts(surface_uid);
        CREATE INDEX IF NOT EXISTS idx_bc_contract_element
            ON boundary_contracts(contract_element_uid);
        CREATE INDEX IF NOT EXISTS idx_bc_contract_kind
            ON boundary_contracts(contract_kind);

        -- Boundary interaction links (GR-3: Provider/consumer linking)
        -- Links provider surfaces to consumer surfaces based on contract matching.
        -- Named boundary_interaction_links to avoid collision with migration 008's
        -- boundary_links table (which references boundary_provider_facts/consumer_facts).
        CREATE TABLE IF NOT EXISTS boundary_interaction_links (
            link_uid TEXT PRIMARY KEY,
            snapshot_uid TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            provider_surface_uid TEXT NOT NULL REFERENCES boundary_interaction_surfaces(surface_uid) ON DELETE CASCADE,
            consumer_surface_uid TEXT NOT NULL REFERENCES boundary_interaction_surfaces(surface_uid) ON DELETE CASCADE,
            link_kind TEXT NOT NULL,                 -- 'internal_link', 'contract_match_only'
            contract_element_uid TEXT REFERENCES contract_elements(element_uid) ON DELETE SET NULL,
            match_basis TEXT NOT NULL,               -- 'contract', 'contract_and_endpoint'
            confidence REAL NOT NULL,
            evidence_json TEXT,
            materialized_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_bil_snapshot
            ON boundary_interaction_links(snapshot_uid);
        CREATE INDEX IF NOT EXISTS idx_bil_provider
            ON boundary_interaction_links(provider_surface_uid);
        CREATE INDEX IF NOT EXISTS idx_bil_consumer
            ON boundary_interaction_links(consumer_surface_uid);
        CREATE INDEX IF NOT EXISTS idx_bil_contract
            ON boundary_interaction_links(contract_element_uid);
        "#,
    )?;

    record_migration(conn, 25, "025-contract-schema")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_025_creates_contract_tables() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Bootstrap with migrations 001 and 024 (creates boundary_interaction_surfaces)
        crate::migrations::migration_001::run(&mut conn).unwrap();
        crate::migrations::migration_024::run(&mut conn).unwrap();

        // Run migration 025
        run(&mut conn).unwrap();

        // Verify tables exist
        let tables = [
            "contract_schemas",
            "contract_elements",
            "generated_code_mappings",
            "boundary_contracts",
            "boundary_interaction_links",
        ];

        for table in tables {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        table
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {} should exist", table);
        }

        // Verify migration recorded
        let version: i64 = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE name = '025-contract-schema'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 25);
    }

    #[test]
    fn migration_025_adds_columns_to_boundary_interaction_surfaces() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();
        crate::migrations::migration_024::run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        let cols = pragma_table_columns(&conn, "boundary_interaction_surfaces").unwrap();
        assert!(
            cols.contains(&"transport_class".to_string()),
            "transport_class column should exist"
        );
        assert!(
            cols.contains(&"provenance".to_string()),
            "provenance column should exist"
        );
        assert!(
            cols.contains(&"confidence_basis".to_string()),
            "confidence_basis column should exist"
        );
    }

    #[test]
    fn migration_025_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();
        crate::migrations::migration_024::run(&mut conn).unwrap();

        // Run twice
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Should still have exactly one of each table
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='contract_schemas'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_025_cascade_delete_works() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();
        crate::migrations::migration_024::run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Create prerequisite rows
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/abs', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'building', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Insert a contract schema
        conn.execute(
            "INSERT INTO contract_schemas (
                schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                package_name, content_hash, extractor, parsed_at
            ) VALUES (
                'cs1', 's1', 'r1', 'protobuf', 'api/v1/user.proto',
                'api.v1', 'abc123', 'proto-parser:0.1.0', '2025-01-01T00:00:00Z'
            )",
            [],
        )
        .unwrap();

        // Insert a contract element
        conn.execute(
            "INSERT INTO contract_elements (
                element_uid, schema_uid, element_kind, name, full_name
            ) VALUES (
                'ce1', 'cs1', 'message', 'User', 'api.v1.User'
            )",
            [],
        )
        .unwrap();

        // Verify both exist
        let schema_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM contract_schemas", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(schema_count, 1);

        let element_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM contract_elements", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(element_count, 1);

        // Delete the snapshot (should cascade to schema and element)
        conn.execute("DELETE FROM snapshots WHERE snapshot_uid = 's1'", [])
            .unwrap();

        // Both should be gone
        let schema_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM contract_schemas", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(schema_count_after, 0);

        let element_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM contract_elements", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(element_count_after, 0);
    }
}
