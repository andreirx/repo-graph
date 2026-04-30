//! Surfaces command family.
//!
//! Project surface discovery and inspection.
//!
//! # Boundary rules
//!
//! This module owns surfaces command-family behavior:
//! - `run_surfaces`, `run_surfaces_list`, `run_surfaces_show` handlers
//! - surfaces-family DTOs
//! - surfaces-family argument parsing
//! - surfaces-family output shaping
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - storage queries (belongs in storage crate)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage, resolve_repo_ref};

// ── surfaces command ─────────────────────────────────────────────

pub fn run_surfaces(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage:");
		eprintln!("  rmap surfaces list <db_path> <repo_uid> [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>]");
		eprintln!("  rmap surfaces show <db_path> <repo_uid> <surface_ref>");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"list" => run_surfaces_list(&args[1..]),
		"show" => run_surfaces_show(&args[1..]),
		other => {
			eprintln!("unknown surfaces subcommand: {}", other);
			eprintln!("usage:");
			eprintln!("  rmap surfaces list <db_path> <repo_uid> [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>]");
			eprintln!("  rmap surfaces show <db_path> <repo_uid> <surface_ref>");
			ExitCode::from(1)
		}
	}
}

// ── surfaces list command ────────────────────────────────────────

/// Output DTO for `surfaces list` command.
#[derive(serde::Serialize)]
struct SurfaceListEntry {
	project_surface_uid: String,
	module_candidate_uid: String,
	/// Module display name (from module_candidates join).
	module_display_name: Option<String>,
	/// Module canonical root path (from module_candidates join).
	module_root_path: Option<String>,
	surface_kind: String,
	display_name: Option<String>,
	root_path: String,
	entrypoint_path: Option<String>,
	build_system: String,
	runtime_kind: String,
	confidence: f64,
	/// Evidence item count for this surface.
	evidence_count: u64,
	// Identity fields (nullable for legacy rows).
	source_type: Option<String>,
	source_specific_id: Option<String>,
	stable_surface_key: Option<String>,
}

