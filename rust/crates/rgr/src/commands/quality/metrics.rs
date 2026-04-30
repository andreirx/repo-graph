//! Metrics command.
//!
//! Quality Control Phase A: Query measurements for display.
//! Supports kind filter, sorting, and limit. Default sort: value desc.
//!
//! # Boundary rules
//!
//! This module owns metrics command behavior:
//! - `run_metrics` handler
//! - `parse_metrics_args`
//! - `MetricsRow`, `MetricsArgs`, `MetricsSort` DTOs
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - measurement storage queries (belongs in storage crate)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};

// ── metrics command ──────────────────────────────────────────────

/// Output row for metrics command.
/// Parses value_json to extract numeric value for sorting/display.
#[derive(serde::Serialize)]
struct MetricsRow {
	target_stable_key: String,
	kind: String,
	value: i64,
	source: String,
}

/// Parsed args for metrics command.
struct MetricsArgs {
	db_path: String,
	repo_uid: String,
	kind_filter: Option<String>,
	limit: usize,
	sort_by: MetricsSort,
}

enum MetricsSort {
	Value,  // desc by value
	Target, // asc by target_stable_key
}

fn parse_metrics_args(args: &[String]) -> Result<MetricsArgs, String> {
	if args.len() < 2 {
		return Err("usage: rmap metrics <db_path> <repo_uid> [--kind <k>] [--limit <n>] [--sort <value|target>]".to_string());
	}

	let db_path = args[0].clone();
	let repo_uid = args[1].clone();

	let mut kind_filter = None;
	let mut limit = 50usize;
	let mut sort_by = MetricsSort::Value;

	let mut i = 2;
	while i < args.len() {
		match args[i].as_str() {
			"--kind" => {
				if i + 1 >= args.len() {
					return Err("--kind requires a value".to_string());
				}
				kind_filter = Some(args[i + 1].clone());
				i += 2;
			}
			"--limit" => {
				if i + 1 >= args.len() {
					return Err("--limit requires a value".to_string());
				}
				limit = args[i + 1]
					.parse()
					.map_err(|_| "--limit must be a positive integer".to_string())?;
				i += 2;
			}
			"--sort" => {
				if i + 1 >= args.len() {
					return Err("--sort requires a value (value|target)".to_string());
				}
				sort_by = match args[i + 1].as_str() {
					"value" => MetricsSort::Value,
					"target" => MetricsSort::Target,
					other => return Err(format!("--sort must be 'value' or 'target', got '{}'", other)),
				};
				i += 2;
			}
			other => {
				return Err(format!("unknown option: {}", other));
			}
		}
	}

	Ok(MetricsArgs {
		db_path,
		repo_uid,
		kind_filter,
		limit,
		sort_by,
	})
}

pub fn run_metrics(args: &[String]) -> ExitCode {
	let parsed = match parse_metrics_args(args) {
		Ok(p) => p,
		Err(msg) => {
			eprintln!("{}", msg);
			return ExitCode::from(1);
		}
	};

	let db_path = Path::new(&parsed.db_path);
	let repo_uid = &parsed.repo_uid;

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

	// Query measurements with optional kind filter
	let measurements = match storage.query_measurements_extended(
		&snapshot.snapshot_uid,
		parsed.kind_filter.as_deref(),
	) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Parse value_json and build output rows
	let mut rows: Vec<MetricsRow> = measurements
		.into_iter()
		.filter_map(|m| {
			// value_json is {"value": N} - extract the numeric value
			let value: i64 = serde_json::from_str::<serde_json::Value>(&m.value_json)
				.ok()
				.and_then(|v| v.get("value")?.as_i64())
				.unwrap_or(0);

			Some(MetricsRow {
				target_stable_key: m.target_stable_key,
				kind: m.kind,
				value,
				source: m.source,
			})
		})
		.collect();

	// Sort
	match parsed.sort_by {
		MetricsSort::Value => rows.sort_by(|a, b| b.value.cmp(&a.value)),
		MetricsSort::Target => rows.sort_by(|a, b| a.target_stable_key.cmp(&b.target_stable_key)),
	}

	// Apply limit
	rows.truncate(parsed.limit);

	let count = rows.len();
	let output = match build_envelope(
		&storage,
		"metrics",
		repo_uid,
		&snapshot,
		serde_json::to_value(&rows).unwrap(),
		count,
		serde_json::Map::new(),
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
