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
//! rmap callers <symbol> [--edge-types <types>] [--include-inferred]
//! rmap callees <symbol> [--edge-types <types>] [--include-inferred]
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
            return Err(ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR));
        }
    };

    Ok(client)
}

/// Handle daemon error response with REG-1 hint for repo not found.
///
/// `ambiguity_listing` (JAVA-SYMBOL-AMBIGUITY-HINT-1): for `callers`/`callees`/`path` — the only
/// commands whose daemon handlers return `AmbiguousSymbol` — renders the error's `data` with
/// `render_ambiguous_matches` and the failing command's `cursor` (`Some((prefix, suffix))` when
/// the command can be formed, `None` when it cannot), so each printed cursor runs as printed.
/// `None` for every other command (no listing).
fn handle_daemon_error(
    err: DaemonClientError,
    ambiguity_listing: Option<&dyn Fn(&serde_json::Value) -> String>,
) -> ExitCode {
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
                // Render structured ambiguity data: the candidates with runnable stable-key cursors.
                eprintln!("error: {}", message);
                if let (Some(data), Some(render_listing)) = (data, ambiguity_listing) {
                    eprint!("{}", render_listing(&data));
                }
            } else {
                eprintln!("error: {}: {}", code, message);
                // EMBED-SEED-IMPL-1 (spec §8, Group B): a `symbol not found` error may
                // carry the additive semantic tier under `data` (candidates + hint) —
                // render it to stderr, the SAME idiom used above for AmbiguousSymbol's
                // `matches`. `None` ⇒ no seed keys ⇒ today's error is unchanged.
                if let Some(rendered) =
                    crate::presentation::seed::render_symbol_not_found_semantic(data.as_ref())
                {
                    eprint!("{}", rendered);
                }
            }
            ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
        }
        e => {
            eprintln!("error: {}", e);
            ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
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

/// PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11): remove `--include-inferred` from the args and
/// report whether it was given. Default: the certain rows only, with the inferred remainder
/// stated beside them.
fn extract_include_inferred_flag(args: Vec<String>) -> (Vec<String>, bool) {
    let mut found = false;
    let out = args
        .into_iter()
        .filter(|a| {
            if a == "--include-inferred" {
                found = true;
                false
            } else {
                true
            }
        })
        .collect();
    (out, found)
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

/// TEST-EDGE-SCOPE-1B (RG-REQ-004-L12, D-TESB-05): the import-partition flags of a command. The
/// default (neither) answers certain imports from production files; each flag widens the view and
/// is sent to the daemon as the boolean parameter of the same name.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PartitionFlags {
    pub(crate) include_tests: bool,
    pub(crate) include_inferred: bool,
}

impl PartitionFlags {
    /// Whether any flag widens the view.
    pub(crate) fn any(&self) -> bool {
        self.include_tests || self.include_inferred
    }

    /// Add the given flags to the daemon parameters (an absent flag is not sent, so a request
    /// without flags is the request it was before the partition).
    pub(crate) fn add_to_params(&self, params: &mut serde_json::Value) {
        if self.include_tests {
            params["include_tests"] = serde_json::Value::Bool(true);
        }
        if self.include_inferred {
            params["include_inferred"] = serde_json::Value::Bool(true);
        }
    }
}

/// Remove `--include-inferred` and, when the command partitions by test status
/// (`accept_include_tests`), `--include-tests` from the args. A flag the command does not accept
/// stays in the args and is refused as an unknown flag by the caller.
pub(crate) fn extract_partition_flags(
    args: Vec<String>,
    accept_include_tests: bool,
) -> (Vec<String>, PartitionFlags) {
    let mut flags = PartitionFlags::default();
    let out = args
        .into_iter()
        .filter(|a| match a.as_str() {
            "--include-inferred" => {
                flags.include_inferred = true;
                false
            }
            "--include-tests" if accept_include_tests => {
                flags.include_tests = true;
                false
            }
            _ => true,
        })
        .collect();
    (out, flags)
}

/// D-TESB-06: the explicit `--engine livegraph|compare` routes serve the unpartitioned import
/// graph, so a partition flag with them is a usage error — never silently ignored.
fn refuse_partition_flags_with_explicit_engine(
    flags: PartitionFlags,
    engine: &str,
) -> Result<(), String> {
    if flags.any() && matches!(engine, "livegraph" | "compare") {
        return Err(format!(
            "error: --include-tests / --include-inferred are not supported with --engine {engine} \
             (it serves the unpartitioned import graph); omit --engine to use them"
        ));
    }
    Ok(())
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
            ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR)
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
                return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
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
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
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
                ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
            }
        },
        Err(e) => handle_daemon_error(e, None),
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
                return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
            }
        }
    }
    let repo = match repo {
        Some(r) => r,
        None => {
            eprintln!(
                "usage: rmap dev cycle-completeness-audit --repo <repo> [--include-fixtures]"
            );
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
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
                ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
            }
        },
        Err(e) => handle_daemon_error(e, None),
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
                return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
            }
        }
    }
    let (repo, partition_id, scip, source_root) = match (repo, partition_id, scip, source_root) {
        (Some(r), Some(p), Some(s), Some(sr)) => (r, p, s, sr),
        _ => {
            eprintln!("usage: rmap dev livegraph-preload --repo <repo> --partition-id <id> --scip <index.scip> --source-root <source-root>");
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
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
                ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
            }
        },
        Err(e) => handle_daemon_error(e, None),
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
    let (filtered_args, include_inferred) = extract_include_inferred_flag(filtered_args);

    let (positional, edge_types) = match parse_edge_types_flag(&filtered_args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callers <symbol> [--edge-types <types>] [--include-inferred] [--engine auto|sqlite|livegraph|compare] [--json]");
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
        }
    };

    // REG-1: one positional arg (symbol), repo from cwd
    if positional.len() != 1 {
        eprintln!(
            "usage: rmap callers <symbol> [--edge-types <types>] [--include-inferred] [--json]"
        );
        return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
    }

    let symbol = &positional[0];

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
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
        "include_inferred": include_inferred,
    });

    match client.request("callers", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
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
                // RECON-M-R3b: render the reference tier (if present) from the RAW value before the
                // strip consumes `result`; append it after the call rows (additive, W-BOTH only —
                // absent, this is byte-identical to the pre-M-R3b render).
                let reference_section =
                    crate::presentation::witnesses::render_reference_tier_section(
                        result.get("references"),
                    );
                if let Some(obj) = result.as_object_mut() {
                    obj.remove("livegraph_compare");
                    obj.remove("livegraph_compare_sidecar");
                    // QUERY-MIGRATION-CLI-1: backend_used/fallback_reason are JSON-only metadata; strip
                    // them so the human render is unaffected (no new trust metadata in human output).
                    obj.remove("backend_used");
                    obj.remove("fallback_reason");
                    obj.remove("references"); // rendered above; keep the struct render unaffected
                }
                use crate::presentation::graph_edges::CallersResponse;
                match serde_json::from_value::<CallersResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human_for_repo(Some(&repo_path)));
                        print!("{}", reference_section);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse callers response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(
            e,
            Some(&|data| {
                super::ambiguous_matches::render_ambiguous_matches(Some(("rmap callers", "")), data)
            }),
        ),
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
    let (filtered_args, include_inferred) = extract_include_inferred_flag(filtered_args);

    let (positional, edge_types) = match parse_edge_types_flag(&filtered_args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("usage: rmap callees <symbol> [--edge-types <types>] [--include-inferred] [--engine auto|sqlite|livegraph|compare] [--json]");
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
        }
    };

    // REG-1: one positional arg (symbol), repo from cwd
    if positional.len() != 1 {
        eprintln!(
            "usage: rmap callees <symbol> [--edge-types <types>] [--include-inferred] [--json]"
        );
        return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
    }

    let symbol = &positional[0];

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
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
        "include_inferred": include_inferred,
    });

    match client.request("callees", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
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
                // RECON-M-R3b: render the reference tier (if present) before the strip; append it
                // after the call rows (additive, W-BOTH only — absent = byte-identical).
                let reference_section =
                    crate::presentation::witnesses::render_reference_tier_section(
                        result.get("references"),
                    );
                if let Some(obj) = result.as_object_mut() {
                    obj.remove("livegraph_compare");
                    obj.remove("livegraph_compare_sidecar");
                    // QUERY-MIGRATION-CLI-1: strip JSON-only metadata before the human render.
                    obj.remove("backend_used");
                    obj.remove("fallback_reason");
                    obj.remove("references"); // rendered above; keep the struct render unaffected
                }
                use crate::presentation::graph_edges::CalleesResponse;
                match serde_json::from_value::<CalleesResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human_for_repo(Some(&repo_path)));
                        print!("{}", reference_section);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse callees response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(
            e,
            Some(&|data| {
                super::ambiguous_matches::render_ambiguous_matches(Some(("rmap callees", "")), data)
            }),
        ),
    }
}

