//! Boundary interaction read port implementation for `StorageConnection`.
//!
//! Implements `BoundaryInteractionReadPort` from the boundary-interaction crate.
//! This is the read-side counterpart to boundary_interaction_impl.rs (write-side).
//!
//! ## Determinism
//!
//! All queries are sorted by (source_file, line_start, col_start) to ensure
//! deterministic output per the project's "same input → same output" rule.

use rusqlite::OptionalExtension;

use crate::connection::StorageConnection;

use repo_graph_boundary_interaction::{
    BasisCount, BoundaryContractView, BoundaryInteractionChannelView, BoundaryInteractionDetail,
    BoundaryInteractionFilter, BoundaryInteractionListItem, BoundaryInteractionReadError,
    BoundaryInteractionReadPort, BoundaryInteractionSummary, BoundaryScope, ChannelKind,
    Direction, DirectionCount, EndpointLocality, FamilyCount, InteractionBasis,
    InteractionPattern, KindCount, ProtocolFamily, ScopeCount, TransportClass,
};

impl BoundaryInteractionReadPort for StorageConnection {
    fn list_boundary_interactions(
        &self,
        snapshot_uid: &str,
        filter: &BoundaryInteractionFilter,
    ) -> Result<Vec<BoundaryInteractionListItem>, BoundaryInteractionReadError> {
        // Build dynamic WHERE clause based on filter.
        let mut conditions = vec!["bis.snapshot_uid = ?1".to_string()];
        let mut param_index = 2;

        // Channel kind filter
        if filter.channel_kind.is_some() {
            conditions.push(format!("bis.channel_kind = ?{}", param_index));
            param_index += 1;
        }

        // Boundary scope filter
        if filter.boundary_scope.is_some() {
            conditions.push(format!("bis.boundary_scope = ?{}", param_index));
            param_index += 1;
        }

        // Direction filter
        if filter.direction.is_some() {
            conditions.push(format!("bis.direction = ?{}", param_index));
            param_index += 1;
        }

        // Protocol family filter
        if filter.protocol_family.is_some() {
            conditions.push(format!("bis.protocol_family = ?{}", param_index));
            param_index += 1;
        }

        // File filter (exact match)
        if filter.file.is_some() {
            conditions.push(format!("bis.source_file = ?{}", param_index));
            param_index += 1;
        }

        // File prefix filter
        if filter.file_prefix.is_some() {
            conditions.push(format!("bis.source_file LIKE ?{}", param_index));
            param_index += 1;
        }

        // Symbol filter
        if filter.symbol.is_some() {
            conditions.push(format!("bis.symbol_stable_key = ?{}", param_index));
            param_index += 1;
        }

        // Min confidence filter
        if filter.min_confidence.is_some() {
            conditions.push(format!("bis.confidence >= ?{}", param_index));
            // param_index += 1; // Not needed, last parameter
        }

        let where_clause = conditions.join(" AND ");

        // Query with channel count subquery and contract association LEFT JOIN.
        // For list view, we pick the first contract (by MIN association_uid) for orientation.
        // Both contract_name and contract_kind come from the same row to avoid
        // synthesizing an impossible pair when multiple associations exist.
        // Full contract list is available in detail view.
        let sql = format!(
            "SELECT
                bis.surface_uid,
                bis.source_file,
                bis.line_start,
                bis.line_end,
                bis.channel_kind,
                bis.boundary_scope,
                bis.direction,
                bis.transport_class,
                bis.provenance,
                bis.confidence_basis,
                bis.protocol_family,
                bis.protocol,
                bis.interaction_pattern,
                bis.symbol_stable_key,
                bis.confidence,
                bis.basis,
                (SELECT COUNT(*) FROM boundary_channel_details WHERE surface_uid = bis.surface_uid) as channel_count,
                bc_first.contract_name,
                bc_first.contract_kind
            FROM boundary_interaction_surfaces bis
            LEFT JOIN (
                SELECT
                    bc.surface_uid,
                    ce.full_name as contract_name,
                    bc.contract_kind
                FROM boundary_contracts bc
                LEFT JOIN contract_elements ce ON bc.contract_element_uid = ce.element_uid
                WHERE bc.association_uid = (
                    SELECT MIN(bc2.association_uid)
                    FROM boundary_contracts bc2
                    WHERE bc2.surface_uid = bc.surface_uid
                )
            ) bc_first ON bc_first.surface_uid = bis.surface_uid
            WHERE {}
            ORDER BY bis.source_file ASC, bis.line_start ASC, bis.col_start ASC",
            where_clause
        );

        let conn = self.connection();
        let mut stmt = conn.prepare(&sql).map_err(map_storage_error)?;

        // Bind parameters dynamically
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(snapshot_uid.to_string())];

