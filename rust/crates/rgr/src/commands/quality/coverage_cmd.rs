//! Coverage command.
//!
//! RS-MS-4-prereq-b/c: Import Istanbul/c8 coverage into measurements.
//! Delete-before-insert for idempotency. Reports matched/unmatched counts.
//!
//! # Boundary rules
//!
//! This module owns coverage command behavior:
//! - `run_coverage` handler
//! - `CoverageImportResult` DTO
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - coverage matching orchestration (lives in `crate::coverage`)
//! - coverage report parsing (belongs in `repo-graph-coverage`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, chrono_now, open_storage};
use crate::coverage;

// ── coverage command ─────────────────────────────────────────────

#[derive(serde::Serialize)]
struct CoverageImportResult {
	file_path: String,
	line_coverage: f64,
	covered_statements: u64,
	total_statements: u64,
}

pub fn run_coverage(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap coverage <db_path> <repo_uid> <report_path>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let report_path = Path::new(&args[2]);

	// Validate report exists
	if !report_path.is_file() {
		eprintln!("error: coverage report not found: {}", report_path.display());
		return ExitCode::from(1);
	}

	// Open storage
	let mut storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Get latest snapshot
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

	// Get repo for root_path
	use repo_graph_storage::types::RepoRef;
	let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
		Ok(Some(r)) => r,
		Ok(None) => {
			eprintln!("error: repo not found: {}", repo_uid);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve repo root to absolute path for coverage normalization
	// The DB may store "." which won't match absolute paths in the coverage report
	let repo_root_abs = match std::fs::canonicalize(&repo.root_path) {
		Ok(p) => p,
		Err(e) => {
			eprintln!(
				"error: cannot resolve repo root '{}': {}",
				repo.root_path, e
			);
			return ExitCode::from(2);
		}
	};

	// Parse coverage report
	use repo_graph_coverage::parse_istanbul_file;
	let parse_result =
		match parse_istanbul_file(report_path.to_str().unwrap(), repo_root_abs.to_str().unwrap()) {
			Ok(r) => r,
			Err(e) => {
				eprintln!("error: failed to parse coverage report: {}", e);
				return ExitCode::from(2);
			}
		};

	// Get indexed files
	let indexed_files = match storage.get_files_by_repo(repo_uid) {
		Ok(files) => files,
		Err(e) => {
			eprintln!("error: failed to read indexed files: {}", e);
			return ExitCode::from(2);
		}
	};

	let indexed_paths: std::collections::HashSet<String> =
		indexed_files.iter().map(|f| f.path.clone()).collect();

	// Match coverage to indexed files
	let now = chrono_now();
	let match_result = match coverage::match_coverage_to_indexed_files(
		&parse_result,
		&indexed_paths,
		repo_uid,
		&snapshot.snapshot_uid,
		&now,
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Atomically replace existing line_coverage measurements with new ones.
	// Single transaction ensures no data loss if insert fails.
	if let Err(e) = storage.replace_measurements_by_kind(
		&snapshot.snapshot_uid,
		&["line_coverage"],
		&match_result.measurements,
	) {
		eprintln!("error: failed to replace coverage measurements: {}", e);
		return ExitCode::from(2);
	}

	// Build output
	let results: Vec<CoverageImportResult> = match_result
		.measurements
		.iter()
		.map(|m| {
			// Parse value_json to extract fields
			let v: serde_json::Value = serde_json::from_str(&m.value_json).unwrap_or_default();
			CoverageImportResult {
				// Extract path from stable key: {repo}:{path}:FILE
				file_path: m
					.target_stable_key
					.strip_prefix(&format!("{}:", repo_uid))
					.and_then(|s| s.strip_suffix(":FILE"))
					.unwrap_or(&m.target_stable_key)
					.to_string(),
				line_coverage: v.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0),
				covered_statements: v.get("covered").and_then(|v| v.as_u64()).unwrap_or(0),
				total_statements: v.get("total").and_then(|v| v.as_u64()).unwrap_or(0),
			}
		})
		.collect();

	// Build envelope with extra stats
	let mut extra = serde_json::Map::new();
	extra.insert(
		"imported_count".to_string(),
		serde_json::Value::Number(match_result.matched_count.into()),
	);
	extra.insert(
		"unnormalized_count".to_string(),
		serde_json::Value::Number(match_result.unnormalized_paths.len().into()),
	);
	extra.insert(
		"unmatched_indexed_count".to_string(),
		serde_json::Value::Number(match_result.unmatched_indexed_paths.len().into()),
	);

	// Include sample unmatched paths for debugging (max 10)
	if !match_result.unnormalized_paths.is_empty() {
		let sample: Vec<_> = match_result
			.unnormalized_paths
			.iter()
			.take(10)
			.cloned()
			.collect();
		extra.insert(
			"unnormalized_paths_sample".to_string(),
			serde_json::to_value(sample).unwrap(),
		);
	}
	if !match_result.unmatched_indexed_paths.is_empty() {
		let sample: Vec<_> = match_result
			.unmatched_indexed_paths
			.iter()
			.take(10)
			.cloned()
			.collect();
		extra.insert(
			"unmatched_indexed_paths_sample".to_string(),
			serde_json::to_value(sample).unwrap(),
		);
	}

	let count = results.len();
	let output = match build_envelope(
		&storage,
		"coverage",
		repo_uid,
		&snapshot,
		serde_json::to_value(&results).unwrap(),
		count,
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
			ExitCode::SUCCESS
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}
