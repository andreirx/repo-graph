//! Graph query command family.
//!
//! Symbol-level and file-level graph traversal commands.
//!
//! # REG-1 Contract
//!
//! All graph query commands resolve repo from current working directory
//! via the daemon registry. No positional `<db_path> <repo_uid>` arguments.
//!
//! ```text
//! rmap callers <symbol> [--edge-types <types>]
//! rmap callees <symbol> [--edge-types <types>]
//! rmap imports <file_path>
//! rmap stats
//! rmap cycles
//! rmap path <from> <to>
//! ```

use std::process::ExitCode;

use crate::daemon_client::{daemon_unavailable_message, DaemonClient, DaemonClientError};

// ── Edge type parsing (graph-family-local) ───────────────────────

/// Valid edge types for `--edge-types` filter (Rust-17, SB-5).
const VALID_EDGE_TYPES: &[&str] = &["CALLS", "INSTANTIATES", "READS", "WRITES"];

/// Parse `--edge-types` from a command's argument slice.
///
/// Returns `(positional_args, edge_types)` on success, or an error
/// message on failure. If `--edge-types` is absent, returns the
/// default `["CALLS"]`.
fn parse_edge_types_flag(args: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let mut positional = Vec::new();
    let mut edge_types: Option<Vec<String>> = None;
    let mut i = 0;

    while i < args.len() {
        if args[i] == "--edge-types" {
            if edge_types.is_some() {
                return Err("repeated --edge-types flag".to_string());
            }
            i += 1;
            if i >= args.len() {
                return Err("missing value after --edge-types".to_string());
            }
            let raw = &args[i];
            if raw.is_empty() {
                return Err("empty --edge-types value".to_string());
            }
            let types: Vec<String> = raw.split(',').map(|t| t.trim().to_string()).collect();
            for t in &types {
                if t.is_empty() {
                    return Err("empty token in --edge-types value".to_string());
                }
                if !VALID_EDGE_TYPES.contains(&t.as_str()) {
                    return Err(format!(
                        "unknown edge type '{}', expected one of: {}",
                        t,
                        VALID_EDGE_TYPES.join(", ")
                    ));
                }
            }
            edge_types = Some(types);
        } else {
            positional.push(args[i].clone());
        }
        i += 1;
    }

    let types = edge_types.unwrap_or_else(|| vec!["CALLS".to_string()]);
    Ok((positional, types))
}

/// Resolve repo from cwd and return canonical path.
fn resolve_repo_from_cwd() -> Result<String, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot get current directory: {}", e))?;
    let canonical = cwd
        .canonicalize()
        .map_err(|e| format!("cannot canonicalize current directory: {}", e))?;
    Ok(canonical.to_string_lossy().to_string())
}

/// Create daemon client and check availability.
fn create_daemon_client(command: &str) -> Result<DaemonClient, ExitCode> {
    let client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return Err(ExitCode::from(2));
        }
    };

    if !client.is_available() {
        eprintln!(
            "{}",
            daemon_unavailable_message(client.socket_path(), command)
        );
        return Err(ExitCode::from(2));
    }

    Ok(client)
}

/// Handle daemon error response with REG-1 hint for repo not found.
fn handle_daemon_error(err: DaemonClientError) -> ExitCode {
    match err {
        DaemonClientError::DaemonError {
            code,
            message,
            data,
        } => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo");
            } else if code == "AmbiguousSymbol" {
                // Render structured ambiguity data
                eprintln!("error: {}", message);
                if let Some(data) = data {
                    if let Some(matches) = data.get("matches").and_then(|m| m.as_array()) {
                        eprintln!();
                        eprintln!("Matches:");
                        for (i, m) in matches.iter().enumerate() {
                            let qualified = m
                                .get("qualified_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                            let file = m.get("file").and_then(|v| v.as_str()).unwrap_or("?");
                            eprintln!("  {}. {}  {}  {}", i + 1, qualified, kind, file);
                        }
                        eprintln!();
                        eprintln!("hint: use qualified name for exact match");
                    }
                }
            } else {
                eprintln!("error: {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        e => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── callers command (REG-1 + CLI-OUT-3) ──────────────────────────────
//
// `rmap callers <symbol> [--edge-types <types>] [--json]`
//
// Human mode (default): plain text with caller list.
// Machine mode (--json): full envelope.

pub fn run_callers(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json before edge_types parsing) ────
    let mut json_mode = false;
    let filtered_args: Vec<String> = args
        .iter()
        .filter(|a| {
            if *a == "--json" {
                json_mode = true;
                false
            } else {
                true
            }
        })
        .cloned()
        .collect();

    let (positional, edge_types) = match parse_edge_types_flag(&filtered_args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callers <symbol> [--edge-types <types>] [--json]");
            return ExitCode::from(1);
        }
    };

    // REG-1: one positional arg (symbol), repo from cwd
    if positional.len() != 1 {
        eprintln!("usage: rmap callers <symbol> [--edge-types <types>] [--json]");
        return ExitCode::from(1);
    }

    let symbol = &positional[0];

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut client = match create_daemon_client("callers") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let params = serde_json::json!({
        "repo": repo_path,
        "symbol": symbol,
        "edge_types": edge_types,
    });

    match client.request("callers", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-3)
                use crate::presentation::graph_edges::CallersResponse;
                match serde_json::from_value::<CallersResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse callers response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e),
    }
}