        if let Some(ref kind) = filter.channel_kind {
            params_vec.push(Box::new(kind.as_str().to_string()));
        }
        if let Some(ref scope) = filter.boundary_scope {
            params_vec.push(Box::new(scope.as_str().to_string()));
        }
        if let Some(ref dir) = filter.direction {
            params_vec.push(Box::new(dir.as_str().to_string()));
        }
        if let Some(ref family) = filter.protocol_family {
            params_vec.push(Box::new(family.as_str().to_string()));
        }
        if let Some(ref file) = filter.file {
            params_vec.push(Box::new(file.clone()));
        }
        if let Some(ref prefix) = filter.file_prefix {
            params_vec.push(Box::new(format!("{}%", prefix)));
        }
        if let Some(ref symbol) = filter.symbol {
            params_vec.push(Box::new(symbol.clone()));
        }
        if let Some(min_conf) = filter.min_confidence {
            params_vec.push(Box::new(min_conf));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        // Query into raw rows first, then parse enum values outside the closure
        // to properly propagate parse errors.
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(RawListRow {
                    surface_uid: row.get(0)?,
                    source_file: row.get(1)?,
                    line_start: row.get(2)?,
                    line_end: row.get(3)?,
                    channel_kind: row.get(4)?,
                    boundary_scope: row.get(5)?,
                    direction: row.get(6)?,
                    transport_class: row.get(7)?,
                    provenance: row.get(8)?,
                    confidence_basis: row.get(9)?,
                    protocol_family: row.get(10)?,
                    protocol: row.get(11)?,
                    interaction_pattern: row.get(12)?,
                    symbol_stable_key: row.get(13)?,
                    confidence: row.get(14)?,
                    basis: row.get(15)?,
                    channel_count: row.get(16)?,
                    contract_name: row.get(17)?,
                    contract_kind: row.get(18)?,
                })
            })
            .map_err(map_storage_error)?;

        let mut results = Vec::new();
        for row_result in rows {
            let raw = row_result.map_err(map_storage_error)?;
            results.push(BoundaryInteractionListItem {
                surface_uid: raw.surface_uid,
                source_file: raw.source_file,
                line_start: raw.line_start,
                line_end: raw.line_end,
                channel_kind: parse_channel_kind(&raw.channel_kind)?,
                boundary_scope: parse_boundary_scope(&raw.boundary_scope)?,
                direction: parse_direction(&raw.direction)?,
                transport_class: raw
                    .transport_class
                    .as_deref()
                    .map(parse_transport_class)
                    .transpose()?,
                provenance: raw.provenance,
                confidence_basis: raw.confidence_basis,
                protocol_family: parse_protocol_family(&raw.protocol_family)?,
                protocol: raw.protocol,
                interaction_pattern: parse_interaction_pattern(&raw.interaction_pattern)?,
                symbol_stable_key: raw.symbol_stable_key,
                confidence: raw.confidence,
                basis: parse_interaction_basis(&raw.basis)?,
                channel_count: raw.channel_count,
                contract_name: raw.contract_name,
                contract_kind: raw.contract_kind,
            });
        }

        Ok(results)
    }

    fn get_boundary_interaction_detail(
        &self,
        surface_uid: &str,
    ) -> Result<Option<BoundaryInteractionDetail>, BoundaryInteractionReadError> {
        let conn = self.connection();

        // Query surface
        let surface_opt: Option<SurfaceRow> = conn
            .query_row(
                "SELECT
                    surface_uid, snapshot_uid, repo_uid,
                    boundary_scope, channel_kind, direction,
                    transport_class, provenance, confidence_basis,
                    protocol, protocol_family, interaction_pattern,
                    endpoint_locality, symbol_stable_key, source_file,
                    line_start, line_end, col_start, col_end,
                    extractor, basis, confidence, evidence_json
                FROM boundary_interaction_surfaces
                WHERE surface_uid = ?",
                [surface_uid],
                |row| {
                    Ok(SurfaceRow {
                        surface_uid: row.get(0)?,
                        snapshot_uid: row.get(1)?,
                        repo_uid: row.get(2)?,
                        boundary_scope: row.get(3)?,
                        channel_kind: row.get(4)?,
                        direction: row.get(5)?,
                        transport_class: row.get(6)?,
                        provenance: row.get(7)?,
                        confidence_basis: row.get(8)?,
                        protocol: row.get(9)?,
                        protocol_family: row.get(10)?,
                        interaction_pattern: row.get(11)?,
                        endpoint_locality: row.get(12)?,
                        symbol_stable_key: row.get(13)?,
                        source_file: row.get(14)?,
                        line_start: row.get(15)?,
                        line_end: row.get(16)?,
                        col_start: row.get(17)?,
                        col_end: row.get(18)?,
                        extractor: row.get(19)?,
                        basis: row.get(20)?,
                        confidence: row.get(21)?,
                        evidence_json: row.get(22)?,
                    })
                },
            )
            .optional()
            .map_err(map_storage_error)?;

        let surface = match surface_opt {
            Some(s) => s,
            None => return Ok(None),
        };

        // Query channels into raw rows, then parse enum values outside closure
        let mut stmt = conn
            .prepare(
                "SELECT
                    channel_uid, channel_kind, channel_identity,
                    socket_path, tcp_endpoint, udp_endpoint,
                    can_id, i2c_address, spi_device, serial_device,
                    shm_key, pipe_path, pipe_context, mqueue_name,
                    baud_rate, can_extended, frame_format, payload_contract,
                    metadata_json
                FROM boundary_channel_details
                WHERE surface_uid = ?
                ORDER BY channel_identity ASC",
            )
            .map_err(map_storage_error)?;

        let channel_rows = stmt
            .query_map([surface_uid], |row| {
                Ok(RawChannelRow {
                    channel_uid: row.get(0)?,
                    channel_kind: row.get(1)?,
                    channel_identity: row.get(2)?,
                    socket_path: row.get(3)?,
                    tcp_endpoint: row.get(4)?,
                    udp_endpoint: row.get(5)?,
                    can_id: row.get(6)?,
                    i2c_address: row.get(7)?,
                    spi_device: row.get(8)?,
                    serial_device: row.get(9)?,
                    shm_key: row.get(10)?,
                    pipe_path: row.get(11)?,
                    pipe_context: row.get(12)?,
                    mqueue_name: row.get(13)?,
                    baud_rate: row.get(14)?,
                    can_extended: row.get(15)?,
                    frame_format: row.get(16)?,
                    payload_contract: row.get(17)?,
                    metadata_json: row.get(18)?,
                })
            })
            .map_err(map_storage_error)?;

        let mut channels = Vec::new();
        for row_result in channel_rows {
            let raw = row_result.map_err(map_storage_error)?;
            channels.push(BoundaryInteractionChannelView {
                channel_uid: raw.channel_uid,
                channel_kind: parse_channel_kind(&raw.channel_kind)?,
                channel_identity: raw.channel_identity,
                socket_path: raw.socket_path,
                tcp_endpoint: raw.tcp_endpoint,
                udp_endpoint: raw.udp_endpoint,
                can_id: raw.can_id,
                i2c_address: raw.i2c_address,
                spi_device: raw.spi_device,
                serial_device: raw.serial_device,
                shm_key: raw.shm_key,
                pipe_path: raw.pipe_path,
                pipe_context: raw.pipe_context,
                mqueue_name: raw.mqueue_name,
                baud_rate: raw.baud_rate,
                can_extended: raw.can_extended,
                frame_format: raw.frame_format,
                payload_contract: raw.payload_contract,
                metadata_json: raw.metadata_json,
            });
        }

        // Query contract associations (Track B: schema-backed RPC)
        let mut contract_stmt = conn
            .prepare(
                "SELECT
                    bc.association_uid,
                    bc.contract_element_uid,
                    bc.contract_kind,
                    ce.full_name as contract_name,
                    bc.association_basis,
                    bc.confidence,
                    bc.evidence_json
                FROM boundary_contracts bc
                LEFT JOIN contract_elements ce ON bc.contract_element_uid = ce.element_uid
                WHERE bc.surface_uid = ?
                ORDER BY bc.association_uid ASC",
            )
            .map_err(map_storage_error)?;

        let contract_rows = contract_stmt
            .query_map([surface_uid], |row| {
                Ok(RawContractRow {
                    association_uid: row.get(0)?,
                    contract_element_uid: row.get(1)?,
                    contract_kind: row.get(2)?,
                    contract_name: row.get(3)?,
                    association_basis: row.get(4)?,
                    confidence: row.get(5)?,
                    evidence_json: row.get(6)?,
                })
            })
            .map_err(map_storage_error)?;

        let mut contracts = Vec::new();
        for row_result in contract_rows {
            let raw = row_result.map_err(map_storage_error)?;
            contracts.push(BoundaryContractView {
                association_uid: raw.association_uid,
                contract_element_uid: raw.contract_element_uid,
                contract_kind: raw.contract_kind,
                contract_name: raw.contract_name,
                association_basis: raw.association_basis,
                confidence: raw.confidence,
                evidence_json: raw.evidence_json,
            });
        }

        Ok(Some(BoundaryInteractionDetail {
            surface_uid: surface.surface_uid,
            snapshot_uid: surface.snapshot_uid,
            repo_uid: surface.repo_uid,
            boundary_scope: parse_boundary_scope(&surface.boundary_scope)?,
            channel_kind: parse_channel_kind(&surface.channel_kind)?,
            direction: parse_direction(&surface.direction)?,
            transport_class: surface
                .transport_class
                .as_deref()
                .map(parse_transport_class)
                .transpose()?,
            provenance: surface.provenance,
            confidence_basis: surface.confidence_basis,
            protocol: surface.protocol,
            protocol_family: parse_protocol_family(&surface.protocol_family)?,
            interaction_pattern: parse_interaction_pattern(&surface.interaction_pattern)?,
            endpoint_locality: parse_endpoint_locality(&surface.endpoint_locality)?,
            symbol_stable_key: surface.symbol_stable_key,
            source_file: surface.source_file,
            line_start: surface.line_start,
            line_end: surface.line_end,
            col_start: surface.col_start,
            col_end: surface.col_end,
            extractor: surface.extractor,
            basis: parse_interaction_basis(&surface.basis)?,
            confidence: surface.confidence,
            evidence_json: surface.evidence_json,
            channels,
            contracts,
        }))
    }

    fn get_boundary_interaction_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<BoundaryInteractionSummary, BoundaryInteractionReadError> {
        let conn = self.connection();

        // Total surfaces
        let total_surfaces: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM boundary_interaction_surfaces WHERE snapshot_uid = ?",
                [snapshot_uid],
                |row| row.get(0),
            )
            .map_err(map_storage_error)?;

        // Total channels
        let total_channels: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM boundary_channel_details bcd
                 JOIN boundary_interaction_surfaces bis ON bcd.surface_uid = bis.surface_uid
                 WHERE bis.snapshot_uid = ?",
                [snapshot_uid],
                |row| row.get(0),
            )
            .map_err(map_storage_error)?;

        // By channel kind
        let by_channel_kind = query_count_by(
            conn,
            "channel_kind",
            snapshot_uid,
            parse_channel_kind,
            |kind, count| KindCount {
                channel_kind: kind,
                count,
            },
        )?;

        // By boundary scope
        let by_boundary_scope = query_count_by(
            conn,
            "boundary_scope",
            snapshot_uid,
            parse_boundary_scope,
            |scope, count| ScopeCount {
                boundary_scope: scope,
                count,
            },
        )?;

        // By direction
        let by_direction = query_count_by(
            conn,
            "direction",
            snapshot_uid,
            parse_direction,
            |dir, count| DirectionCount {
                direction: dir,
                count,
            },
        )?;

        // By protocol family
        let by_protocol_family = query_count_by(
            conn,
            "protocol_family",
            snapshot_uid,
            parse_protocol_family,
            |family, count| FamilyCount {
                protocol_family: family,
                count,
            },
        )?;

        // By basis
        let by_basis = query_count_by(
            conn,
            "basis",
            snapshot_uid,
            parse_interaction_basis,
            |basis, count| BasisCount { basis, count },
        )?;

        // Files with boundaries (distinct, sorted)
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT source_file
                 FROM boundary_interaction_surfaces
                 WHERE snapshot_uid = ?
                 ORDER BY source_file ASC",
            )
            .map_err(map_storage_error)?;

        let file_rows = stmt
            .query_map([snapshot_uid], |row| row.get::<_, String>(0))
            .map_err(map_storage_error)?;

        let mut files_with_boundaries = Vec::new();
        for row_result in file_rows {
            files_with_boundaries.push(row_result.map_err(map_storage_error)?);
        }

        Ok(BoundaryInteractionSummary {
            total_surfaces: total_surfaces as usize,
            total_channels: total_channels as usize,
            by_channel_kind,
            by_boundary_scope,
            by_direction,
            by_protocol_family,
            by_basis,
            files_with_boundaries,
        })
    }
}

