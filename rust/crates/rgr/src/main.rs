//! Minimal Rust CLI for repo-graph.
//!
//! Commands:
//!   rmap index   <repo_path> <db_path>
//!   rmap refresh <repo_path> <db_path>
//!   rmap trust   <db_path> <repo_uid>
//!   rmap callers <db_path> <repo_uid> <symbol> [--edge-types <types>]
//!   rmap callees <db_path> <repo_uid> <symbol> [--edge-types <types>]
//!   rmap path    <db_path> <repo_uid> <from> <to>
//!   rmap imports <db_path> <repo_uid> <file_path>
//!   rmap violations <db_path> <repo_uid>
//!   rmap dead    <db_path> <repo_uid> [kind]
//!   rmap cycles  <db_path> <repo_uid>
//!   rmap stats   <db_path> <repo_uid>
//!
//!   rmap gate    <db_path> <repo_uid> [--strict | --advisory]
//!   rmap orient  <db_path> <repo_uid> [--budget small|medium|large] [--focus <string>]
//!   rmap check   <db_path> <repo_uid>
//!
//!   rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]
//!   rmap declare requirement <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]
//!   rmap declare deactivate <db_path> <declaration_uid>
//!
//!   rmap resource readers <db_path> <repo_uid> <resource_stable_key>
//!   rmap resource writers <db_path> <repo_uid> <resource_stable_key>
//!
//!   rmap modules list <db_path> <repo_uid>
//!   rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]
//!   rmap modules violations <db_path> <repo_uid>
//!   rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]
//!
//! Exit codes:
//!   0 — success (gate: all pass; check: pass; modules violations: no violations)
//!   1 — usage error (gate: any fail; check: fail; modules violations: violations found)
//!   2 — runtime error (gate: incomplete; check: incomplete;
//!       orient: focus-not-implemented, storage failure,
//!       missing repo, missing snapshot, boundary parse failure)

// Gate policy was relocated out of this binary crate into
// `repo-graph-gate` during Rust-43A. The `run_gate` function
// below now calls into the new crate through the
// `GateStorageRead` impl in `repo-graph-storage`. No local
// `mod gate;` declaration.

use std::path::Path;
use std::process::ExitCode;

/// Format a `GateError` using the stderr wording that the
/// pre-relocation `rmap gate` command produced. The
/// relocation changed the error types (gate now returns
/// `GateError` instead of free-form `String` diagnostics), but
/// the CLI test suite pins specific substrings on stderr. This
/// function adapts the new typed errors back to those strings
/// without re-introducing policy coupling in the gate crate.
///
/// When a new operation is added to the gate port, its mapping
/// goes here — not in the gate crate itself, which must stay
/// CLI-agnostic.
fn format_gate_error(err: &repo_graph_gate::GateError) -> String {
	use repo_graph_gate::GateError;
	match err {
		GateError::Storage(e) => match e.operation {
			"find_waivers" => format!("failed to read waivers: {}", e.message),
			"get_boundary_declarations" => {
				format!("failed to read boundary declarations: {}", e.message)
			}
			"find_boundary_imports" => {
				format!("failed to query imports between paths: {}", e.message)
			}
			"get_coverage_measurements" => {
				format!("failed to read coverage measurements: {}", e.message)
			}
			"get_complexity_measurements" => {
				format!("failed to read complexity measurements: {}", e.message)
			}
			"get_hotspot_inferences" => {
				format!("failed to read hotspot inferences: {}", e.message)
			}
			// `get_active_requirements` errors bubble up the
			// StorageError's own Display text (which already
			// contains the "malformed requirement ..." wording
			// the old CLI printed).
			_ => e.message.clone(),
		},
		// Malformed measurement/inference rows: the gate
		// assemble layer built the diagnostic string verbatim
		// to match the pre-relocation format
		// ("malformed X measurement for Y: Z" etc.). Passing
		// `reason` directly preserves that.
		GateError::MalformedEvidence { reason, .. } => reason.clone(),
	}
}

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
		"orient" => run_orient(&args[2..]),
		"check" => run_check_cmd(&args[2..]),
		"explain" => run_explain_cmd(&args[2..]),
		"dead" => run_dead(&args[2..]),
		"cycles" => run_cycles(&args[2..]),
		"stats" => run_stats(&args[2..]),
		"declare" => run_declare(&args[2..]),
		"resource" => run_resource(&args[2..]),
		"modules" => run_modules(&args[2..]),
		other => {
			eprintln!("unknown command: {}", other);
			print_usage();
			ExitCode::from(1)
		}
	}
}