// ── path command (REG-1 + CLI-OUT-3) ─────────────────────────────────
//
// `rmap path <from> <to> [--json]`
//
// Human mode (default): plain text showing route between symbols.
// Machine mode (--json): full envelope.

/// The parsed `rmap path` arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathArgs {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) engine: String,
    pub(crate) json_mode: bool,
    /// D-TESB-15: walk inferred call/import hops too.
    pub(crate) include_inferred: bool,
}

const PATH_USAGE: &str =
    "usage: rmap path <from> <to> [--include-inferred] [--engine auto|sqlite|livegraph|compare] [--json]";

/// Parse `rmap path` arguments (pure). `Err` carries the stderr text of a usage error (exit 1).
pub(crate) fn parse_path_args(args: &[String]) -> Result<PathArgs, String> {
    // PATH-LIVEGRAPH-DEFAULT-1: extract --engine FIRST; `path` now DEFAULTS to `auto` (serve LiveGraph
    // when Exact/Fresh/complete, else labelled SQLite fallback — the daemon decides). `--engine sqlite`
    // forces SQLite, `--engine livegraph`/`compare` stay explicit. Then filter --json from the positionals.
    let (args, engine) = extract_engine_flag(args.to_vec());
    let (args, flags) = extract_partition_flags(args, false);
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
        return Err(PATH_USAGE.to_string());
    }
    refuse_partition_flags_with_explicit_engine(flags, &engine)?;
    Ok(PathArgs {
        from: positional[0].clone(),
        to: positional[1].clone(),
        engine,
        json_mode,
        include_inferred: flags.include_inferred,
    })
}