// ── Helper types ─────────────────────────────────────────────────────

/// Internal raw row type for list queries.
/// Holds string values before enum parsing so errors can be propagated.
struct RawListRow {
    surface_uid: String,
    source_file: String,
    line_start: u32,
    line_end: u32,
    channel_kind: String,
    boundary_scope: String,
    direction: String,
    transport_class: Option<String>,
    provenance: Option<String>,
    confidence_basis: Option<String>,
    protocol_family: String,
    protocol: String,
    interaction_pattern: String,
    symbol_stable_key: String,
    confidence: f64,
    basis: String,
    channel_count: u32,
    // Contract orientation (Track B)
    contract_name: Option<String>,
    contract_kind: Option<String>,
}

/// Internal row type for surface queries.
struct SurfaceRow {
    surface_uid: String,
    snapshot_uid: String,
    repo_uid: String,
    boundary_scope: String,
    channel_kind: String,
    direction: String,
    transport_class: Option<String>,
    provenance: Option<String>,
    confidence_basis: Option<String>,
    protocol: String,
    protocol_family: String,
    interaction_pattern: String,
    endpoint_locality: String,
    symbol_stable_key: String,
    source_file: String,
    line_start: u32,
    line_end: u32,
    col_start: u32,
    col_end: u32,
    extractor: String,
    basis: String,
    confidence: f64,
    evidence_json: String,
}

