//! Violations commands.
//!
//! Contains:
//! - `run_violations` — unified violations command (legacy + discovered-module)
//! - `run_modules_violations` — discovered-module violations only (REG-1)
//!
//! RS-MG-12b: Boundary violation diagnostics.
//! CLI-OUT-4: Human-readable output with `--json` for machine mode (modules violations).
//! CLI-OUT-7: Human-readable output with `--json` for machine mode (top-level violations).
//!
//! # Contract Split
//!
//! - `run_violations`: Legacy direct-storage contract (db_path, repo_uid)
//! - `run_modules_violations`: REG-1 daemon contract (cwd auto-discovery)
//!
//! # Presentation Split
//!
//! - `run_violations`: Uses `presentation::violations` (CLI-OUT-7)
//! - `run_modules_violations`: Uses `presentation::modules_violations` (CLI-OUT-4)
//!
//! # Boundary rules
//!
//! This module owns violations command behavior:
//! - command handlers
//! - argument parsing for violations command
//! - mode switching (human vs --json)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in daemon via module-queries)
//! - boundary evaluation logic (lives in daemon via classification)
//! - human output rendering (lives in `presentation::*`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::{evaluate_violations_from_facts, load_module_graph_facts};
use crate::cli::{build_envelope, open_storage};
use crate::daemon_client::{daemon_unavailable_message, DaemonClient};

// ── unified violations command ───────────────────────────────────
//
// NOTE: This command remains legacy (db_path + repo_uid) for now.
// It will be migrated separately as a top-level command.

