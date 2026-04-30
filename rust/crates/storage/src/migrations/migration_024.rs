//! Migration 024 — add boundary interaction tables.
//!
//! Supports the boundary-interaction slice (BI-1A: Local IPC).
//! See docs/design/boundary-interaction-ipc-device.md for full specification.
//!
//! Two-level model:
//! - boundary_interaction_surfaces (Level 1): architectural relationship
//! - boundary_channel_details (Level 2): mechanism-specific addressing
//!
//! Slice 1A scope: Unix sockets, pipes, shared memory, message queues.
//! All mechanisms have unambiguous `inter_process` boundary scope.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        r#"
        -- Level 1: Boundary interaction surfaces
        -- High-level architectural relationship between communicating parties.
        CREATE TABLE IF NOT EXISTS boundary_interaction_surfaces (
            surface_uid         TEXT PRIMARY KEY,
            snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),

            -- Classification
            boundary_scope      TEXT NOT NULL,    -- inter_process | inter_device | unknown
            channel_kind        TEXT NOT NULL,    -- unix_socket | named_pipe | shared_memory | ...
            direction           TEXT NOT NULL,    -- provider | consumer | bidirectional

            -- Protocol
            protocol            TEXT NOT NULL,    -- unix | tcp | udp | shm | pipe | ...
            protocol_family     TEXT NOT NULL,    -- socket | pipe | shared_memory | message_queue | ...
            interaction_pattern TEXT NOT NULL,    -- request_response | publish_subscribe | stream | fire_and_forget | shared_state

            -- Endpoint locality (observable from callsite)
            endpoint_locality   TEXT NOT NULL,    -- loopback | same_host_named | remote_literal | unknown

            -- Source provenance
            symbol_stable_key   TEXT NOT NULL,
            source_file         TEXT NOT NULL,
            line_start          INTEGER NOT NULL,
            line_end            INTEGER NOT NULL,
            col_start           INTEGER NOT NULL,
            col_end             INTEGER NOT NULL,

            -- Extraction provenance
            extractor           TEXT NOT NULL,
            basis               TEXT NOT NULL,    -- api_call | wrapper_call | annotation | ...
            confidence          REAL NOT NULL,
            evidence_json       TEXT NOT NULL
        );

        -- Indexes for common query patterns
        CREATE INDEX IF NOT EXISTS idx_bis_snapshot_scope
            ON boundary_interaction_surfaces(snapshot_uid, boundary_scope);

        CREATE INDEX IF NOT EXISTS idx_bis_snapshot_kind
            ON boundary_interaction_surfaces(snapshot_uid, channel_kind);

        CREATE INDEX IF NOT EXISTS idx_bis_snapshot_symbol
            ON boundary_interaction_surfaces(snapshot_uid, symbol_stable_key);

        CREATE INDEX IF NOT EXISTS idx_bis_snapshot_protocol
            ON boundary_interaction_surfaces(snapshot_uid, protocol_family, protocol);

        CREATE INDEX IF NOT EXISTS idx_bis_snapshot_file
            ON boundary_interaction_surfaces(snapshot_uid, source_file);

        -- Level 2: Channel details
        -- Mechanism-specific addressing and protocol details.
        CREATE TABLE IF NOT EXISTS boundary_channel_details (
            channel_uid         TEXT PRIMARY KEY,
            surface_uid         TEXT NOT NULL REFERENCES boundary_interaction_surfaces(surface_uid) ON DELETE CASCADE,

            -- Channel identity
            channel_kind        TEXT NOT NULL,    -- unix_socket | named_pipe | shared_memory | ...
            channel_identity    TEXT NOT NULL,    -- Normalized key for matching

            -- Addressing (nullable, mechanism-specific)
            socket_path         TEXT,             -- Unix socket path
            tcp_endpoint        TEXT,             -- host:port
            udp_endpoint        TEXT,             -- host:port
            can_id              INTEGER,          -- CAN message ID
            i2c_address         INTEGER,          -- I2C device address
            spi_device          TEXT,             -- SPI device path
            serial_device       TEXT,             -- Serial device path
            shm_key             TEXT,             -- Shared memory key/name
            pipe_path           TEXT,             -- Named pipe path
            pipe_context        TEXT,             -- Anonymous pipe context
            mqueue_name         TEXT,             -- POSIX message queue name

            -- Protocol details
            baud_rate           INTEGER,          -- Serial/CAN baud rate
            can_extended        INTEGER,          -- CAN extended ID flag (0/1)
            frame_format        TEXT,             -- Binary framing description
            payload_contract    TEXT,             -- IDL/schema reference

            -- Metadata
            metadata_json       TEXT
        );

        -- Indexes for channel queries
        CREATE INDEX IF NOT EXISTS idx_bcd_surface
            ON boundary_channel_details(surface_uid);

        CREATE INDEX IF NOT EXISTS idx_bcd_channel_kind
            ON boundary_channel_details(channel_kind);

        CREATE INDEX IF NOT EXISTS idx_bcd_channel_identity
            ON boundary_channel_details(channel_identity);
        "#,
    )?;

    record_migration(conn, 24, "024-boundary-interactions")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_024_creates_boundary_interaction_tables() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Bootstrap with migration 001 (creates schema_migrations table)
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run migration 024
        run(&mut conn).unwrap();

        // Verify tables exist
        let surface_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='boundary_interaction_surfaces'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(surface_count, 1);

        let channel_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='boundary_channel_details'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(channel_count, 1);

        // Verify migration recorded
        let version: i64 = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE name = '024-boundary-interactions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 24);
    }

    #[test]
    fn migration_024_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run twice
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Should still have exactly one of each table
        let surface_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='boundary_interaction_surfaces'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(surface_count, 1);
    }

    #[test]
    fn migration_024_indexes_exist() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Verify surface indexes
        let surface_index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_bis_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(surface_index_count, 5);

        // Verify channel indexes
        let channel_index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_bcd_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(channel_index_count, 3);
    }

    #[test]
    fn migration_024_cascade_delete_works() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Create prerequisite rows
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/abs', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'building', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Insert a surface
        conn.execute(
            "INSERT INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid, boundary_scope, channel_kind, direction,
                protocol, protocol_family, interaction_pattern, endpoint_locality,
                symbol_stable_key, source_file, line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (
                'surf1', 's1', 'r1', 'inter_process', 'unix_socket', 'provider',
                'unix', 'socket', 'stream', 'same_host_named',
                'r1:src/main.c#server:SYMBOL:function', 'src/main.c', 100, 110, 5, 50,
                'c-ipc:0.1.0', 'api_call', 0.95, '{}'
            )",
            [],
        ).unwrap();

        // Insert a channel detail
        conn.execute(
            "INSERT INTO boundary_channel_details (
                channel_uid, surface_uid, channel_kind, channel_identity, socket_path
            ) VALUES (
                'ch1', 'surf1', 'unix_socket', '/var/run/daemon.sock', '/var/run/daemon.sock'
            )",
            [],
        ).unwrap();

        // Verify both exist
        let surface_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM boundary_interaction_surfaces", [], |row| row.get(0))
            .unwrap();
        assert_eq!(surface_count, 1);

        let channel_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM boundary_channel_details", [], |row| row.get(0))
            .unwrap();
        assert_eq!(channel_count, 1);

        // Delete the snapshot (should cascade to surface and channel)
        conn.execute("DELETE FROM snapshots WHERE snapshot_uid = 's1'", []).unwrap();

        // Both should be gone
        let surface_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM boundary_interaction_surfaces", [], |row| row.get(0))
            .unwrap();
        assert_eq!(surface_count_after, 0);

        let channel_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM boundary_channel_details", [], |row| row.get(0))
            .unwrap();
        assert_eq!(channel_count_after, 0);
    }

    #[test]
    fn migration_024_supports_all_slice_1a_channel_kinds() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Create prerequisite rows
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/abs', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'building', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        let slice_1a_kinds = [
            ("unix_socket", "socket", "/var/run/x.sock"),
            ("named_pipe", "pipe", "/tmp/myfifo"),
            ("anonymous_pipe", "pipe", "pipe:parent_to_child"),
            ("shared_memory", "shared_memory", "/my_shm"),
            ("message_queue", "message_queue", "/my_queue"),
        ];

        for (i, (kind, family, identity)) in slice_1a_kinds.iter().enumerate() {
            let surf_uid = format!("surf{}", i);
            let ch_uid = format!("ch{}", i);

            conn.execute(
                &format!(
                    "INSERT INTO boundary_interaction_surfaces (
                        surface_uid, snapshot_uid, repo_uid, boundary_scope, channel_kind, direction,
                        protocol, protocol_family, interaction_pattern, endpoint_locality,
                        symbol_stable_key, source_file, line_start, line_end, col_start, col_end,
                        extractor, basis, confidence, evidence_json
                    ) VALUES (
                        '{}', 's1', 'r1', 'inter_process', '{}', 'bidirectional',
                        '{}', '{}', 'stream', 'same_host_named',
                        'r1:src/main.c#fn{}:SYMBOL:function', 'src/main.c', {}, {}, {}, {},
                        'c-ipc:0.1.0', 'api_call', 0.95, '{{}}'
                    )", surf_uid, kind, kind, family, i, i * 10, i * 10 + 5, i + 1, i + 20
                ),
                [],
            ).unwrap();

            conn.execute(
                &format!(
                    "INSERT INTO boundary_channel_details (
                        channel_uid, surface_uid, channel_kind, channel_identity
                    ) VALUES ('{}', '{}', '{}', '{}')",
                    ch_uid, surf_uid, kind, identity
                ),
                [],
            ).unwrap();
        }

        // All 5 should exist
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM boundary_interaction_surfaces", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }
}
