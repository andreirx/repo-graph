//! Policy command family.
//!
//! PF-1: STATUS_MAPPING extraction from C files.
//! PF-2: BEHAVIORAL_MARKER extraction from C files.
//! PF-3: RETURN_FATE extraction from C files.
//!
//! Facts are populated automatically during `rmap index` / `rmap refresh`
//! via the policy-facts postpass in repo-index composition.
//!
//! # Legacy Contract Exception
//!
//! **This command does NOT use REG-1 daemon contract.**
//! It requires explicit `db_path` and `repo_uid` arguments.
//! This is preserved behavior, not a bug to be fixed in this slice.
//!
//! # CLI-OUT-5 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Deterministic ordering (by file, line)
//! - Full output, no truncation
//! - Kind-specific rendering for each fact type

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use crate::cli::open_storage;
use crate::presentation::policy::{
    BehavioralMarkerFact, BehavioralMarkerResponse, CaseMapping, ReturnFateFact,
    ReturnFateResponse, ReturnFateSummary, StatusMappingFact, StatusMappingResponse,
};
use repo_graph_policy_facts::{BehavioralMarker, FateKind, ReturnFate, StatusMapping};

// ── Command handler ──────────────────────────────────────────────────────────

/// Run the `rmap policy` command.
///
/// Usage: `rmap policy <db_path> <repo_uid> [--kind STATUS_MAPPING|BEHAVIORAL_MARKER|RETURN_FATE] [--file <path>] [--callee <name>] [--fate <IGNORED|CHECKED|...>] [--json]`
///
/// Exit codes:
/// - 0: success (facts found)
/// - 1: no facts found (not an error, just empty)
/// - 2: runtime error (invalid args, DB error, missing repo/snapshot)
pub fn run_policy(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--kind ...] [--file <path>] [--callee <name>] [--fate <kind>] [--json]
    if args.len() < 2 {
        print_usage();
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    // Parse optional args.
    let mut kind_filter: Option<String> = None;
    let mut file_filter: Option<&str> = None;
    let mut callee_filter: Option<&str> = None;
    let mut fate_filter: Option<FateKind> = None;
    let mut json_mode = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_mode = true;
                i += 1;
            }
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
            "--callee" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --callee requires an argument");
                    return ExitCode::from(1);
                }
                callee_filter = Some(&args[i + 1]);
                i += 2;
            }
            "--fate" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --fate requires an argument");
                    return ExitCode::from(1);
                }
                match args[i + 1].to_uppercase().parse::<FateKind>() {
                    Ok(f) => fate_filter = Some(f),
                    Err(_) => {
                        eprintln!(
                            "error: invalid fate kind: {} (supported: IGNORED, CHECKED, PROPAGATED, TRANSFORMED, STORED)",
                            args[i + 1]
                        );
                        return ExitCode::from(1);
                    }
                }
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
    if kind != "STATUS_MAPPING" && kind != "BEHAVIORAL_MARKER" && kind != "RETURN_FATE" {
        eprintln!(
            "error: unsupported policy kind: {} (supported: STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE)",
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
            let mappings = match storage.query_status_mappings(&snapshot.snapshot_uid, file_filter)
            {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: failed to query policy facts: {}", e);
                    return ExitCode::from(2);
                }
            };

            let count = mappings.len();

            if json_mode {
                // Raw JSON output
                let output = JsonStatusMappingOutput {
                    repo: repo_uid.to_string(),
                    snapshot: snapshot.snapshot_uid.clone(),
                    kind: "STATUS_MAPPING".to_string(),
                    count,
                    facts: mappings,
                };
                output_json(&output, count)
            } else {
                // Human-readable output
                let response = StatusMappingResponse {
                    repo: repo_uid.to_string(),
                    snapshot: snapshot.snapshot_uid.clone(),
                    kind: "STATUS_MAPPING".to_string(),
                    facts: mappings.into_iter().map(convert_status_mapping).collect(),
                    count,
                };
                print!("{}", response.render_human());
                if count == 0 {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            }
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

            let count = markers.len();

            if json_mode {
                // Raw JSON output
                let output = JsonBehavioralMarkerOutput {
                    repo: repo_uid.to_string(),
                    snapshot: snapshot.snapshot_uid.clone(),
                    kind: "BEHAVIORAL_MARKER".to_string(),
                    count,
                    facts: markers,
                };
                output_json(&output, count)
            } else {
                // Human-readable output
                let response = BehavioralMarkerResponse {
                    repo: repo_uid.to_string(),
                    snapshot: snapshot.snapshot_uid.clone(),
                    kind: "BEHAVIORAL_MARKER".to_string(),
                    facts: markers.into_iter().map(convert_behavioral_marker).collect(),
                    count,
                };
                print!("{}", response.render_human());
                if count == 0 {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            }
        }
        "RETURN_FATE" => {
            // Query RETURN_FATE facts.
            let fates = match storage.query_return_fates(
                &snapshot.snapshot_uid,
                file_filter,
                callee_filter,
                fate_filter,
            ) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("error: failed to query policy facts: {}", e);
                    return ExitCode::from(2);
                }
            };

            let count = fates.len();

            // Build summary by fate kind.
            let mut by_fate: BTreeMap<String, usize> = BTreeMap::new();
            for fate in &fates {
                *by_fate.entry(fate.fate.to_string()).or_insert(0) += 1;
            }

            if json_mode {
                // Raw JSON output
                let output = JsonReturnFateOutput {
                    repo: repo_uid.to_string(),
                    snapshot: snapshot.snapshot_uid.clone(),
                    kind: "RETURN_FATE".to_string(),
                    count,
                    facts: fates,
                    summary: JsonReturnFateSummary {
                        by_fate: by_fate.clone(),
                    },
                };
                output_json(&output, count)
            } else {
                // Human-readable output
                let response = ReturnFateResponse {
                    repo: repo_uid.to_string(),
                    snapshot: snapshot.snapshot_uid.clone(),
                    kind: "RETURN_FATE".to_string(),
                    facts: fates.into_iter().map(convert_return_fate).collect(),
                    count,
                    summary: ReturnFateSummary { by_fate },
                };
                print!("{}", response.render_human());
                if count == 0 {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            }
        }
        _ => unreachable!(),
    }
}