/// Internal raw row type for channel queries.
struct RawChannelRow {
    channel_uid: String,
    channel_kind: String,
    channel_identity: String,
    socket_path: Option<String>,
    tcp_endpoint: Option<String>,
    udp_endpoint: Option<String>,
    can_id: Option<u32>,
    i2c_address: Option<u8>,
    spi_device: Option<String>,
    serial_device: Option<String>,
    shm_key: Option<String>,
    pipe_path: Option<String>,
    pipe_context: Option<String>,
    mqueue_name: Option<String>,
    baud_rate: Option<u32>,
    can_extended: Option<bool>,
    frame_format: Option<String>,
    payload_contract: Option<String>,
    metadata_json: Option<String>,
}

/// Internal raw row type for contract association queries (Track B).
struct RawContractRow {
    association_uid: String,
    contract_element_uid: Option<String>,
    contract_kind: String,
    contract_name: Option<String>,
    association_basis: String,
    confidence: f64,
    evidence_json: Option<String>,
}

// ── Parse helpers ────────────────────────────────────────────────────
//
// All parse helpers return Result to surface schema drift or corrupt rows
// instead of silently coercing to fallback values. This follows the
// explicit-degradation rule: `null` = unknown, empty = known-zero, but
// unrecognized stored values = read error.

fn parse_channel_kind(s: &str) -> Result<ChannelKind, BoundaryInteractionReadError> {
    match s {
        "unix_socket" => Ok(ChannelKind::UnixSocket),
        "named_pipe" => Ok(ChannelKind::NamedPipe),
        "anonymous_pipe" => Ok(ChannelKind::AnonymousPipe),
        "shared_memory" => Ok(ChannelKind::SharedMemory),
        "message_queue" => Ok(ChannelKind::MessageQueue),
        "semaphore" => Ok(ChannelKind::Semaphore),
        "process_signal" => Ok(ChannelKind::ProcessSignal),
        "tcp_socket" => Ok(ChannelKind::TcpSocket),
        "udp_socket" => Ok(ChannelKind::UdpSocket),
        "shared_array_buffer" => Ok(ChannelKind::SharedArrayBuffer),
        "amqp_queue" => Ok(ChannelKind::AmqpQueue),
        "kafka_topic" => Ok(ChannelKind::KafkaTopic),
        "nats_subject" => Ok(ChannelKind::NatsSubject),
        "grpc_channel" => Ok(ChannelKind::GrpcChannel),
        "protobuf_stream" => Ok(ChannelKind::ProtobufStream),
        "erpc_channel" => Ok(ChannelKind::ErpcChannel),
        "serial_port" => Ok(ChannelKind::SerialPort),
        "can_message" => Ok(ChannelKind::CanMessage),
        "inter_core_channel" => Ok(ChannelKind::InterCoreChannel),
        "mqtt_topic" => Ok(ChannelKind::MqttTopic),
        "dbus_interface" => Ok(ChannelKind::DbusInterface),
        "zeromq_socket" => Ok(ChannelKind::ZeromqSocket),
        "i2c_device" => Ok(ChannelKind::I2cDevice),
        "spi_device" => Ok(ChannelKind::SpiDevice),
        "usb_endpoint" => Ok(ChannelKind::UsbEndpoint),
        "ble_characteristic" => Ok(ChannelKind::BleCharacteristic),
        "modbus_register" => Ok(ChannelKind::ModbusRegister),
        "custom_protocol" => Ok(ChannelKind::CustomProtocol),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized channel_kind: '{}'",
            other
        ))),
    }
}

