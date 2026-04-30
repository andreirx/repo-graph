//! Modules boundary command.
//!
//! Creates discovered-module boundary declarations.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_boundary` handler
//! - `parse_boundary_args` helper
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)
//! - declaration insertion (belongs in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{open_storage, utc_now_iso8601};
use super::shared::ModuleQueryContext;

// ── modules boundary command ─────────────────────────────────────

pub(super) fn run_modules_boundary(args: &[String]) -> ExitCode {
	// Parse args: <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]
	let (positional, forbids, reason) = match parse_boundary_args(args) {
		Ok(v) => v,
		Err(msg) => {
			eprintln!("error: {}", msg);
			eprintln!(
				"usage: rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]"
			);
			return ExitCode::from(1);
		}
	};

	if positional.len() != 3 {
		eprintln!(
			"usage: rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]"
		);
		return ExitCode::from(1);
	}

	let forbids = match forbids {
		Some(f) => f,
		None => {
			eprintln!("error: --forbids is required");
			eprintln!(
				"usage: rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]"
			);
			return ExitCode::from(1);
		}
	};

	let db_path = Path::new(&positional[0]);
	let repo_uid = &positional[1];
	let source_arg = &positional[2];

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

	// Load module context (don't need full graph facts)
	let ctx = match ModuleQueryContext::load(&storage, &snapshot.snapshot_uid) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: failed to load module context: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve source module
	let source_path = match ctx.resolve_module(source_arg) {
		Some(m) => m.canonical_root_path.clone(),
		None => {
			eprintln!("error: source module not found: {}", source_arg);
			eprintln!("hint: use canonical path (e.g., 'packages/app') or module key");
			return ExitCode::from(1);
		}
	};

	// Resolve target module
	let target_path = match ctx.resolve_module(&forbids) {
		Some(m) => m.canonical_root_path.clone(),
		None => {
			eprintln!("error: target module not found: {}", forbids);
			eprintln!("hint: use canonical path (e.g., 'packages/core') or module key");
			return ExitCode::from(1);
		}
	};

	// Validate: source != target
	if source_path == target_path {
		eprintln!(
			"error: source and target must be different modules (both resolve to '{}')",
			source_path
		);
		return ExitCode::from(1);
	}

	// Build discovered_module boundary declaration
	use repo_graph_storage::crud::declarations::{
		discovered_module_boundary_identity_key, DeclarationInsert,
	};

	let value_json = if let Some(ref r) = reason {
		serde_json::json!({
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": source_path },
			"forbids": { "canonicalRootPath": target_path },
			"reason": r
		})
	} else {
		serde_json::json!({
			"selectorDomain": "discovered_module",
			"source": { "canonicalRootPath": source_path },
			"forbids": { "canonicalRootPath": target_path }
		})
	};

	let target_stable_key = format!("{}:{}:MODULE", repo_uid, source_path);

	let decl = DeclarationInsert {
		identity_key: discovered_module_boundary_identity_key(repo_uid, &source_path, &target_path),
		repo_uid: repo_uid.to_string(),
		target_stable_key,
		kind: "boundary".to_string(),
		value_json: value_json.to_string(),
		created_at: utc_now_iso8601(),
		created_by: Some("rmap".to_string()),
		supersedes_uid: None,
		authored_basis_json: None,
	};

	let result = match storage.insert_declaration(&decl) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: failed to insert declaration: {}", e);
			return ExitCode::from(2);
		}
	};

	// Output JSON result
	let output = serde_json::json!({
		"declaration_uid": result.declaration_uid,
		"source": source_path,
		"target": target_path,
		"inserted": result.inserted,
	});

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

/// Parse --forbids and --reason flags from boundary command args.
fn parse_boundary_args(
	args: &[String],
) -> Result<(Vec<String>, Option<String>, Option<String>), String> {
	let mut positional = Vec::new();
	let mut forbids: Option<String> = None;
	let mut reason: Option<String> = None;
	let mut i = 0;

	while i < args.len() {
		match args[i].as_str() {
			"--forbids" => {
				if forbids.is_some() {
					return Err("repeated --forbids flag".to_string());
				}
				i += 1;
				if i >= args.len() {
					return Err("missing value after --forbids".to_string());
				}
				forbids = Some(args[i].clone());
			}
			"--reason" => {
				if reason.is_some() {
					return Err("repeated --reason flag".to_string());
				}
				i += 1;
				if i >= args.len() {
					return Err("missing value after --reason".to_string());
				}
				reason = Some(args[i].clone());
			}
			other if other.starts_with("--") => {
				return Err(format!("unknown flag: {}", other));
			}
			_ => {
				positional.push(args[i].clone());
			}
		}
		i += 1;
	}

	Ok((positional, forbids, reason))
}
