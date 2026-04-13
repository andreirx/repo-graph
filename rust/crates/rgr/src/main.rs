//! Minimal Rust CLI for repo-graph.
//!
//! Commands:
//!   rgr-rust index   <repo_path> <db_path>
//!   rgr-rust refresh <repo_path> <db_path>
//!   rgr-rust trust   <db_path> <repo_uid>
//!
//! Exit codes:
//!   0 — success
//!   1 — usage error
//!   2 — runtime error

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
	let args: Vec<String> = std::env::args().collect();

	if args.len() < 2 {
		print_usage();
		return ExitCode::from(1);
	}

	match args[1].as_str() {
		"index" => run_index(&args[2..]),
		"refresh" => run_refresh(&args[2..]),
		"trust" => run_trust(&args[2..]),
		other => {
			eprintln!("unknown command: {}", other);
			print_usage();
			ExitCode::from(1)
		}
	}
}

fn print_usage() {
	eprintln!("usage:");
	eprintln!("  rgr-rust index   <repo_path> <db_path>");
	eprintln!("  rgr-rust refresh <repo_path> <db_path>");
	eprintln!("  rgr-rust trust   <db_path> <repo_uid>");
}

// ── index command ────────────────────────────────────────────────

fn run_index(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust index <repo_path> <db_path>");
		return ExitCode::from(1);
	}

	let repo_path = Path::new(&args[0]);
	let db_path = Path::new(&args[1]);

	if !repo_path.is_dir() {
		eprintln!(
			"error: repo path does not exist or is not a directory: {}",
			repo_path.display()
		);
		return ExitCode::from(1);
	}

	let repo_uid = repo_path
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or("repo");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	match index_path(repo_path, db_path, repo_uid, &ComposeOptions::default()) {
		Ok(result) => {
			eprintln!(
				"indexed {} files, {} nodes, {} edges ({} unresolved) → {}",
				result.files_total,
				result.nodes_total,
				result.edges_total,
				result.edges_unresolved,
				result.snapshot_uid,
			);
			ExitCode::SUCCESS
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── refresh command ──────────────────────────────────────────────

fn run_refresh(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust refresh <repo_path> <db_path>");
		return ExitCode::from(1);
	}

	let repo_path = Path::new(&args[0]);
	let db_path = Path::new(&args[1]);

	if !repo_path.is_dir() {
		eprintln!(
			"error: repo path does not exist or is not a directory: {}",
			repo_path.display()
		);
		return ExitCode::from(1);
	}

	let repo_uid = repo_path
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or("repo");

	use repo_graph_repo_index::compose::{refresh_path, ComposeOptions};
	match refresh_path(repo_path, db_path, repo_uid, &ComposeOptions::default()) {
		Ok(result) => {
			eprintln!(
				"refreshed {} files, {} nodes, {} edges ({} unresolved) → {}",
				result.files_total,
				result.nodes_total,
				result.edges_total,
				result.edges_unresolved,
				result.snapshot_uid,
			);
			ExitCode::SUCCESS
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── trust command ────────────────────────────────────────────────

fn run_trust(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust trust <db_path> <repo_uid>");
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
			eprintln!(
				"error: no snapshot found for repo '{}'",
				repo_uid
			);
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

fn open_storage(
	db_path: &Path,
) -> Result<repo_graph_storage::StorageConnection, String> {
	if !db_path.exists() {
		return Err(format!(
			"database file does not exist: {}",
			db_path.display()
		));
	}
	repo_graph_storage::StorageConnection::open(db_path)
		.map_err(|e| format!("failed to open database: {}", e))
}
