//! Minimal Rust CLI for repo-graph.
//!
//! Rust-7B scope: one command only.
//!
//!   rgr-rust index <repo_path> <db_path>
//!
//! Exit codes:
//!   0 — success
//!   1 — usage error (wrong arguments)
//!   2 — indexing/storage failure

use std::path::Path;
use std::process::ExitCode;

use repo_graph_repo_index::compose::{index_path, ComposeOptions};

fn main() -> ExitCode {
	let args: Vec<String> = std::env::args().collect();

	if args.len() < 2 {
		eprintln!("usage: rgr-rust index <repo_path> <db_path>");
		return ExitCode::from(1);
	}

	match args[1].as_str() {
		"index" => run_index(&args[2..]),
		other => {
			eprintln!("unknown command: {}", other);
			eprintln!("usage: rgr-rust index <repo_path> <db_path>");
			ExitCode::from(1)
		}
	}
}

fn run_index(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust index <repo_path> <db_path>");
		return ExitCode::from(1);
	}

	let repo_path = Path::new(&args[0]);
	let db_path = Path::new(&args[1]);

	if !repo_path.is_dir() {
		eprintln!("error: repo path does not exist or is not a directory: {}", repo_path.display());
		return ExitCode::from(1);
	}

	// Derive repo_uid from directory name.
	let repo_uid = repo_path
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or("repo");

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