fn print_usage() {
    eprintln!("usage: rmap policy <db_path> <repo_uid> [options]");
    eprintln!();
    eprintln!("options:");
    eprintln!("  --kind STATUS_MAPPING|BEHAVIORAL_MARKER|RETURN_FATE  (default: STATUS_MAPPING)");
    eprintln!("  --file <path>      filter by file path");
    eprintln!("  --callee <name>    filter by callee name (RETURN_FATE only)");
    eprintln!("  --fate <kind>      filter by fate (RETURN_FATE only)");
    eprintln!("  --json             output raw JSON instead of human-readable");
    eprintln!();
    eprintln!("Note: This command requires explicit db_path and repo_uid.");
    eprintln!("      Use 'rmap repo info --json' to find these values for a repo.");
}

// ── JSON output DTOs ─────────────────────────────────────────────────────────

/// JSON output envelope for STATUS_MAPPING facts.
#[derive(serde::Serialize)]
struct JsonStatusMappingOutput {
    repo: String,
    snapshot: String,
    kind: String,
    facts: Vec<StatusMapping>,
    count: usize,
}

/// JSON output envelope for BEHAVIORAL_MARKER facts.
#[derive(serde::Serialize)]
struct JsonBehavioralMarkerOutput {
    repo: String,
    snapshot: String,
    kind: String,
    facts: Vec<BehavioralMarker>,
    count: usize,
}

/// JSON output envelope for RETURN_FATE facts.
#[derive(serde::Serialize)]
struct JsonReturnFateOutput {
    repo: String,
    snapshot: String,
    kind: String,
    facts: Vec<ReturnFate>,
    count: usize,
    summary: JsonReturnFateSummary,
}

/// JSON summary for RETURN_FATE output.
#[derive(serde::Serialize)]
struct JsonReturnFateSummary {
    by_fate: BTreeMap<String, usize>,
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

// ── Conversion helpers ───────────────────────────────────────────────────────

fn convert_status_mapping(m: StatusMapping) -> StatusMappingFact {
    StatusMappingFact {
        symbol_key: m.symbol_key,
        function_name: m.function_name,
        file_path: m.file_path,
        line_start: m.line_start,
        line_end: m.line_end,
        source_type: m.source_type,
        target_type: m.target_type,
        mappings: m
            .mappings
            .into_iter()
            .map(|cm| CaseMapping {
                inputs: cm.inputs,
                output: cm.output,
            })
            .collect(),
        default_output: m.default_output,
    }
}

fn convert_behavioral_marker(m: BehavioralMarker) -> BehavioralMarkerFact {
    BehavioralMarkerFact {
        symbol_key: m.symbol_key,
        function_name: m.function_name,
        file_path: m.file_path,
        line_start: m.line_start,
        line_end: m.line_end,
        kind: m.kind.to_string(),
        evidence: serde_json::to_value(&m.evidence).unwrap_or(serde_json::Value::Null),
    }
}

fn convert_return_fate(f: ReturnFate) -> ReturnFateFact {
    ReturnFateFact {
        callee_name: f.callee_name,
        caller_key: f.caller_key,
        caller_name: f.caller_name,
        file_path: f.file_path,
        line: f.line,
        column: f.column,
        fate: f.fate.to_string(),
        evidence: serde_json::to_value(&f.evidence).unwrap_or(serde_json::Value::Null),
    }
}
