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
    /// DAEMON-VISIBILITY-1 (F3): interrupted (non-READY) snapshots that were present when prune ran.
    /// These sit OUTSIDE the normal READY retention model (never classified, never auto-pruned).
    /// Named here (state + when) so the reclaim report can list what it freed even after deletion.
    #[serde(default)]
    interrupted_snapshots: Vec<InterruptedSnapshot>,
    /// DAEMON-VISIBILITY-1 (F3, operator Option A): the outcome of reclaiming those orphaned non-READY
    /// snapshots — whether they were deleted and how much disk was freed, or why it was skipped.
    #[serde(default)]
    non_ready_reclaim: NonReadyReclaim,
    /// Repo DB size on disk (whole file; per-snapshot bytes are not tracked). Measured AFTER reclaim.
    #[serde(default)]
    db_size_bytes: u64,
}

/// DAEMON-VISIBILITY-1 (F3): an interrupted (non-READY) snapshot surfaced by prune.
#[derive(Debug, Serialize, Deserialize)]
struct InterruptedSnapshot {
    #[serde(default)]
    state: String,
    #[serde(default)]
    created_at: String,
}

/// DAEMON-VISIBILITY-1 (F3, operator Option A): the daemon's reclaim outcome for orphaned non-READY
/// snapshots. `reclaimed=false` with a `skipped_reason` means a live operation blocked deletion (safe:
/// the interrupted snapshot is still listed and prune can be re-run when idle).
#[derive(Debug, Default, Serialize, Deserialize)]
struct NonReadyReclaim {
    #[serde(default)]
    reclaimed: bool,
    #[serde(default)]
    deleted_count: u64,
    #[serde(default)]
    reclaimed_bytes: u64,
    #[serde(default)]
    skipped_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RetentionStats {
    current: i64,
    parent: i64,
    baseline_auto: i64,
    baseline_user: i64,
    /// DAEMON-CRASH-RECOVERY-1 (F12): READY snapshots classed prunable. Previously omitted from the
    /// client render — part of why the table could read as an (almost) empty store.
    #[serde(default)]
    prunable: i64,
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
        prunable: retention
            .get("prunable")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        total: retention.get("total").and_then(|v| v.as_i64()).unwrap_or(0),
    };

