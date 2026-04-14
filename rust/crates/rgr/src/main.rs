//! Minimal Rust CLI for repo-graph.
//!
//! Commands:
//!   rgr-rust index   <repo_path> <db_path>
//!   rgr-rust refresh <repo_path> <db_path>
//!   rgr-rust trust   <db_path> <repo_uid>
//!   rgr-rust callers <db_path> <repo_uid> <symbol> [--edge-types <types>]
//!   rgr-rust callees <db_path> <repo_uid> <symbol> [--edge-types <types>]
//!   rgr-rust path    <db_path> <repo_uid> <from> <to>
//!   rgr-rust imports <db_path> <repo_uid> <file_path>
//!   rgr-rust violations <db_path> <repo_uid>
//!   rgr-rust dead    <db_path> <repo_uid> [kind]
//!   rgr-rust cycles  <db_path> <repo_uid>
//!   rgr-rust stats   <db_path> <repo_uid>
//!
//!   rgr-rust gate    <db_path> <repo_uid>
//!
//! Exit codes:
//!   0 — success (gate: all pass)
//!   1 — usage error (gate: any fail)
//!   2 — runtime error (gate: incomplete)

mod gate;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
	let args: Vec<String> = std::env::args().collect();

	if args.len() < 2 {
		print_usage();
		return ExitCode::from(1);
	}

	match args[1].as_str() {
		"index" => run_index(&args[2..]),
		"refresh" => run_refresh(&args[2..]),
		"trust" => run_trust(&args[2..]),
		"callers" => run_callers(&args[2..]),
		"callees" => run_callees(&args[2..]),
		"path" => run_path(&args[2..]),
		"imports" => run_imports(&args[2..]),
		"violations" => run_violations(&args[2..]),
		"gate" => run_gate(&args[2..]),
		"dead" => run_dead(&args[2..]),
		"cycles" => run_cycles(&args[2..]),
		"stats" => run_stats(&args[2..]),
		other => {
			eprintln!("unknown command: {}", other);
			print_usage();
			ExitCode::from(1)
		}
	}
}

