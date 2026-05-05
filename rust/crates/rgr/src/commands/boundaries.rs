//! Boundaries command family.
//!
//! BI-1A: Boundary interaction discovery for local IPC mechanisms.
//!
//! ## Commands
//!
//!   rmap boundaries list <db_path> <repo_uid> [filters...]
//!   rmap boundaries show <db_path> <repo_uid> <surface_uid>
//!   rmap boundaries summary <db_path> <repo_uid>
//!
//! ## Exit codes
//!
//!   0 — success (results found)
//!   1 — success (no results found / not found)
//!   2 — runtime error
//!
//! ## Filters (list command)
//!
//!   --kind <unix_socket|named_pipe|...>     Filter by channel kind
//!   --scope <inter_process|inter_device>    Filter by boundary scope
//!   --direction <provider|consumer>         Filter by direction
//!   --family <socket|pipe|shared_memory|...> Filter by protocol family
//!   --file <path>                           Filter by exact file path
//!   --file-prefix <prefix>                  Filter by file path prefix
//!   --symbol <key>                          Filter by enclosing symbol stable key

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage, resolve_repo_ref};

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, BoundaryScope, ChannelKind, Direction,
    ProtocolFamily,
};

// ── boundaries command ───────────────────────────────────────────────

pub fn run_boundaries(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_boundaries_list(&args[1..]),
        "show" => run_boundaries_show(&args[1..]),
        "summary" => run_boundaries_summary(&args[1..]),
        other => {
            eprintln!("unknown boundaries subcommand: {}", other);
            print_usage();
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  rmap boundaries list <db_path> <repo_uid> [--kind <kind>] [--scope <scope>] [--direction <dir>] [--family <fam>] [--file <path>] [--file-prefix <prefix>] [--symbol <key>]");
    eprintln!("  rmap boundaries show <db_path> <repo_uid> <surface_uid>");
    eprintln!("  rmap boundaries summary <db_path> <repo_uid>");
}

// ── boundaries list ──────────────────────────────────────────────────

fn run_boundaries_list(args: &[String]) -> ExitCode {
    let (db_path, repo_ref, filter) = match parse_list_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{}", msg);
            return ExitCode::from(1);
        }
    };

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
        Ok(uid) => uid,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot for repo '{}'", repo_ref);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let items = match storage.list_boundary_interactions(&snapshot.snapshot_uid, &filter) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let count = items.len();

    // Build envelope with filter info.
    let mut extra = serde_json::Map::new();
    if let Some(ref k) = filter.channel_kind {
        extra.insert(
            "filter_kind".to_string(),
            serde_json::Value::String(k.as_str().to_string()),
        );
    }
    if let Some(ref s) = filter.boundary_scope {
        extra.insert(
            "filter_scope".to_string(),
            serde_json::Value::String(s.as_str().to_string()),
        );
    }
    if let Some(ref d) = filter.direction {
        extra.insert(
            "filter_direction".to_string(),
            serde_json::Value::String(d.as_str().to_string()),
        );
    }
    if let Some(ref f) = filter.protocol_family {
        extra.insert(
            "filter_family".to_string(),
            serde_json::Value::String(f.as_str().to_string()),
        );
    }
    if let Some(ref f) = filter.file {
        extra.insert(
            "filter_file".to_string(),
            serde_json::Value::String(f.clone()),
        );
    }
    if let Some(ref p) = filter.file_prefix {
        extra.insert(
            "filter_file_prefix".to_string(),
            serde_json::Value::String(p.clone()),
        );
    }
    if let Some(ref s) = filter.symbol {
        extra.insert(
            "filter_symbol".to_string(),
            serde_json::Value::String(s.clone()),
        );
    }

    let output = match build_envelope(
        &storage,
        "boundaries list",
        &repo_uid,
        &snapshot,
        serde_json::to_value(&items).unwrap(),
        count,
        extra,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            if count == 0 {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

fn parse_list_args(
    args: &[String],
) -> Result<(&Path, &str, BoundaryInteractionFilter), String> {
    if args.len() < 2 {
        return Err(
            "usage: rmap boundaries list <db_path> <repo_uid> [--kind <kind>] ...".to_string(),
        );
    }

    let db_path = Path::new(&args[0]);
    let repo_ref = args[1].as_str();
    let mut filter = BoundaryInteractionFilter::new();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                if i + 1 >= args.len() {
                    return Err("--kind requires a value".to_string());
                }
                filter.channel_kind = Some(parse_channel_kind(&args[i + 1])?);
                i += 2;
            }
            "--scope" => {
                if i + 1 >= args.len() {
                    return Err("--scope requires a value".to_string());
                }
                filter.boundary_scope = Some(parse_boundary_scope(&args[i + 1])?);
                i += 2;
            }
            "--direction" => {
                if i + 1 >= args.len() {
                    return Err("--direction requires a value".to_string());
                }
                filter.direction = Some(parse_direction(&args[i + 1])?);
                i += 2;
            }
            "--family" => {
                if i + 1 >= args.len() {
                    return Err("--family requires a value".to_string());
                }
                filter.protocol_family = Some(parse_protocol_family(&args[i + 1])?);
                i += 2;
            }
            "--file" => {
                if i + 1 >= args.len() {
                    return Err("--file requires a value".to_string());
                }
                filter.file = Some(args[i + 1].clone());
                i += 2;
            }
            "--file-prefix" => {
                if i + 1 >= args.len() {
                    return Err("--file-prefix requires a value".to_string());
                }
                filter.file_prefix = Some(args[i + 1].clone());
                i += 2;
            }
            "--symbol" => {
                if i + 1 >= args.len() {
                    return Err("--symbol requires a value".to_string());
                }
                filter.symbol = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                return Err(format!("unknown option: {}", other));
            }
        }
    }

    Ok((db_path, repo_ref, filter))
}