pub fn run_path(args: &[String]) -> ExitCode {
    let parsed = match parse_path_args(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
        }
    };
    let json_mode = parsed.json_mode;
    let engine = parsed.engine.clone();
    let from_query = &parsed.from;
    let to_query = &parsed.to;

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
        }
    };

    let mut client = match create_daemon_client("path") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let mut params = serde_json::json!({
        "repo": repo_path,
        "from": from_query,
        "to": to_query,
        "engine": engine,
    });
    if parsed.include_inferred {
        params["include_inferred"] = serde_json::Value::Bool(true);
    }

    match client.request("path", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
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
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse path response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(
            e,
            Some(&|data| {
                let query = data.get("query").and_then(|q| q.as_str());
                let affixes =
                    super::ambiguous_matches::path_cursor_affixes(from_query, to_query, query);
                super::ambiguous_matches::render_ambiguous_matches(
                    affixes.as_ref().map(|(p, s)| (p.as_str(), s.as_str())),
                    data,
                )
            }),
        ),
    }
}

// ── imports command (REG-1 + CLI-OUT-3) ──────────────────────────────
//
// `rmap imports <file_path> [--json]`
//
// Human mode (default): plain text showing file dependencies.
// Machine mode (--json): full envelope.

/// The parsed `rmap imports` arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportsArgs {
    pub(crate) engine: String,
    pub(crate) file: Option<String>,
    pub(crate) json_mode: bool,
    /// RG-REQ-002-L11: list inferred imports too (the default lists certain ones and counts the
    /// rest). Test status never filters this per-file answer (D-TESB-READERS-1 §1).
    pub(crate) include_inferred: bool,
}

const IMPORTS_USAGE: &str =
    "usage: rmap imports [<file>] [--include-inferred] [--engine auto|sqlite|livegraph|compare] [--json]";

