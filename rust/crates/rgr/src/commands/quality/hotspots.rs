//! Hotspots command.
//!
//! RS-MS-3b: Query-time hotspot analysis (churn x complexity).
//! No persistence. Git is the authoritative churn source.
//! Complexity from stored measurements.
//!
//! # Boundary rules
//!
//! This module owns hotspots command behavior:
//! - `run_hotspots` handler
//! - `parse_hotspot_args`
//! - `HotspotRow`, `HotspotFiltering`, `HotspotArgs` DTOs
//! - `is_vendored_path` helper
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - hotspot scoring (belongs in `repo-graph-classification`)
//! - git churn extraction (belongs in `repo-graph-git`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};

// ── hotspots command ─────────────────────────────────────────────

/// Output row for hotspots command.
#[derive(serde::Serialize)]
struct HotspotRow {
	file_path: String,
	commit_count: u64,
	lines_changed: u64,
	sum_complexity: u64,
	hotspot_score: u64,
}

/// Filtering metadata for hotspots output.
#[derive(serde::Serialize)]
struct HotspotFiltering {
	exclude_tests: bool,
	exclude_vendored: bool,
	excluded_count: usize,
	excluded_tests_count: usize,
	excluded_vendored_count: usize,
}

/// Vendored directory segments (exact match only).
const VENDORED_SEGMENTS: &[&str] = &[
	"vendor", "vendors", "third_party", "third-party",
	"external", "deps", "node_modules",
];

/// Check if path contains a vendored directory segment.
fn is_vendored_path(path: &str) -> bool {
	path.split('/')
		.any(|segment| {
			let lower = segment.to_lowercase();
			VENDORED_SEGMENTS.contains(&lower.as_str())
		})
}

/// Parsed hotspot command arguments.
struct HotspotArgs<'a> {
	db_path: &'a Path,
	repo_uid: &'a str,
	since: String,
	exclude_tests: bool,
	exclude_vendored: bool,
}

/// Parse hotspots command args.
fn parse_hotspot_args(args: &[String]) -> Result<HotspotArgs<'_>, String> {
	if args.len() < 2 {
		return Err("usage: rmap hotspots <db_path> <repo_uid> [--since <expr>] [--exclude-tests] [--exclude-vendored]".to_string());
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

	let mut since = "90.days.ago".to_string();
	let mut exclude_tests = false;
	let mut exclude_vendored = false;

	let mut i = 2;
	while i < args.len() {
		match args[i].as_str() {
			"--since" => {
				if i + 1 >= args.len() {
					return Err("--since requires a value".to_string());
				}
				since = args[i + 1].clone();
				i += 2;
			}
			"--exclude-tests" => {
				exclude_tests = true;
				i += 1;
			}
			"--exclude-vendored" => {
				exclude_vendored = true;
				i += 1;
			}
			_ => {
				return Err(format!("unknown argument: {}", args[i]));
			}
		}
	}

	Ok(HotspotArgs {
		db_path,
		repo_uid,
		since,
		exclude_tests,
		exclude_vendored,
	})
}

pub fn run_hotspots(args: &[String]) -> ExitCode {
	let parsed = match parse_hotspot_args(args) {
		Ok(p) => p,
		Err(msg) => {
			eprintln!("{}", msg);
			return ExitCode::from(1);
		}
	};

	let db_path = parsed.db_path;
	let repo_uid = parsed.repo_uid;
	let since = parsed.since;
	let exclude_tests = parsed.exclude_tests;
	let exclude_vendored = parsed.exclude_vendored;

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

	// Get per-file complexity via proper join (measurements -> nodes -> files).
	// RS-MS-3a fix: avoids parsing stable_key strings which have the format
	// `{repo}:{path}#{symbol}:SYMBOL:{kind}`, not `{repo}:{path}:SYMBOL:{name}`.
	let complexity_rows = match storage.query_complexity_by_file(&snapshot.snapshot_uid) {
		Ok(rows) => rows,
		Err(e) => {
			eprintln!("error: failed to read complexity measurements: {}", e);
			return ExitCode::from(2);
		}
	};

	// Convert to ComplexityInput for the scorer
	let complexity_inputs: Vec<repo_graph_classification::hotspot_scorer::ComplexityInput> =
		complexity_rows
			.into_iter()
			.map(|row| repo_graph_classification::hotspot_scorer::ComplexityInput {
				file_path: row.file_path,
				sum_complexity: row.sum_complexity,
			})
			.collect();

	// Compute hotspots
	let hotspots = repo_graph_classification::hotspot_scorer::compute_hotspots(
		&churn_inputs,
		&complexity_inputs,
	);

	// Build file_path -> is_test lookup
	let test_files: std::collections::HashSet<&str> = indexed_files
		.iter()
		.filter(|f| f.is_test)
		.map(|f| f.path.as_str())
		.collect();

	// Apply filtering and count exclusions
	let mut excluded_tests_count = 0usize;
	let mut excluded_vendored_count = 0usize;
	let mut excluded_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

	let results: Vec<HotspotRow> = hotspots
		.into_iter()
		.filter_map(|h| {
			let is_test = test_files.contains(h.file_path.as_str());
			let is_vendored = is_vendored_path(&h.file_path);

			let exclude_as_test = exclude_tests && is_test;
			let exclude_as_vendored = exclude_vendored && is_vendored;

			if exclude_as_test {
				excluded_tests_count += 1;
			}
			if exclude_as_vendored {
				excluded_vendored_count += 1;
			}
			if exclude_as_test || exclude_as_vendored {
				excluded_paths.insert(h.file_path.clone());
				return None;
			}

			Some(HotspotRow {
				file_path: h.file_path,
				commit_count: h.commit_count,
				lines_changed: h.lines_changed,
				sum_complexity: h.sum_complexity,
				hotspot_score: h.hotspot_score,
			})
		})
		.collect();

	let excluded_count = excluded_paths.len();

	// Build envelope
	let count = results.len();
	let mut extra = serde_json::Map::new();
	extra.insert("since".to_string(), serde_json::Value::String(since.clone()));
	extra.insert(
		"formula".to_string(),
		serde_json::Value::String("lines_changed * sum_complexity".to_string()),
	);

	// Add filtering metadata only when filters are active
	if exclude_tests || exclude_vendored {
		let filtering = HotspotFiltering {
			exclude_tests,
			exclude_vendored,
			excluded_count,
			excluded_tests_count,
			excluded_vendored_count,
		};
		extra.insert(
			"filtering".to_string(),
			serde_json::to_value(&filtering).unwrap(),
		);
	}

	let output = match build_envelope(
		&storage,
		"hotspots",
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
