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

use crate::daemon_client::{DaemonClient, DaemonClientError};

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

/// Create daemon client.
fn create_daemon_client(_command: &str) -> Result<DaemonClient, ExitCode> {
    let client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return Err(ExitCode::from(2));
        }
    };

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

/// Extract `--engine <value>` (LIVEGRAPH-INTEGRATION-1B; default flipped to `auto` in
/// QUERY-MIGRATION-CLI-1). Default `auto` = LiveGraph when complete (Exact+Fresh+TS-only), else a
/// labelled SQLite fallback. Explicit `sqlite`/`livegraph`/`compare` still force that engine. The value
/// is validated daemon-side (lenient here); removes the flag + its value from the args.
fn extract_engine_flag(args: Vec<String>) -> (Vec<String>, String) {
    let mut engine = "auto".to_string();
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--engine" && i + 1 < args.len() {
            engine = args[i + 1].clone();
            i += 2;
        } else {
            out.push(args[i].clone());
            i += 1;
        }
    }
    (out, engine)
}

/// Extract `--kind <value>` (CYCLES-LIVEGRAPH-CLI-1). Default `""` = no kind (the SQLite default).
fn extract_kind_flag(args: Vec<String>) -> (Vec<String>, String) {
    let mut kind = String::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--kind" && i + 1 < args.len() {
            kind = args[i + 1].clone();
            i += 2;
        } else {
            out.push(args[i].clone());
            i += 1;
        }
    }
    (out, kind)
}

/// `rmap dev <subcommand>` — hidden/dev-only commands (LIVEGRAPH-INTEGRATION-1B). NOT part of the
/// default user workflow.
pub fn run_dev(args: &[String]) -> ExitCode {
    match args.first().map(|s| s.as_str()) {
        Some("livegraph-preload") => run_dev_livegraph_preload(&args[1..]),
        Some("livegraph-refresh") => run_dev_livegraph_refresh(&args[1..]),
        _ => {
            eprintln!("usage: rmap dev <livegraph-preload|livegraph-refresh> ...");
            eprintln!("  livegraph-preload --repo <repo> --partition-id <id> --scip <index.scip> --source-root <source-root>");
            eprintln!("  livegraph-refresh --repo <repo> [--partition <id>]");
            ExitCode::from(1)
        }
    }
}

/// Hidden dev (1C steps 2–3): send the daemon `livegraph_refresh` transport method. Steps 2–3 only
/// validate the absent-producer path (structured response); the daemon does NOT run scip-typescript
/// here (step 4, gated on a provisioned producer).
fn run_dev_livegraph_refresh(args: &[String]) -> ExitCode {
    let mut repo = None;
    let mut partition = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" if i + 1 < args.len() => {
                repo = Some(args[i + 1].clone());
                i += 2;
            }
            "--partition" if i + 1 < args.len() => {
                partition = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("error: unknown arg: {}", other);
                return ExitCode::from(1);
            }
        }
    }
    let repo = match repo {
        Some(r) => r,
        None => {
            eprintln!("usage: rmap dev livegraph-refresh --repo <repo> [--partition <id>]");
            return ExitCode::from(1);
        }
    };
    let mut client = match create_daemon_client("dev") {
        Ok(c) => c,
        Err(code) => return code,
    };
    let mut params = serde_json::json!({ "repo": repo });
    if let Some(p) = partition {
        params["partition"] = serde_json::json!(p);
    }
    match client.request("livegraph_refresh", Some(params)) {
        Ok(result) => match serde_json::to_string_pretty(&result) {
            Ok(json) => {
                println!("{}", json);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(2)
            }
        },
        Err(e) => handle_daemon_error(e),
    }
}