/// Parse and validate `rmap imports` arguments (pure). `Err` carries the stderr text (exit 1).
pub(crate) fn parse_imports_args(args: &[String]) -> Result<ImportsArgs, String> {
    // IMPORTS-LIVEGRAPH-DEFAULT-1 (D2=B): extract --engine FIRST. Absent == `auto` -- the LiveGraph-first
    // default (per-call no-loss compare + labelled SQLite fallback). `--engine sqlite` is the explicit escape
    // hatch (unchanged listing); `--engine livegraph|compare` are the read-model / compare surfaces.
    let (args, engine) = extract_engine_flag(args.to_vec());
    let (args, flags) = extract_partition_flags(args, false);

    // Parse --json + the optional positional <file> from the remaining args.
    let mut json_mode = false;
    let mut positional: Vec<String> = Vec::new();
    for a in &args {
        match a.as_str() {
            "--json" => json_mode = true,
            flag if flag.starts_with("--") => {
                return Err(format!("error: unknown flag: {flag}\n{IMPORTS_USAGE}"));
            }
            other => positional.push(other.to_string()),
        }
    }

    // Validate the engine/arg combination (D6: sqlite REQUIRES <file>; livegraph file OPTIONAL
    // -> repo-wide).
    match engine.as_str() {
        // auto (DEFAULT, LiveGraph-first) + sqlite (explicit escape hatch) are both single-file (file
        // REQUIRED). The daemon routes on `engine`.
        "auto" | "sqlite" if positional.len() != 1 => {
            return Err(format!(
                "error: imports requires exactly one <file>\n{IMPORTS_USAGE}"
            ));
        }
        "auto" | "sqlite" => {}
        "livegraph" if positional.len() > 1 => {
            return Err(format!(
                "error: at most one <file> (omit for a repo-wide view)\n{IMPORTS_USAGE}"
            ));
        }
        // compare: WITH file -> per-file (READINESS-1); NO file -> repo-wide readiness aggregate
        // (REPOWIDE-1 D6). At most one <file>.
        "compare" if positional.len() > 1 => {
            return Err(format!(
                "error: at most one <file> (omit for the repo-wide readiness aggregate)\n{IMPORTS_USAGE}"
            ));
        }
        "livegraph" | "compare" => {}
        other => {
            return Err(format!(
                "error: unknown --engine '{other}' (supported: sqlite, livegraph, compare)"
            ));
        }
    }
    refuse_partition_flags_with_explicit_engine(flags, &engine)?;
    Ok(ImportsArgs {
        engine,
        file: positional.into_iter().next(),
        json_mode,
        include_inferred: flags.include_inferred,
    })
}

