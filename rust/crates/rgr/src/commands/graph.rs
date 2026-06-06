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
        Some("cycle-completeness-audit") => run_dev_cycle_completeness_audit(&args[1..]),
        _ => {
            eprintln!(
                "usage: rmap dev <livegraph-preload|livegraph-refresh|cycle-completeness-audit> ..."
            );
            eprintln!("  livegraph-preload --repo <repo> --partition-id <id> --scip <index.scip> --source-root <source-root>");
            eprintln!("  livegraph-refresh --repo <repo> [--partition <id>] [--source-root <repo-relative-root>]... [--all-discovered] [--include-fixtures]");
            eprintln!("  cycle-completeness-audit --repo <repo> [--include-fixtures]   (read-only; load first via livegraph-refresh --all-discovered)");
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
    // IMPORTS-XPART-ENUMERATION-1 (D4): repeated --source-root -> one partition each (multi-partition,
    // best-effort). 0/1 root preserves single-partition behaviour.
    let mut source_roots: Vec<String> = Vec::new();
    // CYCLES-COMPLETENESS-ENUMERATION-1 (D2/D3): --all-discovered loads the shared-discovery included roots;
    // --include-fixtures disables the fixture-segment exclusion.
    let mut all_discovered = false;
    let mut include_fixtures = false;
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
            "--source-root" if i + 1 < args.len() => {
                source_roots.push(args[i + 1].clone());
                i += 2;
            }
            "--all-discovered" => {
                all_discovered = true;
                i += 1;
            }
            "--include-fixtures" => {
                include_fixtures = true;
                i += 1;
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
            eprintln!(
                "usage: rmap dev livegraph-refresh --repo <repo> [--partition <id>] \
                 [--source-root <repo-relative-root>]..."
            );
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
    if !source_roots.is_empty() {
        params["source_roots"] = serde_json::json!(source_roots);
    }
    if all_discovered {
        params["all_discovered"] = serde_json::json!(true);
    }
    if include_fixtures {
        params["include_fixtures"] = serde_json::json!(true);
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

/// Hidden dev (CYCLES-COMPLETENESS-AUDIT-1): send the daemon `cycle_completeness_audit` method. READ-ONLY
/// diagnostic — the daemon discovers the expected TS partition set (filesystem), reads the SQLite language
/// inventory (audit boundary), and reports the SQLite-free module-cycle completeness certificate for the
/// CURRENT in-memory LiveGraph. Load partitions first via `livegraph-refresh`; this does NOT load them and
/// changes no default.
fn run_dev_cycle_completeness_audit(args: &[String]) -> ExitCode {
    let mut repo = None;
    // ENUMERATION-1 (D3): --include-fixtures certifies a fixture corpus (disables fixture-segment exclusion).
    let mut include_fixtures = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" if i + 1 < args.len() => {
                repo = Some(args[i + 1].clone());
                i += 2;
            }
            "--include-fixtures" => {
                include_fixtures = true;
                i += 1;
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
            eprintln!(
                "usage: rmap dev cycle-completeness-audit --repo <repo> [--include-fixtures]"
            );
            return ExitCode::from(1);
        }
    };
    let mut client = match create_daemon_client("dev") {
        Ok(c) => c,
        Err(code) => return code,
    };
    let mut params = serde_json::json!({ "repo": repo });
    if include_fixtures {
        params["include_fixtures"] = serde_json::json!(true);
    }
    match client.request("cycle_completeness_audit", Some(params)) {
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
    // IMPORTS-LIVEGRAPH-CLI-1: extract --engine FIRST. Absent == "auto" -> "sqlite" (NO default migration:
    // the SQLite single-file listing stays the default; `--engine livegraph` is the explicit opt-in, D3).
    let (args, engine_raw) = extract_engine_flag(args.to_vec());
    let engine = if engine_raw == "auto" {
        "sqlite"
    } else {
        engine_raw.as_str()
    };
    let usage = "usage: rmap imports [<file>] [--engine sqlite|livegraph] [--json]";

    // Parse --json + the optional positional <file> from the remaining args.
    let mut json_mode = false;
    let mut positional: Vec<String> = Vec::new();
    for a in &args {
        match a.as_str() {
            "--json" => json_mode = true,
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {flag}");
                eprintln!("{usage}");
                return ExitCode::from(1);
            }
            other => positional.push(other.to_string()),
        }
    }

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Validate the engine/arg combination + build params (D6: sqlite REQUIRES <file>; livegraph file OPTIONAL
    // -> repo-wide).
    let params = match engine {
        "sqlite" => {
            if positional.len() != 1 {
                eprintln!("error: the sqlite engine requires exactly one <file>");
                eprintln!("{usage}");
                return ExitCode::from(1);
            }
            serde_json::json!({ "repo": repo_path, "file": positional[0] })
        }
        "livegraph" => {
            if positional.len() > 1 {
                eprintln!("error: at most one <file> (omit for a repo-wide view)");
                eprintln!("{usage}");
                return ExitCode::from(1);
            }
            let mut p = serde_json::json!({ "repo": repo_path, "engine": "livegraph" });
            if let Some(file) = positional.first() {
                p["file"] = serde_json::Value::String(file.clone());
            }
            p
        }
        other => {
            eprintln!("error: unknown --engine '{other}' (supported: sqlite, livegraph)");
            return ExitCode::from(1);
        }
    };

    let mut client = match create_daemon_client("imports") {
        Ok(c) => c,
        Err(code) => return code,
    };

    match client.request("imports", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: the full envelope (the AUTHORITATIVE, complete evidence for livegraph, D4).
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
            } else if engine == "livegraph" {
                use crate::presentation::imports::LivegraphImportsResponse;
                match serde_json::from_value::<LivegraphImportsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse imports response: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode (sqlite, CLI-OUT-3): the existing single-file listing renderer (unchanged).
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

/// The resolved cycles route (MODULE-CYCLES-CLI-1 D1): replaces the prior `livegraph: bool` now that the
/// engine/kind matrix has 4 live routes. Derived from the (engine, kind) pair; each maps to the daemon
/// params + the human renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CyclesRoute {
    /// SQLite MODULE-import cycles (the default; `--engine sqlite [--kind module-import]`).
    SqliteModule,
    /// LiveGraph captured FILE-import cycles (`--engine livegraph --kind file-import`).
    LivegraphFile,
    /// LiveGraph directory-aggregated MODULE-import cycles (`--engine livegraph --kind module-import`).
    LivegraphModule,
    /// SQLite MODULE cycles PRIMARY + a LiveGraph-vs-SQLite compare report
    /// (`--engine compare --kind module-import`).
    CompareModule,
}

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

    let usage =
        "usage: rmap cycles [--engine sqlite|livegraph] [--kind file-import|module-import] [--json]";
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
    let route = match (engine, kind.as_str()) {
        ("sqlite", "") => CyclesRoute::SqliteModule, // SQLite MODULE-import default (unchanged)
        ("sqlite", "module-import") => CyclesRoute::SqliteModule, // D6: explicit spelling of the default
        ("livegraph", "file-import") => CyclesRoute::LivegraphFile,
        ("livegraph", "module-import") => CyclesRoute::LivegraphModule,
        ("livegraph", _) => {
            eprintln!("error: --engine livegraph requires --kind file-import or module-import");
            return ExitCode::from(1);
        }
        ("sqlite", "file-import") => {
            eprintln!("error: SQLite does not answer captured FILE import cycles; use --engine livegraph --kind file-import");
            return ExitCode::from(1);
        }
        ("compare", "module-import") => CyclesRoute::CompareModule,
        ("compare", "file-import") => {
            eprintln!("error: --engine compare --kind file-import is not supported (FILE-import has no SQLite peer graph); use --kind module-import");
            return ExitCode::from(1);
        }
        ("compare", _) => {
            eprintln!("error: --engine compare requires --kind module-import");
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
            eprintln!("error: unknown --kind '{k}' (supported: file-import, module-import)");
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

    let params = match route {
        CyclesRoute::SqliteModule => serde_json::json!({ "repo": repo_path }),
        CyclesRoute::LivegraphFile => {
            serde_json::json!({ "repo": repo_path, "engine": "livegraph", "kind": "file-import" })
        }
        CyclesRoute::LivegraphModule => {
            serde_json::json!({ "repo": repo_path, "engine": "livegraph", "kind": "module-import" })
        }
        CyclesRoute::CompareModule => {
            serde_json::json!({ "repo": repo_path, "engine": "compare", "kind": "module-import" })
        }
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
                // CompareModule (MODULE-CYCLES-CLI-1): capture the diagnostic compare summary BEFORE
                // `result` is consumed by from_value. The PRIMARY answer is SQLite (render_human); the
                // compare metadata rides alongside as one summary line.
                let compare_summary: Option<String> = if route == CyclesRoute::CompareModule {
                    let cmp = result.get("livegraph_module_compare");
                    let n = |k: &str| {
                        cmp.and_then(|c| c.get(k))
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0)
                    };
                    let matched = cmp
                        .and_then(|c| c.get("matched"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let lg_count = cmp
                        .and_then(|c| c.get("livegraph_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let lg_class = cmp
                        .and_then(|c| c.get("livegraph_class"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let subset = cmp
                        .and_then(|c| c.get("livegraph_subset"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let sidecar = result
                        .get("livegraph_module_compare_sidecar")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<none>");
                    Some(format!(
                        "LiveGraph module-cycle compare: {matched} matched, {} missing (UnknownDivergence), \
                         {} extra (UnexpectedExtraInLiveGraph); livegraph_count={lg_count} class={lg_class}; \
                         livegraph_subset={subset}; sidecar={sidecar}",
                        n("missing_in_livegraph"),
                        n("extra_in_livegraph"),
                    ))
                } else {
                    None
                };
                // D4/D7: LiveGraph file-import output LABELS its scope + surfaces the trust class (never a
                // silent SQLite fallback). SQLite output is unchanged (no extra line).
                // Scope line for the LiveGraph routes (file + module); the SQLite default prints no extra
                // line. `scope` is a STRUCTURED object; the human line is stringified FROM its flags.
                if matches!(
                    route,
                    CyclesRoute::LivegraphFile | CyclesRoute::LivegraphModule
                ) {
                    let scope = result.get("scope");
                    let intra = scope
                        .and_then(|s| s.get("intra_partition"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let cross = scope
                        .and_then(|s| s.get("cross_partition"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let xpart = scope
                        .and_then(|s| s.get("xpart_edge_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let class = result
                        .get("answer_class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let freshness = result
                        .get("freshness")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let mut surfaces: Vec<String> = Vec::new();
                    if intra {
                        surfaces.push("intra-partition".to_string());
                    }
                    if cross {
                        surfaces.push(format!("cross-partition({xpart})"));
                    }
                    let surfaces = if surfaces.is_empty() {
                        "none".to_string()
                    } else {
                        surfaces.join(" + ")
                    };
                    if route == CyclesRoute::LivegraphModule {
                        println!(
                            "Scope: captured resolved-relative MODULE import cycles by-directory \
                             [{surfaces}] (backend=livegraph; aggregation=dirname; class={class}; \
                             freshness={freshness})"
                        );
                    } else {
                        println!(
                            "Scope: captured resolved-relative FILE import cycles [{surfaces}] \
                             (backend=livegraph; class={class}; freshness={freshness})"
                        );
                    }
                }
                match serde_json::from_value::<CyclesResponse>(result) {
                    Ok(response) => {
                        // Route to the matching renderer: FILE-import + MODULE-import each have their own
                        // (precise vocabulary, no "rmap modules deps"); SQLite keeps the generic MODULE
                        // renderer verbatim.
                        let rendered = match route {
                            CyclesRoute::LivegraphFile => response.render_human_file_import(),
                            CyclesRoute::LivegraphModule => response.render_human_module_import(),
                            // CompareModule serves the SQLite primary (generic MODULE renderer) + the
                            // compare summary line below.
                            CyclesRoute::SqliteModule | CyclesRoute::CompareModule => {
                                response.render_human()
                            }
                        };
                        println!("{}", rendered);
                        if let Some(summary) = compare_summary {
                            println!("{summary}");
                        }
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