pub fn run_violations(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--json]
    let mut positional: Vec<&String> = Vec::new();
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with('-') => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap violations <db_path> <repo_uid> [--json]");
                return ExitCode::from(1);
            }
            _ => {
                positional.push(arg);
            }
        }
    }

    if positional.len() != 2 {
        eprintln!("usage: rmap violations <db_path> <repo_uid> [--json]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(positional[0]);
    let repo_uid = positional[1].as_str();

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

    // ── Section 1: Declared boundary violations (legacy) ─────────

    // Load active boundary declarations (directory-level MODULE targets).
    let boundaries = match storage.get_active_boundary_declarations(repo_uid) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Deduplicate rules by (boundary_module, forbids).
    use std::collections::HashMap;
    let mut rule_map: HashMap<(String, String), (String, String, Option<String>)> = HashMap::new();
    for decl in &boundaries {
        let key = (decl.boundary_module.clone(), decl.forbids.clone());
        rule_map.entry(key).or_insert_with(|| {
            (
                decl.boundary_module.clone(),
                decl.forbids.clone(),
                decl.reason.clone(),
            )
        });
    }

    // For each unique rule, find violating IMPORTS edges.
    use repo_graph_storage::queries::BoundaryViolation;
    let mut declared_violations: Vec<BoundaryViolation> = Vec::new();

    // Sort rules for deterministic output.
    let mut rules: Vec<_> = rule_map.into_values().collect();
    rules.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));

    for (boundary_module, forbids, reason) in &rules {
        let edges = match storage.find_imports_between_paths(
            &snapshot.snapshot_uid,
            boundary_module,
            forbids,
        ) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::from(2);
            }
        };

        for edge in &edges {
            declared_violations.push(BoundaryViolation {
                boundary_module: boundary_module.clone(),
                forbidden_module: forbids.clone(),
                reason: reason.clone(),
                source_file: edge.source_file.clone(),
                target_file: edge.target_file.clone(),
                line: edge.line,
            });
        }
    }

    // ── Section 2: Discovered-module boundary violations ─────────

    // Load module graph facts once
    let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Evaluate using preloaded facts
    let discovered_result = match evaluate_violations_from_facts(&storage, repo_uid, &facts) {
        Ok(r) => r,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Convert discovered violations to JSON
    use repo_graph_classification::boundary_evaluator::StaleSide;

    let discovered_violations_json: Vec<serde_json::Value> = discovered_result
        .evaluation
        .violations
        .iter()
        .map(|v| {
            serde_json::json!({
                "declaration_uid": v.declaration_uid,
                "source": v.source_canonical_path,
                "target": v.target_canonical_path,
                "import_count": v.import_count,
                "source_file_count": v.source_file_count,
                "reason": v.reason,
            })
        })
        .collect();

    let stale_declarations_json: Vec<serde_json::Value> = discovered_result
        .evaluation
        .stale_declarations
        .iter()
        .map(|s| {
            serde_json::json!({
                "declaration_uid": s.declaration_uid,
                "stale_side": match s.stale_side {
                    StaleSide::Source => "source",
                    StaleSide::Target => "target",
                    StaleSide::Both => "both",
                },
                "missing_paths": s.missing_paths,
            })
        })
        .collect();

    // ── Build unified output ─────────────────────────────────────

    let declared_count = declared_violations.len();
    let discovered_count = discovered_result.evaluation.violations.len();
    let stale_count = discovered_result.evaluation.stale_declarations.len();
    let total_count = declared_count + discovered_count;

    let results = serde_json::json!({
        "declared_boundary_violations": serde_json::to_value(&declared_violations).unwrap(),
        "discovered_module_violations": discovered_violations_json,
    });

    // Build extra fields for envelope
    let mut extra = serde_json::Map::new();
    extra.insert(
        "declared_boundary_count".to_string(),
        serde_json::Value::Number(declared_count.into()),
    );
    extra.insert(
        "discovered_module_count".to_string(),
        serde_json::Value::Number(discovered_count.into()),
    );
    extra.insert(
        "stale_declarations".to_string(),
        serde_json::Value::Array(stale_declarations_json),
    );
    extra.insert(
        "stale_count".to_string(),
        serde_json::Value::Number(stale_count.into()),
    );

    let output = match build_envelope(
        &storage,
        "arch violations",
        repo_uid,
        &snapshot,
        results,
        total_count,
        extra,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    if json_mode {
        // Raw JSON output
        match serde_json::to_string_pretty(&output) {
            Ok(json) => {
                println!("{}", json);
                // Preserve legacy exit behavior: always 0 on success
                // Exit code change (fail on violations) is a separate contract slice
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::from(2)
            }
        }
    } else {
        // Human-readable output
        use crate::presentation::violations::ViolationsResponse;

        match serde_json::from_value::<ViolationsResponse>(output) {
            Ok(response) => {
                print!("{}", response.render_human());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to parse response for rendering: {}", e);
                ExitCode::from(2)
            }
        }
    }
}

// ── modules violations command (REG-1) ───────────────────────────
//
// `rmap modules violations [--json]`
//
// Human mode (default): plain text with violation diagnostics.
// Machine mode (--json): full envelope.
//
// Exit codes:
// - 0: no violations
// - 1: violations found (stale declarations alone do not force exit 1)
// - 2: runtime error

pub(super) fn run_modules_violations(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json) ──────────────────────────
    let mut json_mode = false;
    let mut unexpected: Option<&String> = None;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap modules violations [--json]");
                return ExitCode::from(1);
            }
            _ => {
                if unexpected.is_none() {
                    unexpected = Some(arg);
                }
            }
        }
    }

    if let Some(arg) = unexpected {
        eprintln!("error: unexpected argument: {}", arg);
        eprintln!("usage: rmap modules violations [--json]");
        eprintln!();
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    // Get cwd for repo resolution
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    let repo_path = match cwd.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot canonicalize current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    if !client.is_available() {
        eprintln!(
            "{}",
            daemon_unavailable_message(client.socket_path(), "modules violations")
        );
        return ExitCode::from(2);
    }

    // Build request params
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("modules_violations", Some(params)) {
        Ok(result) => {
            // Extract violation count for exit code
            let violation_count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        // Exit code: 0 if no violations, 1 if violations
                        if violation_count > 0 {
                            ExitCode::from(1)
                        } else {
                            ExitCode::SUCCESS
                        }
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize result: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-4)
                use crate::presentation::modules_violations::ModulesViolationsResponse;
                match serde_json::from_value::<ModulesViolationsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        // Exit code: 0 if no violations, 1 if violations
                        if violation_count > 0 {
                            ExitCode::from(1)
                        } else {
                            ExitCode::SUCCESS
                        }
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse modules violations response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
