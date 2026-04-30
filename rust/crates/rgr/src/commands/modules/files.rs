//! Modules files command.
//!
//! Lists files owned by a specific module.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_files` handler
//! - `ModuleFileOutput` DTO
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)
//! - file ownership queries (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};
use super::shared::ModuleQueryContext;

// ── modules files command ────────────────────────────────────────

/// Output DTO for `modules files` command.
///
/// Dedicated CLI output shape — combines file metadata with ownership info.
#[derive(serde::Serialize)]
struct ModuleFileOutput {
	file_uid: String,
	path: String,
	language: Option<String>,
	assignment_kind: String,
	confidence: f64,
}

pub(super) fn run_modules_files(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap modules files <db_path> <repo_uid> <module>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let module_arg = &args[2];

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
			return ExitCode::from(2)
		}
	};

	// Load module context (don't need full graph facts, just context for resolution)
	let ctx = match ModuleQueryContext::load(&storage, &snapshot.snapshot_uid) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: failed to load module context: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve module argument
	let resolved_module = match ctx.resolve_module(module_arg) {
		Some(m) => m.clone(),
		None => {
			eprintln!("error: module not found: {}", module_arg);
			eprintln!("hint: use canonical path (e.g., 'packages/app') or module key");
			return ExitCode::from(1);
		}
	};

	// Load files for the resolved module.
	// First try the detailed query (TS-indexed repos).
	// If empty, fall back to context's owned files (Rust-indexed repos, degraded metadata).
	let files = match storage.get_files_for_module(
		&snapshot.snapshot_uid,
		&resolved_module.module_candidate_uid,
	) {
		Ok(f) if !f.is_empty() => f,
		Ok(_) => {
			// Fallback: use context's files_for_module (degraded: no language/assignment_kind/confidence)
			ctx.files_for_module(&resolved_module.module_candidate_uid)
				.into_iter()
				.map(|of| repo_graph_storage::crud::module_edges_support::ModuleFileEntry {
					file_uid: of.file_uid.clone(),
					path: of.file_path.clone(),
					language: None,
					assignment_kind: "inferred".to_string(),
					confidence: 1.0,
				})
				.collect()
		}
		Err(e) => {
			eprintln!("error: failed to load module files: {}", e);
			return ExitCode::from(2);
		}
	};

	// Map to output DTO
	let results: Vec<ModuleFileOutput> = files
		.into_iter()
		.map(|f| ModuleFileOutput {
			file_uid: f.file_uid,
			path: f.path,
			language: f.language,
			assignment_kind: f.assignment_kind,
			confidence: f.confidence,
		})
		.collect();

	let count = results.len();

	// Add module identity to envelope extras
	let mut extras = serde_json::Map::new();
	extras.insert(
		"module".to_string(),
		serde_json::json!({
			"module_uid": resolved_module.module_candidate_uid,
			"module_key": resolved_module.module_key,
			"canonical_root_path": resolved_module.canonical_root_path,
		}),
	);

	let output = match build_envelope(
		&storage,
		"modules files",
		repo_uid,
		&snapshot,
		serde_json::to_value(&results).unwrap(),
		count,
		extras,
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
