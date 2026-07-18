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
    // CACHE-SEMANTICS-1: retention class breakdown
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_auto: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_user: Option<i64>,
    /// EC-M7: stamp-only baseline marks (provenance stamp + measurements
    /// retained; graph rows narrowed). Absent from pre-M-7 daemons.
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_stamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prunable: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unclassified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stale_epoch: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "_debug_error")]
    debug_error: Option<String>,
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
    let cwd =
        std::env::current_dir().map_err(|e| format!("failed to get current directory: {}", e))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// A daemon-shaped `perf` response (the `handlers::metrics` JSON contract),
    /// trimmed to the retention breakdown under test.
    fn daemon_response(retention: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "repo_path": "/repos/x",
            "db_size_bytes": 1024,
            "page_size": 4096,
            "page_count": 10,
            "tables": [],
            "tiers": { "tier_a_rows": 1, "tier_b_rows": 2 },
            "layers": { "layer_01_rows": 1, "layer_2_rows": 0, "layer_3_rows": 0 },
            "retention": retention,
        })
    }

    // EC-M7 (review-1 #6): the retention breakdown carries `baseline_stamp` —
    // without it a whole retention class was silently absent from `rmap perf`.
    #[test]
    fn perf_parses_baseline_stamp_in_retention_breakdown() {
        let response = daemon_response(serde_json::json!({
            "total_snapshots": 3,
            "ready_snapshots": 3,
            "failed_snapshots": 0,
            "oldest_snapshot": "2026-01-01T00:00:00Z",
            "newest_snapshot": "2026-01-03T00:00:00Z",
            "current": 1,
            "parent": 1,
            "baseline_auto": 0,
            "baseline_user": 0,
            "baseline_stamp": 1,
            "prunable": 0,
            "unclassified": 0,
            "stale_epoch": 0,
        }));
        let output: PerfOutput = serde_json::from_value(response).expect("daemon shape parses");
        let retention = output.retention.as_ref().expect("retention present");
        assert_eq!(retention.baseline_stamp, Some(1));
        assert_eq!(retention.baseline_user, Some(0));

        // The class survives re-serialization (the --json surface).
        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["retention"]["baseline_stamp"], 1);
    }

    // Back-compat: a pre-M-7 daemon response without the field parses with
    // `None` and the re-serialized JSON omits it (no fabricated zero).
    #[test]
    fn perf_tolerates_daemons_without_baseline_stamp() {
        let response = daemon_response(serde_json::json!({
            "total_snapshots": 1,
            "ready_snapshots": 1,
            "failed_snapshots": 0,
            "oldest_snapshot": null,
            "newest_snapshot": null,
        }));
        let output: PerfOutput = serde_json::from_value(response).expect("older shape parses");
        let retention = output.retention.as_ref().expect("retention present");
        assert_eq!(retention.baseline_stamp, None);
        let json = serde_json::to_value(&output).unwrap();
        assert!(
            json["retention"].get("baseline_stamp").is_none(),
            "an unmeasured class is omitted, not rendered as zero: {json}"
        );
    }
}