fn print_usage() {
	eprintln!("usage:");
	eprintln!("  rgr-rust index   <repo_path> <db_path>");
	eprintln!("  rgr-rust refresh <repo_path> <db_path>");
	eprintln!("  rgr-rust trust   <db_path> <repo_uid>");
	eprintln!("  rgr-rust callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
	eprintln!("  rgr-rust callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
	eprintln!("  rgr-rust path    <db_path> <repo_uid> <from> <to>");
	eprintln!("  rgr-rust imports <db_path> <repo_uid> <file_path>");
	eprintln!("  rgr-rust violations <db_path> <repo_uid>");
	eprintln!("  rgr-rust gate       <db_path> <repo_uid>");
	eprintln!("  rgr-rust dead    <db_path> <repo_uid> [kind]");
	eprintln!("  rgr-rust cycles  <db_path> <repo_uid>");
	eprintln!("  rgr-rust stats   <db_path> <repo_uid>");
}

// ── index command ────────────────────────────────────────────────

fn run_index(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust index <repo_path> <db_path>");
		return ExitCode::from(1);
	}

	let repo_path = Path::new(&args[0]);
	let db_path = Path::new(&args[1]);

	if !repo_path.is_dir() {
		eprintln!(
			"error: repo path does not exist or is not a directory: {}",
			repo_path.display()
		);
		return ExitCode::from(1);
	}

	let repo_uid = repo_path
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or("repo");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	match index_path(repo_path, db_path, repo_uid, &ComposeOptions::default()) {
		Ok(result) => {
			eprintln!(
				"indexed {} files, {} nodes, {} edges ({} unresolved) → {}",
				result.files_total,
				result.nodes_total,
				result.edges_total,
				result.edges_unresolved,
				result.snapshot_uid,
			);
			ExitCode::SUCCESS
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── refresh command ──────────────────────────────────────────────

fn run_refresh(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust refresh <repo_path> <db_path>");
		return ExitCode::from(1);
	}

	let repo_path = Path::new(&args[0]);
	let db_path = Path::new(&args[1]);

	if !repo_path.is_dir() {
		eprintln!(
			"error: repo path does not exist or is not a directory: {}",
			repo_path.display()
		);
		return ExitCode::from(1);
	}

	let repo_uid = repo_path
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or("repo");

	use repo_graph_repo_index::compose::{refresh_path, ComposeOptions};
	match refresh_path(repo_path, db_path, repo_uid, &ComposeOptions::default()) {
		Ok(result) => {
			eprintln!(
				"refreshed {} files, {} nodes, {} edges ({} unresolved) → {}",
				result.files_total,
				result.nodes_total,
				result.edges_total,
				result.edges_unresolved,
				result.snapshot_uid,
			);
			ExitCode::SUCCESS
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── trust command ────────────────────────────────────────────────

fn run_trust(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust trust <db_path> <repo_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

	// Open storage.
	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Get latest snapshot.
	let snapshot = match storage.get_latest_snapshot(repo_uid) {
		Ok(Some(snap)) => snap,
		Ok(None) => {
			eprintln!(
				"error: no snapshot found for repo '{}'",
				repo_uid
			);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: failed to get latest snapshot: {}", e);
			return ExitCode::from(2);
		}
	};

	if snapshot.status != "ready" {
		eprintln!(
			"error: latest snapshot for '{}' is not ready (status: {})",
			repo_uid, snapshot.status
		);
		return ExitCode::from(2);
	}

	// Compute trust report.
	use repo_graph_trust::service::assemble_trust_report;
	let report = match assemble_trust_report(
		&storage,
		repo_uid,
		&snapshot.snapshot_uid,
		snapshot.basis_commit.as_deref(),
		snapshot.toolchain_json.as_deref(),
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: trust computation failed: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout only.
	match serde_json::to_string_pretty(&report) {
		Ok(json) => {
			println!("{}", json);
			ExitCode::SUCCESS
		}
		Err(e) => {
			eprintln!("error: failed to serialize trust report: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── callers command ──────────────────────────────────────────────

fn run_callers(args: &[String]) -> ExitCode {
	let (positional, edge_types) = match parse_edge_types_flag(args) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("error: {}", e);
			eprintln!("usage: rgr-rust callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
			return ExitCode::from(1);
		}
	};
	if positional.len() != 3 {
		eprintln!("usage: rgr-rust callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&positional[0]);
	let repo_uid = &positional[1];
	let symbol_query = &positional[2];

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Latest READY snapshot (same rule as trust command).
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

	// Resolve symbol (exact match only).
	use repo_graph_storage::queries::SymbolResolveError;
	let target = match storage.resolve_symbol(&snapshot.snapshot_uid, symbol_query) {
		Ok(sym) => sym,
		Err(SymbolResolveError::NotFound) => {
			eprintln!("error: symbol not found: {}", symbol_query);
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Ambiguous(keys)) => {
			eprintln!("error: ambiguous symbol '{}', matches:", symbol_query);
			for k in &keys {
				eprintln!("  {}", k);
			}
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Storage(e)) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Find direct callers.
	let et_refs: Vec<&str> = edge_types.iter().map(|s| s.as_str()).collect();
	let callers = match storage.find_direct_callers(&snapshot.snapshot_uid, &target.stable_key, &et_refs) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout (TS-compatible QueryResult envelope).
	let count = callers.len();
	let mut extra = serde_json::Map::new();
	extra.insert("target".to_string(), serde_json::to_value(&target).unwrap());
	let output = match build_envelope(
		&storage, "graph callers", repo_uid, &snapshot,
		serde_json::to_value(&callers).unwrap(), count, extra,
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

// ── callees command ──────────────────────────────────────────────

fn run_callees(args: &[String]) -> ExitCode {
	let (positional, edge_types) = match parse_edge_types_flag(args) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("error: {}", e);
			eprintln!("usage: rgr-rust callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
			return ExitCode::from(1);
		}
	};
	if positional.len() != 3 {
		eprintln!("usage: rgr-rust callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&positional[0]);
	let repo_uid = &positional[1];
	let symbol_query = &positional[2];

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Latest READY snapshot (same rule as callers/trust).
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

	// Resolve symbol (exact match only, SYMBOL kind only).
	use repo_graph_storage::queries::SymbolResolveError;
	let target = match storage.resolve_symbol(&snapshot.snapshot_uid, symbol_query) {
		Ok(sym) => sym,
		Err(SymbolResolveError::NotFound) => {
			eprintln!("error: symbol not found: {}", symbol_query);
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Ambiguous(keys)) => {
			eprintln!("error: ambiguous symbol '{}', matches:", symbol_query);
			for k in &keys {
				eprintln!("  {}", k);
			}
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Storage(e)) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Find direct callees.
	let et_refs: Vec<&str> = edge_types.iter().map(|s| s.as_str()).collect();
	let callees = match storage.find_direct_callees(&snapshot.snapshot_uid, &target.stable_key, &et_refs) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout (TS-compatible QueryResult envelope).
	let count = callees.len();
	let mut extra = serde_json::Map::new();
	extra.insert("target".to_string(), serde_json::to_value(&target).unwrap());
	let output = match build_envelope(
		&storage, "graph callees", repo_uid, &snapshot,
		serde_json::to_value(&callees).unwrap(), count, extra,
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

// ── path command ─────────────────────────────────────────────────

fn run_path(args: &[String]) -> ExitCode {
	if args.len() != 4 {
		eprintln!("usage: rgr-rust path <db_path> <repo_uid> <from> <to>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let from_query = &args[2];
	let to_query = &args[3];

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

	// Resolve both endpoints (exact SYMBOL resolution only).
	use repo_graph_storage::queries::SymbolResolveError;

	let from_sym = match storage.resolve_symbol(&snapshot.snapshot_uid, from_query) {
		Ok(sym) => sym,
		Err(SymbolResolveError::NotFound) => {
			eprintln!("error: symbol not found: {}", from_query);
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Ambiguous(keys)) => {
			eprintln!("error: ambiguous symbol '{}', matches:", from_query);
			for k in &keys {
				eprintln!("  {}", k);
			}
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Storage(e)) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	let to_sym = match storage.resolve_symbol(&snapshot.snapshot_uid, to_query) {
		Ok(sym) => sym,
		Err(SymbolResolveError::NotFound) => {
			eprintln!("error: symbol not found: {}", to_query);
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Ambiguous(keys)) => {
			eprintln!("error: ambiguous symbol '{}', matches:", to_query);
			for k in &keys {
				eprintln!("  {}", k);
			}
			return ExitCode::from(2);
		}
		Err(SymbolResolveError::Storage(e)) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Shortest path: CALLS + IMPORTS, max depth 8.
	let path_result = match storage.find_shortest_path(
		&snapshot.snapshot_uid,
		&from_sym.stable_key,
		&to_sym.stable_key,
		8,
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout (TS-compatible QueryResult envelope).
	// TS wraps the single PathResult in a 1-element array.
	let count = if path_result.found { 1 } else { 0 };
	let output = match build_envelope(
		&storage,
		"graph path",
		repo_uid,
		&snapshot,
		serde_json::json!([path_result]),
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

// ── imports command ──────────────────────────────────────────────

fn run_imports(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rgr-rust imports <db_path> <repo_uid> <file_path>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let file_path = &args[2];

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

	// Construct the FILE stable key: {repo_uid}:{file_path}:FILE
	// This matches the TS resolution path (graph.ts:175).
	let file_stable_key = format!("{}:{}:FILE", repo_uid, file_path);

	// Verify the FILE node exists.
	match storage.node_exists(&snapshot.snapshot_uid, &file_stable_key) {
		Ok(true) => {}
		Ok(false) => {
			eprintln!("error: file not found: {}", file_path);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	}

	// Dedicated imports query (TS-compatible NodeResult wire format).
	let imports = match storage.find_imports(
		&snapshot.snapshot_uid,
		&file_stable_key,
	) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout (TS-compatible QueryResult envelope).
	let count = imports.len();
	let output = match build_envelope(
		&storage, "graph imports", repo_uid, &snapshot,
		serde_json::to_value(&imports).unwrap(), count, serde_json::Map::new(),
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

// ── violations command ───────────────────────────────────────────

fn run_violations(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust violations <db_path> <repo_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

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

	// Load active boundary declarations.
	let boundaries = match storage.get_active_boundary_declarations(repo_uid) {
		Ok(b) => b,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Deduplicate rules by (boundary_module, forbids).
	use std::collections::HashMap;
	let mut rule_map: HashMap<(String, String), (String, String, Option<String>)> = HashMap::new();
	for decl in &boundaries {
		let key = (decl.boundary_module.clone(), decl.forbids.clone());
		rule_map.entry(key).or_insert_with(|| {
			(decl.boundary_module.clone(), decl.forbids.clone(), decl.reason.clone())
		});
	}

	// For each unique rule, find violating IMPORTS edges.
	use repo_graph_storage::queries::BoundaryViolation;
	let mut violations: Vec<BoundaryViolation> = Vec::new();

	// Sort rules for deterministic output.
	let mut rules: Vec<_> = rule_map.into_values().collect();
	rules.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));

	for (boundary_module, forbids, reason) in &rules {
		let edges = match storage.find_imports_between_paths(
			&snapshot.snapshot_uid,
			boundary_module,
			forbids,
		) {
			Ok(e) => e,
			Err(e) => {
				eprintln!("error: {}", e);
				return ExitCode::from(2);
			}
		};

		for edge in &edges {
			violations.push(BoundaryViolation {
				boundary_module: boundary_module.clone(),
				forbidden_module: forbids.clone(),
				reason: reason.clone(),
				source_file: edge.source_file.clone(),
				target_file: edge.target_file.clone(),
				line: edge.line,
			});
		}
	}

	// JSON to stdout (TS-compatible QueryResult envelope).
	let count = violations.len();
	let output = match build_envelope(
		&storage,
		"arch violations",
		repo_uid,
		&snapshot,
		serde_json::to_value(&violations).unwrap(),
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

// ── gate command ─────────────────────────────────────────────────

fn run_gate(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust gate <db_path> <repo_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

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

	// Load active requirement declarations.
	let requirements = match storage.get_active_requirement_declarations(repo_uid) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Current UTC time for waiver expiry comparison (ISO 8601).
	let now = utc_now_iso8601();

	// Evaluate obligations (with waiver overlay).
	let obligations = match gate::evaluate_obligations(
		&storage,
		&snapshot.snapshot_uid,
		repo_uid,
		&requirements,
		&now,
	) {
		Ok(o) => o,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Reduce to gate outcome.
	let gate_result = gate::reduce_to_gate_outcome(&obligations);
	let exit_code = gate_result.exit_code;

	// Repo name for the report.
	use repo_graph_storage::types::RepoRef;
	let repo_name = storage
		.get_repo(&RepoRef::Uid(repo_uid.to_string()))
		.ok()
		.flatten()
		.map(|r| r.name)
		.unwrap_or_else(|| repo_uid.to_string());

	// Toolchain metadata from snapshot (may be null).
	let toolchain: serde_json::Value = snapshot
		.toolchain_json
		.as_deref()
		.and_then(|s| serde_json::from_str(s).ok())
		.unwrap_or(serde_json::Value::Null);

	// Gate report JSON (TS-compatible shape, NOT QueryResult envelope).
	let output = serde_json::json!({
		"command": "gate",
		"repo": repo_name,
		"snapshot": snapshot.snapshot_uid,
		"toolchain": toolchain,
		"obligations": obligations,
		"gate": gate_result,
	});

	match serde_json::to_string_pretty(&output) {
		Ok(json) => {
			println!("{}", json);
			ExitCode::from(exit_code as u8)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── dead command ─────────────────────────────────────────────────

fn run_dead(args: &[String]) -> ExitCode {
	if args.len() < 2 || args.len() > 3 {
		eprintln!("usage: rgr-rust dead <db_path> <repo_uid> [kind]");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let kind_filter = args.get(2).map(|s| s.as_str());

	// Validate kind filter against known node kinds.
	const VALID_KINDS: &[&str] = &["SYMBOL", "FILE", "MODULE"];
	if let Some(kind) = kind_filter {
		if !VALID_KINDS.contains(&kind) {
			eprintln!(
				"error: unknown kind '{}', expected one of: {}",
				kind,
				VALID_KINDS.join(", ")
			);
			return ExitCode::from(1);
		}
	}

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Latest READY snapshot (same rule as callers/callees/trust).
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

	// Find dead nodes.
	let dead = match storage.find_dead_nodes(
		&snapshot.snapshot_uid,
		repo_uid,
		kind_filter,
	) {
		Ok(d) => d,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout (TS-compatible QueryResult envelope).
	let count = dead.len();
	let mut extra = serde_json::Map::new();
	extra.insert("kind_filter".to_string(), serde_json::json!(kind_filter));
	let output = match build_envelope(
		&storage, "graph dead", repo_uid, &snapshot,
		serde_json::to_value(&dead).unwrap(), count, extra,
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

// ── cycles command ───────────────────────────────────────────────

fn run_cycles(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust cycles <db_path> <repo_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Latest READY snapshot (same rule as all read commands).
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

	// Module-level cycles (TS default).
	let cycles = match storage.find_cycles(&snapshot.snapshot_uid, "module") {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout (TS-compatible QueryResult envelope).
	let count = cycles.len();
	let output = match build_envelope(
		&storage, "graph cycles", repo_uid, &snapshot,
		serde_json::to_value(&cycles).unwrap(), count, serde_json::Map::new(),
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

// ── stats command ────────────────────────────────────────────────

fn run_stats(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rgr-rust stats <db_path> <repo_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

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

	let stats = match storage.compute_module_stats(&snapshot.snapshot_uid) {
		Ok(s) => s,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON to stdout (TS-compatible QueryResult envelope).
	let count = stats.len();
	let output = match build_envelope(
		&storage, "graph stats", repo_uid, &snapshot,
		serde_json::to_value(&stats).unwrap(), count, serde_json::Map::new(),
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

fn open_storage(
	db_path: &Path,
) -> Result<repo_graph_storage::StorageConnection, String> {
	if !db_path.exists() {
		return Err(format!(
			"database file does not exist: {}",
			db_path.display()
		));
	}
	repo_graph_storage::StorageConnection::open(db_path)
		.map_err(|e| format!("failed to open database: {}", e))
}

/// Valid edge types for `--edge-types` filter (Rust-17).
const VALID_EDGE_TYPES: &[&str] = &["CALLS", "INSTANTIATES"];

/// Parse `--edge-types` from a command's argument slice.
///
/// Returns `(positional_args, edge_types)` on success, or an error
/// message on failure. If `--edge-types` is absent, returns the
/// default `["CALLS"]`.
///
/// Rules:
///   - Comma-separated, trimmed, uppercase only
///   - Unknown tokens → usage error
///   - Empty value → usage error
///   - Repeated `--edge-types` flag → usage error
///   - Missing value after `--edge-types` → usage error
fn parse_edge_types_flag(args: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
	let mut positional = Vec::new();
	let mut edge_types: Option<Vec<String>> = None;
	let mut i = 0;

	while i < args.len() {
		if args[i] == "--edge-types" {
			if edge_types.is_some() {
				return Err("repeated --edge-types flag".to_string());
			}
			i += 1;
			if i >= args.len() {
				return Err("missing value after --edge-types".to_string());
			}
			let raw = &args[i];
			if raw.is_empty() {
				return Err("empty --edge-types value".to_string());
			}
			let types: Vec<String> = raw
				.split(',')
				.map(|t| t.trim().to_string())
				.collect();
			for t in &types {
				if t.is_empty() {
					return Err("empty token in --edge-types value".to_string());
				}
				if !VALID_EDGE_TYPES.contains(&t.as_str()) {
					return Err(format!(
						"unknown edge type '{}', expected one of: {}",
						t,
						VALID_EDGE_TYPES.join(", ")
					));
				}
			}
			edge_types = Some(types);
		} else {
			positional.push(args[i].clone());
		}
		i += 1;
	}

	let types = edge_types.unwrap_or_else(|| vec!["CALLS".to_string()]);
	Ok((positional, types))
}

/// Build a TS-compatible QueryResult JSON envelope.
///
/// Mirrors the TS `formatQueryResult` wrapper (json.ts:25-40).
/// `extra_fields` are merged into the envelope alongside the
/// standard metadata fields (e.g., `target` for callers/callees,
/// `kind_filter` for dead).
fn build_envelope(
	storage: &repo_graph_storage::StorageConnection,
	command: &str,
	repo_uid: &str,
	snapshot: &repo_graph_storage::types::Snapshot,
	results: serde_json::Value,
	count: usize,
	extra_fields: serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
	use repo_graph_storage::types::RepoRef;

	let repo_name = storage
		.get_repo(&RepoRef::Uid(repo_uid.to_string()))
		.ok()
		.flatten()
		.map(|r| r.name)
		.unwrap_or_else(|| repo_uid.to_string());

	let snapshot_scope = if snapshot.kind == "full" { "full" } else { "incremental" };

	let stale = storage
		.get_stale_files(&snapshot.snapshot_uid)
		.map(|files| !files.is_empty())
		.map_err(|e| format!("failed to compute stale state: {}", e))?;

	let mut envelope = serde_json::json!({
		"command": command,
		"repo": repo_name,
		"snapshot": snapshot.snapshot_uid,
		"snapshot_scope": snapshot_scope,
		"basis_commit": snapshot.basis_commit,
		"results": results,
		"count": count,
		"stale": stale,
	});

	// Merge command-specific fields into the envelope.
	if let serde_json::Value::Object(ref mut map) = envelope {
		for (k, v) in extra_fields {
			map.insert(k, v);
		}
	}

	Ok(envelope)
}

/// Return the current UTC time as an ISO 8601 string.
///
/// Format: `YYYY-MM-DDTHH:MM:SS.mmmZ` — compatible with the
/// lexicographic comparison used by `find_active_waivers` for
/// expiry checks. No external crate dependency (no chrono).
fn utc_now_iso8601() -> String {
	use std::time::{SystemTime, UNIX_EPOCH};
	let dur = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default();
	let secs = dur.as_secs();
	let millis = dur.subsec_millis();

	// Break epoch seconds into date/time components.
	// Algorithm: civil_from_days (Howard Hinnant, public domain).
	let days = (secs / 86400) as i64;
	let day_secs = (secs % 86400) as u32;
	let hours = day_secs / 3600;
	let minutes = (day_secs % 3600) / 60;
	let seconds = day_secs % 60;

	// Days since 0000-03-01 (shifted epoch for leap year calc).
	let z = days + 719468;
	let era = if z >= 0 { z } else { z - 146096 } / 146097;
	let doe = (z - era * 146097) as u32;
	let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
	let y = yoe as i64 + era * 400;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let d = doy - (153 * mp + 2) / 5 + 1;
	let m = if mp < 10 { mp + 3 } else { mp - 9 };
	let year = if m <= 2 { y + 1 } else { y };

	format!(
		"{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
		year, m, d, hours, minutes, seconds, millis,
	)
}
