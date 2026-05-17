//! Violations commands.
//!
//! Contains:
//! - `run_violations` — unified violations command (legacy + discovered-module)
//! - `run_modules_violations` — discovered-module violations only (REG-1)
//!
//! # REG-1 Contract
//!
//! `run_modules_violations` resolves repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Boundary rules
//!
//! This module owns violations command behavior:
//! - command handlers
//! - family-local output shaping
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in daemon via module-queries)
//! - boundary evaluation logic (lives in daemon via classification)

use std::path::Path;
use std::process::ExitCode;

use super::shared::{evaluate_violations_from_facts, load_module_graph_facts};
use crate::cli::{build_envelope, open_storage};
use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

// ── unified violations command ───────────────────────────────────
//
// NOTE: This command remains legacy (db_path + repo_uid) for now.
// It will be migrated separately as a top-level command.

pub fn run_violations(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap violations <db_path> <repo_uid>");
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
}

// ── modules violations command (REG-1) ───────────────────────────

pub(super) fn run_modules_violations(args: &[String]) -> ExitCode {
    // Check for unexpected args
    if !args.is_empty() {
        eprintln!("error: unexpected argument: {}", args[0]);
        eprintln!("usage: rmap modules violations");
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
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
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

            match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{}", json);
                    // Exit code: 0 if no violations, 1 if violations
                    // stale_declarations alone do not force exit 1
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
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
