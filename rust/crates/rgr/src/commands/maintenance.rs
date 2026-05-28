//! `rmap maintenance` command.
//!
//! MAINTENANCE-CLI-1: Explicit maintenance operations for retention cleanup.
//!
//! This command provides user access to deferred maintenance operations that
//! were removed from the interactive hot path by REFRESH-HANG-1.
//!
//! Usage:
//!   rmap maintenance prune           # prune prunable snapshots for current repo
//!   rmap maintenance prune --json    # JSON output
//!
//! The prune operation:
//! 1. Classifies all snapshots (assigns retention classes)
//! 2. Deletes all snapshots marked as `prunable`
//! 3. Reports pruned count and remaining retention stats
//!
//! This can be slow (60+ seconds) on repos with large prunable backlogs.
//! That is why it was removed from the index/refresh foreground path.

use std::process::ExitCode;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::daemon_client::DaemonClient;

/// Maintenance prune output for JSON mode.
#[derive(Debug, Serialize, Deserialize)]
struct PruneOutput {
    /// Repo path
    repo_path: String,
    /// Whether classification ran
    classified: bool,
    /// Number of snapshots pruned
    pruned_count: i64,
    /// Duration in milliseconds
    duration_ms: u64,
    /// Retention stats after prune
    retention: RetentionStats,
}

#[derive(Debug, Serialize, Deserialize)]
struct RetentionStats {
    current: i64,
    parent: i64,
    baseline_auto: i64,
    baseline_user: i64,
    total: i64,
}

/// Run the maintenance command.
pub fn run_maintenance(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "prune" => run_prune(&args[1..]),
        "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown subcommand: {}", other);
            print_usage();
            ExitCode::from(1)
        }
    }
}

/// Run the prune subcommand.
fn run_prune(args: &[String]) -> ExitCode {
    let mut json_output = false;

    // Parse arguments
    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_output = true;
            }
            "--help" | "-h" => {
                print_prune_usage();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("unknown option: {}", other);
                print_prune_usage();
                return ExitCode::from(1);
            }
            _ => {
                // Ignore positional args for now (future: specific path)
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

    let result = execute_prune(&mut client);

    match result {
        Ok(output) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
                );
            } else {
                print_prune_human(&output);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Maintenance prune timeout in seconds.
///
/// # MAINTENANCE-CLI-1 Technical Debt
///
/// This is a workaround for the 300s default daemon timeout. Prune operations
/// on repos with large backlogs can exceed 300s. We use 900s (15 minutes) to
/// provide sufficient headroom.
///
/// The proper fix is to have the daemon emit progress events during prune,
/// which would keep the connection alive and provide user feedback.
///
/// See: docs/slices/maintenance-cli-1.md
const PRUNE_TIMEOUT_SECS: u64 = 900;

fn execute_prune(client: &mut DaemonClient) -> Result<PruneOutput, String> {
    let start = Instant::now();

    // Get current repo via cwd
    let cwd =
        std::env::current_dir().map_err(|e| format!("failed to get current directory: {}", e))?;

    let params = serde_json::json!({
        "path": cwd.to_string_lossy()
    });

    // Call daemon classify_retention method (which includes prune)
    // Use extended timeout because prune can take >300s on large backlogs
    let response = client
        .request_with_timeout("classify_retention", Some(params), PRUNE_TIMEOUT_SECS)
        .map_err(|e| format!("daemon error: {}", e))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    // Parse response
    let classified = response
        .get("classified")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let pruned_count = response
        .get("pruned_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let repo_path = response
        .get("repo_path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let retention = response.get("retention").ok_or("missing retention stats")?;

    let stats = RetentionStats {
        current: retention
            .get("current")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        parent: retention
            .get("parent")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        baseline_auto: retention
            .get("baseline_auto")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        baseline_user: retention
            .get("baseline_user")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        total: retention.get("total").and_then(|v| v.as_i64()).unwrap_or(0),
    };

    Ok(PruneOutput {
        repo_path,
        classified,
        pruned_count,
        duration_ms,
        retention: stats,
    })
}

fn print_prune_human(output: &PruneOutput) {
    if output.pruned_count == 0 {
        println!("no prunable snapshots found");
    } else {
        println!(
            "pruned {} snapshot(s) in {}ms",
            output.pruned_count, output.duration_ms
        );
    }
    println!();
    println!("retention stats:");
    println!("  current:      {}", output.retention.current);
    println!("  parent:       {}", output.retention.parent);
    println!("  baseline_auto:{}", output.retention.baseline_auto);
    println!("  baseline_user:{}", output.retention.baseline_user);
    println!("  total:        {}", output.retention.total);
}

fn print_usage() {
    eprintln!(
        "Usage: rmap maintenance <SUBCOMMAND>

MAINTENANCE-CLI-1: Explicit maintenance operations.

Subcommands:
  prune          Prune prunable snapshots for current repo

Options:
  --help, -h     Show this help

Examples:
  rmap maintenance prune           # prune current repo
  rmap maintenance prune --json    # JSON output"
    );
}

fn print_prune_usage() {
    eprintln!(
        "Usage: rmap maintenance prune [OPTIONS]

Prune prunable snapshots for the current repository.

This operation:
1. Classifies all snapshots (assigns retention classes)
2. Deletes snapshots marked as 'prunable'
3. Reports results

Protected snapshots (current, parent, baseline_auto, baseline_user) are never deleted.

This operation can be slow (60+ seconds) on repos with large prunable backlogs.

Options:
  --json         Output JSON instead of human-readable text
  --help, -h     Show this help

Examples:
  rmap maintenance prune           # prune and show human output
  rmap maintenance prune --json    # prune and show JSON output"
    );
}