fn parse_boundary_scope(s: &str) -> Result<BoundaryScope, BoundaryInteractionReadError> {
    match s {
        "intra_process" => Ok(BoundaryScope::IntraProcess),
        "inter_process" => Ok(BoundaryScope::InterProcess),
        "inter_device" => Ok(BoundaryScope::InterDevice),
        "unknown" => Ok(BoundaryScope::Unknown),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized boundary_scope: '{}'",
            other
        ))),
    }
}

fn parse_direction(s: &str) -> Result<Direction, BoundaryInteractionReadError> {
    match s {
        "provider" => Ok(Direction::Provider),
        "consumer" => Ok(Direction::Consumer),
        "bidirectional" => Ok(Direction::Bidirectional),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized direction: '{}'",
            other
        ))),
    }
}

fn parse_protocol_family(s: &str) -> Result<ProtocolFamily, BoundaryInteractionReadError> {
    match s {
        "socket" => Ok(ProtocolFamily::Socket),
        "pipe" => Ok(ProtocolFamily::Pipe),
        "shared_memory" => Ok(ProtocolFamily::SharedMemory),
        "message_queue" => Ok(ProtocolFamily::MessageQueue),
        "signal" => Ok(ProtocolFamily::Signal),
        "semaphore" => Ok(ProtocolFamily::Semaphore),
        "inter_core" => Ok(ProtocolFamily::InterCore),
        "rpc" => Ok(ProtocolFamily::Rpc),
        "serial" => Ok(ProtocolFamily::Serial),
        "bus" => Ok(ProtocolFamily::Bus),
        "message_broker" => Ok(ProtocolFamily::MessageBroker),
        "usb" => Ok(ProtocolFamily::Usb),
        "bluetooth" => Ok(ProtocolFamily::Bluetooth),
        "custom" => Ok(ProtocolFamily::Custom),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized protocol_family: '{}'",
            other
        ))),
    }
}

fn parse_interaction_pattern(s: &str) -> Result<InteractionPattern, BoundaryInteractionReadError> {
    match s {
        "request_response" => Ok(InteractionPattern::RequestResponse),
        "publish_subscribe" => Ok(InteractionPattern::PublishSubscribe),
        "stream" => Ok(InteractionPattern::Stream),
        "datagram" => Ok(InteractionPattern::Datagram),
        "fire_and_forget" => Ok(InteractionPattern::FireAndForget),
        "shared_state" => Ok(InteractionPattern::SharedState),
        "synchronization" => Ok(InteractionPattern::Synchronization),
        "message_passing" => Ok(InteractionPattern::MessagePassing),
        "unknown" => Ok(InteractionPattern::Unknown),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized interaction_pattern: '{}'",
            other
        ))),
    }
}

fn parse_endpoint_locality(s: &str) -> Result<EndpointLocality, BoundaryInteractionReadError> {
    match s {
        "loopback" => Ok(EndpointLocality::Loopback),
        "same_host_named" => Ok(EndpointLocality::SameHostNamed),
        "remote_literal" => Ok(EndpointLocality::RemoteLiteral),
        "unknown" => Ok(EndpointLocality::Unknown),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized endpoint_locality: '{}'",
            other
        ))),
    }
}

fn parse_interaction_basis(s: &str) -> Result<InteractionBasis, BoundaryInteractionReadError> {
    match s {
        "api_call" => Ok(InteractionBasis::ApiCall),
        "wrapper_call" => Ok(InteractionBasis::WrapperCall),
        "annotation" => Ok(InteractionBasis::Annotation),
        "convention" => Ok(InteractionBasis::Convention),
        "declaration" => Ok(InteractionBasis::Declaration),
        "inferred" => Ok(InteractionBasis::Inferred),
        "extends_impl_base" => Ok(InteractionBasis::ExtendsImplBase),
        // GR-1B: boosted basis when registration proof found
        "extends_impl_base_registered" => Ok(InteractionBasis::ExtendsImplBase),
        // GR-2A: gRPC client stub creation
        "stub_creation" => Ok(InteractionBasis::StubCreation),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized interaction_basis: '{}'",
            other
        ))),
    }
}

fn parse_transport_class(s: &str) -> Result<TransportClass, BoundaryInteractionReadError> {
    match s {
        "raw_socket" => Ok(TransportClass::RawSocket),
        "raw_ipc" => Ok(TransportClass::RawIpc),
        "schema_rpc" => Ok(TransportClass::SchemaRpc),
        "message_broker" => Ok(TransportClass::MessageBroker),
        "custom_protocol" => Ok(TransportClass::CustomProtocol),
        other => Err(BoundaryInteractionReadError::Storage(format!(
            "unrecognized transport_class: '{}'",
            other
        ))),
    }
}

