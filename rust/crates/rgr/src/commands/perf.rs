//! `rmap perf` command.
//!
//! PERF-OBS-1: Storage performance observability.
//!
//! One-shot measurement command for baseline instrumentation.
//! Outputs per-table row counts, tier/layer aggregates, and size estimates.
//!
//! **This is a diagnostic command, not a production read path.**
//!
//! Usage:
//!   rmap perf                    # metrics for current repo
//!   rmap perf --json             # JSON output (default)

use std::process::ExitCode;

use serde::{Deserialize, Serialize};

use crate::daemon_client::DaemonClient;

/// Perf output for JSON mode.
#[derive(Debug, Serialize, Deserialize)]
struct PerfOutput {
    /// Repo path (if single-repo mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_path: Option<String>,
    /// Database file size in bytes
    db_size_bytes: i64,
    /// Page size (SQLite)
    page_size: i64,
    /// Page count (SQLite)
    page_count: i64,
    /// Per-table metrics
    tables: Vec<TableOutput>,
    /// Tier aggregates
    tiers: TierAggregates,
    /// Layer aggregates
    layers: LayerAggregates,
    /// Classification coverage (tier/layer completeness)
    #[serde(skip_serializing_if = "Option::is_none")]
    classification: Option<ClassificationCoverage>,
    /// Snapshot retention (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    retention: Option<RetentionOutput>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TableOutput {
    name: String,
    row_count: i64,
    size_bytes: i64,
    tier: String,
    layer: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClassificationCoverage {
    total_rows: i64,
    classified_tier_rows: i64,
    unclassified_tier_rows: i64,
    classified_layer_rows: i64,
    unclassified_layer_rows: i64,
    unknown_tier_tables: Vec<String>,
    unknown_layer_tables: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TierAggregates {
    tier_a_rows: i64,
    tier_b_rows: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct LayerAggregates {
    layer_01_rows: i64,
    layer_2_rows: i64,
    layer_3_rows: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct RetentionOutput {
    total_snapshots: i64,
    ready_snapshots: i64,
    failed_snapshots: i64,
    oldest_snapshot: Option<String>,
    newest_snapshot: Option<String>,
}

/// Run the perf command.
pub fn run_perf(args: &[String]) -> ExitCode {
    let mut json_output = true; // Default to JSON

    // Parse arguments
    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_output = true;
            }
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("unknown option: {}", other);
                print_usage();
                return ExitCode::from(1);
            }
            _ => {
                // Ignore positional args for now
            }
        }
    }

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to connect to daemon: {}", e);
            return ExitCode::from(1);
        }
    };

    let result = execute_perf_current(&mut client);

    match result {
        Ok(output) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(1)
        }
    }
}

fn execute_perf_current(client: &mut DaemonClient) -> Result<PerfOutput, String> {
    // Get current repo via daemon
    let cwd = std::env::current_dir()
        .map_err(|e| format!("failed to get current directory: {}", e))?;

    let params = serde_json::json!({
        "path": cwd.to_string_lossy()
    });

    // Call daemon perf method
    let response = client
        .request("perf", Some(params))
        .map_err(|e| format!("daemon error: {}", e))?;

    // Parse response
    serde_json::from_value(response).map_err(|e| format!("failed to parse response: {}", e))
}

fn print_usage() {
    eprintln!(
        "Usage: rmap perf [OPTIONS]

PERF-OBS-1: Storage performance observability.

Options:
  --json           Output JSON (default)
  --help, -h       Show this help

Output:
  Per-table row counts and size estimates grouped by tier (A=authority, B=cache)
  and layer (0-1=extracted, 2=derived, 3=hints).
  Database size estimates from SQLite page count.
  Classification coverage report for tier/layer percentages.

Examples:
  rmap perf                    # current repo metrics"
    );
}
