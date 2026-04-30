//! Modules deps command.
//!
//! Shows module dependency edges (import-based).
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_deps` handler
//! - `parse_deps_args` helper
//! - `DepsDirection` enum
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};
use super::shared::load_module_graph_facts;

// ── modules deps command ─────────────────────────────────────────

/// Direction filter for module deps command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepsDirection {
	/// Show all cross-module edges (default).
	All,
	/// Show only edges where the specified module is the source.
	Outbound,
	/// Show only edges where the specified module is the target.
	Inbound,
}

pub(super) fn run_modules_deps(args: &[String]) -> ExitCode {
	// Parse args: <db_path> <repo_uid> [module] [--outbound|--inbound]
	let (positional, direction) = match parse_deps_args(args) {
		Ok(v) => v,
		Err(msg) => {
			eprintln!("error: {}", msg);
			eprintln!("usage: rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
			return ExitCode::from(1);
		}
	};

	if positional.len() < 2 || positional.len() > 3 {
		eprintln!("usage: rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&positional[0]);
	let repo_uid = &positional[1];
	let module_filter: Option<&str> = positional.get(2).map(|s| s.as_str());

	// Direction flag requires module filter
	if direction != DepsDirection::All && module_filter.is_none() {
		eprintln!("error: --outbound and --inbound require a module argument");
		eprintln!("usage: rmap modules deps <db_path> <repo_uid> <module> --outbound");
		return ExitCode::from(1);
	}

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

	// ── Step 1: Load module graph facts (single load with precomputed edges) ─
	let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
		Ok(f) => f,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 2: Resolve module filter argument against discovered modules ─
	// Resolution precedence: canonical_root_path exact → module_key exact → module_uid exact.
	// Unknown module → error (not empty results).
	let resolved_module_path: Option<String> = match module_filter {
		Some(filter) => match facts.resolve_module(filter) {
			Some(m) => Some(m.canonical_root_path.clone()),
			None => {
				eprintln!("error: module not found: {}", filter);
				eprintln!("hint: use canonical path (e.g., 'packages/app') or module key");
				return ExitCode::from(1);
			}
		},
		None => None,
	};

	// ── Step 3: Filter precomputed edges ──────────────────────────
	let filtered_edges: Vec<_> = match &resolved_module_path {
		Some(module_path) => {
			facts.edges
				.iter()
				.filter(|e| match direction {
					DepsDirection::All => {
						e.source_canonical_path == *module_path
							|| e.target_canonical_path == *module_path
					}
					DepsDirection::Outbound => e.source_canonical_path == *module_path,
					DepsDirection::Inbound => e.target_canonical_path == *module_path,
				})
				.collect()
		}
		None => facts.edges.iter().collect(),
	};

	// Build JSON output
	let results: Vec<serde_json::Value> = filtered_edges
		.iter()
		.map(|e| {
			serde_json::json!({
				"source": e.source_canonical_path,
				"target": e.target_canonical_path,
				"import_count": e.import_count,
				"source_file_count": e.source_file_count,
			})
		})
		.collect();

	let count = results.len();

	// Build extra fields for envelope
	let mut extra = serde_json::Map::new();
	if let Some(ref m) = resolved_module_path {
		extra.insert("module".to_string(), serde_json::Value::String(m.clone()));
	}
	extra.insert(
		"direction".to_string(),
		serde_json::Value::String(match direction {
			DepsDirection::All => "all".to_string(),
			DepsDirection::Outbound => "outbound".to_string(),
			DepsDirection::Inbound => "inbound".to_string(),
		}),
	);
	extra.insert(
		"diagnostics".to_string(),
		serde_json::json!({
			"imports_total": facts.diagnostics.imports_total,
			"imports_cross_module": facts.diagnostics.imports_cross_module,
			"imports_intra_module": facts.diagnostics.imports_intra_module,
			"imports_source_unowned": facts.diagnostics.imports_source_unowned,
			"imports_target_unowned": facts.diagnostics.imports_target_unowned,
		}),
	);

	let output = match build_envelope(
		&storage,
		"modules deps",
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

/// Parse --outbound / --inbound flags from args.
fn parse_deps_args(args: &[String]) -> Result<(Vec<String>, DepsDirection), String> {
	let mut positional = Vec::new();
	let mut direction = DepsDirection::All;
	let mut direction_set = false;

	for arg in args {
		match arg.as_str() {
			"--outbound" => {
				if direction_set {
					return Err("cannot specify both --outbound and --inbound".to_string());
				}
				direction = DepsDirection::Outbound;
				direction_set = true;
			}
			"--inbound" => {
				if direction_set {
					return Err("cannot specify both --outbound and --inbound".to_string());
				}
				direction = DepsDirection::Inbound;
				direction_set = true;
			}
			other if other.starts_with("--") => {
				return Err(format!("unknown flag: {}", other));
			}
			_ => {
				positional.push(arg.clone());
			}
		}
	}

	Ok((positional, direction))
}
