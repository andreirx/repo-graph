//! Risk command.
//!
//! RS-MS-4: Query-time risk analysis (hotspot x coverage gap).
//! Only files with BOTH hotspot AND coverage data are included.
//! Missing coverage = file excluded (not degraded to risk = hotspot).
//!
//! # Boundary rules
//!
//! This module owns risk command behavior:
//! - `run_risk` handler
//! - `RiskRow` DTO
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - arg parsing (reuses `super::churn::parse_churn_args`)
//! - risk scoring (belongs in `repo-graph-classification`)
//! - hotspot scoring (belongs in `repo-graph-classification`)
//! - git churn extraction (belongs in `repo-graph-git`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};
use super::churn::{parse_since_args, SinceArgsError};

// ── risk command ─────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct RiskRow {
	file_path: String,
	risk_score: f64,
	hotspot_score: u64,
	line_coverage: f64,
	lines_changed: u64,
	sum_complexity: u64,
}

pub fn run_risk(args: &[String]) -> ExitCode {
	// Parse args: same signature as churn
	let (db_path, repo_uid, since) = match parse_since_args(args) {
		Ok(parsed) => parsed,
		Err(e) => {
			match e {
				SinceArgsError::MissingArgs => {
					eprintln!("usage: rmap risk <db_path> <repo_uid> [--since <expr>]");
				}
				SinceArgsError::SinceMissingValue => {
					eprintln!("error: --since requires a value");
				}
				SinceArgsError::UnknownArgument(arg) => {
					eprintln!("error: unknown argument: {}", arg);
				}
			}
			return ExitCode::from(1);
		}
	};

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

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

	// Get indexed files
	let indexed_files = match storage.get_files_by_repo(repo_uid) {
		Ok(files) => files,
		Err(e) => {
			eprintln!("error: failed to read indexed files: {}", e);
			return ExitCode::from(2);
		}
	};

	let indexed_paths: std::collections::HashSet<&str> =
		indexed_files.iter().map(|f| f.path.as_str()).collect();

	// Get churn from git
	use repo_graph_git::{get_file_churn, ChurnWindow};
	let window = ChurnWindow::new(&since);
	let repo_path = Path::new(&repo.root_path);

	let raw_churn = match get_file_churn(repo_path, &window) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: git churn failed: {}", e);
			return ExitCode::from(2);
		}
	};

	// Filter churn to indexed files
	let churn_inputs: Vec<repo_graph_classification::hotspot_scorer::ChurnInput> = raw_churn
		.into_iter()
		.filter(|entry| indexed_paths.contains(entry.file_path.as_str()))
		.map(|entry| repo_graph_classification::hotspot_scorer::ChurnInput {
			file_path: entry.file_path,
			commit_count: entry.commit_count,
			lines_changed: entry.lines_changed,
		})
		.collect();

	// Get per-file complexity
	let complexity_rows = match storage.query_complexity_by_file(&snapshot.snapshot_uid) {
		Ok(rows) => rows,
		Err(e) => {
			eprintln!("error: failed to read complexity measurements: {}", e);
			return ExitCode::from(2);
		}
	};

	let complexity_inputs: Vec<repo_graph_classification::hotspot_scorer::ComplexityInput> =
		complexity_rows
			.into_iter()
			.map(|row| repo_graph_classification::hotspot_scorer::ComplexityInput {
				file_path: row.file_path,
				sum_complexity: row.sum_complexity,
			})
			.collect();

	// Compute hotspots first
	let hotspots = repo_graph_classification::hotspot_scorer::compute_hotspots(
		&churn_inputs,
		&complexity_inputs,
	);

	// Get coverage measurements
	let coverage_rows = match storage.query_measurements_by_kind(&snapshot.snapshot_uid, "line_coverage") {
		Ok(rows) => rows,
		Err(e) => {
			eprintln!("error: failed to read coverage measurements: {}", e);
			return ExitCode::from(2);
		}
	};

	// Parse coverage measurements into CoverageInput with strict validation.
	// Malformed measurements abort (exit 2), matching gate surface contract.
	// target_stable_key format: {repo_uid}:{file_path}:FILE
	let expected_prefix = format!("{}:", repo_uid);
	let mut coverage_inputs: Vec<repo_graph_classification::risk_scorer::CoverageInput> =
		Vec::with_capacity(coverage_rows.len());

	for row in &coverage_rows {
		// Validate target_stable_key format
		let file_path = match row
			.target_stable_key
			.strip_prefix(&expected_prefix)
			.and_then(|s| s.strip_suffix(":FILE"))
		{
			Some(p) => p,
			None => {
				eprintln!(
					"error: malformed coverage measurement target_stable_key: {}",
					row.target_stable_key
				);
				return ExitCode::from(2);
			}
		};

		// Parse value_json strictly
		let v: serde_json::Value = match serde_json::from_str(&row.value_json) {
			Ok(v) => v,
			Err(e) => {
				eprintln!(
					"error: malformed coverage measurement JSON for {}: {}",
					file_path, e
				);
				return ExitCode::from(2);
			}
		};

		let line_coverage = match v.get("value").and_then(|v| v.as_f64()) {
			Some(c) => c,
			None => {
				eprintln!(
					"error: coverage measurement missing 'value' field for {}",
					file_path
				);
				return ExitCode::from(2);
			}
		};

		coverage_inputs.push(repo_graph_classification::risk_scorer::CoverageInput {
			file_path: file_path.to_string(),
			line_coverage,
		});
	}

	// Compute risk scores
	let risk_entries = repo_graph_classification::risk_scorer::compute_risk(&hotspots, &coverage_inputs);

	// Convert to output rows
	let results: Vec<RiskRow> = risk_entries
		.into_iter()
		.map(|r| RiskRow {
			file_path: r.file_path,
			risk_score: r.risk_score,
			hotspot_score: r.hotspot_score,
			line_coverage: r.line_coverage,
			lines_changed: r.lines_changed,
			sum_complexity: r.sum_complexity,
		})
		.collect();

	// Build envelope
	let count = results.len();
	let hotspot_count = hotspots.len();
	let coverage_count = coverage_inputs.len();

	let mut extra = serde_json::Map::new();
	extra.insert("since".to_string(), serde_json::Value::String(since.clone()));
	extra.insert(
		"formula".to_string(),
		serde_json::Value::String("hotspot_score * (1 - line_coverage)".to_string()),
	);
	extra.insert(
		"hotspot_files".to_string(),
		serde_json::Value::Number(hotspot_count.into()),
	);
	extra.insert(
		"coverage_files".to_string(),
		serde_json::Value::Number(coverage_count.into()),
	);
	extra.insert(
		"joined_files".to_string(),
		serde_json::Value::Number(count.into()),
	);

	let output = match build_envelope(
		&storage,
		"risk",
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
