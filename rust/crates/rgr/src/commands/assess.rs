//! Assess command family.
//!
//! Quality policy assessment for snapshots.
//!
//! # Boundary rules
//!
//! This module owns assess command-family behavior:
//! - `run_assess` handler
//! - assess-local argument parsing (inline)
//! - assess-local output shaping (inline JSON)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - assessment domain logic (belongs in `repo-graph-quality-policy-runner`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::open_storage;

// ── assess command ───────────────────────────────────────────────

/// Run quality policy assessment for a snapshot.
///
/// Full-snapshot recomputation: evaluates all active quality policies
/// against the target snapshot's measurements and persists assessments
/// atomically (replaces existing assessments for the snapshot).
///
/// Exit codes:
///   0 — success (assessments persisted)
///   1 — usage error
///   2 — runtime error (storage failure, invalid policy, missing baseline)
pub fn run_assess(args: &[String]) -> ExitCode {
	// Parse positional args and optional --baseline flag.
	let mut positional: Vec<&String> = Vec::new();
	let mut baseline_snapshot_uid: Option<String> = None;

	let mut i = 0;
	while i < args.len() {
		let arg = &args[i];
		match arg.as_str() {
			"--baseline" => {
				if i + 1 >= args.len() {
					eprintln!("error: --baseline requires a snapshot_uid argument");
					eprintln!("usage: rmap assess <db_path> <repo_uid> [--baseline <snapshot_uid>]");
					return ExitCode::from(1);
				}
				baseline_snapshot_uid = Some(args[i + 1].clone());
				i += 2;
			}
			_ if arg.starts_with('-') => {
				eprintln!("error: unknown flag: {}", arg);
				eprintln!("usage: rmap assess <db_path> <repo_uid> [--baseline <snapshot_uid>]");
				return ExitCode::from(1);
			}
			_ => {
				positional.push(arg);
				i += 1;
			}
		}
	}

	if positional.len() != 2 {
		eprintln!("usage: rmap assess <db_path> <repo_uid> [--baseline <snapshot_uid>]");
		return ExitCode::from(1);
	}

	let db_path = Path::new(positional[0]);
	let repo_uid = positional[1];

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Get latest snapshot for the repo.
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

	// Run assessment via the runner.
	// The runner takes ownership of storage because assess_snapshot
	// requires mutable access for atomic persistence.
	use repo_graph_quality_policy_runner::QualityPolicyRunner;

	let mut runner = QualityPolicyRunner::new(storage);
	let result = match runner.assess_snapshot(
		repo_uid,
		&snapshot.snapshot_uid,
		baseline_snapshot_uid.as_deref(),
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Build JSON output.
	let output = serde_json::json!({
		"command": "assess",
		"repo": repo_uid,
		"snapshot": snapshot.snapshot_uid,
		"baseline_snapshot": baseline_snapshot_uid,
		"assessments": {
			"total": result.total_assessments,
			"pass": result.pass_count,
			"fail": result.fail_count,
			"not_applicable": result.not_applicable_count,
			"not_comparable": result.not_comparable_count,
		},
		"baseline_required_count": result.baseline_required_count,
	});

	match serde_json::to_string_pretty(&output) {
		Ok(json) => {
			println!("{}", json);
			ExitCode::from(0)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}
