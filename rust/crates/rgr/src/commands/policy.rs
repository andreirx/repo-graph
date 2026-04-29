//! Policy command family.
//!
//! PF-1: STATUS_MAPPING extraction from C files.
//! PF-2: BEHAVIORAL_MARKER extraction from C files.
//!
//! Facts are populated automatically during `rmap index` / `rmap refresh`
//! via the policy-facts postpass in repo-index composition.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::open_storage;
use repo_graph_policy_facts::{BehavioralMarker, StatusMapping};

// ── DTOs ─────────────────────────────────────────────────────────

/// Output envelope for STATUS_MAPPING facts.
#[derive(serde::Serialize)]
struct StatusMappingOutput {
    repo: String,
    snapshot: String,
    kind: String,
    facts: Vec<StatusMapping>,
    count: usize,
}

/// Output envelope for BEHAVIORAL_MARKER facts.
#[derive(serde::Serialize)]
struct BehavioralMarkerOutput {
    repo: String,
    snapshot: String,
    kind: String,
    facts: Vec<BehavioralMarker>,
    count: usize,
}

// ── Command handler ──────────────────────────────────────────────

/// Run the `rmap policy` command.
///
/// Usage: `rmap policy <db_path> <repo_uid> [--kind STATUS_MAPPING|BEHAVIORAL_MARKER] [--file <path>]`
///
/// Exit codes:
/// - 0: success (facts found)
/// - 1: no facts found (not an error, just empty)
/// - 2: runtime error (invalid args, DB error, missing repo/snapshot)
pub fn run_policy(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--kind ...] [--file <path>]
    if args.len() < 2 {
        eprintln!("usage: rmap policy <db_path> <repo_uid> [--kind STATUS_MAPPING|BEHAVIORAL_MARKER] [--file <path>]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    // Parse optional args.
    let mut kind_filter: Option<String> = None;
    let mut file_filter: Option<&str> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --kind requires an argument");
                    return ExitCode::from(1);
                }
                kind_filter = Some(args[i + 1].to_uppercase());
                i += 2;
            }
            "--file" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --file requires an argument");
                    return ExitCode::from(1);
                }
                file_filter = Some(&args[i + 1]);
                i += 2;
            }
            other => {
                eprintln!("error: unknown option: {}", other);
                return ExitCode::from(1);
            }
        }
    }

    // Validate kind filter.
    let kind = kind_filter.as_deref().unwrap_or("STATUS_MAPPING");
    if kind != "STATUS_MAPPING" && kind != "BEHAVIORAL_MARKER" {
        eprintln!(
            "error: unsupported policy kind: {} (supported: STATUS_MAPPING, BEHAVIORAL_MARKER)",
            kind
        );
        return ExitCode::from(1);
    }

    // Open storage.
    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Get latest snapshot.
    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("error: no snapshot for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: failed to query snapshot: {}", e);
            return ExitCode::from(2);
        }
    };

    use repo_graph_policy_facts::PolicyFactsStorageRead;

    match kind {
        "STATUS_MAPPING" => {
            // Query STATUS_MAPPING facts.
            let mappings = match storage.query_status_mappings(&snapshot.snapshot_uid, file_filter) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: failed to query policy facts: {}", e);
                    return ExitCode::from(2);
                }
            };

            let output = StatusMappingOutput {
                repo: repo_uid.to_string(),
                snapshot: snapshot.snapshot_uid.clone(),
                kind: "STATUS_MAPPING".to_string(),
                count: mappings.len(),
                facts: mappings,
            };

            output_json(&output, output.count)
        }
        "BEHAVIORAL_MARKER" => {
            // Query BEHAVIORAL_MARKER facts.
            let markers = match storage.query_behavioral_markers(
                &snapshot.snapshot_uid,
                file_filter,
                None, // No marker kind sub-filter for now
            ) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: failed to query policy facts: {}", e);
                    return ExitCode::from(2);
                }
            };

            let output = BehavioralMarkerOutput {
                repo: repo_uid.to_string(),
                snapshot: snapshot.snapshot_uid.clone(),
                kind: "BEHAVIORAL_MARKER".to_string(),
                count: markers.len(),
                facts: markers,
            };

            output_json(&output, output.count)
        }
        _ => unreachable!(),
    }
}

/// Helper to output JSON and return exit code.
fn output_json<T: serde::Serialize>(output: &T, count: usize) -> ExitCode {
    match serde_json::to_string_pretty(output) {
        Ok(json) => {
            println!("{}", json);
            if count == 0 {
                ExitCode::from(1) // No facts found
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
