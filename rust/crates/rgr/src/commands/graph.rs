//! Graph query command family.
//!
//! Symbol-level and file-level graph traversal commands.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, compute_trust_overlay_for_snapshot, open_storage};

// ── Edge type parsing (graph-family-local) ───────────────────────

/// Valid edge types for `--edge-types` filter (Rust-17, SB-5).
const VALID_EDGE_TYPES: &[&str] = &["CALLS", "INSTANTIATES", "READS", "WRITES"];

/// Parse `--edge-types` from a command's argument slice.
///
/// Returns `(positional_args, edge_types)` on success, or an error
/// message on failure. If `--edge-types` is absent, returns the
/// default `["CALLS"]`.
///
/// Rules:
///   - Comma-separated, trimmed, uppercase only
///   - Unknown tokens -> usage error
///   - Empty value -> usage error
///   - Repeated `--edge-types` flag -> usage error
///   - Missing value after `--edge-types` -> usage error
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

// ── callers command ──────────────────────────────────────────────

pub fn run_callers(args: &[String]) -> ExitCode {
    let (positional, edge_types) = match parse_edge_types_flag(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
            return ExitCode::from(1);
        }
    };
    if positional.len() != 3 {
        eprintln!("usage: rmap callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&positional[0]);
    let repo_uid = &positional[1];
    let symbol_query = &positional[2];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Latest READY snapshot (same rule as trust command).
    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve symbol (exact match only).
    use repo_graph_storage::queries::SymbolResolveError;
    let target = match storage.resolve_symbol(&snapshot.snapshot_uid, symbol_query) {
        Ok(sym) => sym,
        Err(SymbolResolveError::NotFound) => {
            eprintln!("error: symbol not found: {}", symbol_query);
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Ambiguous(keys)) => {
            eprintln!("error: ambiguous symbol '{}', matches:", symbol_query);
            for k in &keys {
                eprintln!("  {}", k);
            }
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Storage(e)) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Find direct callers.
    let et_refs: Vec<&str> = edge_types.iter().map(|s| s.as_str()).collect();
    let callers = match storage.find_direct_callers(&snapshot.snapshot_uid, &target.stable_key, &et_refs)
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON to stdout (TS-compatible QueryResult envelope).
    let count = callers.len();
    let mut extra = serde_json::Map::new();
    extra.insert("target".to_string(), serde_json::to_value(&target).unwrap());

    // Trust overlay (Option A: only when repo has degradations).
    if let Some(trust) = compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "CALLS") {
        if trust.has_degradation() || !trust.caveats.is_empty() {
            extra.insert("trust".to_string(), serde_json::to_value(&trust).unwrap());
        }
    }

    let output = match build_envelope(
        &storage,
        "graph callers",
        repo_uid,
        &snapshot,
        serde_json::to_value(&callers).unwrap(),
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── callees command ──────────────────────────────────────────────

pub fn run_callees(args: &[String]) -> ExitCode {
    let (positional, edge_types) = match parse_edge_types_flag(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
            return ExitCode::from(1);
        }
    };
    if positional.len() != 3 {
        eprintln!("usage: rmap callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&positional[0]);
    let repo_uid = &positional[1];
    let symbol_query = &positional[2];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Latest READY snapshot (same rule as callers/trust).
    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve symbol (exact match only, SYMBOL kind only).
    use repo_graph_storage::queries::SymbolResolveError;
    let target = match storage.resolve_symbol(&snapshot.snapshot_uid, symbol_query) {
        Ok(sym) => sym,
        Err(SymbolResolveError::NotFound) => {
            eprintln!("error: symbol not found: {}", symbol_query);
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Ambiguous(keys)) => {
            eprintln!("error: ambiguous symbol '{}', matches:", symbol_query);
            for k in &keys {
                eprintln!("  {}", k);
            }
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Storage(e)) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Find direct callees.
    let et_refs: Vec<&str> = edge_types.iter().map(|s| s.as_str()).collect();
    let callees = match storage.find_direct_callees(&snapshot.snapshot_uid, &target.stable_key, &et_refs)
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON to stdout (TS-compatible QueryResult envelope).
    let count = callees.len();
    let mut extra = serde_json::Map::new();
    extra.insert("target".to_string(), serde_json::to_value(&target).unwrap());

    // Trust overlay (Option A: only when repo has degradations).
    if let Some(trust) = compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "CALLS") {
        if trust.has_degradation() || !trust.caveats.is_empty() {
            extra.insert("trust".to_string(), serde_json::to_value(&trust).unwrap());
        }
    }

    let output = match build_envelope(
        &storage,
        "graph callees",
        repo_uid,
        &snapshot,
        serde_json::to_value(&callees).unwrap(),
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── path command ─────────────────────────────────────────────────

pub fn run_path(args: &[String]) -> ExitCode {
    if args.len() != 4 {
        eprintln!("usage: rmap path <db_path> <repo_uid> <from> <to>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];
    let from_query = &args[2];
    let to_query = &args[3];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve both endpoints (exact SYMBOL resolution only).
    use repo_graph_storage::queries::SymbolResolveError;

    let from_sym = match storage.resolve_symbol(&snapshot.snapshot_uid, from_query) {
        Ok(sym) => sym,
        Err(SymbolResolveError::NotFound) => {
            eprintln!("error: symbol not found: {}", from_query);
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Ambiguous(keys)) => {
            eprintln!("error: ambiguous symbol '{}', matches:", from_query);
            for k in &keys {
                eprintln!("  {}", k);
            }
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Storage(e)) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let to_sym = match storage.resolve_symbol(&snapshot.snapshot_uid, to_query) {
        Ok(sym) => sym,
        Err(SymbolResolveError::NotFound) => {
            eprintln!("error: symbol not found: {}", to_query);
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Ambiguous(keys)) => {
            eprintln!("error: ambiguous symbol '{}', matches:", to_query);
            for k in &keys {
                eprintln!("  {}", k);
            }
            return ExitCode::from(2);
        }
        Err(SymbolResolveError::Storage(e)) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Shortest path: CALLS + IMPORTS, max depth 8.
    let path_result = match storage.find_shortest_path(
        &snapshot.snapshot_uid,
        &from_sym.stable_key,
        &to_sym.stable_key,
        8,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON to stdout (TS-compatible QueryResult envelope).
    // TS wraps the single PathResult in a 1-element array.
    let count = if path_result.found { 1 } else { 0 };

    // Trust overlay (Option A: only when repo has degradations).
    let mut extra = serde_json::Map::new();
    if let Some(trust) =
        compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "CALLS+IMPORTS")
    {
        if trust.has_degradation() || !trust.caveats.is_empty() {
            extra.insert("trust".to_string(), serde_json::to_value(&trust).unwrap());
        }
    }

    let output = match build_envelope(
        &storage,
        "graph path",
        repo_uid,
        &snapshot,
        serde_json::json!([path_result]),
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── imports command ──────────────────────────────────────────────

pub fn run_imports(args: &[String]) -> ExitCode {
    if args.len() != 3 {
        eprintln!("usage: rmap imports <db_path> <repo_uid> <file_path>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];
    let file_path = &args[2];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Construct the FILE stable key: {repo_uid}:{file_path}:FILE
    // This matches the TS resolution path (graph.ts:175).
    let file_stable_key = format!("{}:{}:FILE", repo_uid, file_path);

    // Verify the FILE node exists.
    match storage.node_exists(&snapshot.snapshot_uid, &file_stable_key) {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("error: file not found: {}", file_path);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    }

    // Dedicated imports query (TS-compatible NodeResult wire format).
    let imports = match storage.find_imports(&snapshot.snapshot_uid, &file_stable_key) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON to stdout (TS-compatible QueryResult envelope).
    let count = imports.len();
    let output = match build_envelope(
        &storage,
        "graph imports",
        repo_uid,
        &snapshot,
        serde_json::to_value(&imports).unwrap(),
        count,
        serde_json::Map::new(),
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── cycles command ───────────────────────────────────────────────

pub fn run_cycles(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap cycles <db_path> <repo_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Latest READY snapshot (same rule as all read commands).
    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Module-level cycles (TS default).
    let cycles = match storage.find_cycles(&snapshot.snapshot_uid, "module") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON to stdout (TS-compatible QueryResult envelope).
    let count = cycles.len();
    let output = match build_envelope(
        &storage,
        "graph cycles",
        repo_uid,
        &snapshot,
        serde_json::to_value(&cycles).unwrap(),
        count,
        serde_json::Map::new(),
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── stats command ────────────────────────────────────────────────

pub fn run_stats(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap stats <db_path> <repo_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let stats = match storage.compute_module_stats(&snapshot.snapshot_uid) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON to stdout (TS-compatible QueryResult envelope).
    let count = stats.len();
    let output = match build_envelope(
        &storage,
        "graph stats",
        repo_uid,
        &snapshot,
        serde_json::to_value(&stats).unwrap(),
        count,
        serde_json::Map::new(),
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
