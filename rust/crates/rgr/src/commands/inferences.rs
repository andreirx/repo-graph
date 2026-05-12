//! Inferences command family.
//!
//! FD-SUPPORT-2: Query surface for Layer 3 inferences.
//!
//! # Boundary rules
//!
//! This module owns inferences command-family behavior:
//! - `run_inferences`, `run_inferences_list` handlers
//! - inferences-family DTOs
//! - inferences-family argument parsing
//! - inferences-family output shaping
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - storage queries (belongs in storage crate)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage, resolve_repo_ref};

// ── inferences command ───────────────────────────────────────────

pub fn run_inferences(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage:");
		eprintln!("  rmap inferences list <db_path> <repo_uid> [--kind <kind>]");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"list" => run_inferences_list(&args[1..]),
		other => {
			eprintln!("unknown inferences subcommand: {}", other);
			eprintln!("usage:");
			eprintln!("  rmap inferences list <db_path> <repo_uid> [--kind <kind>]");
			ExitCode::from(1)
		}
	}
}

// ── inferences list command ──────────────────────────────────────

/// Output DTO for `inferences list` command.
#[derive(serde::Serialize)]
struct InferenceListEntry {
	inference_uid: String,
	target_stable_key: String,
	kind: String,
	/// Parsed value JSON (null if invalid).
	value: Option<serde_json::Value>,
	confidence: f64,
	extractor: String,
	created_at: String,
}

/// Parse inferences list args.
/// Returns (db_path, repo_uid, kind_filter) or error.
fn parse_inferences_list_args(
	args: &[String],
) -> Result<(&Path, &str, Option<String>), String> {
	if args.len() < 2 {
		return Err(
			"usage: rmap inferences list <db_path> <repo_uid> [--kind <kind>]".to_string(),
		);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = args[1].as_str();

	let mut kind_filter = None;
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
			other => {
				return Err(format!("unknown option: {}", other));
			}
		}
	}

	Ok((db_path, repo_uid, kind_filter))
}

fn run_inferences_list(args: &[String]) -> ExitCode {
	let (db_path, repo_ref, kind_filter) = match parse_inferences_list_args(args) {
		Ok(v) => v,
		Err(msg) => {
			eprintln!("{}", msg);
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

	// Resolve repo ref (UID, name, or root_path).
	let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
		Ok(uid) => uid,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	let snapshot = match storage.get_latest_snapshot(&repo_uid) {
		Ok(Some(snap)) => snap,
		Ok(None) => {
			eprintln!("error: no snapshot found for repo '{}'", repo_ref);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Load inferences with optional kind filter.
	let inferences = match storage.list_inferences_for_snapshot(
		&snapshot.snapshot_uid,
		kind_filter.as_deref(),
	) {
		Ok(i) => i,
		Err(e) => {
			eprintln!("error: failed to load inferences: {}", e);
			return ExitCode::from(2);
		}
	};

	// Build output entries.
	let results: Vec<InferenceListEntry> = inferences
		.into_iter()
		.map(|i| InferenceListEntry {
			inference_uid: i.inference_uid,
			target_stable_key: i.target_stable_key,
			kind: i.kind,
			value: serde_json::from_str(&i.value_json).ok(),
			confidence: i.confidence,
			extractor: i.extractor,
			created_at: i.created_at,
		})
		.collect();

	// Build envelope.
	let count = results.len();
	let mut extra = serde_json::Map::new();

	// Add filter info to envelope.
	if let Some(ref k) = kind_filter {
		extra.insert(
			"filter_kind".to_string(),
			serde_json::Value::String(k.clone()),
		);
	}

	let output = match build_envelope(
		&storage,
		"inferences list",
		&repo_uid,
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