fn print_usage() {
	eprintln!("usage:");
	eprintln!("  rmap index   <repo_path> <db_path>");
	eprintln!("  rmap refresh <repo_path> <db_path>");
	eprintln!("  rmap trust   <db_path> <repo_uid>");
	eprintln!("  rmap callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
	eprintln!("  rmap callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
	eprintln!("  rmap path    <db_path> <repo_uid> <from> <to>");
	eprintln!("  rmap imports <db_path> <repo_uid> <file_path>");
	eprintln!("  rmap violations <db_path> <repo_uid>");
	eprintln!("  rmap gate       <db_path> <repo_uid>");
	eprintln!("  rmap orient     <db_path> <repo_uid> [--budget small|medium|large] [--focus <string>]");
	eprintln!("  rmap check      <db_path> <repo_uid>");
	eprintln!("  rmap explain    <db_path> <repo_uid> <target> [--budget medium|large]");
	eprintln!("  rmap dead    <db_path> <repo_uid> [kind]");
	eprintln!("  rmap cycles  <db_path> <repo_uid>");
	eprintln!("  rmap stats   <db_path> <repo_uid>");
	eprintln!("  rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]");
	eprintln!("  rmap declare requirement <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]");
	eprintln!("  rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
	eprintln!("  rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
	eprintln!("  rmap modules list <db_path> <repo_uid>");
	eprintln!("  rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
	eprintln!("  rmap modules violations <db_path> <repo_uid>");
	eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
}

// ── index command ────────────────────────────────────────────────

fn run_index(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap index <repo_path> <db_path>");
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
		eprintln!("usage: rmap refresh <repo_path> <db_path>");
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
		eprintln!("usage: rmap trust <db_path> <repo_uid>");
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
			eprintln!("usage: rmap callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
			return ExitCode::from(1);
		}
	};
	if positional.len() != 3 {
		eprintln!("usage: rmap callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
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
			eprintln!("usage: rmap callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
			return ExitCode::from(1);
		}
	};
	if positional.len() != 3 {
		eprintln!("usage: rmap callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
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
		eprintln!("usage: rmap path <db_path> <repo_uid> <from> <to>");
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
		eprintln!("usage: rmap imports <db_path> <repo_uid> <file_path>");
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
//
// Unified architectural violations surface. Evaluates both:
// - Declared directory-boundary violations (legacy)
// - Discovered-module boundary violations (RS-MG integration)
//
// Output shape has separate sections for each policy substrate.
// Exit code is preserved from pre-integration behavior (always 0 on success).

fn run_violations(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap violations <db_path> <repo_uid>");
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

	// ── Section 1: Declared boundary violations (legacy) ─────────

	// Load active boundary declarations (directory-level MODULE targets).
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
	let mut declared_violations: Vec<BoundaryViolation> = Vec::new();

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
			declared_violations.push(BoundaryViolation {
				boundary_module: boundary_module.clone(),
				forbidden_module: forbids.clone(),
				reason: reason.clone(),
				source_file: edge.source_file.clone(),
				target_file: edge.target_file.clone(),
				line: edge.line,
			});
		}
	}

	// ── Section 2: Discovered-module boundary violations ─────────

	let discovered_evaluation =
		match evaluate_discovered_module_violations(&storage, repo_uid, &snapshot.snapshot_uid) {
			Ok(e) => e,
			Err(msg) => {
				eprintln!("error: {}", msg);
				return ExitCode::from(2);
			}
		};

	// Convert discovered violations to JSON
	use repo_graph_classification::boundary_evaluator::StaleSide;

	let discovered_violations_json: Vec<serde_json::Value> = discovered_evaluation
		.violations
		.iter()
		.map(|v| {
			serde_json::json!({
				"declaration_uid": v.declaration_uid,
				"source": v.source_canonical_path,
				"target": v.target_canonical_path,
				"import_count": v.import_count,
				"source_file_count": v.source_file_count,
				"reason": v.reason,
			})
		})
		.collect();

	let stale_declarations_json: Vec<serde_json::Value> = discovered_evaluation
		.stale_declarations
		.iter()
		.map(|s| {
			serde_json::json!({
				"declaration_uid": s.declaration_uid,
				"stale_side": match s.stale_side {
					StaleSide::Source => "source",
					StaleSide::Target => "target",
					StaleSide::Both => "both",
				},
				"missing_paths": s.missing_paths,
			})
		})
		.collect();

	// ── Build unified output ─────────────────────────────────────

	let declared_count = declared_violations.len();
	let discovered_count = discovered_evaluation.violations.len();
	let stale_count = discovered_evaluation.stale_declarations.len();
	let total_count = declared_count + discovered_count;

	let results = serde_json::json!({
		"declared_boundary_violations": serde_json::to_value(&declared_violations).unwrap(),
		"discovered_module_violations": discovered_violations_json,
	});

	// Build extra fields for envelope
	let mut extra = serde_json::Map::new();
	extra.insert(
		"declared_boundary_count".to_string(),
		serde_json::Value::Number(declared_count.into()),
	);
	extra.insert(
		"discovered_module_count".to_string(),
		serde_json::Value::Number(discovered_count.into()),
	);
	extra.insert(
		"stale_declarations".to_string(),
		serde_json::Value::Array(stale_declarations_json),
	);
	extra.insert(
		"stale_count".to_string(),
		serde_json::Value::Number(stale_count.into()),
	);

	let output = match build_envelope(
		&storage,
		"arch violations",
		repo_uid,
		&snapshot,
		results,
		total_count,
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
			// Preserve legacy exit behavior: always 0 on success
			// Exit code change (fail on violations) is a separate contract slice
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
	// Parse positional args and optional mode flags.
	let mut positional = Vec::new();
	let mut strict = false;
	let mut advisory = false;

	for arg in args {
		match arg.as_str() {
			"--strict" => strict = true,
			"--advisory" => advisory = true,
			_ if arg.starts_with('-') => {
				eprintln!("error: unknown flag: {}", arg);
				eprintln!("usage: rmap gate <db_path> <repo_uid> [--strict | --advisory]");
				return ExitCode::from(1);
			}
			_ => positional.push(arg),
		}
	}

	if positional.len() != 2 {
		eprintln!("usage: rmap gate <db_path> <repo_uid> [--strict | --advisory]");
		return ExitCode::from(1);
	}

	if strict && advisory {
		eprintln!("error: --strict and --advisory are mutually exclusive");
		return ExitCode::from(1);
	}

	let mode = if strict {
		repo_graph_gate::GateMode::Strict
	} else if advisory {
		repo_graph_gate::GateMode::Advisory
	} else {
		repo_graph_gate::GateMode::Default
	};

	let db_path = Path::new(positional[0]);
	let repo_uid = positional[1];

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

	// Current UTC time for waiver expiry comparison (ISO 8601).
	let now = utc_now_iso8601();

	// Delegate the entire gate pipeline (requirement load +
	// obligation evaluation + waiver overlay + mode reduction)
	// to the relocated `repo-graph-gate` crate. The
	// `GateStorageRead` trait is implemented on
	// `StorageConnection` in `repo-graph-storage::gate_impl`.
	//
	// Error formatting preserves the pre-relocation stderr
	// wording used by `rmap gate` so the test suite's
	// regression assertions stay valid. New callers of the
	// gate crate should use `GateError::Display` directly.
	let report = match repo_graph_gate::assemble(
		&storage,
		repo_uid,
		&snapshot.snapshot_uid,
		mode,
		&now,
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", format_gate_error(&e));
			return ExitCode::from(2);
		}
	};
	let exit_code = report.outcome.exit_code;

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
	// Field names and nesting preserved from the pre-relocation
	// gate.rs output so `rmap gate` consumers see no shape change.
	let output = serde_json::json!({
		"command": "gate",
		"repo": repo_name,
		"snapshot": snapshot.snapshot_uid,
		"toolchain": toolchain,
		"obligations": report.obligations,
		"gate": report.outcome,
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

// ── orient command (Rust-43B) ────────────────────────────────────
//
// `rmap orient <db_path> <repo_uid> [--budget small|medium|large] [--focus <string>]`
//
// First exposure of the agent orientation surface. Calls
// `repo_graph_agent::orient` with a caller-supplied `now` drawn
// from `utc_now_iso8601()`. Output is `rgr.agent.v1` JSON on
// stdout, pretty-printed.
//
// Positional shape uses `<db_path> <repo_uid>` to match every
// other Rust structural/governance command. The agent contract
// target (`rgr orient <repo_name>`) assumes a repo registry that
// the Rust CLI does not have yet; repo-name invocation is
// deferred to the rename/registry slice (Rust-43C+).
//
// Exit codes:
//   0 — success, JSON on stdout
//   1 — usage error (missing args, unknown flag, unknown or
//       missing budget value, repeated --budget, repeated
//       --focus)
//   2 — runtime error: missing DB, missing repo, missing
//       snapshot, storage failure, focus-not-implemented (the
//       focus value was syntactically valid but the runtime
//       surface has not yet been implemented — Rust-44 for
//       module/path focus, Rust-45 for symbol focus)
//
// No `--json` flag: output is always JSON. See the agent
// orientation contract for the schema invariants.

fn run_orient(args: &[String]) -> ExitCode {
	// ── Parse args ───────────────────────────────────────────
	let mut positional: Vec<&String> = Vec::new();
	let mut budget_raw: Option<String> = None;
	let mut focus_raw: Option<String> = None;

	let mut i = 0;
	while i < args.len() {
		let arg = &args[i];
		match arg.as_str() {
			"--budget" => {
				if budget_raw.is_some() {
					eprintln!("error: --budget specified more than once");
					print_orient_usage();
					return ExitCode::from(1);
				}
				i += 1;
				let value = match args.get(i) {
					Some(v) => v,
					None => {
						eprintln!("error: --budget requires a value");
						print_orient_usage();
						return ExitCode::from(1);
					}
				};
				// A value that begins with "--" is almost
				// certainly the next flag, not the budget
				// value. Rejecting it here beats emitting a
				// "unknown budget value" diagnostic that
				// confusingly echoes the flag name.
				if value.starts_with("--") {
					eprintln!("error: --budget requires a value, got flag: {}", value);
					print_orient_usage();
					return ExitCode::from(1);
				}
				budget_raw = Some(value.clone());
			}
			"--focus" => {
				if focus_raw.is_some() {
					eprintln!("error: --focus specified more than once");
					print_orient_usage();
					return ExitCode::from(1);
				}
				i += 1;
				let value = match args.get(i) {
					Some(v) => v,
					None => {
						eprintln!("error: --focus requires a value");
						print_orient_usage();
						return ExitCode::from(1);
					}
				};
				// Same flag-as-value guard as --budget. Without
				// this check `rmap orient <db> <repo>
				// --focus --bogus` would silently accept
				// "--bogus" as a focus string and then exit
				// through the FocusNotImplementedYet runtime
				// path — a usage error masquerading as a
				// runtime error.
				if value.starts_with("--") {
					eprintln!("error: --focus requires a value, got flag: {}", value);
					print_orient_usage();
					return ExitCode::from(1);
				}
				focus_raw = Some(value.clone());
			}
			flag if flag.starts_with("--") => {
				eprintln!("error: unknown flag: {}", flag);
				print_orient_usage();
				return ExitCode::from(1);
			}
			_ => positional.push(arg),
		}
		i += 1;
	}

	if positional.len() != 2 {
		print_orient_usage();
		return ExitCode::from(1);
	}

	let db_path = Path::new(positional[0].as_str());
	let repo_uid = positional[1].as_str();

	// ── Validate budget ──────────────────────────────────────
	//
	// Strict: only "small", "medium", "large". No aliases, no
	// case-insensitive matching. Default: small.
	let budget = match budget_raw.as_deref() {
		None => repo_graph_agent::Budget::Small,
		Some("small") => repo_graph_agent::Budget::Small,
		Some("medium") => repo_graph_agent::Budget::Medium,
		Some("large") => repo_graph_agent::Budget::Large,
		Some(other) => {
			eprintln!(
				"error: invalid --budget value: {} (expected small|medium|large)",
				other
			);
			print_orient_usage();
			return ExitCode::from(1);
		}
	};

	// ── Open storage ─────────────────────────────────────────
	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// ── Call the use case ────────────────────────────────────
	//
	// `now` is the wall-clock timestamp used by the gate
	// aggregator for waiver expiry comparison. The agent crate
	// is clock-free by contract; this CLI wiring reads the
	// system clock at the outermost boundary and passes it in.
	// Reuses the existing `utc_now_iso8601` helper — do NOT
	// invent another clock helper.
	let now = utc_now_iso8601();
	let focus = focus_raw.as_deref();

	let result = match repo_graph_agent::orient(
		&storage, repo_uid, focus, budget, &now,
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Serialize and emit ───────────────────────────────────
	match serde_json::to_string_pretty(&result) {
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

fn print_orient_usage() {
	eprintln!(
		"usage: rmap orient <db_path> <repo_uid> \
		 [--budget small|medium|large] [--focus <string>]"
	);
}

// ── check command ────────────────────────────────────────────────

fn run_check_cmd(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap check <db_path> <repo_uid>");
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

	let now = utc_now_iso8601();

	let result = match repo_graph_agent::run_check(&storage, repo_uid, &now) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Map verdict to exit code.
	// The verdict is the first signal with a Check category code.
	let exit_code = result.signals.iter()
		.find_map(|s| match s.code() {
			repo_graph_agent::SignalCode::CheckPass => Some(0),
			repo_graph_agent::SignalCode::CheckFail => Some(1),
			repo_graph_agent::SignalCode::CheckIncomplete => Some(2),
			_ => None,
		})
		.unwrap_or(2); // defensive: if no verdict signal found, treat as incomplete

	match serde_json::to_string_pretty(&result) {
		Ok(json) => {
			println!("{}", json);
			ExitCode::from(exit_code)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── explain command ──────────────────────────────────────────────

fn run_explain_cmd(args: &[String]) -> ExitCode {
	// Parse positional args and optional --budget flag.
	let mut positional: Vec<&String> = Vec::new();
	let mut budget_raw: Option<String> = None;

	let mut i = 0;
	while i < args.len() {
		let arg = &args[i];
		match arg.as_str() {
			"--budget" => {
				if budget_raw.is_some() {
					eprintln!("error: --budget specified more than once");
					print_explain_usage();
					return ExitCode::from(1);
				}
				i += 1;
				let value = match args.get(i) {
					Some(v) => v,
					None => {
						eprintln!("error: --budget requires a value");
						print_explain_usage();
						return ExitCode::from(1);
					}
				};
				if value.starts_with("--") {
					eprintln!("error: --budget requires a value, got flag: {}", value);
					print_explain_usage();
					return ExitCode::from(1);
				}
				budget_raw = Some(value.clone());
			}
			flag if flag.starts_with("--") => {
				eprintln!("error: unknown flag: {}", flag);
				print_explain_usage();
				return ExitCode::from(1);
			}
			_ => positional.push(arg),
		}
		i += 1;
	}

	if positional.len() != 3 {
		print_explain_usage();
		return ExitCode::from(1);
	}

	let db_path = Path::new(positional[0].as_str());
	let repo_uid = positional[1].as_str();
	let target = positional[2].as_str();

	// Budget: default medium, accept medium or large only.
	let budget = match budget_raw.as_deref() {
		None => repo_graph_agent::Budget::Medium,
		Some("medium") => repo_graph_agent::Budget::Medium,
		Some("large") => repo_graph_agent::Budget::Large,
		Some(other) => {
			eprintln!(
				"error: invalid --budget value: {} (expected medium|large)",
				other
			);
			print_explain_usage();
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

	let now = utc_now_iso8601();

	let result = match repo_graph_agent::run_explain(
		&storage, repo_uid, target, budget, &now,
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	match serde_json::to_string_pretty(&result) {
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

fn print_explain_usage() {
	eprintln!(
		"usage: rmap explain <db_path> <repo_uid> <target> \
		 [--budget medium|large]"
	);
}

// ── dead command ─────────────────────────────────────────────────

fn run_dead(args: &[String]) -> ExitCode {
	if args.len() < 2 || args.len() > 3 {
		eprintln!("usage: rmap dead <db_path> <repo_uid> [kind]");
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
		eprintln!("usage: rmap cycles <db_path> <repo_uid>");
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
		eprintln!("usage: rmap stats <db_path> <repo_uid>");
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

// ── declare command ──────────────────────────────────────────────

fn run_declare(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage: rmap declare <subcommand> ...");
		eprintln!("subcommands: boundary, requirement, waiver, deactivate, supersede");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"boundary" => run_declare_boundary(&args[1..]),
		"requirement" => run_declare_requirement(&args[1..]),
		"waiver" => run_declare_waiver(&args[1..]),
		"deactivate" => run_declare_deactivate(&args[1..]),
		"supersede" => run_declare_supersede(&args[1..]),
		other => {
			eprintln!("unknown declare subcommand: {}", other);
			eprintln!("subcommands: boundary, requirement, waiver, deactivate, supersede");
			ExitCode::from(1)
		}
	}
}

fn run_declare_boundary(args: &[String]) -> ExitCode {
	// Parse positional args and flags.
	let mut positional = Vec::new();
	let mut forbids: Option<String> = None;
	let mut reason: Option<String> = None;
	let mut i = 0;

	while i < args.len() {
		match args[i].as_str() {
			"--forbids" => {
				if forbids.is_some() {
					eprintln!("error: --forbids specified more than once");
					return ExitCode::from(1);
				}
				i += 1;
				if i >= args.len() || args[i].starts_with('-') {
					eprintln!("error: --forbids requires a non-empty value");
					return ExitCode::from(1);
				}
				let v = args[i].trim().to_string();
				if v.is_empty() {
					eprintln!("error: --forbids requires a non-empty value");
					return ExitCode::from(1);
				}
				forbids = Some(v);
			}
			"--reason" => {
				if reason.is_some() {
					eprintln!("error: --reason specified more than once");
					return ExitCode::from(1);
				}
				i += 1;
				if i >= args.len() || args[i].starts_with('-') {
					eprintln!("error: --reason requires a non-empty value");
					return ExitCode::from(1);
				}
				let v = args[i].trim().to_string();
				if v.is_empty() {
					eprintln!("error: --reason requires a non-empty value");
					return ExitCode::from(1);
				}
				reason = Some(v);
			}
			other if other.starts_with('-') => {
				eprintln!("error: unknown flag: {}", other);
				eprintln!("usage: rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]");
				return ExitCode::from(1);
			}
			_ => positional.push(&args[i]),
		}
		i += 1;
	}

	if positional.len() != 3 {
		eprintln!("usage: rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]");
		return ExitCode::from(1);
	}

	let forbids = match forbids {
		Some(f) => f,
		None => {
			eprintln!("error: --forbids is required");
			return ExitCode::from(1);
		}
	};

	let db_path = Path::new(positional[0].as_str());
	let repo_uid = positional[1].as_str();
	let module_path = positional[2].as_str();

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Build the declaration.
	use repo_graph_storage::crud::declarations::{
		DeclarationInsert, boundary_identity_key,
	};

	let target_stable_key = format!("{}:{}:MODULE", repo_uid, module_path);

	let mut value = serde_json::json!({ "forbids": forbids });
	if let Some(ref r) = reason {
		value["reason"] = serde_json::Value::String(r.clone());
	}

	let now = utc_now_iso8601();

	let decl = DeclarationInsert {
		identity_key: boundary_identity_key(repo_uid, module_path, &forbids),
		repo_uid: repo_uid.to_string(),
		target_stable_key,
		kind: "boundary".to_string(),
		value_json: value.to_string(),
		created_at: now,
		created_by: Some("cli".to_string()),
		supersedes_uid: None,
		authored_basis_json: None,
	};

	match storage.insert_declaration(&decl) {
		Ok(result) => {
			let output = serde_json::json!({
				"declaration_uid": result.declaration_uid,
				"kind": "boundary",
				"target": module_path,
				"forbids": forbids,
				"inserted": result.inserted,
			});
			println!("{}", serde_json::to_string_pretty(&output).unwrap());
			ExitCode::from(0)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

const VALID_OPERATORS: &[&str] = &[">=", ">", "<=", "<", "=="];

const DECLARE_REQUIREMENT_USAGE: &str =
	"usage: rmap declare requirement <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]";

fn run_declare_requirement(args: &[String]) -> ExitCode {
	let mut positional = Vec::new();
	let mut version: Option<String> = None;
	let mut obligation_id: Option<String> = None;
	let mut method: Option<String> = None;
	let mut obligation: Option<String> = None;
	let mut target: Option<String> = None;
	let mut threshold: Option<String> = None;
	let mut operator: Option<String> = None;
	let mut i = 0;

	/// Parse a flag value. Returns `None` and prints an error if
	/// the flag is repeated, the value is missing, looks like
	/// another flag, or is empty after trimming.
	fn parse_flag_value<'a>(
		flag_name: &str,
		current: &Option<String>,
		args: &'a [String],
		i: &mut usize,
	) -> Option<String> {
		if current.is_some() {
			eprintln!("error: {} specified more than once", flag_name);
			return None;
		}
		*i += 1;
		if *i >= args.len() || args[*i].starts_with('-') {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		let v = args[*i].trim().to_string();
		if v.is_empty() {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		Some(v)
	}

	while i < args.len() {
		match args[i].as_str() {
			"--version" => match parse_flag_value("--version", &version, args, &mut i) {
				Some(v) => version = Some(v),
				None => return ExitCode::from(1),
			},
			"--obligation-id" => match parse_flag_value("--obligation-id", &obligation_id, args, &mut i) {
				Some(v) => obligation_id = Some(v),
				None => return ExitCode::from(1),
			},
			"--method" => match parse_flag_value("--method", &method, args, &mut i) {
				Some(v) => method = Some(v),
				None => return ExitCode::from(1),
			},
			"--obligation" => match parse_flag_value("--obligation", &obligation, args, &mut i) {
				Some(v) => obligation = Some(v),
				None => return ExitCode::from(1),
			},
			"--target" => match parse_flag_value("--target", &target, args, &mut i) {
				Some(v) => target = Some(v),
				None => return ExitCode::from(1),
			},
			"--threshold" => match parse_flag_value("--threshold", &threshold, args, &mut i) {
				Some(v) => threshold = Some(v),
				None => return ExitCode::from(1),
			},
			"--operator" => match parse_flag_value("--operator", &operator, args, &mut i) {
				Some(v) => operator = Some(v),
				None => return ExitCode::from(1),
			},
			other if other.starts_with('-') => {
				eprintln!("error: unknown flag: {}", other);
				eprintln!("{}", DECLARE_REQUIREMENT_USAGE);
				return ExitCode::from(1);
			}
			_ => positional.push(&args[i]),
		}
		i += 1;
	}

	// Validate positional args: db_path, repo_uid, req_id.
	if positional.len() != 3 {
		eprintln!("{}", DECLARE_REQUIREMENT_USAGE);
		return ExitCode::from(1);
	}

	// Validate required flags.
	let version_str = match version {
		Some(v) => v,
		None => {
			eprintln!("error: --version is required");
			return ExitCode::from(1);
		}
	};
	let version_num: i64 = match version_str.parse() {
		Ok(v) => v,
		Err(_) => {
			eprintln!("error: --version must be an integer, got: {}", version_str);
			return ExitCode::from(1);
		}
	};
	let obligation_id = match obligation_id {
		Some(v) => v,
		None => {
			eprintln!("error: --obligation-id is required");
			return ExitCode::from(1);
		}
	};
	let method = match method {
		Some(v) => v,
		None => {
			eprintln!("error: --method is required");
			return ExitCode::from(1);
		}
	};
	let obligation = match obligation {
		Some(v) => v,
		None => {
			eprintln!("error: --obligation is required");
			return ExitCode::from(1);
		}
	};

	// Validate optional typed fields.
	let threshold_num: Option<f64> = match threshold {
		Some(ref t) => match t.parse() {
			Ok(v) => Some(v),
			Err(_) => {
				eprintln!("error: --threshold must be a number, got: {}", t);
				return ExitCode::from(1);
			}
		},
		None => None,
	};

	if let Some(ref op) = operator {
		if !VALID_OPERATORS.contains(&op.as_str()) {
			eprintln!(
				"error: --operator must be one of {:?}, got: {}",
				VALID_OPERATORS, op
			);
			return ExitCode::from(1);
		}
	}

	let db_path = Path::new(positional[0].as_str());
	let repo_uid = positional[1].as_str();
	let req_id = positional[2].as_str();

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Build obligation object.
	let mut obl = serde_json::json!({
		"obligation_id": obligation_id,
		"obligation": obligation,
		"method": method,
	});
	if let Some(ref t) = target {
		obl["target"] = serde_json::Value::String(t.clone());
	}
	if let Some(t) = threshold_num {
		obl["threshold"] = serde_json::json!(t);
	}
	if let Some(ref op) = operator {
		obl["operator"] = serde_json::Value::String(op.clone());
	}

	let value = serde_json::json!({
		"req_id": req_id,
		"version": version_num,
		"verification": [obl],
	});

	use repo_graph_storage::crud::declarations::{
		DeclarationInsert, requirement_identity_key,
	};

	let target_stable_key = format!("{}:requirement:{}:{}", repo_uid, req_id, version_num);
	let now = utc_now_iso8601();

	let decl = DeclarationInsert {
		identity_key: requirement_identity_key(repo_uid, req_id, version_num),
		repo_uid: repo_uid.to_string(),
		target_stable_key,
		kind: "requirement".to_string(),
		value_json: value.to_string(),
		created_at: now,
		created_by: Some("cli".to_string()),
		supersedes_uid: None,
		authored_basis_json: None,
	};

	match storage.insert_declaration(&decl) {
		Ok(result) => {
			let output = serde_json::json!({
				"declaration_uid": result.declaration_uid,
				"kind": "requirement",
				"req_id": req_id,
				"version": version_num,
				"inserted": result.inserted,
			});
			println!("{}", serde_json::to_string_pretty(&output).unwrap());
			ExitCode::from(0)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

fn run_declare_deactivate(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap declare deactivate <db_path> <declaration_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let declaration_uid = &args[1];

	if declaration_uid.trim().is_empty() {
		eprintln!("error: declaration_uid must be non-empty");
		return ExitCode::from(1);
	}

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	match storage.deactivate_declaration(declaration_uid) {
		Ok(rows) => {
			let output = serde_json::json!({
				"declaration_uid": declaration_uid,
				"deactivated": rows > 0,
			});
			println!("{}", serde_json::to_string_pretty(&output).unwrap());
			ExitCode::from(0)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

const DECLARE_WAIVER_USAGE: &str =
	"usage: rmap declare waiver <db_path> <repo_uid> <req_id> --requirement-version <n> --obligation-id <id> --reason <text> [--expires-at <iso>] [--created-by <actor>] [--rationale-category <cat>] [--policy-basis <text>]";

fn run_declare_waiver(args: &[String]) -> ExitCode {
	let mut positional = Vec::new();
	let mut requirement_version: Option<String> = None;
	let mut obligation_id: Option<String> = None;
	let mut reason: Option<String> = None;
	let mut expires_at: Option<String> = None;
	let mut created_by: Option<String> = None;
	let mut rationale_category: Option<String> = None;
	let mut policy_basis: Option<String> = None;
	let mut i = 0;

	fn parse_flag<'a>(
		flag_name: &str,
		current: &Option<String>,
		args: &'a [String],
		i: &mut usize,
	) -> Option<String> {
		if current.is_some() {
			eprintln!("error: {} specified more than once", flag_name);
			return None;
		}
		*i += 1;
		if *i >= args.len() || args[*i].starts_with('-') {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		let v = args[*i].trim().to_string();
		if v.is_empty() {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		Some(v)
	}

	while i < args.len() {
		match args[i].as_str() {
			"--requirement-version" => match parse_flag("--requirement-version", &requirement_version, args, &mut i) {
				Some(v) => requirement_version = Some(v),
				None => return ExitCode::from(1),
			},
			"--obligation-id" => match parse_flag("--obligation-id", &obligation_id, args, &mut i) {
				Some(v) => obligation_id = Some(v),
				None => return ExitCode::from(1),
			},
			"--reason" => match parse_flag("--reason", &reason, args, &mut i) {
				Some(v) => reason = Some(v),
				None => return ExitCode::from(1),
			},
			"--expires-at" => match parse_flag("--expires-at", &expires_at, args, &mut i) {
				Some(v) => expires_at = Some(v),
				None => return ExitCode::from(1),
			},
			"--created-by" => match parse_flag("--created-by", &created_by, args, &mut i) {
				Some(v) => created_by = Some(v),
				None => return ExitCode::from(1),
			},
			"--rationale-category" => match parse_flag("--rationale-category", &rationale_category, args, &mut i) {
				Some(v) => rationale_category = Some(v),
				None => return ExitCode::from(1),
			},
			"--policy-basis" => match parse_flag("--policy-basis", &policy_basis, args, &mut i) {
				Some(v) => policy_basis = Some(v),
				None => return ExitCode::from(1),
			},
			other if other.starts_with('-') => {
				eprintln!("error: unknown flag: {}", other);
				eprintln!("{}", DECLARE_WAIVER_USAGE);
				return ExitCode::from(1);
			}
			_ => positional.push(&args[i]),
		}
		i += 1;
	}

	if positional.len() != 3 {
		eprintln!("{}", DECLARE_WAIVER_USAGE);
		return ExitCode::from(1);
	}

	// Validate required flags.
	let version_str = match requirement_version {
		Some(v) => v,
		None => {
			eprintln!("error: --requirement-version is required");
			return ExitCode::from(1);
		}
	};
	let version_num: i64 = match version_str.parse() {
		Ok(v) => v,
		Err(_) => {
			eprintln!("error: --requirement-version must be an integer, got: {}", version_str);
			return ExitCode::from(1);
		}
	};
	let obligation_id = match obligation_id {
		Some(v) => v,
		None => {
			eprintln!("error: --obligation-id is required");
			return ExitCode::from(1);
		}
	};
	let reason = match reason {
		Some(v) => v,
		None => {
			eprintln!("error: --reason is required");
			return ExitCode::from(1);
		}
	};

	let db_path = Path::new(positional[0].as_str());
	let repo_uid = positional[1].as_str();
	let req_id = positional[2].as_str();

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	let now = utc_now_iso8601();
	let effective_created_by = created_by.unwrap_or_else(|| "cli".to_string());

	// Build value_json — only include optional fields when present.
	let mut value = serde_json::json!({
		"req_id": req_id,
		"requirement_version": version_num,
		"obligation_id": obligation_id,
		"reason": reason,
		"created_at": now,
		"created_by": effective_created_by,
	});
	if let Some(ref exp) = expires_at {
		value["expires_at"] = serde_json::Value::String(exp.clone());
	}
	if let Some(ref rc) = rationale_category {
		value["rationale_category"] = serde_json::Value::String(rc.clone());
	}
	if let Some(ref pb) = policy_basis {
		value["policy_basis"] = serde_json::Value::String(pb.clone());
	}

	use repo_graph_storage::crud::declarations::{
		DeclarationInsert, waiver_identity_key,
	};

	let target_stable_key = format!("{}:waiver:{}#{}", repo_uid, req_id, obligation_id);

	let decl = DeclarationInsert {
		identity_key: waiver_identity_key(repo_uid, req_id, version_num, &obligation_id),
		repo_uid: repo_uid.to_string(),
		target_stable_key,
		kind: "waiver".to_string(),
		value_json: value.to_string(),
		created_at: now.clone(),
		created_by: Some(effective_created_by),
		supersedes_uid: None,
		authored_basis_json: None,
	};

	match storage.insert_declaration(&decl) {
		Ok(result) => {
			let output = serde_json::json!({
				"declaration_uid": result.declaration_uid,
				"kind": "waiver",
				"req_id": req_id,
				"requirement_version": version_num,
				"obligation_id": obligation_id,
				"inserted": result.inserted,
			});
			println!("{}", serde_json::to_string_pretty(&output).unwrap());
			ExitCode::from(0)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

fn run_declare_supersede(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage: rmap declare supersede <kind> ...");
		eprintln!("kinds: boundary, requirement, waiver");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"boundary" => run_declare_supersede_boundary(&args[1..]),
		"requirement" => run_declare_supersede_requirement(&args[1..]),
		"waiver" => run_declare_supersede_waiver(&args[1..]),
		other => {
			eprintln!("unknown supersede kind: {}", other);
			eprintln!("kinds: boundary, requirement, waiver");
			ExitCode::from(1)
		}
	}
}

const SUPERSEDE_BOUNDARY_USAGE: &str =
	"usage: rmap declare supersede boundary <db_path> <old_declaration_uid> --forbids <target> [--reason <text>]";

fn run_declare_supersede_boundary(args: &[String]) -> ExitCode {
	let mut positional = Vec::new();
	let mut forbids: Option<String> = None;
	let mut reason: Option<String> = None;
	let mut i = 0;

	while i < args.len() {
		match args[i].as_str() {
			"--forbids" => {
				if forbids.is_some() {
					eprintln!("error: --forbids specified more than once");
					return ExitCode::from(1);
				}
				i += 1;
				if i >= args.len() || args[i].starts_with('-') {
					eprintln!("error: --forbids requires a non-empty value");
					return ExitCode::from(1);
				}
				let v = args[i].trim().to_string();
				if v.is_empty() {
					eprintln!("error: --forbids requires a non-empty value");
					return ExitCode::from(1);
				}
				forbids = Some(v);
			}
			"--reason" => {
				if reason.is_some() {
					eprintln!("error: --reason specified more than once");
					return ExitCode::from(1);
				}
				i += 1;
				if i >= args.len() || args[i].starts_with('-') {
					eprintln!("error: --reason requires a non-empty value");
					return ExitCode::from(1);
				}
				let v = args[i].trim().to_string();
				if v.is_empty() {
					eprintln!("error: --reason requires a non-empty value");
					return ExitCode::from(1);
				}
				reason = Some(v);
			}
			other if other.starts_with('-') => {
				eprintln!("error: unknown flag: {}", other);
				eprintln!("{}", SUPERSEDE_BOUNDARY_USAGE);
				return ExitCode::from(1);
			}
			_ => positional.push(&args[i]),
		}
		i += 1;
	}

	if positional.len() != 2 {
		eprintln!("{}", SUPERSEDE_BOUNDARY_USAGE);
		return ExitCode::from(1);
	}

	let forbids = match forbids {
		Some(f) => f,
		None => {
			eprintln!("error: --forbids is required");
			return ExitCode::from(1);
		}
	};

	let db_path = Path::new(positional[0].as_str());
	let old_uid = positional[1].as_str();

	if old_uid.trim().is_empty() {
		eprintln!("error: old_declaration_uid must be non-empty");
		return ExitCode::from(1);
	}

	let mut storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Fetch old declaration and validate.
	let old_row = match storage.get_declaration_by_uid(old_uid) {
		Ok(Some(row)) => row,
		Ok(None) => {
			eprintln!("error: declaration {} does not exist", old_uid);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	if !old_row.is_active {
		eprintln!("error: declaration {} is already inactive", old_uid);
		return ExitCode::from(2);
	}

	if old_row.kind != "boundary" {
		eprintln!(
			"error: declaration {} is kind '{}', expected 'boundary'",
			old_uid, old_row.kind
		);
		return ExitCode::from(2);
	}

	// Extract module_path from target_stable_key: {repo}:{path}:MODULE
	let module_path = match extract_module_path_from_key(&old_row.target_stable_key) {
		Some(p) => p,
		None => {
			eprintln!(
				"error: cannot parse module path from target_stable_key: {}",
				old_row.target_stable_key
			);
			return ExitCode::from(2);
		}
	};

	// Build replacement.
	use repo_graph_storage::crud::declarations::{
		DeclarationInsert, boundary_identity_key,
	};

	let mut value = serde_json::json!({ "forbids": forbids });
	if let Some(ref r) = reason {
		value["reason"] = serde_json::Value::String(r.clone());
	}

	let now = utc_now_iso8601();

	let new_decl = DeclarationInsert {
		identity_key: boundary_identity_key(&old_row.repo_uid, &module_path, &forbids),
		repo_uid: old_row.repo_uid.clone(),
		target_stable_key: old_row.target_stable_key.clone(),
		kind: "boundary".to_string(),
		value_json: value.to_string(),
		created_at: now,
		created_by: Some("cli".to_string()),
		supersedes_uid: None, // overridden by supersede_declaration
		authored_basis_json: None,
	};

	match storage.supersede_declaration(old_uid, &new_decl) {
		Ok(result) => {
			let output = serde_json::json!({
				"old_declaration_uid": result.old_declaration_uid,
				"new_declaration_uid": result.new_declaration_uid,
				"kind": "boundary",
				"target": module_path,
				"forbids": forbids,
				"superseded": true,
			});
			println!("{}", serde_json::to_string_pretty(&output).unwrap());
			ExitCode::from(0)
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

const SUPERSEDE_REQUIREMENT_USAGE: &str =
	"usage: rmap declare supersede requirement <db_path> <old_declaration_uid> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]";

fn run_declare_supersede_requirement(args: &[String]) -> ExitCode {
	let mut positional = Vec::new();
	let mut obligation_id: Option<String> = None;
	let mut method: Option<String> = None;
	let mut obligation: Option<String> = None;
	let mut target: Option<String> = None;
	let mut threshold: Option<String> = None;
	let mut operator: Option<String> = None;
	let mut i = 0;

	fn parse_flag<'a>(
		flag_name: &str,
		current: &Option<String>,
		args: &'a [String],
		i: &mut usize,
	) -> Option<String> {
		if current.is_some() {
			eprintln!("error: {} specified more than once", flag_name);
			return None;
		}
		*i += 1;
		if *i >= args.len() || args[*i].starts_with('-') {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		let v = args[*i].trim().to_string();
		if v.is_empty() {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		Some(v)
	}

	while i < args.len() {
		match args[i].as_str() {
			"--obligation-id" => match parse_flag("--obligation-id", &obligation_id, args, &mut i) {
				Some(v) => obligation_id = Some(v),
				None => return ExitCode::from(1),
			},
			"--method" => match parse_flag("--method", &method, args, &mut i) {
				Some(v) => method = Some(v),
				None => return ExitCode::from(1),
			},
			"--obligation" => match parse_flag("--obligation", &obligation, args, &mut i) {
				Some(v) => obligation = Some(v),
				None => return ExitCode::from(1),
			},
			"--target" => match parse_flag("--target", &target, args, &mut i) {
				Some(v) => target = Some(v),
				None => return ExitCode::from(1),
			},
			"--threshold" => match parse_flag("--threshold", &threshold, args, &mut i) {
				Some(v) => threshold = Some(v),
				None => return ExitCode::from(1),
			},
			"--operator" => match parse_flag("--operator", &operator, args, &mut i) {
				Some(v) => operator = Some(v),
				None => return ExitCode::from(1),
			},
			other if other.starts_with('-') => {
				eprintln!("error: unknown flag: {}", other);
				eprintln!("{}", SUPERSEDE_REQUIREMENT_USAGE);
				return ExitCode::from(1);
			}
			_ => positional.push(&args[i]),
		}
		i += 1;
	}

	if positional.len() != 2 {
		eprintln!("{}", SUPERSEDE_REQUIREMENT_USAGE);
		return ExitCode::from(1);
	}

	// Validate required flags.
	let obligation_id = match obligation_id {
		Some(v) => v,
		None => { eprintln!("error: --obligation-id is required"); return ExitCode::from(1); }
	};
	let method = match method {
		Some(v) => v,
		None => { eprintln!("error: --method is required"); return ExitCode::from(1); }
	};
	let obligation = match obligation {
		Some(v) => v,
		None => { eprintln!("error: --obligation is required"); return ExitCode::from(1); }
	};

	// Validate optional typed fields.
	let threshold_num: Option<f64> = match threshold {
		Some(ref t) => match t.parse() {
			Ok(v) => Some(v),
			Err(_) => {
				eprintln!("error: --threshold must be a number, got: {}", t);
				return ExitCode::from(1);
			}
		},
		None => None,
	};
	if let Some(ref op) = operator {
		if !VALID_OPERATORS.contains(&op.as_str()) {
			eprintln!("error: --operator must be one of {:?}, got: {}", VALID_OPERATORS, op);
			return ExitCode::from(1);
		}
	}

	let db_path = Path::new(positional[0].as_str());
	let old_uid = positional[1].as_str();

	if old_uid.trim().is_empty() {
		eprintln!("error: old_declaration_uid must be non-empty");
		return ExitCode::from(1);
	}

	let mut storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => { eprintln!("error: {}", msg); return ExitCode::from(2); }
	};

	// Fetch and validate old declaration.
	let old_row = match storage.get_declaration_by_uid(old_uid) {
		Ok(Some(row)) => row,
		Ok(None) => {
			eprintln!("error: declaration {} does not exist", old_uid);
			return ExitCode::from(2);
		}
		Err(e) => { eprintln!("error: {}", e); return ExitCode::from(2); }
	};

	if !old_row.is_active {
		eprintln!("error: declaration {} is already inactive", old_uid);
		return ExitCode::from(2);
	}
	if old_row.kind != "requirement" {
		eprintln!("error: declaration {} is kind '{}', expected 'requirement'", old_uid, old_row.kind);
		return ExitCode::from(2);
	}

	// Parse old value_json to extract req_id and version.
	let old_value: serde_json::Value = match serde_json::from_str(&old_row.value_json) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("error: old requirement has malformed value_json: {}", e);
			return ExitCode::from(2);
		}
	};
	let req_id = match old_value["req_id"].as_str() {
		Some(s) => s.to_string(),
		None => {
			eprintln!("error: old requirement missing req_id in value_json");
			return ExitCode::from(2);
		}
	};
	let version = match old_value["version"].as_i64() {
		Some(v) => v,
		None => {
			eprintln!("error: old requirement missing version in value_json");
			return ExitCode::from(2);
		}
	};

	// Build replacement obligation.
	let mut obl = serde_json::json!({
		"obligation_id": obligation_id,
		"obligation": obligation,
		"method": method,
	});
	if let Some(ref t) = target {
		obl["target"] = serde_json::Value::String(t.clone());
	}
	if let Some(t) = threshold_num {
		obl["threshold"] = serde_json::json!(t);
	}
	if let Some(ref op) = operator {
		obl["operator"] = serde_json::Value::String(op.clone());
	}

	let value = serde_json::json!({
		"req_id": req_id,
		"version": version,
		"verification": [obl],
	});

	use repo_graph_storage::crud::declarations::{
		DeclarationInsert, requirement_identity_key,
	};

	let now = utc_now_iso8601();

	let new_decl = DeclarationInsert {
		identity_key: requirement_identity_key(&old_row.repo_uid, &req_id, version),
		repo_uid: old_row.repo_uid.clone(),
		target_stable_key: old_row.target_stable_key.clone(),
		kind: "requirement".to_string(),
		value_json: value.to_string(),
		created_at: now,
		created_by: Some("cli".to_string()),
		supersedes_uid: None, // overridden by supersede_declaration
		authored_basis_json: None,
	};

	match storage.supersede_declaration(old_uid, &new_decl) {
		Ok(result) => {
			let output = serde_json::json!({
				"old_declaration_uid": result.old_declaration_uid,
				"new_declaration_uid": result.new_declaration_uid,
				"kind": "requirement",
				"req_id": req_id,
				"version": version,
				"superseded": true,
			});
			println!("{}", serde_json::to_string_pretty(&output).unwrap());
			ExitCode::from(0)
		}
		Err(e) => { eprintln!("error: {}", e); ExitCode::from(2) }
	}
}

const SUPERSEDE_WAIVER_USAGE: &str =
	"usage: rmap declare supersede waiver <db_path> <old_declaration_uid> --reason <text> [--expires-at <iso>] [--created-by <actor>] [--rationale-category <cat>] [--policy-basis <text>]";

fn run_declare_supersede_waiver(args: &[String]) -> ExitCode {
	let mut positional = Vec::new();
	let mut reason: Option<String> = None;
	let mut expires_at: Option<String> = None;
	let mut created_by: Option<String> = None;
	let mut rationale_category: Option<String> = None;
	let mut policy_basis: Option<String> = None;
	let mut i = 0;

	fn parse_flag<'a>(
		flag_name: &str,
		current: &Option<String>,
		args: &'a [String],
		i: &mut usize,
	) -> Option<String> {
		if current.is_some() {
			eprintln!("error: {} specified more than once", flag_name);
			return None;
		}
		*i += 1;
		if *i >= args.len() || args[*i].starts_with('-') {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		let v = args[*i].trim().to_string();
		if v.is_empty() {
			eprintln!("error: {} requires a non-empty value", flag_name);
			return None;
		}
		Some(v)
	}

	while i < args.len() {
		match args[i].as_str() {
			"--reason" => match parse_flag("--reason", &reason, args, &mut i) {
				Some(v) => reason = Some(v),
				None => return ExitCode::from(1),
			},
			"--expires-at" => match parse_flag("--expires-at", &expires_at, args, &mut i) {
				Some(v) => expires_at = Some(v),
				None => return ExitCode::from(1),
			},
			"--created-by" => match parse_flag("--created-by", &created_by, args, &mut i) {
				Some(v) => created_by = Some(v),
				None => return ExitCode::from(1),
			},
			"--rationale-category" => match parse_flag("--rationale-category", &rationale_category, args, &mut i) {
				Some(v) => rationale_category = Some(v),
				None => return ExitCode::from(1),
			},
			"--policy-basis" => match parse_flag("--policy-basis", &policy_basis, args, &mut i) {
				Some(v) => policy_basis = Some(v),
				None => return ExitCode::from(1),
			},
			other if other.starts_with('-') => {
				eprintln!("error: unknown flag: {}", other);
				eprintln!("{}", SUPERSEDE_WAIVER_USAGE);
				return ExitCode::from(1);
			}
			_ => positional.push(&args[i]),
		}
		i += 1;
	}

	if positional.len() != 2 {
		eprintln!("{}", SUPERSEDE_WAIVER_USAGE);
		return ExitCode::from(1);
	}

	let reason = match reason {
		Some(v) => v,
		None => { eprintln!("error: --reason is required"); return ExitCode::from(1); }
	};

	let db_path = Path::new(positional[0].as_str());
	let old_uid = positional[1].as_str();

	if old_uid.trim().is_empty() {
		eprintln!("error: old_declaration_uid must be non-empty");
		return ExitCode::from(1);
	}

	let mut storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => { eprintln!("error: {}", msg); return ExitCode::from(2); }
	};

	// Fetch and validate old declaration.
	let old_row = match storage.get_declaration_by_uid(old_uid) {
		Ok(Some(row)) => row,
		Ok(None) => {
			eprintln!("error: declaration {} does not exist", old_uid);
			return ExitCode::from(2);
		}
		Err(e) => { eprintln!("error: {}", e); return ExitCode::from(2); }
	};

	if !old_row.is_active {
		eprintln!("error: declaration {} is already inactive", old_uid);
		return ExitCode::from(2);
	}
	if old_row.kind != "waiver" {
		eprintln!("error: declaration {} is kind '{}', expected 'waiver'", old_uid, old_row.kind);
		return ExitCode::from(2);
	}

	// Parse old value_json to extract identity fields.
	let old_value: serde_json::Value = match serde_json::from_str(&old_row.value_json) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("error: old waiver has malformed value_json: {}", e);
			return ExitCode::from(2);
		}
	};
	let req_id = match old_value["req_id"].as_str() {
		Some(s) => s.to_string(),
		None => {
			eprintln!("error: old waiver missing req_id in value_json");
			return ExitCode::from(2);
		}
	};
	let requirement_version = match old_value["requirement_version"].as_i64() {
		Some(v) => v,
		None => {
			eprintln!("error: old waiver missing requirement_version in value_json");
			return ExitCode::from(2);
		}
	};
	let obligation_id = match old_value["obligation_id"].as_str() {
		Some(s) => s.to_string(),
		None => {
			eprintln!("error: old waiver missing obligation_id in value_json");
			return ExitCode::from(2);
		}
	};

	// Build replacement value_json.
	let now = utc_now_iso8601();
	let effective_created_by = created_by.unwrap_or_else(|| "cli".to_string());

	let mut value = serde_json::json!({
		"req_id": req_id,
		"requirement_version": requirement_version,
		"obligation_id": obligation_id,
		"reason": reason,
		"created_at": now,
		"created_by": effective_created_by,
	});
	if let Some(ref exp) = expires_at {
		value["expires_at"] = serde_json::Value::String(exp.clone());
	}
	if let Some(ref rc) = rationale_category {
		value["rationale_category"] = serde_json::Value::String(rc.clone());
	}
	if let Some(ref pb) = policy_basis {
		value["policy_basis"] = serde_json::Value::String(pb.clone());
	}

	use repo_graph_storage::crud::declarations::{
		DeclarationInsert, waiver_identity_key,
	};

	let new_decl = DeclarationInsert {
		identity_key: waiver_identity_key(&old_row.repo_uid, &req_id, requirement_version, &obligation_id),
		repo_uid: old_row.repo_uid.clone(),
		target_stable_key: old_row.target_stable_key.clone(),
		kind: "waiver".to_string(),
		value_json: value.to_string(),
		created_at: now.clone(),
		created_by: Some(effective_created_by),
		supersedes_uid: None,
		authored_basis_json: None,
	};

	match storage.supersede_declaration(old_uid, &new_decl) {
		Ok(result) => {
			let output = serde_json::json!({
				"old_declaration_uid": result.old_declaration_uid,
				"new_declaration_uid": result.new_declaration_uid,
				"kind": "waiver",
				"req_id": req_id,
				"requirement_version": requirement_version,
				"obligation_id": obligation_id,
				"superseded": true,
			});
			println!("{}", serde_json::to_string_pretty(&output).unwrap());
			ExitCode::from(0)
		}
		Err(e) => { eprintln!("error: {}", e); ExitCode::from(2) }
	}
}

// ── resource command (SB-5) ──────────────────────────────────────

fn run_resource(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage:");
		eprintln!("  rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
		eprintln!("  rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"readers" => run_resource_readers(&args[1..]),
		"writers" => run_resource_writers(&args[1..]),
		other => {
			eprintln!("unknown resource subcommand: {}", other);
			eprintln!("usage:");
			eprintln!("  rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
			eprintln!("  rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
			ExitCode::from(1)
		}
	}
}

fn run_resource_readers(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let resource_key = &args[2];

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	let snapshot = match storage.get_latest_snapshot(repo_uid) {
		Ok(Some(s)) => s,
		Ok(None) => {
			eprintln!("error: no snapshot found for repo '{}'", repo_uid);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve resource (exact stable_key, must be resource kind).
	use repo_graph_storage::queries::ResourceResolveError;
	let target = match storage.resolve_resource(&snapshot.snapshot_uid, resource_key) {
		Ok(r) => r,
		Err(ResourceResolveError::NotFound) => {
			eprintln!("error: resource not found: {}", resource_key);
			return ExitCode::from(2);
		}
		Err(ResourceResolveError::NotAResource(kind)) => {
			eprintln!(
				"error: '{}' is not a resource node (kind: {}). \
				 Expected FS_PATH, DB_RESOURCE, BLOB, or STATE+CACHE.",
				resource_key, kind
			);
			return ExitCode::from(2);
		}
		Err(ResourceResolveError::Storage(e)) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Find readers.
	let readers = match storage.find_resource_readers(&snapshot.snapshot_uid, &target.stable_key) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON output (QueryResult envelope).
	let count = readers.len();
	let mut extra = serde_json::Map::new();
	extra.insert("target".to_string(), serde_json::json!(target.stable_key));
	let output = match build_envelope(
		&storage, "resource readers", repo_uid, &snapshot,
		serde_json::to_value(&readers).unwrap(), count, extra,
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

fn run_resource_writers(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let resource_key = &args[2];

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	let snapshot = match storage.get_latest_snapshot(repo_uid) {
		Ok(Some(s)) => s,
		Ok(None) => {
			eprintln!("error: no snapshot found for repo '{}'", repo_uid);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve resource (exact stable_key, must be resource kind).
	use repo_graph_storage::queries::ResourceResolveError;
	let target = match storage.resolve_resource(&snapshot.snapshot_uid, resource_key) {
		Ok(r) => r,
		Err(ResourceResolveError::NotFound) => {
			eprintln!("error: resource not found: {}", resource_key);
			return ExitCode::from(2);
		}
		Err(ResourceResolveError::NotAResource(kind)) => {
			eprintln!(
				"error: '{}' is not a resource node (kind: {}). \
				 Expected FS_PATH, DB_RESOURCE, BLOB, or STATE+CACHE.",
				resource_key, kind
			);
			return ExitCode::from(2);
		}
		Err(ResourceResolveError::Storage(e)) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Find writers.
	let writers = match storage.find_resource_writers(&snapshot.snapshot_uid, &target.stable_key) {
		Ok(w) => w,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// JSON output (QueryResult envelope).
	let count = writers.len();
	let mut extra = serde_json::Map::new();
	extra.insert("target".to_string(), serde_json::json!(target.stable_key));
	let output = match build_envelope(
		&storage, "resource writers", repo_uid, &snapshot,
		serde_json::to_value(&writers).unwrap(), count, extra,
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

// ── modules command ──────────────────────────────────────────────

fn run_modules(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage:");
		eprintln!("  rmap modules list <db_path> <repo_uid>");
		eprintln!("  rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
		eprintln!("  rmap modules violations <db_path> <repo_uid>");
		eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"list" => run_modules_list(&args[1..]),
		"deps" => run_modules_deps(&args[1..]),
		"violations" => run_modules_violations(&args[1..]),
		"boundary" => run_modules_boundary(&args[1..]),
		other => {
			eprintln!("unknown modules subcommand: {}", other);
			eprintln!("usage:");
			eprintln!("  rmap modules list <db_path> <repo_uid>");
			eprintln!("  rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
			eprintln!("  rmap modules violations <db_path> <repo_uid>");
			eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
			ExitCode::from(1)
		}
	}
}

// ── modules list command ─────────────────────────────────────────

/// Output DTO for `modules list` command.
///
/// Dedicated CLI output shape — does not expose storage internals
/// like `snapshot_uid`, `repo_uid`, or `metadata_json`.
#[derive(serde::Serialize)]
struct ModuleListEntry {
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
	display_name: Option<String>,
	confidence: f64,
}

fn run_modules_list(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap modules list <db_path> <repo_uid>");
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

	// Load module candidates (already sorted by canonical_root_path)
	let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("error: failed to load module candidates: {}", e);
			return ExitCode::from(2);
		}
	};

	// Map to output DTO
	let results: Vec<ModuleListEntry> = modules
		.into_iter()
		.map(|m| ModuleListEntry {
			module_uid: m.module_candidate_uid,
			module_key: m.module_key,
			canonical_root_path: m.canonical_root_path,
			module_kind: m.module_kind,
			display_name: m.display_name,
			confidence: m.confidence,
		})
		.collect();

	let count = results.len();

	let output = match build_envelope(
		&storage,
		"modules list",
		repo_uid,
		&snapshot,
		serde_json::to_value(&results).unwrap(),
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

fn run_modules_deps(args: &[String]) -> ExitCode {
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

	// Load module candidates
	let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("error: failed to load module candidates: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve module filter argument against discovered modules.
	// Resolution precedence: canonical_root_path exact → module_key exact.
	// Unknown module → error (not empty results).
	let resolved_module_path: Option<String> = match module_filter {
		Some(filter) => {
			// Try canonical_root_path exact match first
			let by_path = modules.iter().find(|m| m.canonical_root_path == filter);
			if let Some(m) = by_path {
				Some(m.canonical_root_path.clone())
			} else {
				// Try module_key exact match
				let by_key = modules.iter().find(|m| m.module_key == filter);
				if let Some(m) = by_key {
					Some(m.canonical_root_path.clone())
				} else {
					eprintln!("error: module not found: {}", filter);
					eprintln!(
						"hint: use canonical path (e.g., 'packages/app') or module key"
					);
					return ExitCode::from(1);
				}
			}
		}
		None => None,
	};

	// Load resolved imports
	let imports = match storage.get_resolved_imports_for_snapshot(&snapshot.snapshot_uid) {
		Ok(i) => i,
		Err(e) => {
			eprintln!("error: failed to load imports: {}", e);
			return ExitCode::from(2);
		}
	};

	// Load file ownership
	let ownership = match storage.get_file_ownership_for_snapshot(&snapshot.snapshot_uid) {
		Ok(o) => o,
		Err(e) => {
			eprintln!("error: failed to load file ownership: {}", e);
			return ExitCode::from(2);
		}
	};

	// Convert to classification DTOs
	use repo_graph_classification::module_edges::{
		derive_module_dependency_edges, FileOwnershipFact, ModuleEdgeDerivationInput,
		ModuleRef, ResolvedImportFact,
	};

	let import_facts: Vec<ResolvedImportFact> = imports
		.into_iter()
		.map(|i| ResolvedImportFact {
			source_file_uid: i.source_file_uid,
			target_file_uid: i.target_file_uid,
		})
		.collect();

	let ownership_facts: Vec<FileOwnershipFact> = ownership
		.into_iter()
		.map(|o| FileOwnershipFact {
			file_uid: o.file_uid,
			module_uid: o.module_candidate_uid,
		})
		.collect();

	let module_refs: Vec<ModuleRef> = modules
		.iter()
		.map(|m| ModuleRef {
			module_uid: m.module_candidate_uid.clone(),
			canonical_path: m.canonical_root_path.clone(),
		})
		.collect();

	let input = ModuleEdgeDerivationInput {
		imports: import_facts,
		ownership: ownership_facts,
		modules: module_refs,
	};

	// Derive edges
	let derivation_result = match derive_module_dependency_edges(input) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Filter by resolved module path if specified
	let filtered_edges: Vec<_> = match &resolved_module_path {
		Some(module_path) => {
			derivation_result
				.edges
				.into_iter()
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
		None => derivation_result.edges,
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
			"imports_total": derivation_result.diagnostics.imports_total,
			"imports_cross_module": derivation_result.diagnostics.imports_cross_module,
			"imports_intra_module": derivation_result.diagnostics.imports_intra_module,
			"imports_source_unowned": derivation_result.diagnostics.imports_source_unowned,
			"imports_target_unowned": derivation_result.diagnostics.imports_target_unowned,
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

// ── discovered-module violation helper ───────────────────────────
//
// Shared orchestration for discovered-module boundary evaluation.
// Used by both `modules violations` and unified `violations` commands.
// Returns the evaluation result or an error string for CLI display.

use repo_graph_classification::boundary_evaluator::ModuleBoundaryEvaluation;

fn evaluate_discovered_module_violations(
	storage: &repo_graph_storage::StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
) -> Result<ModuleBoundaryEvaluation, String> {
	// 1. Load module candidates
	let modules = storage
		.get_module_candidates_for_snapshot(snapshot_uid)
		.map_err(|e| format!("failed to load module candidates: {}", e))?;

	// 2. Load active boundary declarations (discovered-module style)
	let declarations = storage
		.get_active_boundary_declarations_for_repo(repo_uid)
		.map_err(|e| format!("failed to load boundary declarations: {}", e))?;

	// 3. Parse discovered-module boundaries
	use repo_graph_classification::boundary_parser::{
		parse_discovered_module_boundaries, RawBoundaryDeclaration,
	};

	let raw_boundaries: Vec<RawBoundaryDeclaration> = declarations
		.iter()
		.map(|d| RawBoundaryDeclaration {
			declaration_uid: d.declaration_uid.clone(),
			value_json: d.value_json.clone(),
		})
		.collect();

	let parsed_boundaries =
		parse_discovered_module_boundaries(&raw_boundaries).map_err(|e| e.to_string())?;

	// 4. Load imports and file ownership for edge derivation
	let imports = storage
		.get_resolved_imports_for_snapshot(snapshot_uid)
		.map_err(|e| format!("failed to load imports: {}", e))?;

	let ownership = storage
		.get_file_ownership_for_snapshot(snapshot_uid)
		.map_err(|e| format!("failed to load file ownership: {}", e))?;

	// 5. Derive module edges
	use repo_graph_classification::module_edges::{
		derive_module_dependency_edges, FileOwnershipFact, ModuleEdgeDerivationInput,
		ModuleRef, ResolvedImportFact,
	};

	let import_facts: Vec<ResolvedImportFact> = imports
		.into_iter()
		.map(|i| ResolvedImportFact {
			source_file_uid: i.source_file_uid,
			target_file_uid: i.target_file_uid,
		})
		.collect();

	let ownership_facts: Vec<FileOwnershipFact> = ownership
		.into_iter()
		.map(|o| FileOwnershipFact {
			file_uid: o.file_uid,
			module_uid: o.module_candidate_uid,
		})
		.collect();

	let module_refs: Vec<ModuleRef> = modules
		.iter()
		.map(|m| ModuleRef {
			module_uid: m.module_candidate_uid.clone(),
			canonical_path: m.canonical_root_path.clone(),
		})
		.collect();

	let derivation_input = ModuleEdgeDerivationInput {
		imports: import_facts,
		ownership: ownership_facts,
		modules: module_refs,
	};

	let derivation_result =
		derive_module_dependency_edges(derivation_input).map_err(|e| e.to_string())?;

	// 6. Build module index for stale detection
	use std::collections::HashMap;
	let module_index: HashMap<String, String> = modules
		.iter()
		.map(|m| {
			(
				m.canonical_root_path.clone(),
				m.module_candidate_uid.clone(),
			)
		})
		.collect();

	// 7. Evaluate boundaries
	use repo_graph_classification::boundary_evaluator::evaluate_module_boundaries;

	let evaluation = evaluate_module_boundaries(
		&parsed_boundaries,
		&derivation_result.edges,
		&module_index,
	);

	Ok(evaluation)
}

// ── modules violations command ───────────────────────────────────

fn run_modules_violations(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap modules violations <db_path> <repo_uid>");
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

	// Use shared helper for discovered-module evaluation
	let evaluation =
		match evaluate_discovered_module_violations(&storage, repo_uid, &snapshot.snapshot_uid) {
			Ok(e) => e,
			Err(msg) => {
				eprintln!("error: {}", msg);
				return ExitCode::from(2);
			}
		};

	use repo_graph_classification::boundary_evaluator::StaleSide;

	// 8. Build JSON output — preserve evaluator order exactly
	let violations_json: Vec<serde_json::Value> = evaluation
		.violations
		.iter()
		.map(|v| {
			serde_json::json!({
				"declaration_uid": v.declaration_uid,
				"source": v.source_canonical_path,
				"target": v.target_canonical_path,
				"import_count": v.import_count,
				"source_file_count": v.source_file_count,
				"reason": v.reason,
			})
		})
		.collect();

	let stale_json: Vec<serde_json::Value> = evaluation
		.stale_declarations
		.iter()
		.map(|s| {
			serde_json::json!({
				"declaration_uid": s.declaration_uid,
				"stale_side": match s.stale_side {
					StaleSide::Source => "source",
					StaleSide::Target => "target",
					StaleSide::Both => "both",
				},
				"missing_paths": s.missing_paths,
			})
		})
		.collect();

	let violation_count = evaluation.violations.len();
	let stale_count = evaluation.stale_declarations.len();

	let results = serde_json::json!({
		"violations": violations_json,
		"stale_declarations": stale_json,
	});

	// Build envelope with count and stale_count
	let mut extra = serde_json::Map::new();
	extra.insert(
		"stale_count".to_string(),
		serde_json::Value::Number(stale_count.into()),
	);

	let output = match build_envelope(
		&storage,
		"modules violations",
		repo_uid,
		&snapshot,
		results,
		violation_count,
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
			// Exit code: 0 if no violations, 1 if violations
			// stale_declarations alone do not force exit 1
			if violation_count > 0 {
				ExitCode::from(1)
			} else {
				ExitCode::SUCCESS
			}
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}

// ── modules boundary command ─────────────────────────────────────

fn run_modules_boundary(args: &[String]) -> ExitCode {
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

	// Load module candidates for resolution
	let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("error: failed to load module candidates: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve source module (canonicalRootPath → moduleKey precedence)
	let source_path = match resolve_module_to_path(&modules, source_arg) {
		Some(p) => p,
		None => {
			eprintln!("error: source module not found: {}", source_arg);
			eprintln!("hint: use canonical path (e.g., 'packages/app') or module key");
			return ExitCode::from(1);
		}
	};

	// Resolve target module
	let target_path = match resolve_module_to_path(&modules, &forbids) {
		Some(p) => p,
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

/// Resolve a module argument to its canonical path.
/// Resolution precedence: canonical_root_path exact → module_key exact.
fn resolve_module_to_path(
	modules: &[repo_graph_storage::types::ModuleCandidate],
	arg: &str,
) -> Option<String> {
	// Try canonical_root_path exact match first
	if let Some(m) = modules.iter().find(|m| m.canonical_root_path == arg) {
		return Some(m.canonical_root_path.clone());
	}
	// Try module_key exact match
	if let Some(m) = modules.iter().find(|m| m.module_key == arg) {
		return Some(m.canonical_root_path.clone());
	}
	None
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

/// Extract module path from a MODULE stable key: `{repo}:{path}:MODULE`
fn extract_module_path_from_key(stable_key: &str) -> Option<String> {
	if !stable_key.ends_with(":MODULE") {
		return None;
	}
	let without_suffix = &stable_key[..stable_key.len() - ":MODULE".len()];
	let colon_pos = without_suffix.find(':')?;
	Some(without_suffix[colon_pos + 1..].to_string())
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

/// Valid edge types for `--edge-types` filter (Rust-17, SB-5).
const VALID_EDGE_TYPES: &[&str] = &["CALLS", "INSTANTIATES", "READS", "WRITES"];

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