// ── callees command (REG-1 + CLI-OUT-3) ──────────────────────────────
//
// `rmap callees <symbol> [--edge-types <types>] [--json]`
//
// Human mode (default): plain text with callee list.
// Machine mode (--json): full envelope.

pub fn run_callees(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json before edge_types parsing) ────
    let mut json_mode = false;
    let filtered_args: Vec<String> = args
        .iter()
        .filter(|a| {
            if *a == "--json" {
                json_mode = true;
                false
            } else {
                true
            }
        })
        .cloned()
        .collect();

    let (positional, edge_types) = match parse_edge_types_flag(&filtered_args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callees <symbol> [--edge-types <types>] [--json]");
            return ExitCode::from(1);
        }
    };

    // REG-1: one positional arg (symbol), repo from cwd
    if positional.len() != 1 {
        eprintln!("usage: rmap callees <symbol> [--edge-types <types>] [--json]");
        return ExitCode::from(1);
    }

    let symbol = &positional[0];

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut client = match create_daemon_client("callees") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let params = serde_json::json!({
        "repo": repo_path,
        "symbol": symbol,
        "edge_types": edge_types,
    });

    match client.request("callees", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-3)
                use crate::presentation::graph_edges::CalleesResponse;
                match serde_json::from_value::<CalleesResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse callees response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e),
    }
}

// ── path command (REG-1 + CLI-OUT-3) ─────────────────────────────────
//
// `rmap path <from> <to> [--json]`
//
// Human mode (default): plain text showing route between symbols.
// Machine mode (--json): full envelope.

pub fn run_path(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json) ──────────────────────────
    let mut json_mode = false;
    let positional: Vec<&String> = args
        .iter()
        .filter(|a| {
            if *a == "--json" {
                json_mode = true;
                false
            } else {
                true
            }
        })
        .collect();

    // REG-1: two positional args (from, to), repo from cwd
    if positional.len() != 2 {
        eprintln!("usage: rmap path <from> <to> [--json]");
        return ExitCode::from(1);
    }

    let from_query = positional[0];
    let to_query = positional[1];

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut client = match create_daemon_client("path") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let params = serde_json::json!({
        "repo": repo_path,
        "from": from_query,
        "to": to_query,
    });

    match client.request("path", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-3)
                use crate::presentation::path::PathResponse;
                match serde_json::from_value::<PathResponse>(result) {
                    Ok(response) => {
                        // Pass query terms so not-found header preserves user intent
                        print!("{}", response.render_human_with_query(from_query, to_query));
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse path response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e),
    }
}

// ── imports command (REG-1 + CLI-OUT-3) ──────────────────────────────
//
// `rmap imports <file_path> [--json]`
//
// Human mode (default): plain text showing file dependencies.
// Machine mode (--json): full envelope.

pub fn run_imports(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json) ──────────────────────────
    let mut json_mode = false;
    let positional: Vec<&String> = args
        .iter()
        .filter(|a| {
            if *a == "--json" {
                json_mode = true;
                false
            } else {
                true
            }
        })
        .collect();

    // REG-1: one positional arg (file_path), repo from cwd
    if positional.len() != 1 {
        eprintln!("usage: rmap imports <file_path> [--json]");
        return ExitCode::from(1);
    }

    let file_path = positional[0];

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut client = match create_daemon_client("imports") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let params = serde_json::json!({
        "repo": repo_path,
        "file": file_path,
    });

    match client.request("imports", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-3)
                use crate::presentation::imports::ImportsResponse;
                match serde_json::from_value::<ImportsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse imports response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e),
    }
}

// ── cycles command (REG-1 + CLI-OUT-2B) ──────────────────────────────
//
// `rmap cycles [--json]`
//
// Human mode (default): plain text with cycle topology.
// Machine mode (--json): full envelope.

pub fn run_cycles(args: &[String]) -> ExitCode {
    // ── Parse args ───────────────────────────────────────────
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap cycles [--json]");
                return ExitCode::from(1);
            }
            other => {
                eprintln!("error: unexpected argument: {}", other);
                eprintln!("usage: rmap cycles [--json]");
                return ExitCode::from(1);
            }
        }
    }

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut client = match create_daemon_client("cycles") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("cycles", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render
                use crate::presentation::cycles::CyclesResponse;
                match serde_json::from_value::<CyclesResponse>(result) {
                    Ok(response) => {
                        println!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse cycles response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e),
    }
}

// ── stats command (REG-1, CLI-OUT-2C) ────────────────────────────────

pub fn run_stats(args: &[String]) -> ExitCode {
    // ── Parse args ───────────────────────────────────────────
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap stats [--json]");
                return ExitCode::from(1);
            }
            other => {
                eprintln!("error: unexpected argument: {}", other);
                eprintln!("usage: rmap stats [--json]");
                return ExitCode::from(1);
            }
        }
    }

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut client = match create_daemon_client("stats") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("stats", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-2C)
                use crate::presentation::stats::StatsResponse;
                match serde_json::from_value::<StatsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse stats response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e),
    }
}
