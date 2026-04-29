//! Trust command family.
//!
//! Computes and outputs the trust report for a repository snapshot.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::open_storage;

/// Run the `rmap trust` command.
///
/// Usage: `rmap trust <db_path> <repo_uid>`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (DB error, missing repo/snapshot, computation failure)
pub fn run_trust(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap trust <db_path> <repo_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    // Open storage.
    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Get latest snapshot.
    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: failed to get latest snapshot: {}", e);
            return ExitCode::from(2);
        }
    };

    if snapshot.status != "ready" {
        eprintln!(
            "error: latest snapshot for '{}' is not ready (status: {})",
            repo_uid, snapshot.status
        );
        return ExitCode::from(2);
    }

    // Compute trust report.
    use repo_graph_trust::service::assemble_trust_report;
    let report = match assemble_trust_report(
        &storage,
        repo_uid,
        &snapshot.snapshot_uid,
        snapshot.basis_commit.as_deref(),
        snapshot.toolchain_json.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: trust computation failed: {}", e);
            return ExitCode::from(2);
        }
    };

    // JSON to stdout only.
    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to serialize trust report: {}", e);
            ExitCode::from(2)
        }
    }
}