// ── Error mapping ────────────────────────────────────────────────────

fn map_storage_error(e: impl std::fmt::Display) -> BoundaryInteractionReadError {
    BoundaryInteractionReadError::Storage(e.to_string())
}

// ── Count-by helper ──────────────────────────────────────────────────

fn query_count_by<T, F, M>(
    conn: &rusqlite::Connection,
    column: &str,
    snapshot_uid: &str,
    parse: F,
    map: M,
) -> Result<Vec<T>, BoundaryInteractionReadError>
where
    F: Fn(&str) -> Result<T::Key, BoundaryInteractionReadError>,
    M: Fn(T::Key, usize) -> T,
    T: CountItem,
{
    let sql = format!(
        "SELECT {}, COUNT(*) as cnt
         FROM boundary_interaction_surfaces
         WHERE snapshot_uid = ?
         GROUP BY {}
         ORDER BY {} ASC",
        column, column, column
    );

    let mut stmt = conn.prepare(&sql).map_err(map_storage_error)?;

    let rows = stmt
        .query_map([snapshot_uid], |row| {
            let key: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((key, count as usize))
        })
        .map_err(map_storage_error)?;

    let mut results = Vec::new();
    for row_result in rows {
        let (key, count) = row_result.map_err(map_storage_error)?;
        results.push(map(parse(&key)?, count));
    }

    Ok(results)
}

/// Trait for count items with associated key type.
trait CountItem {
    type Key;
}

impl CountItem for KindCount {
    type Key = ChannelKind;
}

impl CountItem for ScopeCount {
    type Key = BoundaryScope;
}

impl CountItem for DirectionCount {
    type Key = Direction;
}

impl CountItem for FamilyCount {
    type Key = ProtocolFamily;
}

impl CountItem for BasisCount {
    type Key = InteractionBasis;
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_boundary_interaction::{
        surface::SurfaceBuilder, ChannelDetail, InteractionBasis as Basis,
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

    fn insert_test_surface(conn: &mut StorageConnection, file: &str, line: u32) {
        let surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Provider)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key(format!("test-repo:{}#fn:SYMBOL:function", file))
            .source_file(file)
            .location(line, line + 5, 5, 50)
            .extractor("c-ipc:0.1.0")
            .basis(Basis::ApiCall)
            .build()
            .unwrap();

        conn.insert_boundary_surfaces(&[surface]).unwrap();
    }

    #[test]
    fn list_returns_sorted_results() {
        let mut conn = create_test_db();

        // Insert in non-sorted order
        insert_test_surface(&mut conn, "src/z.c", 100);
        insert_test_surface(&mut conn, "src/a.c", 50);
        insert_test_surface(&mut conn, "src/a.c", 30);

        let filter = BoundaryInteractionFilter::new();
        let results = conn.list_boundary_interactions("snap-1", &filter).unwrap();

        assert_eq!(results.len(), 3);
        // Should be sorted by file, then line
        assert_eq!(results[0].source_file, "src/a.c");
        assert_eq!(results[0].line_start, 30);
        assert_eq!(results[1].source_file, "src/a.c");
        assert_eq!(results[1].line_start, 50);
        assert_eq!(results[2].source_file, "src/z.c");
    }