// ── boundaries show ──────────────────────────────────────────────────

fn run_boundaries_show(args: &[String]) -> ExitCode {
    if args.len() != 3 {
        eprintln!("usage: rmap boundaries show <db_path> <repo_uid> <surface_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_ref = &args[1];
    let surface_uid = &args[2];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Resolve repo to validate it exists.
    let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
        Ok(uid) => uid,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let detail = match storage.get_boundary_interaction_detail(surface_uid) {
        Ok(Some(d)) => d,
        Ok(None) => {
            eprintln!("error: surface not found: {}", surface_uid);
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Verify surface belongs to requested repo.
    if detail.repo_uid != repo_uid {
        eprintln!(
            "error: surface {} belongs to repo '{}', not '{}'",
            surface_uid, detail.repo_uid, repo_ref
        );
        return ExitCode::from(1);
    }

    match serde_json::to_string_pretty(&detail) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── boundaries summary ───────────────────────────────────────────────

fn run_boundaries_summary(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap boundaries summary <db_path> <repo_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_ref = &args[1];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
        Ok(uid) => uid,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot for repo '{}'", repo_ref);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let summary = match storage.get_boundary_interaction_summary(&snapshot.snapshot_uid) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Wrap in envelope with repo/snapshot context.
    let output = serde_json::json!({
        "command": "boundaries summary",
        "repo": repo_uid,
        "snapshot": snapshot.snapshot_uid,
        "summary": summary,
    });

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            if summary.total_surfaces == 0 {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── Parse helpers ────────────────────────────────────────────────────

fn parse_channel_kind(s: &str) -> Result<ChannelKind, String> {
    match s.to_lowercase().as_str() {
        "unix_socket" | "unixsocket" | "unix" => Ok(ChannelKind::UnixSocket),
        "named_pipe" | "namedpipe" | "fifo" => Ok(ChannelKind::NamedPipe),
        "anonymous_pipe" | "anonymouspipe" | "pipe" => Ok(ChannelKind::AnonymousPipe),
        "shared_memory" | "sharedmemory" | "shm" => Ok(ChannelKind::SharedMemory),
        "message_queue" | "messagequeue" | "mq" | "mqueue" => Ok(ChannelKind::MessageQueue),
        "semaphore" | "sem" => Ok(ChannelKind::Semaphore),
        "process_signal" | "processsignal" | "signal" => Ok(ChannelKind::ProcessSignal),
        "tcp_socket" | "tcpsocket" | "tcp" => Ok(ChannelKind::TcpSocket),
        "udp_socket" | "udpsocket" | "udp" => Ok(ChannelKind::UdpSocket),
        "shared_array_buffer" | "sharedarraybuffer" | "sab" | "atomics" => Ok(ChannelKind::SharedArrayBuffer),
        "amqp_queue" | "amqpqueue" | "amqp" | "rabbitmq" => Ok(ChannelKind::AmqpQueue),
        "kafka_topic" | "kafkatopic" | "kafka" => Ok(ChannelKind::KafkaTopic),
        "nats_subject" | "natssubject" | "nats" => Ok(ChannelKind::NatsSubject),
        "serial_port" | "serialport" | "serial" => Ok(ChannelKind::SerialPort),
        "can_message" | "canmessage" | "can" => Ok(ChannelKind::CanMessage),
        other => Err(format!(
            "unknown channel kind: {} (try: unix_socket, named_pipe, shared_memory, shared_array_buffer, amqp_queue, kafka_topic, nats_subject, ...)",
            other
        )),
    }
}

fn parse_boundary_scope(s: &str) -> Result<BoundaryScope, String> {
    match s.to_lowercase().as_str() {
        "intra_process" | "intraprocess" | "thread" => Ok(BoundaryScope::IntraProcess),
        "inter_process" | "interprocess" | "ipc" => Ok(BoundaryScope::InterProcess),
        "inter_device" | "interdevice" | "device" => Ok(BoundaryScope::InterDevice),
        "unknown" => Ok(BoundaryScope::Unknown),
        other => Err(format!(
            "unknown boundary scope: {} (try: intra_process, inter_process, inter_device, unknown)",
            other
        )),
    }
}

fn parse_direction(s: &str) -> Result<Direction, String> {
    match s.to_lowercase().as_str() {
        "provider" | "server" | "listen" => Ok(Direction::Provider),
        "consumer" | "client" | "connect" => Ok(Direction::Consumer),
        "bidirectional" | "both" => Ok(Direction::Bidirectional),
        other => Err(format!(
            "unknown direction: {} (try: provider, consumer, bidirectional)",
            other
        )),
    }
}

fn parse_protocol_family(s: &str) -> Result<ProtocolFamily, String> {
    match s.to_lowercase().as_str() {
        "socket" => Ok(ProtocolFamily::Socket),
        "pipe" => Ok(ProtocolFamily::Pipe),
        "shared_memory" | "sharedmemory" | "shm" => Ok(ProtocolFamily::SharedMemory),
        "message_queue" | "messagequeue" | "mq" => Ok(ProtocolFamily::MessageQueue),
        "signal" | "signals" | "process_signal" => Ok(ProtocolFamily::Signal),
        "semaphore" | "sem" => Ok(ProtocolFamily::Semaphore),
        "serial" => Ok(ProtocolFamily::Serial),
        "bus" => Ok(ProtocolFamily::Bus),
        "message_broker" | "messagebroker" | "broker" => Ok(ProtocolFamily::MessageBroker),
        "usb" => Ok(ProtocolFamily::Usb),
        "bluetooth" | "bt" | "ble" => Ok(ProtocolFamily::Bluetooth),
        "custom" => Ok(ProtocolFamily::Custom),
        other => Err(format!(
            "unknown protocol family: {} (try: socket, pipe, shared_memory, ...)",
            other
        )),
    }
}
