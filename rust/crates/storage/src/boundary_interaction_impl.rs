//! Boundary interaction storage implementation for `StorageConnection`.
//!
//! Persists `BoundaryInteractionSurface` and `ChannelDetail` to the tables
//! created by migration 024.
//!
//! This is a direct storage implementation (not a port trait) because:
//! - Boundary interaction facts are always written during indexing
//! - No consumer outside the indexer needs the write port
//! - Read queries can use existing raw SQL patterns
//!
//! **TECH DEBT:** A proper port trait following the Clean Architecture pattern
//! would be cleaner, but for Slice 1A we optimize for shipping the feature.

use rusqlite::params;

use crate::connection::StorageConnection;
use crate::error::StorageError;

use repo_graph_boundary_interaction::{BoundaryInteractionSurface, ChannelDetail};

impl StorageConnection {
    /// Insert boundary interaction surfaces for a snapshot.
    ///
    /// Uses INSERT OR IGNORE to handle re-indexing idempotently.
    /// Surfaces are keyed by `surface_uid` which is deterministic.
    ///
    /// **Note:** For production use during refresh, prefer `insert_boundary_surfaces_and_channels`
    /// which generates fresh UUIDs to avoid conflicts with surfaces from previous snapshots.
    /// This function is kept for backward compatibility and testing.
    pub fn insert_boundary_surfaces(
        &mut self,
        surfaces: &[BoundaryInteractionSurface],
    ) -> Result<usize, StorageError> {
        if surfaces.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let mut stmt = conn.prepare(
            "INSERT OR IGNORE INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                transport_class, provenance, confidence_basis,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality,
                symbol_stable_key, source_file, line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        let mut inserted = 0;
        for surface in surfaces {
            let result = stmt.execute(params![
                surface.surface_uid,
                surface.snapshot_uid,
                surface.repo_uid,
                surface.boundary_scope.as_str(),
                surface.channel_kind.as_str(),
                surface.direction.as_str(),
                surface.transport_class.map(|tc| tc.as_str()),
                surface.provenance,
                surface.confidence_basis,
                surface.protocol,
                surface.protocol_family.as_str(),
                surface.interaction_pattern.as_str(),
                surface.endpoint_locality.as_str(),
                surface.symbol_stable_key,
                surface.source_file,
                surface.line_start,
                surface.line_end,
                surface.col_start,
                surface.col_end,
                surface.extractor,
                surface.basis.as_str(),
                surface.confidence,
                surface.evidence_json,
            ])?;
            inserted += result;
        }

        Ok(inserted)
    }

    /// Insert channel details for boundary interaction surfaces.
    ///
    /// Uses INSERT OR IGNORE to handle re-indexing idempotently.
    /// Channels are keyed by `channel_uid` which is deterministic.
    pub fn insert_boundary_channels(
        &mut self,
        channels: &[ChannelDetail],
    ) -> Result<usize, StorageError> {
        if channels.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let mut stmt = conn.prepare(
            "INSERT OR IGNORE INTO boundary_channel_details (
                channel_uid, surface_uid, channel_kind, channel_identity,
                socket_path, tcp_endpoint, udp_endpoint,
                can_id, i2c_address, spi_device, serial_device,
                shm_key, pipe_path, pipe_context, mqueue_name,
                baud_rate, can_extended, frame_format, payload_contract,
                metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        let mut inserted = 0;
        for channel in channels {
            let result = stmt.execute(params![
                channel.channel_uid,
                channel.surface_uid,
                channel.channel_kind.as_str(),
                channel.channel_identity,
                channel.socket_path,
                channel.tcp_endpoint,
                channel.udp_endpoint,
                channel.can_id,
                channel.i2c_address,
                channel.spi_device,
                channel.serial_device,
                channel.shm_key,
                channel.pipe_path,
                channel.pipe_context,
                channel.mqueue_name,
                channel.baud_rate,
                channel.can_extended,
                channel.frame_format,
                channel.payload_contract,
                channel.metadata_json,
            ])?;
            inserted += result;
        }

        Ok(inserted)
    }

    /// Insert boundary surfaces and channels together with proper UID mapping.
    ///
    /// This is the preferred method for inserting boundary data because:
    /// 1. Generates fresh UUIDs for surfaces (allowing same logical surface across snapshots)
    /// 2. Builds mapping from emitter's deterministic UIDs to storage UUIDs
    /// 3. Rewrites channel surface_uid references using that mapping
    ///
    /// Returns (surfaces_inserted, channels_inserted).
    pub fn insert_boundary_surfaces_and_channels(
        &mut self,
        surfaces: &[BoundaryInteractionSurface],
        channels: &[ChannelDetail],
    ) -> Result<(usize, usize), StorageError> {
        if surfaces.is_empty() {
            return Ok((0, 0));
        }

        let conn = self.connection_mut();

        // Build mapping: emitter deterministic UID -> storage UUID
        let mut uid_mapping: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // Insert surfaces with fresh UUIDs
        let mut surf_stmt = conn.prepare(
            "INSERT INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                transport_class, provenance, confidence_basis,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality,
                symbol_stable_key, source_file, line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        let mut surfaces_inserted = 0;
        for surface in surfaces {
            let storage_uid = uuid::Uuid::new_v4().to_string();
            uid_mapping.insert(surface.surface_uid.clone(), storage_uid.clone());

            let result = surf_stmt.execute(params![
                storage_uid,
                surface.snapshot_uid,
                surface.repo_uid,
                surface.boundary_scope.as_str(),
                surface.channel_kind.as_str(),
                surface.direction.as_str(),
                surface.transport_class.map(|tc| tc.as_str()),
                surface.provenance,
                surface.confidence_basis,
                surface.protocol,
                surface.protocol_family.as_str(),
                surface.interaction_pattern.as_str(),
                surface.endpoint_locality.as_str(),
                surface.symbol_stable_key,
                surface.source_file,
                surface.line_start,
                surface.line_end,
                surface.col_start,
                surface.col_end,
                surface.extractor,
                surface.basis.as_str(),
                surface.confidence,
                surface.evidence_json,
            ])?;
            surfaces_inserted += result;
        }
        drop(surf_stmt);

        // Insert channels with remapped surface_uids
        if channels.is_empty() {
            return Ok((surfaces_inserted, 0));
        }

        let mut chan_stmt = conn.prepare(
            "INSERT INTO boundary_channel_details (
                channel_uid, surface_uid, channel_kind, channel_identity,
                socket_path, tcp_endpoint, udp_endpoint,
                can_id, i2c_address, spi_device, serial_device,
                shm_key, pipe_path, pipe_context, mqueue_name,
                baud_rate, can_extended, frame_format, payload_contract,
                metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        let mut channels_inserted = 0;
        for channel in channels {
            // Remap surface_uid to storage UUID
            let storage_surface_uid = uid_mapping
                .get(&channel.surface_uid)
                .cloned()
                .unwrap_or_else(|| channel.surface_uid.clone());

            // Generate fresh channel UID too
            let storage_channel_uid = uuid::Uuid::new_v4().to_string();

            let result = chan_stmt.execute(params![
                storage_channel_uid,
                storage_surface_uid,
                channel.channel_kind.as_str(),
                channel.channel_identity,
                channel.socket_path,
                channel.tcp_endpoint,
                channel.udp_endpoint,
                channel.can_id,
                channel.i2c_address,
                channel.spi_device,
                channel.serial_device,
                channel.shm_key,
                channel.pipe_path,
                channel.pipe_context,
                channel.mqueue_name,
                channel.baud_rate,
                channel.can_extended,
                channel.frame_format,
                channel.payload_contract,
                channel.metadata_json,
            ])?;
            channels_inserted += result;
        }

        Ok((surfaces_inserted, channels_inserted))
    }

    /// Count boundary interaction surfaces for a snapshot.
    pub fn count_boundary_surfaces(&self, snapshot_uid: &str) -> Result<usize, StorageError> {
        let conn = self.connection();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM boundary_interaction_surfaces WHERE snapshot_uid = ?",
            [snapshot_uid],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Count channel details for a snapshot (via surface join).
    pub fn count_boundary_channels(&self, snapshot_uid: &str) -> Result<usize, StorageError> {
        let conn = self.connection();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM boundary_channel_details bcd
             JOIN boundary_interaction_surfaces bis ON bcd.surface_uid = bis.surface_uid
             WHERE bis.snapshot_uid = ?",
            [snapshot_uid],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Delete ALL boundary interaction facts for a snapshot (every extractor).
    ///
    /// Channel details are deleted via CASCADE from surface deletion.
    ///
    /// **Per-postpass failure isolation must NOT use this** — the C and TS boundary
    /// postpasses both write this table under one snapshot, so a snapshot-wide delete
    /// would over-delete the sibling postpass's already-committed facts. Use
    /// [`Self::delete_boundary_facts_by_extractor`] there.
    pub fn delete_boundary_facts(&mut self, snapshot_uid: &str) -> Result<usize, StorageError> {
        let conn = self.connection_mut();
        let deleted = conn.execute(
            "DELETE FROM boundary_interaction_surfaces WHERE snapshot_uid = ?",
            [snapshot_uid],
        )?;
        Ok(deleted)
    }

    /// Delete a snapshot's boundary interaction facts that were emitted by ONE
    /// `extractor` (e.g. `c-ipc:0.1.0` or `ts-worker:0.1.0`), leaving other
    /// extractors' facts for the same snapshot intact.
    ///
    /// PERSIST-RECURSION-1 (failure isolation, review-2 item 2): the C and TS
    /// boundary postpasses BOTH write `boundary_interaction_surfaces` for the same
    /// snapshot, distinguished only by `extractor`. When one postpass fails, its
    /// compensating cleanup must remove ONLY its own (possibly partial) facts — the
    /// snapshot-wide [`Self::delete_boundary_facts`] would also erase the sibling
    /// postpass's already-committed, correct facts (over-deletion) and misreport them
    /// as measured-absent. So per-postpass isolation MUST scope by extractor.
    ///
    /// Channels are deleted EXPLICITLY via the surface join (not left to the
    /// `ON DELETE CASCADE` from `boundary_interaction_surfaces`) so the cleanup is
    /// correct regardless of the connection's `PRAGMA foreign_keys` state; both
    /// deletes run in ONE transaction. Returns the total rows deleted across the two
    /// tables.
    pub fn delete_boundary_facts_by_extractor(
        &mut self,
        snapshot_uid: &str,
        extractor: &str,
    ) -> Result<usize, StorageError> {
        let tx = self.connection_mut().transaction()?;
        let mut deleted = 0;
        // Child rows first: channels of THIS extractor's surfaces.
        deleted += tx.execute(
            "DELETE FROM boundary_channel_details
             WHERE surface_uid IN (
                 SELECT surface_uid FROM boundary_interaction_surfaces
                 WHERE snapshot_uid = ?1 AND extractor = ?2
             )",
            params![snapshot_uid, extractor],
        )?;
        deleted += tx.execute(
            "DELETE FROM boundary_interaction_surfaces
             WHERE snapshot_uid = ?1 AND extractor = ?2",
            params![snapshot_uid, extractor],
        )?;
        tx.commit()?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_boundary_interaction::surface::SurfaceBuilder;
    use repo_graph_boundary_interaction::{
        BoundaryScope, ChannelKind, Direction, EndpointLocality, InteractionBasis,
        InteractionPattern,
    };

    fn create_test_db() -> StorageConnection {
        let mut conn = StorageConnection::open_in_memory().unwrap();

        conn.connection_mut()
            .execute_batch(
                "INSERT INTO repos (repo_uid, name, root_path, created_at)
                 VALUES ('test-repo', 'Test', '/tmp/test', datetime('now'));
                 INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at)
                 VALUES ('snap-1', 'test-repo', 'full', 'ready', datetime('now'));",
            )
            .unwrap();

        conn
    }

    fn test_surface() -> BoundaryInteractionSurface {
        SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Provider)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("test-repo:src/server.c#start:SYMBOL:function")
            .source_file("src/server.c")
            .location(100, 105, 5, 50)
            .extractor("c-ipc:0.1.0")
            .basis(InteractionBasis::ApiCall)
            .build()
            .unwrap()
    }

    #[test]
    fn insert_and_count_surfaces() {
        let mut conn = create_test_db();

        let surface = test_surface();
        let count = conn.insert_boundary_surfaces(&[surface]).unwrap();
        assert_eq!(count, 1);

        let stored = conn.count_boundary_surfaces("snap-1").unwrap();
        assert_eq!(stored, 1);
    }

    #[test]
    fn insert_is_idempotent() {
        let mut conn = create_test_db();

        let surface = test_surface();

        // Insert twice
        conn.insert_boundary_surfaces(std::slice::from_ref(&surface))
            .unwrap();
        conn.insert_boundary_surfaces(&[surface]).unwrap();

        // Should only have one
        let stored = conn.count_boundary_surfaces("snap-1").unwrap();
        assert_eq!(stored, 1);
    }

    #[test]
    fn insert_channel_details() {
        let mut conn = create_test_db();

        let surface = test_surface();
        conn.insert_boundary_surfaces(std::slice::from_ref(&surface))
            .unwrap();

        let channel = ChannelDetail {
            channel_uid: ChannelDetail::build_uid(&surface.surface_uid, "/var/run/app.sock"),
            surface_uid: surface.surface_uid,
            channel_kind: ChannelKind::UnixSocket,
            channel_identity: "/var/run/app.sock".to_string(),
            socket_path: Some("/var/run/app.sock".to_string()),
            tcp_endpoint: None,
            udp_endpoint: None,
            can_id: None,
            i2c_address: None,
            spi_device: None,
            serial_device: None,
            shm_key: None,
            pipe_path: None,
            pipe_context: None,
            mqueue_name: None,
            baud_rate: None,
            can_extended: None,
            frame_format: None,
            payload_contract: None,
            metadata_json: None,
        };

        let count = conn.insert_boundary_channels(&[channel]).unwrap();
        assert_eq!(count, 1);

        let stored = conn.count_boundary_channels("snap-1").unwrap();
        assert_eq!(stored, 1);
    }

    #[test]
    fn delete_cascades_to_channels() {
        let mut conn = create_test_db();

        let surface = test_surface();
        conn.insert_boundary_surfaces(std::slice::from_ref(&surface))
            .unwrap();

        let channel = ChannelDetail {
            channel_uid: ChannelDetail::build_uid(&surface.surface_uid, "/tmp/pipe"),
            surface_uid: surface.surface_uid,
            channel_kind: ChannelKind::NamedPipe,
            channel_identity: "/tmp/pipe".to_string(),
            socket_path: None,
            tcp_endpoint: None,
            udp_endpoint: None,
            can_id: None,
            i2c_address: None,
            spi_device: None,
            serial_device: None,
            shm_key: None,
            pipe_path: Some("/tmp/pipe".to_string()),
            pipe_context: None,
            mqueue_name: None,
            baud_rate: None,
            can_extended: None,
            frame_format: None,
            payload_contract: None,
            metadata_json: None,
        };
        conn.insert_boundary_channels(&[channel]).unwrap();

        // Delete surfaces (should cascade to channels)
        conn.delete_boundary_facts("snap-1").unwrap();

        assert_eq!(conn.count_boundary_surfaces("snap-1").unwrap(), 0);
        assert_eq!(conn.count_boundary_channels("snap-1").unwrap(), 0);
    }

    /// PERSIST-RECURSION-1 (review-2 item 2): per-postpass failure isolation must
    /// delete ONLY the failing postpass's own boundary facts. The C (`c-ipc`) and TS
    /// (`ts-worker`) boundary postpasses share this table under one snapshot, keyed by
    /// `extractor`, so an extractor-scoped delete must remove the target extractor's
    /// surfaces AND their channels while leaving the sibling extractor's facts intact.
    #[test]
    fn delete_boundary_facts_by_extractor_scopes_to_extractor_and_cascades() {
        let mut conn = create_test_db();

        // Build a channel for a surface (references the builder surface_uid so
        // `insert_boundary_surfaces_and_channels` remaps it to the fresh UUID).
        fn channel_for(surface_uid: &str, identity: &str) -> ChannelDetail {
            ChannelDetail {
                channel_uid: ChannelDetail::build_uid(surface_uid, identity),
                surface_uid: surface_uid.to_string(),
                channel_kind: ChannelKind::UnixSocket,
                channel_identity: identity.to_string(),
                socket_path: Some(identity.to_string()),
                tcp_endpoint: None,
                udp_endpoint: None,
                can_id: None,
                i2c_address: None,
                spi_device: None,
                serial_device: None,
                shm_key: None,
                pipe_path: None,
                pipe_context: None,
                mqueue_name: None,
                baud_rate: None,
                can_extended: None,
                frame_format: None,
                payload_contract: None,
                metadata_json: None,
            }
        }

        // One surface + channel per boundary postpass, distinguished by extractor.
        let c_surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Provider)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("test-repo:src/server.c#start:SYMBOL:function")
            .source_file("src/server.c")
            .location(100, 105, 5, 50)
            .extractor("c-ipc:0.1.0")
            .basis(InteractionBasis::ApiCall)
            .build()
            .unwrap();
        let ts_surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Consumer)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("test-repo:src/worker.ts#run:SYMBOL:function")
            .source_file("src/worker.ts")
            .location(10, 12, 1, 20)
            .extractor("ts-worker:0.1.0")
            .basis(InteractionBasis::ApiCall)
            .build()
            .unwrap();

        let c_channel = channel_for(&c_surface.surface_uid, "/var/run/c.sock");
        let ts_channel = channel_for(&ts_surface.surface_uid, "/var/run/ts.sock");
        conn.insert_boundary_surfaces_and_channels(
            std::slice::from_ref(&c_surface),
            std::slice::from_ref(&c_channel),
        )
        .unwrap();
        conn.insert_boundary_surfaces_and_channels(
            std::slice::from_ref(&ts_surface),
            std::slice::from_ref(&ts_channel),
        )
        .unwrap();

        assert_eq!(conn.count_boundary_surfaces("snap-1").unwrap(), 2);
        assert_eq!(conn.count_boundary_channels("snap-1").unwrap(), 2);

        let count_ext = |c: &StorageConnection, ext: &str| -> i64 {
            c.connection()
                .query_row(
                    "SELECT COUNT(*) FROM boundary_interaction_surfaces
                     WHERE snapshot_uid = 'snap-1' AND extractor = ?",
                    [ext],
                    |r| r.get(0),
                )
                .unwrap()
        };

        // Isolate the TS postpass's facts only.
        let deleted = conn
            .delete_boundary_facts_by_extractor("snap-1", "ts-worker:0.1.0")
            .unwrap();
        assert_eq!(deleted, 2, "one TS surface + its one channel deleted");

        // The sibling C facts survive; the TS facts (surface + channel) are gone.
        assert_eq!(
            count_ext(&conn, "c-ipc:0.1.0"),
            1,
            "sibling C facts preserved"
        );
        assert_eq!(count_ext(&conn, "ts-worker:0.1.0"), 0, "TS facts removed");
        assert_eq!(conn.count_boundary_surfaces("snap-1").unwrap(), 1);
        assert_eq!(
            conn.count_boundary_channels("snap-1").unwrap(),
            1,
            "only the C channel remains (TS channel cascaded away)"
        );
    }

    #[test]
    fn empty_insert_returns_zero() {
        let mut conn = create_test_db();

        let count = conn.insert_boundary_surfaces(&[]).unwrap();
        assert_eq!(count, 0);

        let count = conn.insert_boundary_channels(&[]).unwrap();
        assert_eq!(count, 0);
    }
}