    // DAEMON-VISIBILITY-1 (F3 visibility): interrupted (non-READY) snapshots + repo DB size.
    let interrupted_snapshots: Vec<InterruptedSnapshot> = response
        .get("interrupted_snapshots")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|s| InterruptedSnapshot {
                    state: s
                        .get("state")
                        .and_then(|v| v.as_str())
                        .unwrap_or("non-READY")
                        .to_string(),
                    created_at: s
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let db_size_bytes = response
        .get("db_size_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // DAEMON-VISIBILITY-1 (F3): the reclaim outcome (deleted count + freed bytes, or skip reason).
    let non_ready_reclaim: NonReadyReclaim = response
        .get("non_ready_reclaim")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    Ok(PruneOutput {
        repo_path,
        classified,
        pruned_count,
        duration_ms,
        retention: stats,
        interrupted_snapshots,
        non_ready_reclaim,
        db_size_bytes,
    })
}

fn print_prune_human(output: &PruneOutput) {
    let interrupted_n = output.interrupted_snapshots.len();
    if let Some(headline) = prune_headline(output.pruned_count, output.duration_ms, interrupted_n) {
        println!("{headline}");
    }
    println!();
    println!("retention stats:");
    println!("  current:       {}", output.retention.current);
    println!("  parent:        {}", output.retention.parent);
    println!("  baseline_auto: {}", output.retention.baseline_auto);
    println!("  baseline_user: {}", output.retention.baseline_user);
    println!("  prunable:      {}", output.retention.prunable);
    println!("  total:         {}", output.retention.total);
    // F12: name genuinely-UNCLASSIFIED rows so the table NEVER implies an empty store when the class
    // counts above do not sum to `total`. After DAEMON-CRASH-RECOVERY-1 a reconciled crash orphan is
    // counted in `prunable` above (NOT unclassified), so this fires only for rows no class covers yet
    // (e.g. a `building` orphan the boot sweep has not reached) — never contradicting the `prunable`
    // line for a reconciled orphan. The orphaned partials are still named by the reclaim section below.
    if unclassified_count(&output.retention) > 0 {
        println!(
            "  unclassified:  {} (orphaned — daemon restart?)",
            orphan_state_summary(&output.interrupted_snapshots)
        );
    }

    // DAEMON-VISIBILITY-1 (F3 visibility): surface interrupted (non-READY) snapshots so they no
    // longer silently hold disk.
    for line in interrupted_report_lines(output) {
        println!("{line}");
    }
}

/// DAEMON-CRASH-RECOVERY-1 (F12): the prune headline, or `None`.
///
/// The field bug: `maintenance prune` printed "no prunable snapshots found" over 11 GB of orphaned
/// non-READY partials (they are not READY-`prunable`, so `pruned_count` was 0). Here we print that
/// line ONLY when there is genuinely nothing — no READY prune AND no orphaned non-READY snapshot.
/// When orphans are present the reclaim/orphan lines below carry the truth, so the headline is
/// suppressed rather than lying.
fn prune_headline(pruned_count: i64, duration_ms: u64, interrupted_n: usize) -> Option<String> {
    if pruned_count > 0 {
        Some(format!(
            "pruned {pruned_count} snapshot(s) in {duration_ms}ms"
        ))
    } else if interrupted_n == 0 {
        Some("no prunable snapshots found".to_string())
    } else {
        None
    }
}

/// DAEMON-CRASH-RECOVERY-1 (F12): a compact "N <state>" breakdown of the orphaned non-READY
/// snapshots, grouped by reader-frame state (deterministic order). Crash-orphans all render
/// "interrupted"; a mixed set lists each state. Pure/testable.
fn orphan_state_summary(interrupted: &[InterruptedSnapshot]) -> String {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for s in interrupted {
        let state = if s.state.is_empty() {
            "non-READY"
        } else {
            s.state.as_str()
        };
        *counts.entry(state).or_default() += 1;
    }
    counts
        .iter()
        .map(|(state, n)| format!("{n} {state}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// DAEMON-CRASH-RECOVERY-1 (F12): snapshots no retention class covers — `total` minus every classified
/// row. `> 0` is the honest "the class counts do not sum to `total`" signal (the field bug was
/// `total 3, all classes 0`). After reconciliation a crash orphan is counted in `prunable`, so this is
/// 0 for it (no phantom line contradicting the `prunable` count); it stays positive only for rows no
/// class covers yet (e.g. a not-yet-reconciled `building` orphan). Pure/testable; clamped at 0 so a
/// transient over-count never prints a negative.
fn unclassified_count(r: &RetentionStats) -> i64 {
    (r.total - r.current - r.parent - r.baseline_auto - r.baseline_user - r.prunable).max(0)
}

/// DAEMON-VISIBILITY-1 (F3, operator Option A): the interrupted-snapshots section of the prune report
/// (pure, testable). Empty when there were none. Reports the ACTUAL reclaim outcome:
///
/// - **reclaimed**: "reclaimed N interrupted snapshot(s), freed X on disk" + the list — the orphaned
///   partials were deleted and their disk returned to the OS (the operator's field complaint fixed);
/// - **skipped**: "N interrupted snapshot(s) not reclaimed — <reason>" — a live operation blocked the
///   safe delete; the snapshots are still listed and prune can be re-run when the repo is idle.
fn interrupted_report_lines(output: &PruneOutput) -> Vec<String> {
    if output.interrupted_snapshots.is_empty() {
        return Vec::new();
    }
    let reclaim = &output.non_ready_reclaim;
    let n = output.interrupted_snapshots.len();
    let mut lines = vec![String::new()];

    if reclaim.reclaimed {
        lines.push(format!(
            "reclaimed {} interrupted snapshot(s), freed {} on disk:",
            reclaim.deleted_count,
            format_bytes(reclaim.reclaimed_bytes)
        ));
        for snap in &output.interrupted_snapshots {
            lines.push(format!("  - {}, created {}", snap.state, snap.created_at));
        }
        lines.push(format!(
            "  repo DB now holds {} on disk.",
            format_bytes(output.db_size_bytes)
        ));
    } else {
        let reason = reclaim
            .skipped_reason
            .as_deref()
            .unwrap_or("an operation is in progress on this repo");
        lines.push(format!(
            "interrupted snapshots ({n}, not reclaimed — {reason}):"
        ));
        for snap in &output.interrupted_snapshots {
            lines.push(format!("  - {}, created {}", snap.state, snap.created_at));
        }
        lines.push(format!(
            "  note: an interrupted index never finalized; the repo DB holds {} on disk.",
            format_bytes(output.db_size_bytes)
        ));
        lines.push(
            "  re-run `rmap maintenance prune` when the repo is idle to reclaim them.".to_string(),
        );
    }
    lines
}

/// Humanise a byte count (GB/MB/KB) for the prune report.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_output(interrupted: Vec<InterruptedSnapshot>, reclaim: NonReadyReclaim) -> PruneOutput {
        PruneOutput {
            repo_path: "/repos/big".to_string(),
            classified: true,
            pruned_count: 0,
            duration_ms: 1,
            retention: RetentionStats {
                current: 1,
                parent: 0,
                baseline_auto: 0,
                baseline_user: 0,
                prunable: 0,
                total: 2,
            },
            interrupted_snapshots: interrupted,
            non_ready_reclaim: reclaim,
            db_size_bytes: 4_000_000_000,
        }
    }

    fn one_interrupted() -> Vec<InterruptedSnapshot> {
        vec![InterruptedSnapshot {
            state: "interrupted".to_string(),
            created_at: "2026-07-02T10:00:00Z".to_string(),
        }]
    }

    // DAEMON-VISIBILITY-1 (F3, operator Option A): when the daemon deleted the orphaned non-READY
    // snapshot(s), `rmap maintenance prune` reports the ACTUAL reclaim — count + bytes freed — not a
    // "not available yet" stub. This is the operator's field complaint (a 4 GB partial holding disk) fixed.
    #[test]
    fn prune_reports_actual_reclaim_when_deleted() {
        let output = base_output(
            one_interrupted(),
            NonReadyReclaim {
                reclaimed: true,
                deleted_count: 1,
                reclaimed_bytes: 4_000_000_000,
                skipped_reason: None,
            },
        );
        let joined = interrupted_report_lines(&output).join("\n");
        assert!(
            joined.contains("reclaimed 1 interrupted snapshot(s)"),
            "states the delete happened: {joined}"
        );
        assert!(
            joined.contains("freed 3.7 GB"),
            "states the disk freed: {joined}"
        );
        assert!(
            joined.contains("interrupted, created 2026-07-02"),
            "names the reclaimed snapshot: {joined}"
        );
        assert!(
            !joined.contains("not available yet"),
            "must NOT be the old deferred stub: {joined}"
        );
    }

    // DAEMON-VISIBILITY-1 (F3): when a live operation blocked the safe delete, prune says so honestly
    // and points to re-running when idle — it never silently claims a reclaim it did not do.
    #[test]
    fn prune_reports_skip_when_operation_in_progress() {
        let output = base_output(
            one_interrupted(),
            NonReadyReclaim {
                reclaimed: false,
                deleted_count: 0,
                reclaimed_bytes: 0,
                skipped_reason: Some("an operation is in progress on this repo".to_string()),
            },
        );
        let joined = interrupted_report_lines(&output).join("\n");
        assert!(
            joined.contains("not reclaimed — an operation is in progress"),
            "explains the skip honestly: {joined}"
        );
        assert!(
            joined.contains("re-run `rmap maintenance prune` when the repo is idle"),
            "gives the retry action: {joined}"
        );
        assert!(joined.contains("3.7 GB"), "still shows disk held: {joined}");
    }

    #[test]
    fn prune_report_empty_when_no_interrupted() {
        let lines = interrupted_report_lines(&base_output(vec![], NonReadyReclaim::default()));
        assert!(lines.is_empty());
    }

    // DAEMON-CRASH-RECOVERY-1 (F12): the exact field bug — 3 orphaned partials, 0 READY-prunable.
    // The headline must NOT be the misleading "no prunable snapshots found"; it is suppressed so the
    // reclaim lines carry the truth. When there is genuinely nothing, the honest line still prints.
    #[test]
    fn headline_never_implies_empty_over_orphans() {
        // 3 orphaned non-READY snapshots, nothing READY-prunable → NO misleading headline.
        assert_eq!(prune_headline(0, 5, 3), None);
        // Genuinely nothing → the honest "no prunable" line.
        assert_eq!(
            prune_headline(0, 5, 0).as_deref(),
            Some("no prunable snapshots found")
        );
        // Something pruned → the count line.
        assert_eq!(
            prune_headline(2, 12, 0).as_deref(),
            Some("pruned 2 snapshot(s) in 12ms")
        );
    }

    // F12: the stats-table orphan line NAMES the excluded rows by reader-frame state so the READY-only
    // class counts (which can all be 0) never read as an empty store.
    #[test]
    fn orphan_state_summary_names_the_excluded_by_state() {
        let three_interrupted = vec![
            InterruptedSnapshot {
                state: "interrupted".to_string(),
                created_at: "t1".to_string(),
            },
            InterruptedSnapshot {
                state: "interrupted".to_string(),
                created_at: "t2".to_string(),
            },
            InterruptedSnapshot {
                state: "interrupted".to_string(),
                created_at: "t3".to_string(),
            },
        ];
        assert_eq!(orphan_state_summary(&three_interrupted), "3 interrupted");
        // A missing state degrades to a labelled "non-READY", never a blank.
        let blank = vec![InterruptedSnapshot {
            state: String::new(),
            created_at: "t".to_string(),
        }];
        assert_eq!(orphan_state_summary(&blank), "1 non-READY");
    }

    // DAEMON-CRASH-RECOVERY-1 (F12, review-1): the "unclassified" stats line fires on the TRUE
    // `total > Σ(classes)` condition — NOT merely "interrupted present". After reconciliation a crash
    // orphan is counted in `prunable`, so `unclassified_count` is 0 for it and the line does NOT print
    // (it would otherwise contradict the `prunable: N` line just above — a name-vs-behavior defect).
    #[test]
    fn unclassified_count_is_total_minus_classified() {
        let stats = |current, prunable, total| RetentionStats {
            current,
            parent: 0,
            baseline_auto: 0,
            baseline_user: 0,
            prunable,
            total,
        };
        // The pre-reconciliation field bug: 3 orphans, no class → 3 unclassified (the line SHOULD fire).
        assert_eq!(unclassified_count(&stats(0, 0, 3)), 3);
        // Post-reconciliation: the 3 orphans are counted `prunable` → 0 unclassified (no phantom line).
        assert_eq!(unclassified_count(&stats(0, 3, 3)), 0);
        // A healthy repo (current + a prunable READY) → 0.
        assert_eq!(unclassified_count(&stats(1, 1, 2)), 0);
        // Clamped: a transient over-count never yields a negative.
        assert_eq!(unclassified_count(&stats(2, 2, 3)), 0);
    }
}