/// Parse surfaces list args.
/// Returns (db_path, repo_uid, filter) or error.
fn parse_surfaces_list_args(args: &[String]) -> Result<(&Path, &str, repo_graph_storage::crud::project_surfaces::SurfaceFilter), String> {
	use repo_graph_storage::crud::project_surfaces::SurfaceFilter;

	if args.len() < 2 {
		return Err("usage: rmap surfaces list <db_path> <repo_uid> [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>]".to_string());
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = args[1].as_str();

	let mut filter = SurfaceFilter::default();
	let mut i = 2;
	while i < args.len() {
		match args[i].as_str() {
			"--kind" => {
				if i + 1 >= args.len() {
					return Err("--kind requires a value".to_string());
				}
				filter.kind = Some(args[i + 1].clone());
				i += 2;
			}
			"--runtime" => {
				if i + 1 >= args.len() {
					return Err("--runtime requires a value".to_string());
				}
				filter.runtime = Some(args[i + 1].clone());
				i += 2;
			}
			"--source" => {
				if i + 1 >= args.len() {
					return Err("--source requires a value".to_string());
				}
				filter.source = Some(args[i + 1].clone());
				i += 2;
			}
			"--module" => {
				if i + 1 >= args.len() {
					return Err("--module requires a value".to_string());
				}
				filter.module = Some(args[i + 1].clone());
				i += 2;
			}
			other => {
				return Err(format!("unknown option: {}", other));
			}
		}
	}

	Ok((db_path, repo_uid, filter))
}

fn run_surfaces_list(args: &[String]) -> ExitCode {
	let (db_path, repo_ref, filter) = match parse_surfaces_list_args(args) {
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

	// Load surfaces with filtering.
	let surfaces = match storage.get_project_surfaces_for_snapshot(&snapshot.snapshot_uid, &filter) {
		Ok(s) => s,
		Err(e) => {
			eprintln!("error: failed to load surfaces: {}", e);
			return ExitCode::from(2);
		}
	};

	// Load module candidates for display_name/root_path enrichment.
	let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("error: failed to load module candidates: {}", e);
			return ExitCode::from(2);
		}
	};
	let module_map: std::collections::HashMap<&str, &repo_graph_storage::types::ModuleCandidate> =
		modules.iter().map(|m| (m.module_candidate_uid.as_str(), m)).collect();

	// Load evidence counts.
	let evidence_counts = match storage.count_evidence_by_surface(&snapshot.snapshot_uid) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: failed to count evidence: {}", e);
			return ExitCode::from(2);
		}
	};

	// Build output entries.
	let results: Vec<SurfaceListEntry> = surfaces
		.into_iter()
		.map(|s| {
			let module = module_map.get(s.module_candidate_uid.as_str());
			SurfaceListEntry {
				project_surface_uid: s.project_surface_uid.clone(),
				module_candidate_uid: s.module_candidate_uid.clone(),
				module_display_name: module.and_then(|m| m.display_name.clone()),
				module_root_path: module.map(|m| m.canonical_root_path.clone()),
				surface_kind: s.surface_kind,
				display_name: s.display_name,
				root_path: s.root_path,
				entrypoint_path: s.entrypoint_path,
				build_system: s.build_system,
				runtime_kind: s.runtime_kind,
				confidence: s.confidence,
				evidence_count: *evidence_counts.get(&s.project_surface_uid).unwrap_or(&0),
				source_type: s.source_type,
				source_specific_id: s.source_specific_id,
				stable_surface_key: s.stable_surface_key,
			}
		})
		.collect();

	// Build envelope.
	let count = results.len();
	let mut extra = serde_json::Map::new();

	// Add filter info to envelope.
	if let Some(ref k) = filter.kind {
		extra.insert("filter_kind".to_string(), serde_json::Value::String(k.clone()));
	}
	if let Some(ref r) = filter.runtime {
		extra.insert("filter_runtime".to_string(), serde_json::Value::String(r.clone()));
	}
	if let Some(ref s) = filter.source {
		extra.insert("filter_source".to_string(), serde_json::Value::String(s.clone()));
	}
	if let Some(ref m) = filter.module {
		extra.insert("filter_module".to_string(), serde_json::Value::String(m.clone()));
	}

	let output = match build_envelope(
		&storage,
		"surfaces list",
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

// ── surfaces show command ────────────────────────────────────────

/// Output DTO for `surfaces show` command.
#[derive(serde::Serialize)]
struct SurfaceShowOutput {
	surface: SurfaceDetail,
	module: Option<ModuleRef>,
	evidence: Vec<EvidenceItem>,
}

#[derive(serde::Serialize)]
struct SurfaceDetail {
	project_surface_uid: String,
	surface_kind: String,
	display_name: Option<String>,
	root_path: String,
	entrypoint_path: Option<String>,
	build_system: String,
	runtime_kind: String,
	confidence: f64,
	source_type: Option<String>,
	source_specific_id: Option<String>,
	stable_surface_key: Option<String>,
	/// Metadata JSON with fallback to raw string when parsing fails.
	/// - `parsed`: the parsed JSON when valid, null otherwise
	/// - `raw`: the raw string when parsing fails, null when valid or absent
	/// - `parse_error`: error message when parsing fails, null otherwise
	metadata_json: MetadataJsonField,
}

/// Metadata JSON output with fallback for invalid JSON.
///
/// Preserves inspectability of corrupt/legacy metadata by including
/// the raw string and parse error when JSON parsing fails.
#[derive(serde::Serialize)]
struct MetadataJsonField {
	/// Parsed JSON value (null if absent or invalid).
	parsed: Option<serde_json::Value>,
	/// Raw string (null if absent or successfully parsed).
	raw: Option<String>,
	/// Parse error message (null if absent or successfully parsed).
	parse_error: Option<String>,
}

#[derive(serde::Serialize)]
struct ModuleRef {
	module_candidate_uid: String,
	module_key: String,
	display_name: Option<String>,
	canonical_root_path: String,
}

#[derive(serde::Serialize)]
struct EvidenceItem {
	source_type: String,
	source_path: String,
	evidence_kind: String,
	confidence: f64,
	payload: Option<serde_json::Value>,
}

fn run_surfaces_show(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap surfaces show <db_path> <repo_uid> <surface_ref>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_ref = &args[1];
	let surface_ref = &args[2];

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

	// Resolve surface by ref.
	let surface = match storage.get_project_surface_by_ref(&snapshot.snapshot_uid, surface_ref) {
		Ok(Some(s)) => s,
		Ok(None) => {
			eprintln!("error: surface not found: {}", surface_ref);
			return ExitCode::from(1);
		}
		Err(e) => {
			// Ambiguity or other error.
			eprintln!("error: {}", e);
			return ExitCode::from(1);
		}
	};

	// Load owning module by UID (not by key).
	let module = match storage.get_module_by_uid(&surface.module_candidate_uid) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("error: failed to load module: {}", e);
			return ExitCode::from(2);
		}
	};

	// Load evidence.
	let evidence_rows = match storage.get_project_surface_evidence(&surface.project_surface_uid) {
		Ok(e) => e,
		Err(e) => {
			eprintln!("error: failed to load evidence: {}", e);
			return ExitCode::from(2);
		}
	};

	// Build output.
	let output = SurfaceShowOutput {
		surface: SurfaceDetail {
			project_surface_uid: surface.project_surface_uid,
			surface_kind: surface.surface_kind,
			display_name: surface.display_name,
			root_path: surface.root_path,
			entrypoint_path: surface.entrypoint_path,
			build_system: surface.build_system,
			runtime_kind: surface.runtime_kind,
			confidence: surface.confidence,
			source_type: surface.source_type,
			source_specific_id: surface.source_specific_id,
			stable_surface_key: surface.stable_surface_key,
			// Parse metadata_json; preserve raw string when parsing fails.
			metadata_json: match &surface.metadata_json {
				None => MetadataJsonField {
					parsed: None,
					raw: None,
					parse_error: None,
				},
				Some(raw) => match serde_json::from_str(raw) {
					Ok(parsed) => MetadataJsonField {
						parsed: Some(parsed),
						raw: None,
						parse_error: None,
					},
					Err(e) => MetadataJsonField {
						parsed: None,
						raw: Some(raw.clone()),
						parse_error: Some(e.to_string()),
					},
				},
			},
		},
		module: module.map(|m| ModuleRef {
			module_candidate_uid: m.module_candidate_uid,
			module_key: m.module_key,
			display_name: m.display_name,
			canonical_root_path: m.canonical_root_path,
		}),
		evidence: evidence_rows
			.into_iter()
			.map(|e| EvidenceItem {
				source_type: e.source_type,
				source_path: e.source_path,
				evidence_kind: e.evidence_kind,
				confidence: e.confidence,
				payload: e.payload_json.as_ref().and_then(|p| serde_json::from_str(p).ok()),
			})
			.collect(),
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