pub fn run_imports(args: &[String]) -> ExitCode {
    let parsed = match parse_imports_args(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
        }
    };
    let engine = parsed.engine.as_str();
    let json_mode = parsed.json_mode;
    let positional: Vec<String> = parsed.file.iter().cloned().collect();

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
        }
    };

    let mut params = serde_json::json!({ "repo": repo_path, "engine": engine });
    if let Some(file) = &parsed.file {
        params["file"] = serde_json::Value::String(file.clone());
    }
    if parsed.include_inferred {
        params["include_inferred"] = serde_json::Value::Bool(true);
    }

    let mut client = match create_daemon_client("imports") {
        Ok(c) => c,
        Err(code) => return code,
    };

    match client.request("imports", Some(params)) {
        Ok(result) => {
            if json_mode {
                // D-TESB-17: the JSON consumer boundary marks unreadable partition evidence.
                let mut result = result;
                if let Err(e) = crate::presentation::import_partition::mark_partition_evidence(
                    &mut result,
                    crate::presentation::import_partition::JsonSurface::Imports,
                ) {
                    eprintln!("error: {e}");
                    return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
                }
                // Machine mode: the full envelope (the AUTHORITATIVE, complete evidence for livegraph, D4).
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            } else if engine == "livegraph" {
                use crate::presentation::imports::LivegraphImportsResponse;
                match serde_json::from_value::<LivegraphImportsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse imports response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            } else if engine == "compare" && positional.is_empty() {
                // IMPORTS-LIVEGRAPH-REPOWIDE-READINESS-1 (D6): the repo-wide readiness aggregate.
                use crate::presentation::imports::ImportsReadinessReport;
                match serde_json::from_value::<ImportsReadinessReport>(result) {
                    Ok(report) => {
                        print!("{}", report.render_human());
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse imports response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            } else if engine == "compare" {
                // IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1 (D6): per-file SQLite listing PRIMARY + the summary.
                use crate::presentation::imports::ImportsCompareResponse;
                match serde_json::from_value::<ImportsCompareResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse imports response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            } else {
                // Human mode (auto | sqlite): the SQLite-compatible listing. For `auto`, STRIP the JSON-only
                // backend_used / fallback_reason / comparison (the QUERY-MIGRATION-CLI-1 precedent) so the human
                // render is byte-unchanged whichever backend served (D3).
                use crate::presentation::imports::ImportsResponse;
                let mut result = result;
                if let Some(obj) = result.as_object_mut() {
                    obj.remove("backend_used");
                    obj.remove("fallback_reason");
                    obj.remove("comparison");
                }
                match serde_json::from_value::<ImportsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse imports response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e, None),
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
pub(crate) enum CyclesRoute {
    /// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: the DEFAULT (`auto`, no flags) -- the cert-gated LiveGraph-first
    /// MODULE-cycle answer (the daemon serves LiveGraph on a GREEN repo cert, else a labelled SQLite fallback).
    AutoModule,
    /// Forced SQLite MODULE-import cycles (the explicit `--engine sqlite [--kind module-import]` escape hatch).
    SqliteModule,
    /// LiveGraph captured FILE-import cycles (`--engine livegraph --kind file-import`).
    LivegraphFile,
    /// LiveGraph directory-aggregated MODULE-import cycles (`--engine livegraph --kind module-import`).
    LivegraphModule,
    /// SQLite MODULE cycles PRIMARY + a LiveGraph-vs-SQLite compare report
    /// (`--engine compare --kind module-import`).
    CompareModule,
}

const CYCLES_USAGE: &str = "usage: rmap cycles [--include-tests] [--include-inferred] [--engine auto|sqlite|livegraph|compare] [--kind file-import|module-import] [--json]";

/// Parse and validate `rmap cycles` arguments (pure): the route, `--json`, and the partition
/// flags (TEST-EDGE-SCOPE-1B). `Err` carries the stderr text of a usage error (exit 1).
pub(crate) fn parse_cycles_args(
    args: &[String],
) -> Result<(CyclesRoute, bool, PartitionFlags), String> {
    // CYCLES-LIVEGRAPH-CLI-1: extract --engine + --kind FIRST. Default (no flags) = SQLite MODULE-import
    // cycles (unchanged). `--engine livegraph --kind file-import` = LiveGraph captured FILE import cycles
    // (a DIFFERENT graph; NO SQLite fallback). Then parse --json from the remaining positionals.
    let (args, engine_raw) = extract_engine_flag(args.to_vec());
    let (args, kind) = extract_kind_flag(args);
    let (args, flags) = extract_partition_flags(args, true);
    // CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: absent engine == `auto` -- the cert-gated LiveGraph-first default
    // (the daemon serves LiveGraph module cycles when a GREEN repo no-loss certificate holds, else a labelled
    // SQLite fallback -- BYTE-IDENTICAL either way). `--engine sqlite` forces the SQLite escape hatch
    // (UNCHANGED); `--engine livegraph|compare` stay the explicit read-model / compare surfaces.
    let engine = engine_raw.as_str();

    let mut json_mode = false;
    for arg in &args {
        match arg.as_str() {
            "--json" => json_mode = true,
            flag if flag.starts_with("--") => {
                return Err(format!("error: unknown flag: {flag}\n{CYCLES_USAGE}"));
            }
            other => {
                return Err(format!(
                    "error: unexpected argument: {other}\n{CYCLES_USAGE}"
                ));
            }
        }
    }

    // Validate the engine/kind combination (D2/D6/D7): reject invalid combos with a clear error rather
    // than silently computing a different graph.
    let route = match (engine, kind.as_str()) {
        ("auto", "") => CyclesRoute::AutoModule, // the DEFAULT: cert-gated LiveGraph-first (FASTPATH-1)
        ("auto", "module-import") => CyclesRoute::AutoModule, // explicit spelling of the default
        ("sqlite", "") => CyclesRoute::SqliteModule, // forced SQLite escape hatch
        ("sqlite", "module-import") => CyclesRoute::SqliteModule, // D6: explicit spelling of forced SQLite
        ("livegraph", "file-import") => CyclesRoute::LivegraphFile,
        ("livegraph", "module-import") => CyclesRoute::LivegraphModule,
        ("livegraph", _) => {
            return Err(
                "error: --engine livegraph requires --kind file-import or module-import"
                    .to_string(),
            );
        }
        ("sqlite", "file-import") => {
            return Err("error: SQLite does not answer captured FILE import cycles; use --engine livegraph --kind file-import".to_string());
        }
        ("compare", "module-import") => CyclesRoute::CompareModule,
        ("compare", "file-import") => {
            return Err("error: --engine compare --kind file-import is not supported (FILE-import has no SQLite peer graph); use --kind module-import".to_string());
        }
        ("compare", _) => {
            return Err("error: --engine compare requires --kind module-import".to_string());
        }
        (_, "file-import") => {
            return Err("error: --kind file-import requires --engine livegraph".to_string());
        }
        (e, "") => {
            return Err(format!(
                "error: unknown --engine '{e}' (supported: auto, sqlite, livegraph, compare)"
            ));
        }
        (_, k) => {
            return Err(format!(
                "error: unknown --kind '{k}' (supported: file-import, module-import)"
            ));
        }
    };
    refuse_partition_flags_with_explicit_engine(flags, engine)?;
    Ok((route, json_mode, flags))
}

pub fn run_cycles(args: &[String]) -> ExitCode {
    let (route, json_mode, flags) = match parse_cycles_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
        }
    };

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
        }
    };

    let mut client = match create_daemon_client("cycles") {
        Ok(c) => c,
        Err(code) => return code,
    };

    let params = match route {
        // FASTPATH-1: the default sends `engine:"auto"`; explicit `--engine sqlite` sends `"sqlite"` -- so the
        // daemon distinguishes the cert-gated fastpath from the forced SQLite escape hatch (rule 7).
        CyclesRoute::AutoModule => serde_json::json!({ "repo": repo_path, "engine": "auto" }),
        CyclesRoute::SqliteModule => serde_json::json!({ "repo": repo_path, "engine": "sqlite" }),
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
    // TEST-EDGE-SCOPE-1B: the view's flags (refused above with the explicit engines).
    let mut params = params;
    flags.add_to_params(&mut params);

    match client.request("cycles", Some(params)) {
        Ok(result) => {
            if json_mode {
                // D-TESB-17: the JSON consumer boundary marks unreadable partition evidence.
                let mut result = result;
                if let Err(e) = crate::presentation::import_partition::mark_partition_evidence(
                    &mut result,
                    crate::presentation::import_partition::JsonSurface::Cycles,
                ) {
                    eprintln!("error: {e}");
                    return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
                }
                // Machine mode: print full envelope (includes scope/backend_used/answer_class/freshness/
                // missing_partitions/degradation_reasons for the LiveGraph path; D5).
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
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
                            // The default (Auto), forced SQLite, and Compare all serve the generic MODULE
                            // renderer (byte-identical canonical cycles; backend_used/fallback_reason are
                            // ignored by CyclesResponse). Compare adds its summary line below.
                            CyclesRoute::AutoModule | CyclesRoute::SqliteModule => {
                                response.render_human()
                            }
                            // TEST-EDGE-SCOPE-1B (D-TESB-06): compare serves the unpartitioned
                            // persisted graph and says so.
                            CyclesRoute::CompareModule => response.render_human_explicit_engine(),
                        };
                        println!("{}", rendered);
                        if let Some(summary) = compare_summary {
                            println!("{summary}");
                        }
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse cycles response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e, None),
    }
}

// ── stats command (REG-1, CLI-OUT-2C) ────────────────────────────────

pub fn run_stats(args: &[String]) -> ExitCode {
    // STATS-LIVEGRAPH-IMPL-1: extract --engine FIRST. Absent == `auto` -- the cert-gated LiveGraph-first
    // default (the daemon serves LiveGraph module stats when a GREEN repo no-loss certificate holds at the
    // current fingerprint, else a labelled SQLite fallback -- BYTE-IDENTICAL human output either way).
    // `--engine sqlite` forces the SQLite escape hatch (UNCHANGED); `--engine livegraph|compare` are the
    // diagnostic surfaces (forced LiveGraph serve / SQLite-primary + field-exact compare report).
    let (args, engine_raw) = extract_engine_flag(args.to_vec());
    let engine = engine_raw.as_str();
    let usage = "usage: rmap stats [--engine auto|sqlite|livegraph|compare] [--json]";

    // ── Parse remaining args ───────────────────────────────────────────
    let mut json_mode = false;
    for arg in &args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("{usage}");
                return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
            }
            other => {
                eprintln!("error: unexpected argument: {}", other);
                eprintln!("{usage}");
                return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
            }
        }
    }

    // Validate the engine value (reject unknown rather than silently defaulting).
    if !matches!(engine, "auto" | "sqlite" | "livegraph" | "compare") {
        eprintln!(
            "error: unknown --engine '{engine}' (supported: auto, sqlite, livegraph, compare)"
        );
        eprintln!("{usage}");
        return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
    }

    let repo_path = match resolve_repo_from_cwd() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
        }
    };

    let mut client = match create_daemon_client("stats") {
        Ok(c) => c,
        Err(code) => return code,
    };

    // The default sends `engine:"auto"`; explicit `--engine sqlite` sends `"sqlite"` -- so the daemon
    // distinguishes the cert-gated fastpath from the forced SQLite escape hatch (rule 7).
    let params = serde_json::json!({ "repo": repo_path, "engine": engine });

    match client.request("stats", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope (includes backend_used/fallback_reason for the
                // auto/livegraph paths; the livegraph_stats_compare report for compare).
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            } else {
                use crate::presentation::stats::StatsResponse;
                // Compare (STATS-LIVEGRAPH-IMPL-1): capture the diagnostic summary BEFORE `result` is
                // consumed by from_value. The PRIMARY answer is SQLite (render_human, byte-identical);
                // the compare metadata rides alongside as one summary line.
                let compare_summary: Option<String> = if engine == "compare" {
                    let cmp = result.get("livegraph_stats_compare");
                    let n = |k: &str| {
                        cmp.and_then(|c| c.get(k))
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0)
                    };
                    let u = |k: &str| {
                        cmp.and_then(|c| c.get(k))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    };
                    let lg_class = cmp
                        .and_then(|c| c.get("livegraph_class"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let is_exact = cmp
                        .and_then(|c| c.get("is_exact"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let sidecar = result
                        .get("livegraph_stats_compare_sidecar")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<none>");
                    Some(format!(
                        "LiveGraph module-stats compare: {} matched, {} missing, {} extra, {} field-mismatch; \
                         livegraph_count={} class={lg_class}; is_exact={is_exact}; sidecar={sidecar}",
                        u("matched"),
                        n("missing_in_livegraph"),
                        n("extra_in_livegraph"),
                        n("field_mismatches"),
                        u("livegraph_count"),
                    ))
                } else {
                    None
                };
                // Human mode: parse and render (CLI-OUT-2C). The renderer is UNTOUCHED (byte-preserving);
                // backend_used/fallback_reason/livegraph_stats_compare are additive JSON-only fields the
                // StatsResponse ignores.
                match serde_json::from_value::<StatsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        if let Some(summary) = compare_summary {
                            println!("{summary}");
                        }
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse stats response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => handle_daemon_error(e, None),
    }
}

#[cfg(test)]
mod partition_flag_tests {
    //! TEST-EDGE-SCOPE-1B (D-TESB-05/06/15): the partition flags on `cycles`, `imports`, `path`.
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn cycles_partition_flag_with_livegraph_engine_is_a_usage_error() {
        // Default route with each flag: accepted and sent.
        let (route, json, flags) = parse_cycles_args(&argv("--include-tests --json")).unwrap();
        assert_eq!(route, CyclesRoute::AutoModule);
        assert!(json && flags.include_tests && !flags.include_inferred);
        let (route, _, flags) =
            parse_cycles_args(&argv("--engine sqlite --include-inferred --include-tests")).unwrap();
        assert_eq!(route, CyclesRoute::SqliteModule);
        assert_eq!(
            flags,
            PartitionFlags {
                include_tests: true,
                include_inferred: true
            }
        );
        let mut params = serde_json::json!({"repo": "/r", "engine": "sqlite"});
        flags.add_to_params(&mut params);
        assert_eq!(params["include_tests"], true);
        assert_eq!(params["include_inferred"], true);
        // The explicit engines serve the unpartitioned graph: a flag with them is refused.
        for bad in [
            "--engine livegraph --kind module-import --include-tests",
            "--engine livegraph --kind file-import --include-inferred",
            "--engine compare --kind module-import --include-tests",
        ] {
            let err = parse_cycles_args(&argv(bad)).unwrap_err();
            assert!(err.contains("not supported with --engine"), "{bad}: {err}");
        }
        // Without a flag the explicit engines parse as before, and no flag is sent by default.
        assert_eq!(
            parse_cycles_args(&argv("--engine livegraph --kind module-import"))
                .unwrap()
                .0,
            CyclesRoute::LivegraphModule
        );
        let (_, _, none) = parse_cycles_args(&argv("")).unwrap();
        let mut params = serde_json::json!({"repo": "/r", "engine": "auto"});
        none.add_to_params(&mut params);
        assert_eq!(params, serde_json::json!({"repo": "/r", "engine": "auto"}));
    }

    #[test]
    fn imports_accepts_include_inferred() {
        let a = parse_imports_args(&argv(
            "tests/kafkatest/services/streams.py --include-inferred",
        ))
        .unwrap();
        assert!(a.include_inferred);
        assert_eq!(
            a.file.as_deref(),
            Some("tests/kafkatest/services/streams.py")
        );
        assert_eq!(a.engine, "auto");
        assert!(!parse_imports_args(&argv("a.py")).unwrap().include_inferred);
        // Test status never filters the per-file answer: `--include-tests` is not an imports flag.
        assert!(parse_imports_args(&argv("a.py --include-tests"))
            .unwrap_err()
            .contains("unknown flag: --include-tests"));
        for bad in [
            "a.py --engine livegraph --include-inferred",
            "a.py --engine compare --include-inferred",
        ] {
            assert!(parse_imports_args(&argv(bad))
                .unwrap_err()
                .contains("not supported with --engine"));
        }
        // The engine/file rules are unchanged.
        assert!(parse_imports_args(&argv("--engine sqlite")).is_err());
        assert!(parse_imports_args(&argv("--engine livegraph")).is_ok());
    }

    #[test]
    fn path_accepts_include_inferred() {
        let p = parse_path_args(&argv("A.f B.g --include-inferred --json")).unwrap();
        assert_eq!((p.from.as_str(), p.to.as_str()), ("A.f", "B.g"));
        assert!(p.include_inferred && p.json_mode);
        assert_eq!(p.engine, "auto");
        let p = parse_path_args(&argv("--include-inferred A.f B.g --engine sqlite")).unwrap();
        assert!(p.include_inferred);
        assert_eq!(p.engine, "sqlite");
        assert!(!parse_path_args(&argv("A.f B.g")).unwrap().include_inferred);
        assert!(parse_path_args(&argv("A.f --include-inferred")).is_err());
    }

    #[test]
    fn path_partition_flag_with_livegraph_engine_is_a_usage_error() {
        for bad in [
            "A.f B.g --engine livegraph --include-inferred",
            "A.f B.g --engine compare --include-inferred",
        ] {
            assert!(parse_path_args(&argv(bad))
                .unwrap_err()
                .contains("not supported with --engine"));
        }
        assert!(parse_path_args(&argv("A.f B.g --engine livegraph")).is_ok());
    }
}