    #[test]
    fn list_with_file_filter() {
        let mut conn = create_test_db();

        insert_test_surface(&mut conn, "src/server.c", 100);
        insert_test_surface(&mut conn, "src/client.c", 50);

        let filter = BoundaryInteractionFilter::new().with_file("src/server.c");
        let results = conn.list_boundary_interactions("snap-1", &filter).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_file, "src/server.c");
    }

    #[test]
    fn list_with_file_prefix_filter() {
        let mut conn = create_test_db();

        insert_test_surface(&mut conn, "src/server.c", 100);
        insert_test_surface(&mut conn, "src/client.c", 50);
        insert_test_surface(&mut conn, "lib/util.c", 25);

        let filter = BoundaryInteractionFilter::new().with_file_prefix("src/");
        let results = conn.list_boundary_interactions("snap-1", &filter).unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.source_file.starts_with("src/")));
    }

    #[test]
    fn get_detail_returns_none_for_missing() {
        let conn = create_test_db();

        let result = conn
            .get_boundary_interaction_detail("non-existent")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_detail_includes_channels() {
        let mut conn = create_test_db();

        let surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Provider)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("test-repo:src/server.c#fn:SYMBOL:function")
            .source_file("src/server.c")
            .location(100, 105, 5, 50)
            .extractor("c-ipc:0.1.0")
            .basis(Basis::ApiCall)
            .build()
            .unwrap();

        let surface_uid = surface.surface_uid.clone();
        conn.insert_boundary_surfaces(&[surface]).unwrap();

        let channel = ChannelDetail {
            channel_uid: ChannelDetail::build_uid(&surface_uid, "/var/run/app.sock"),
            surface_uid: surface_uid.clone(),
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
        conn.insert_boundary_channels(&[channel]).unwrap();

        let detail = conn
            .get_boundary_interaction_detail(&surface_uid)
            .unwrap()
            .unwrap();

        assert_eq!(detail.channels.len(), 1);
        assert_eq!(
            detail.channels[0].socket_path.as_deref(),
            Some("/var/run/app.sock")
        );
    }

    #[test]
    fn summary_counts_correctly() {
        let mut conn = create_test_db();

        insert_test_surface(&mut conn, "src/a.c", 10);
        insert_test_surface(&mut conn, "src/b.c", 20);
        insert_test_surface(&mut conn, "src/b.c", 30);

        let summary = conn.get_boundary_interaction_summary("snap-1").unwrap();

        assert_eq!(summary.total_surfaces, 3);
        assert_eq!(summary.files_with_boundaries.len(), 2);
        assert_eq!(summary.files_with_boundaries[0], "src/a.c");
        assert_eq!(summary.files_with_boundaries[1], "src/b.c");

        // All are unix_socket
        assert_eq!(summary.by_channel_kind.len(), 1);
        assert_eq!(summary.by_channel_kind[0].channel_kind, ChannelKind::UnixSocket);
        assert_eq!(summary.by_channel_kind[0].count, 3);
    }

    #[test]
    fn empty_snapshot_returns_empty_results() {
        let conn = create_test_db();

        let filter = BoundaryInteractionFilter::new();
        let results = conn.list_boundary_interactions("snap-1", &filter).unwrap();
        assert!(results.is_empty());

        let summary = conn.get_boundary_interaction_summary("snap-1").unwrap();
        assert_eq!(summary.total_surfaces, 0);
        assert_eq!(summary.total_channels, 0);
        assert!(summary.files_with_boundaries.is_empty());
    }

    // ── Contract association tests (Track B / GR-1A) ─────────────────────

    #[test]
    fn list_shows_contract_name_when_contract_association_exists() {
        let mut conn = create_test_db();

        // Create a gRPC-style surface with contract association
        let surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::Unknown)
            .channel_kind(ChannelKind::GrpcChannel)
            .direction(Direction::Provider)
            .protocol("grpc")
            .transport_class(TransportClass::SchemaRpc)
            .interaction_pattern(InteractionPattern::RequestResponse)
            .endpoint_locality(EndpointLocality::Unknown)
            .symbol_stable_key("test-repo:src/GreeterImpl.java#GreeterImpl:SYMBOL:class")
            .source_file("src/GreeterImpl.java")
            .location(10, 50, 1, 1)
            .extractor("grpc_impl_hint_java")
            .basis(Basis::Inferred)
            .build()
            .unwrap();

        let surface_uid = surface.surface_uid.clone();
        conn.insert_boundary_surfaces(&[surface]).unwrap();

        // Create contract schema and element
        conn.connection_mut()
            .execute_batch(&format!(
                "INSERT INTO contract_schemas (schema_uid, snapshot_uid, repo_uid, schema_kind, file_path, package_name, content_hash, extractor, parsed_at)
                 VALUES ('cs-1', 'snap-1', 'test-repo', 'protobuf', 'api/v1/greeter.proto', 'api.v1', 'abc123', 'proto-parser:0.1.0', datetime('now'));
                 INSERT INTO contract_elements (element_uid, schema_uid, element_kind, name, full_name)
                 VALUES ('ce-greeter', 'cs-1', 'service', 'Greeter', 'api.v1.Greeter');
                 INSERT INTO boundary_contracts (association_uid, surface_uid, contract_element_uid, contract_kind, association_basis, confidence)
                 VALUES ('bc-1', '{}', 'ce-greeter', 'grpc_service', 'generated_code_mapping', 0.95);",
                surface_uid
            ))
            .unwrap();

        // List should show contract info
        let filter = BoundaryInteractionFilter::new();
        let results = conn.list_boundary_interactions("snap-1", &filter).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].contract_name.as_deref(), Some("api.v1.Greeter"));
        assert_eq!(results[0].contract_kind.as_deref(), Some("grpc_service"));
    }

    #[test]
    fn list_shows_none_for_contract_when_no_association() {
        let mut conn = create_test_db();

        // Plain IPC surface with no contract
        insert_test_surface(&mut conn, "src/ipc.c", 100);

        let filter = BoundaryInteractionFilter::new();
        let results = conn.list_boundary_interactions("snap-1", &filter).unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].contract_name.is_none());
        assert!(results[0].contract_kind.is_none());
    }

    #[test]
    fn list_shows_row_consistent_contract_when_multiple_associations() {
        let mut conn = create_test_db();

        // Create a gRPC surface
        let surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::Unknown)
            .channel_kind(ChannelKind::GrpcChannel)
            .direction(Direction::Provider)
            .protocol("grpc")
            .transport_class(TransportClass::SchemaRpc)
            .interaction_pattern(InteractionPattern::RequestResponse)
            .endpoint_locality(EndpointLocality::Unknown)
            .symbol_stable_key("test-repo:src/MultiImpl.java#MultiImpl:SYMBOL:class")
            .source_file("src/MultiImpl.java")
            .location(10, 50, 1, 1)
            .extractor("grpc_impl_hint_java")
            .basis(Basis::Inferred)
            .build()
            .unwrap();

        let surface_uid = surface.surface_uid.clone();
        conn.insert_boundary_surfaces(&[surface]).unwrap();

        // Create TWO contract elements with DIFFERENT kinds and names.
        // The key test: ensure list picks fields from the SAME row (by MIN association_uid).
        // - bc-aaa → ce-alpha → full_name="alpha.Service", kind="grpc_service"
        // - bc-zzz → ce-zeta → full_name="zeta.Handler", kind="grpc_method"
        // List should show ("alpha.Service", "grpc_service") from bc-aaa,
        // NOT ("alpha.Service", "grpc_method") which would be a cross-row synthesis.
        conn.connection_mut()
            .execute_batch(&format!(
                "INSERT INTO contract_schemas (schema_uid, snapshot_uid, repo_uid, schema_kind, file_path, package_name, content_hash, extractor, parsed_at)
                 VALUES ('cs-multi', 'snap-1', 'test-repo', 'protobuf', 'api/multi.proto', 'api', 'abc123', 'proto-parser:0.1.0', datetime('now'));
                 INSERT INTO contract_elements (element_uid, schema_uid, element_kind, name, full_name)
                 VALUES ('ce-alpha', 'cs-multi', 'service', 'AlphaService', 'alpha.Service');
                 INSERT INTO contract_elements (element_uid, schema_uid, element_kind, name, full_name)
                 VALUES ('ce-zeta', 'cs-multi', 'method', 'ZetaHandler', 'zeta.Handler');
                 INSERT INTO boundary_contracts (association_uid, surface_uid, contract_element_uid, contract_kind, association_basis, confidence)
                 VALUES ('bc-aaa', '{}', 'ce-alpha', 'grpc_service', 'generated_code_mapping', 0.95);
                 INSERT INTO boundary_contracts (association_uid, surface_uid, contract_element_uid, contract_kind, association_basis, confidence)
                 VALUES ('bc-zzz', '{}', 'ce-zeta', 'grpc_method', 'generated_code_mapping', 0.90);",
                surface_uid, surface_uid
            ))
            .unwrap();

        let filter = BoundaryInteractionFilter::new();
        let results = conn.list_boundary_interactions("snap-1", &filter).unwrap();

        assert_eq!(results.len(), 1);
        // Must be from the SAME row (bc-aaa, the MIN association_uid)
        assert_eq!(
            results[0].contract_name.as_deref(),
            Some("alpha.Service"),
            "contract_name should come from bc-aaa row"
        );
        assert_eq!(
            results[0].contract_kind.as_deref(),
            Some("grpc_service"),
            "contract_kind should come from bc-aaa row, NOT grpc_method from bc-zzz"
        );
    }

    #[test]
    fn detail_includes_contract_associations() {
        let mut conn = create_test_db();

        // Create surface
        let surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::Unknown)
            .channel_kind(ChannelKind::GrpcChannel)
            .direction(Direction::Provider)
            .protocol("grpc")
            .transport_class(TransportClass::SchemaRpc)
            .interaction_pattern(InteractionPattern::RequestResponse)
            .endpoint_locality(EndpointLocality::Unknown)
            .symbol_stable_key("test-repo:src/GreeterImpl.java#GreeterImpl:SYMBOL:class")
            .source_file("src/GreeterImpl.java")
            .location(10, 50, 1, 1)
            .extractor("grpc_impl_hint_java")
            .basis(Basis::Inferred)
            .build()
            .unwrap();

        let surface_uid = surface.surface_uid.clone();
        conn.insert_boundary_surfaces(&[surface]).unwrap();

        // Create contract schema, element, and association
        conn.connection_mut()
            .execute_batch(&format!(
                "INSERT INTO contract_schemas (schema_uid, snapshot_uid, repo_uid, schema_kind, file_path, package_name, content_hash, extractor, parsed_at)
                 VALUES ('cs-1', 'snap-1', 'test-repo', 'protobuf', 'api/v1/greeter.proto', 'api.v1', 'abc123', 'proto-parser:0.1.0', datetime('now'));
                 INSERT INTO contract_elements (element_uid, schema_uid, element_kind, name, full_name)
                 VALUES ('ce-greeter', 'cs-1', 'service', 'Greeter', 'api.v1.Greeter');
                 INSERT INTO boundary_contracts (association_uid, surface_uid, contract_element_uid, contract_kind, association_basis, confidence, evidence_json)
                 VALUES ('bc-1', '{}', 'ce-greeter', 'grpc_service', 'generated_code_mapping', 0.95, '{{\"mapping_uid\":\"m1\"}}');",
                surface_uid
            ))
            .unwrap();

        // Detail should include contracts
        let detail = conn
            .get_boundary_interaction_detail(&surface_uid)
            .unwrap()
            .unwrap();

        assert_eq!(detail.contracts.len(), 1);
        assert_eq!(detail.contracts[0].association_uid, "bc-1");
        assert_eq!(
            detail.contracts[0].contract_element_uid.as_deref(),
            Some("ce-greeter")
        );
        assert_eq!(detail.contracts[0].contract_kind, "grpc_service");
        assert_eq!(
            detail.contracts[0].contract_name.as_deref(),
            Some("api.v1.Greeter")
        );
        assert_eq!(detail.contracts[0].association_basis, "generated_code_mapping");
        assert!((detail.contracts[0].confidence - 0.95).abs() < 0.001);
        assert!(detail.contracts[0].evidence_json.is_some());
    }

    #[test]
    fn detail_returns_empty_contracts_when_no_association() {
        let mut conn = create_test_db();

        // Plain IPC surface
        let surface = SurfaceBuilder::new()
            .snapshot_uid("snap-1")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Provider)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("test-repo:src/ipc.c#main:SYMBOL:function")
            .source_file("src/ipc.c")
            .location(100, 105, 5, 50)
            .extractor("c-ipc:0.1.0")
            .basis(Basis::ApiCall)
            .build()
            .unwrap();

        let surface_uid = surface.surface_uid.clone();
        conn.insert_boundary_surfaces(&[surface]).unwrap();

        let detail = conn
            .get_boundary_interaction_detail(&surface_uid)
            .unwrap()
            .unwrap();

        assert!(detail.contracts.is_empty());
    }
}