/// Hidden dev (S1): send the daemon `livegraph_preload` transport method over the SAME DaemonClient
/// the query commands use (Rust-only, no TypeScript). The daemon DECODES the supplied `.scip`, ingests
/// it, and feeds it into the repo's in-memory LiveGraph — it does NOT run scip-typescript.
fn run_dev_livegraph_preload(args: &[String]) -> ExitCode {
    let mut repo = None;
    let mut partition_id = None;
    let mut scip = None;
    let mut source_root = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" if i + 1 < args.len() => {
                repo = Some(args[i + 1].clone());
                i += 2;
            }
            "--partition-id" if i + 1 < args.len() => {
                partition_id = Some(args[i + 1].clone());
                i += 2;
            }
            "--scip" if i + 1 < args.len() => {
                scip = Some(args[i + 1].clone());
                i += 2;
            }
            "--source-root" if i + 1 < args.len() => {
                source_root = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("error: unknown arg: {}", other);
                return ExitCode::from(1);
            }
        }
    }
    let (repo, partition_id, scip, source_root) = match (repo, partition_id, scip, source_root) {
        (Some(r), Some(p), Some(s), Some(sr)) => (r, p, s, sr),
        _ => {
            eprintln!("usage: rmap dev livegraph-preload --repo <repo> --partition-id <id> --scip <index.scip> --source-root <source-root>");
            return ExitCode::from(1);
        }
    };
    let mut client = match create_daemon_client("dev") {
        Ok(c) => c,
        Err(code) => return code,
    };
    let params = serde_json::json!({
        "repo": repo,
        "partition_id": partition_id,
        "scip": scip,
        "source_root": source_root,
    });
    match client.request("livegraph_preload", Some(params)) {
        Ok(result) => match serde_json::to_string_pretty(&result) {
            Ok(json) => {
                println!("{}", json);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(2)
            }
        },
        Err(e) => handle_daemon_error(e),
    }
}

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

    // LIVEGRAPH-INTEGRATION-1B: extract --engine before edge-type parsing.
    let (filtered_args, engine) = extract_engine_flag(filtered_args);

    let (positional, edge_types) = match parse_edge_types_flag(&filtered_args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callers <symbol> [--edge-types <types>] [--engine auto|sqlite|livegraph|compare] [--json]");
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
        "engine": engine,
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
                // Human mode (CLI-OUT-3). 1B: surface the comparison sidecar (if present) and strip
                // the diagnostic fields so the sqlite-compatible render is byte-unchanged.
                let mut result = result;
                if let Some(p) = result
                    .get("livegraph_compare_sidecar")
                    .and_then(|v| v.as_str())
                {
                    eprintln!("livegraph comparison written to {}", p);
                }
                if let Some(obj) = result.as_object_mut() {
                    obj.remove("livegraph_compare");
                    obj.remove("livegraph_compare_sidecar");
                    // QUERY-MIGRATION-CLI-1: backend_used/fallback_reason are JSON-only metadata; strip
                    // them so the human render is unaffected (no new trust metadata in human output).
                    obj.remove("backend_used");
                    obj.remove("fallback_reason");
                }
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

    // LIVEGRAPH-INTEGRATION-1B: extract --engine before edge-type parsing.
    let (filtered_args, engine) = extract_engine_flag(filtered_args);

    let (positional, edge_types) = match parse_edge_types_flag(&filtered_args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callees <symbol> [--edge-types <types>] [--engine auto|sqlite|livegraph|compare] [--json]");
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
        "engine": engine,
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
                // Human mode (CLI-OUT-3). 1B: surface the comparison sidecar (if present) and strip
                // the diagnostic fields so the sqlite-compatible render is byte-unchanged.
                let mut result = result;
                if let Some(p) = result
                    .get("livegraph_compare_sidecar")
                    .and_then(|v| v.as_str())
                {
                    eprintln!("livegraph comparison written to {}", p);
                }
                if let Some(obj) = result.as_object_mut() {
                    obj.remove("livegraph_compare");
                    obj.remove("livegraph_compare_sidecar");
                    // QUERY-MIGRATION-CLI-1: strip JSON-only metadata before the human render.
                    obj.remove("backend_used");
                    obj.remove("fallback_reason");
                }
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
    // PATH-LIVEGRAPH-DEFAULT-1: extract --engine FIRST; `path` now DEFAULTS to `auto` (serve LiveGraph
    // when Exact/Fresh/complete, else labelled SQLite fallback — the daemon decides). `--engine sqlite`
    // forces SQLite, `--engine livegraph`/`compare` stay explicit. Then filter --json from the positionals.
    let (args, engine) = extract_engine_flag(args.to_vec());
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
        eprintln!("usage: rmap path <from> <to> [--engine auto|sqlite|livegraph|compare] [--json]");
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
        "engine": engine,
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
                // Human mode: parse and render (CLI-OUT-3). PATH-CYCLES-LIVEGRAPH-1: surface the compare
                // sidecar (if present) + strip JSON-only metadata so the human render is unaffected.
                let mut result = result;
                if let Some(p) = result
                    .get("livegraph_path_compare_sidecar")
                    .and_then(|v| v.as_str())
                {
                    eprintln!("livegraph path comparison written to {}", p);
                }
                if let Some(obj) = result.as_object_mut() {
                    obj.remove("livegraph_path_compare");
                    obj.remove("livegraph_path_compare_sidecar");
                    obj.remove("backend_used");
                    obj.remove("fallback_reason");
                    obj.remove("trust_class");
                    obj.remove("freshness");
                }
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
    // CYCLES-LIVEGRAPH-CLI-1: extract --engine + --kind FIRST. Default (no flags) = SQLite MODULE-import
    // cycles (unchanged). `--engine livegraph --kind file-import` = LiveGraph captured FILE import cycles
    // (a DIFFERENT graph; NO SQLite fallback). Then parse --json from the remaining positionals.
    let (args, engine_raw) = extract_engine_flag(args.to_vec());
    let (args, kind) = extract_kind_flag(args);
    // Absent engine == sqlite for cycles — the default stays SQLite (no `auto` migration).
    let engine = if engine_raw == "auto" {
        "sqlite"
    } else {
        engine_raw.as_str()
    };

    let usage = "usage: rmap cycles [--engine sqlite|livegraph] [--kind file-import] [--json]";
    let mut json_mode = false;
    for arg in &args {
        match arg.as_str() {
            "--json" => json_mode = true,
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {flag}");
                eprintln!("{usage}");
                return ExitCode::from(1);
            }
            other => {
                eprintln!("error: unexpected argument: {other}");
                eprintln!("{usage}");
                return ExitCode::from(1);
            }
        }
    }

    // Validate the engine/kind combination (D2/D6/D7): reject invalid combos with a clear error rather
    // than silently computing a different graph.
    let livegraph = match (engine, kind.as_str()) {
        ("sqlite", "") => false, // SQLite MODULE-import cycles (default, unchanged)
        ("livegraph", "file-import") => true, // LiveGraph captured FILE import cycles
        ("livegraph", _) => {
            eprintln!("error: --engine livegraph requires --kind file-import");
            return ExitCode::from(1);
        }
        ("sqlite", "file-import") => {
            eprintln!("error: SQLite does not answer captured FILE import cycles; use --engine livegraph --kind file-import");
            return ExitCode::from(1);
        }
        ("compare", _) => {
            eprintln!("error: --engine compare is not supported for cycles (FILE-import and MODULE-import are different graphs)");
            return ExitCode::from(1);
        }
        (_, "file-import") => {
            eprintln!("error: --kind file-import requires --engine livegraph");
            return ExitCode::from(1);
        }
        (e, "") => {
            eprintln!("error: unknown --engine '{e}' (supported: sqlite, livegraph)");
            return ExitCode::from(1);
        }
        (_, k) => {
            eprintln!("error: unknown --kind '{k}' (supported: file-import)");
            return ExitCode::from(1);
        }
    };

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

    let params = if livegraph {
        serde_json::json!({ "repo": repo_path, "engine": "livegraph", "kind": "file-import" })
    } else {
        serde_json::json!({ "repo": repo_path })
    };

    match client.request("cycles", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope (includes scope/backend_used/answer_class/freshness/
                // missing_partitions/degradation_reasons for the LiveGraph path; D5).
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
                use crate::presentation::cycles::CyclesResponse;
                // D4/D7: LiveGraph file-import output LABELS its scope + surfaces the trust class (never a
                // silent SQLite fallback). SQLite output is unchanged (no extra line).
                if livegraph {
                    let scope = result
                        .get("scope")
                        .and_then(|v| v.as_str())
                        .unwrap_or("CapturedResolvedRelativeIntraPartition");
                    let class = result
                        .get("answer_class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let freshness = result
                        .get("freshness")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    println!(
                        "Scope: captured resolved-relative intra-partition FILE import cycles \
                         (backend=livegraph; scope={scope}; class={class}; freshness={freshness})"
                    );
                    // Empty case: render a FILE-import-specific message (the generic CyclesResponse text
                    // says "module-level", which would contradict the scope label). SQLite is unchanged.
                    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                    if count == 0 {
                        println!("No FILE import cycles found within the captured scope.");
                        return ExitCode::SUCCESS;
                    }
                }
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
