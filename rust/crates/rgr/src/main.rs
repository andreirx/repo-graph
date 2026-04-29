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
//!   rmap cycles  <db_path> <repo_uid>
//!   rmap stats   <db_path> <repo_uid>
//!
//!   rmap gate    <db_path> <repo_uid> [--strict | --advisory]
//!   rmap orient  <db_path> <repo_uid> [--budget small|medium|large] [--focus <string>]
//!   rmap check   <db_path> <repo_uid>
//!   rmap docs list    <db_path> <repo_uid>
//!   rmap docs extract <db_path> <repo_uid>
//!   rmap churn    <db_path> <repo_uid> [--since <expr>]
//!   rmap hotspots <db_path> <repo_uid> [--since <expr>]
//!   rmap assess   <db_path> <repo_uid> [--baseline <snapshot_uid>]
//!
//!   rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]
//!   rmap declare requirement <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]
//!   rmap declare quality-policy <db_path> <repo_uid> <policy_id> --measurement <kind> --policy-kind <kind> --threshold <n> [--version <n>] [--severity <fail|advisory>] [--scope-clause <type>:<selector>]... [--description <text>]
//!   rmap declare deactivate <db_path> <declaration_uid>
//!
//!   rmap resource readers <db_path> <repo_uid> <resource_stable_key>
//!   rmap resource writers <db_path> <repo_uid> <resource_stable_key>
//!
//!   rmap modules list <db_path> <repo_uid>
//!   rmap modules files <db_path> <repo_uid> <module>
//!   rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]
//!   rmap modules violations <db_path> <repo_uid>
//!   rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]
//!
//!   rmap surfaces list <db_path> <repo_uid> [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>]
//!   rmap surfaces show <db_path> <repo_uid> <surface_ref>
//!
//!   rmap policy <db_path> <repo_uid> [--kind STATUS_MAPPING|BEHAVIORAL_MARKER] [--file <path>]
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

mod cli;
mod commands;
mod coverage;
mod module_resolution;

use cli::{
    build_envelope, chrono_now, compute_trust_overlay_for_snapshot, format_gate_error,
    open_storage, print_usage, resolve_repo_ref, utc_now_iso8601,
};
use commands::{
    run_callers, run_callees, run_check_cmd, run_cycles, run_explain_cmd, run_imports,
    run_index, run_orient, run_path, run_policy, run_refresh, run_resource, run_stats,
    run_trust,
};
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
		"orient" => run_orient(&args[2..]),
		"check" => run_check_cmd(&args[2..]),
		"churn" => run_churn(&args[2..]),
		"hotspots" => run_hotspots(&args[2..]),
		"metrics" => run_metrics(&args[2..]),
		"coverage" => run_coverage(&args[2..]),
		"risk" => run_risk(&args[2..]),
		"assess" => run_assess(&args[2..]),
		"explain" => run_explain_cmd(&args[2..]),
		"dead" => run_dead(&args[2..]),
		"cycles" => run_cycles(&args[2..]),
		"stats" => run_stats(&args[2..]),
		"declare" => run_declare(&args[2..]),
		"docs" => run_docs_family(&args[2..]),
		"resource" => run_resource(&args[2..]),
		"modules" => run_modules(&args[2..]),
		"surfaces" => run_surfaces(&args[2..]),
		"policy" => run_policy(&args[2..]),
		other => {
			eprintln!("unknown command: {}", other);
			print_usage();
			ExitCode::from(1)
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

	let discovered_result =
		match evaluate_discovered_module_violations(&storage, repo_uid, &snapshot.snapshot_uid) {
			Ok(r) => r,
			Err(msg) => {
				eprintln!("error: {}", msg);
				return ExitCode::from(2);
			}
		};

	// Convert discovered violations to JSON
	use repo_graph_classification::boundary_evaluator::StaleSide;

	let discovered_violations_json: Vec<serde_json::Value> = discovered_result
		.evaluation
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

	let stale_declarations_json: Vec<serde_json::Value> = discovered_result
		.evaluation
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
	let discovered_count = discovered_result.evaluation.violations.len();
	let stale_count = discovered_result.evaluation.stale_declarations.len();
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
fn run_assess(args: &[String]) -> ExitCode {
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

// ── docs command family ──────────────────────────────────────────
//
// docs is a command family:
//   docs list    — documentation inventory (primary surface)
//   docs extract — semantic fact extraction (secondary hints)
//
// Docs are primary; semantic_facts are secondary derived hints.

fn run_docs_family(args: &[String]) -> ExitCode {
	if args.is_empty() {
		print_docs_usage();
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"list" => run_docs_list(&args[1..]),
		"extract" => run_docs_extract(&args[1..]),
		other => {
			eprintln!("unknown docs subcommand: {}", other);
			print_docs_usage();
			ExitCode::from(1)
		}
	}
}

fn print_docs_usage() {
	eprintln!("usage:");
	eprintln!("  rmap docs list    <db_path> <repo_uid>  — documentation inventory");
	eprintln!("  rmap docs extract <db_path> <repo_uid>  — extract semantic hints");
}

/// List documentation inventory (primary documentation surface).
///
/// Returns doc file paths, kinds, and generated flags. Does NOT
/// derive from semantic_facts — uses live filesystem discovery.
fn run_docs_list(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap docs list <db_path> <repo_uid>");
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

	// Get repo to find root_path
	use repo_graph_storage::types::RepoRef;
	let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
		Ok(Some(r)) => r,
		Ok(None) => {
			eprintln!("error: repo '{}' not found", repo_uid);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	let repo_path = Path::new(&repo.root_path);

	// Discover documentation inventory (live filesystem, not semantic_facts)
	let inventory = match repo_graph_doc_facts::discover_doc_inventory(repo_path, true) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: discovery failed: {}", e);
			return ExitCode::from(2);
		}
	};

	// Build JSON output
	let output = serde_json::json!({
		"command": "docs list",
		"repo": repo_uid,
		"repo_path": repo.root_path,
		"entries": inventory.entries,
		"count": inventory.entries.len(),
		"counts_by_kind": inventory.counts_by_kind,
		"generated_count": inventory.generated_count
	});

	println!("{}", serde_json::to_string_pretty(&output).unwrap());
	ExitCode::from(0)
}

/// Extract semantic facts from documentation (secondary hints).
///
/// Populates semantic_facts table with derived hints for ranking
/// and filtering. The docs themselves remain the primary data.
fn run_docs_extract(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap docs extract <db_path> <repo_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

	let mut storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Get repo to find root_path
	use repo_graph_storage::types::RepoRef;
	let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
		Ok(Some(r)) => r,
		Ok(None) => {
			eprintln!("error: repo '{}' not found", repo_uid);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	let repo_path = Path::new(&repo.root_path);

	// Extract semantic facts from documentation
	let extraction_result = match repo_graph_doc_facts::extract_semantic_facts(repo_path) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: extraction failed: {}", e);
			return ExitCode::from(2);
		}
	};

	// Map ExtractedFact to NewSemanticFact
	let new_facts: Vec<repo_graph_storage::crud::semantic_facts::NewSemanticFact> =
		extraction_result
			.facts
			.iter()
			.map(|f| map_extracted_to_storage(repo_uid, f))
			.collect();

	// Replace facts in storage atomically
	let replace_result = match storage.replace_semantic_facts_for_repo(repo_uid, &new_facts) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: storage failed: {}", e);
			return ExitCode::from(2);
		}
	};

	// Build counts by fact kind
	let mut counts_by_kind: std::collections::HashMap<String, usize> =
		std::collections::HashMap::new();
	for fact in &extraction_result.facts {
		*counts_by_kind
			.entry(fact.fact_kind.as_str().to_string())
			.or_insert(0) += 1;
	}

	// Build files by kind
	let mut files_by_kind: std::collections::HashMap<String, usize> =
		std::collections::HashMap::new();
	for (kind, count) in &extraction_result.files_by_kind {
		files_by_kind.insert(kind.as_str().to_string(), *count);
	}

	// Build JSON output
	let output = serde_json::json!({
		"command": "docs extract",
		"repo": repo_uid,
		"repo_path": repo.root_path,
		"files_scanned": extraction_result.files_scanned,
		"files_by_kind": files_by_kind,
		"facts_extracted": extraction_result.facts.len(),
		"facts_inserted": replace_result.inserted,
		"facts_deleted": replace_result.deleted,
		"counts_by_kind": counts_by_kind,
		"generated_docs_count": extraction_result.generated_docs_count,
		"warnings": extraction_result.warnings.iter()
			.map(|w| serde_json::json!({
				"file": w.file,
				"message": w.message
			}))
			.collect::<Vec<_>>()
	});

	println!("{}", serde_json::to_string_pretty(&output).unwrap());
	ExitCode::from(0)
}

/// Map an ExtractedFact to a NewSemanticFact for storage.
fn map_extracted_to_storage(
	repo_uid: &str,
	fact: &repo_graph_doc_facts::ExtractedFact,
) -> repo_graph_storage::crud::semantic_facts::NewSemanticFact {
	repo_graph_storage::crud::semantic_facts::NewSemanticFact {
		repo_uid: repo_uid.to_string(),
		fact_kind: fact.fact_kind.as_str().to_string(),
		subject_ref: fact.subject_ref.clone(),
		subject_ref_kind: fact.subject_ref_kind.as_str().to_string(),
		object_ref: fact.object_ref.clone(),
		object_ref_kind: fact.object_ref_kind.map(|k| k.as_str().to_string()),
		source_file: fact.source_file.clone(),
		source_line_start: fact.line_start.map(|n| n as i64),
		source_line_end: fact.line_end.map(|n| n as i64),
		source_text_excerpt: fact.excerpt.clone(),
		content_hash: fact.content_hash.clone(),
		extraction_method: fact.extraction_method.as_str().to_string(),
		confidence: fact.confidence,
		generated: fact.generated,
		doc_kind: fact.doc_kind.as_str().to_string(),
	}
}

// ── dead command ─────────────────────────────────────────────────

/// CLI output DTO for dead-code results with per-result trust.
///
/// Wraps the storage DeadNodeResult and adds a local trust section.
/// Every dead result carries explicit confidence — no Option A hiding.
///
/// NOTE: Struct is kept for reintroduction of the dead-code surface.
/// The `dead` command is currently disabled; see run_dead() comment.
#[allow(dead_code)]
#[derive(serde::Serialize)]
struct DeadNodeOutput {
	stable_key: String,
	symbol: String,
	kind: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	subtype: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	file: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	line: Option<i64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	line_count: Option<i64>,
	is_test: bool,
	/// Per-result dead-code confidence.
	trust: repo_graph_trust::DeadResultTrust,
}

fn run_dead(_args: &[String]) -> ExitCode {
	// ══════════════════════════════════════════════════════════════════
	// DELIBERATELY DISABLED — 2026-04-27
	//
	// The `dead` command is removed from the CLI surface because:
	//
	// 1. Current signal quality produces 85-95% false positive rates
	//    on real-world codebases (smoke-run validation 2026-04-27).
	//
	// 2. Missing framework detectors (Spring, React, Axum, FastAPI)
	//    cause all runtime-owned symbols to appear dead.
	//
	// 3. Missing entrypoint declarations cause all entry symbols to
	//    appear dead.
	//
	// 4. A misleading "dead" label is worse than no label — it directs
	//    agents toward the wrong investigation frontier.
	//
	// The underlying substrate is preserved:
	// - storage::find_dead_nodes() still works
	// - trust::assess_dead_confidence() still works
	// - tests pinning current behavior remain
	//
	// This surface will be reintroduced as TWO separate commands:
	//
	// 1. `rmap orphans` — structural graph orphans, no deadness claim
	// 2. `rmap dead` — coverage-backed + framework-liveness-backed,
	//    much stronger evidence required
	//
	// See docs/TECH-DEBT.md for the full rationale and reintroduction
	// criteria.
	// ══════════════════════════════════════════════════════════════════

	eprintln!("error: `rmap dead` is disabled");
	eprintln!();
	eprintln!("Dead-code detection is not available in rmap because current");
	eprintln!("signal quality produces high false-positive rates (85-95% on");
	eprintln!("real codebases). Using this output would mislead agents into");
	eprintln!("investigating or deleting live code.");
	eprintln!();
	eprintln!("Root causes:");
	eprintln!("  - Missing framework detectors (Spring, React, Axum, FastAPI)");
	eprintln!("  - Missing entrypoint declarations");
	eprintln!("  - No coverage-backed evidence");
	eprintln!();
	eprintln!("Alternative discovery commands that work:");
	eprintln!("  rmap callers  - trace who calls a symbol");
	eprintln!("  rmap callees  - trace what a symbol calls");
	eprintln!("  rmap imports  - trace file imports");
	eprintln!("  rmap orient   - repo overview with trust signals");
	eprintln!("  rmap trust    - detailed reliability report");
	eprintln!();
	eprintln!("Dead-code surface will be reintroduced when:");
	eprintln!("  - Framework entrypoint detection is mature, OR");
	eprintln!("  - Coverage-backed evidence is available");

	ExitCode::from(2)
}

// ── churn command ────────────────────────────────────────────────
//
// RS-MS-2: Query-time per-file git churn for indexed files.
// No persistence. Git is the authoritative history source.

/// Output row for churn command.
#[derive(serde::Serialize)]
struct ChurnRow {
	file_path: String,
	commit_count: u64,
	lines_changed: u64,
}

fn run_churn(args: &[String]) -> ExitCode {
	// Parse args: <db_path> <repo_uid> [--since <expr>]
	// Default --since: 90.days.ago
	let (db_path, repo_uid, since) = match parse_churn_args(args) {
		Ok(parsed) => parsed,
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

	// Get repo for root_path (needed to invoke git)
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

	// Get indexed files for filtering
	let indexed_files = match storage.get_files_by_repo(repo_uid) {
		Ok(files) => files,
		Err(e) => {
			eprintln!("error: failed to read indexed files: {}", e);
			return ExitCode::from(2);
		}
	};

	let indexed_paths: std::collections::HashSet<&str> =
		indexed_files.iter().map(|f| f.path.as_str()).collect();

	// Call git crate for churn
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

	// Filter to indexed files only, preserving git crate ordering
	let results: Vec<ChurnRow> = raw_churn
		.into_iter()
		.filter(|entry| indexed_paths.contains(entry.file_path.as_str()))
		.map(|entry| ChurnRow {
			file_path: entry.file_path,
			commit_count: entry.commit_count,
			lines_changed: entry.lines_changed,
		})
		.collect();

	// Build envelope with extra `since` field
	let count = results.len();
	let mut extra = serde_json::Map::new();
	extra.insert("since".to_string(), serde_json::Value::String(since.clone()));

	let output = match build_envelope(
		&storage,
		"churn",
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

/// Parse churn command args.
/// Returns (db_path, repo_uid, since).
fn parse_churn_args(args: &[String]) -> Result<(&Path, &str, String), String> {
	if args.len() < 2 {
		return Err("usage: rmap churn <db_path> <repo_uid> [--since <expr>]".to_string());
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

	// Default window
	let mut since = "90.days.ago".to_string();

	// Parse optional --since flag
	let mut i = 2;
	while i < args.len() {
		if args[i] == "--since" {
			if i + 1 >= args.len() {
				return Err("--since requires a value".to_string());
			}
			since = args[i + 1].clone();
			i += 2;
		} else {
			return Err(format!("unknown argument: {}", args[i]));
		}
	}

	Ok((db_path, repo_uid, since))
}

// ── hotspots command ─────────────────────────────────────────────
//
// RS-MS-3b: Query-time hotspot analysis (churn × complexity).
// No persistence. Git is the authoritative churn source.
// Complexity from stored measurements.

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

fn run_hotspots(args: &[String]) -> ExitCode {
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

	// Get per-file complexity via proper join (measurements → nodes → files).
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

	// Build file_path → is_test lookup
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

// ── metrics command ──────────────────────────────────────────────
//
// Quality Control Phase A: Query measurements for display.
// Supports kind filter, sorting, and limit. Default sort: value desc.

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

fn run_metrics(args: &[String]) -> ExitCode {
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

// ── coverage command ─────────────────────────────────────────────
//
// RS-MS-4-prereq-b/c: Import Istanbul/c8 coverage into measurements.
// Delete-before-insert for idempotency. Reports matched/unmatched counts.

#[derive(serde::Serialize)]
struct CoverageImportResult {
	file_path: String,
	line_coverage: f64,
	covered_statements: u64,
	total_statements: u64,
}

fn run_coverage(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap coverage <db_path> <repo_uid> <report_path>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let report_path = Path::new(&args[2]);

	// Validate report exists
	if !report_path.is_file() {
		eprintln!("error: coverage report not found: {}", report_path.display());
		return ExitCode::from(1);
	}

	// Open storage
	let mut storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Get latest snapshot
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

	// Resolve repo root to absolute path for coverage normalization
	// The DB may store "." which won't match absolute paths in the coverage report
	let repo_root_abs = match std::fs::canonicalize(&repo.root_path) {
		Ok(p) => p,
		Err(e) => {
			eprintln!(
				"error: cannot resolve repo root '{}': {}",
				repo.root_path, e
			);
			return ExitCode::from(2);
		}
	};

	// Parse coverage report
	use repo_graph_coverage::parse_istanbul_file;
	let parse_result =
		match parse_istanbul_file(report_path.to_str().unwrap(), repo_root_abs.to_str().unwrap()) {
			Ok(r) => r,
			Err(e) => {
				eprintln!("error: failed to parse coverage report: {}", e);
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

	let indexed_paths: std::collections::HashSet<String> =
		indexed_files.iter().map(|f| f.path.clone()).collect();

	// Match coverage to indexed files
	let now = chrono_now();
	let match_result = match coverage::match_coverage_to_indexed_files(
		&parse_result,
		&indexed_paths,
		repo_uid,
		&snapshot.snapshot_uid,
		&now,
	) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Atomically replace existing line_coverage measurements with new ones.
	// Single transaction ensures no data loss if insert fails.
	if let Err(e) = storage.replace_measurements_by_kind(
		&snapshot.snapshot_uid,
		&["line_coverage"],
		&match_result.measurements,
	) {
		eprintln!("error: failed to replace coverage measurements: {}", e);
		return ExitCode::from(2);
	}

	// Build output
	let results: Vec<CoverageImportResult> = match_result
		.measurements
		.iter()
		.map(|m| {
			// Parse value_json to extract fields
			let v: serde_json::Value = serde_json::from_str(&m.value_json).unwrap_or_default();
			CoverageImportResult {
				// Extract path from stable key: {repo}:{path}:FILE
				file_path: m
					.target_stable_key
					.strip_prefix(&format!("{}:", repo_uid))
					.and_then(|s| s.strip_suffix(":FILE"))
					.unwrap_or(&m.target_stable_key)
					.to_string(),
				line_coverage: v.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0),
				covered_statements: v.get("covered").and_then(|v| v.as_u64()).unwrap_or(0),
				total_statements: v.get("total").and_then(|v| v.as_u64()).unwrap_or(0),
			}
		})
		.collect();

	// Build envelope with extra stats
	let mut extra = serde_json::Map::new();
	extra.insert(
		"imported_count".to_string(),
		serde_json::Value::Number(match_result.matched_count.into()),
	);
	extra.insert(
		"unnormalized_count".to_string(),
		serde_json::Value::Number(match_result.unnormalized_paths.len().into()),
	);
	extra.insert(
		"unmatched_indexed_count".to_string(),
		serde_json::Value::Number(match_result.unmatched_indexed_paths.len().into()),
	);

	// Include sample unmatched paths for debugging (max 10)
	if !match_result.unnormalized_paths.is_empty() {
		let sample: Vec<_> = match_result
			.unnormalized_paths
			.iter()
			.take(10)
			.cloned()
			.collect();
		extra.insert(
			"unnormalized_paths_sample".to_string(),
			serde_json::to_value(sample).unwrap(),
		);
	}
	if !match_result.unmatched_indexed_paths.is_empty() {
		let sample: Vec<_> = match_result
			.unmatched_indexed_paths
			.iter()
			.take(10)
			.cloned()
			.collect();
		extra.insert(
			"unmatched_indexed_paths_sample".to_string(),
			serde_json::to_value(sample).unwrap(),
		);
	}

	let count = results.len();
	let output = match build_envelope(
		&storage,
		"coverage",
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

// ── risk command ─────────────────────────────────────────────────
//
// RS-MS-4: Query-time risk analysis (hotspot × coverage gap).
// Only files with BOTH hotspot AND coverage data are included.
// Missing coverage = file excluded (not degraded to risk = hotspot).

#[derive(serde::Serialize)]
struct RiskRow {
	file_path: String,
	risk_score: f64,
	hotspot_score: u64,
	line_coverage: f64,
	lines_changed: u64,
	sum_complexity: u64,
}

fn run_risk(args: &[String]) -> ExitCode {
	// Parse args: same as churn/hotspots
	let (db_path, repo_uid, since) = match parse_churn_args(args) {
		Ok(parsed) => parsed,
		Err(msg) => {
			if msg.contains("churn") {
				eprintln!("usage: rmap risk <db_path> <repo_uid> [--since <expr>]");
			} else {
				eprintln!("{}", msg);
			}
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

	// Get per-file complexity
	let complexity_rows = match storage.query_complexity_by_file(&snapshot.snapshot_uid) {
		Ok(rows) => rows,
		Err(e) => {
			eprintln!("error: failed to read complexity measurements: {}", e);
			return ExitCode::from(2);
		}
	};

	let complexity_inputs: Vec<repo_graph_classification::hotspot_scorer::ComplexityInput> =
		complexity_rows
			.into_iter()
			.map(|row| repo_graph_classification::hotspot_scorer::ComplexityInput {
				file_path: row.file_path,
				sum_complexity: row.sum_complexity,
			})
			.collect();

	// Compute hotspots first
	let hotspots = repo_graph_classification::hotspot_scorer::compute_hotspots(
		&churn_inputs,
		&complexity_inputs,
	);

	// Get coverage measurements
	let coverage_rows = match storage.query_measurements_by_kind(&snapshot.snapshot_uid, "line_coverage") {
		Ok(rows) => rows,
		Err(e) => {
			eprintln!("error: failed to read coverage measurements: {}", e);
			return ExitCode::from(2);
		}
	};

	// Parse coverage measurements into CoverageInput with strict validation.
	// Malformed measurements abort (exit 2), matching gate surface contract.
	// target_stable_key format: {repo_uid}:{file_path}:FILE
	let expected_prefix = format!("{}:", repo_uid);
	let mut coverage_inputs: Vec<repo_graph_classification::risk_scorer::CoverageInput> =
		Vec::with_capacity(coverage_rows.len());

	for row in &coverage_rows {
		// Validate target_stable_key format
		let file_path = match row
			.target_stable_key
			.strip_prefix(&expected_prefix)
			.and_then(|s| s.strip_suffix(":FILE"))
		{
			Some(p) => p,
			None => {
				eprintln!(
					"error: malformed coverage measurement target_stable_key: {}",
					row.target_stable_key
				);
				return ExitCode::from(2);
			}
		};

		// Parse value_json strictly
		let v: serde_json::Value = match serde_json::from_str(&row.value_json) {
			Ok(v) => v,
			Err(e) => {
				eprintln!(
					"error: malformed coverage measurement JSON for {}: {}",
					file_path, e
				);
				return ExitCode::from(2);
			}
		};

		let line_coverage = match v.get("value").and_then(|v| v.as_f64()) {
			Some(c) => c,
			None => {
				eprintln!(
					"error: coverage measurement missing 'value' field for {}",
					file_path
				);
				return ExitCode::from(2);
			}
		};

		coverage_inputs.push(repo_graph_classification::risk_scorer::CoverageInput {
			file_path: file_path.to_string(),
			line_coverage,
		});
	}

	// Compute risk scores
	let risk_entries = repo_graph_classification::risk_scorer::compute_risk(&hotspots, &coverage_inputs);

	// Convert to output rows
	let results: Vec<RiskRow> = risk_entries
		.into_iter()
		.map(|r| RiskRow {
			file_path: r.file_path,
			risk_score: r.risk_score,
			hotspot_score: r.hotspot_score,
			line_coverage: r.line_coverage,
			lines_changed: r.lines_changed,
			sum_complexity: r.sum_complexity,
		})
		.collect();

	// Build envelope
	let count = results.len();
	let hotspot_count = hotspots.len();
	let coverage_count = coverage_inputs.len();

	let mut extra = serde_json::Map::new();
	extra.insert("since".to_string(), serde_json::Value::String(since.clone()));
	extra.insert(
		"formula".to_string(),
		serde_json::Value::String("hotspot_score * (1 - line_coverage)".to_string()),
	);
	extra.insert(
		"hotspot_files".to_string(),
		serde_json::Value::Number(hotspot_count.into()),
	);
	extra.insert(
		"coverage_files".to_string(),
		serde_json::Value::Number(coverage_count.into()),
	);
	extra.insert(
		"joined_files".to_string(),
		serde_json::Value::Number(count.into()),
	);

	let output = match build_envelope(
		&storage,
		"risk",
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

// ── declare command ──────────────────────────────────────────────

fn run_declare(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage: rmap declare <subcommand> ...");
		eprintln!("subcommands: boundary, requirement, waiver, quality-policy, deactivate, supersede");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"boundary" => run_declare_boundary(&args[1..]),
		"requirement" => run_declare_requirement(&args[1..]),
		"waiver" => run_declare_waiver(&args[1..]),
		"quality-policy" => run_declare_quality_policy(&args[1..]),
		"deactivate" => run_declare_deactivate(&args[1..]),
		"supersede" => run_declare_supersede(&args[1..]),
		other => {
			eprintln!("unknown declare subcommand: {}", other);
			eprintln!("subcommands: boundary, requirement, waiver, quality-policy, deactivate, supersede");
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

const DECLARE_QUALITY_POLICY_USAGE: &str = "usage: rmap declare quality-policy <db_path> <repo_uid> <policy_id> \\
  --measurement <kind> --policy-kind <kind> --threshold <n> [--version <n>] \\
  [--severity <fail|advisory>] [--scope-clause <type>:<selector>]... [--description <text>]";

fn run_declare_quality_policy(args: &[String]) -> ExitCode {
	use repo_graph_quality_policy::{
		parse_measurement_kind, validate_quality_policy_payload, SupportedMeasurementKind,
	};
	use repo_graph_storage::crud::declarations::{
		quality_policy_identity_key, DeclarationInsert,
	};
	use repo_graph_storage::types::{
		QualityPolicyKind, QualityPolicyPayload, QualityPolicySeverity, ScopeClause,
		ScopeClauseKind,
	};

	let mut positional = Vec::new();
	let mut version: Option<String> = None;
	let mut measurement: Option<String> = None;
	let mut policy_kind: Option<String> = None;
	let mut threshold: Option<String> = None;
	let mut severity: Option<String> = None;
	let mut scope_clauses_raw: Vec<String> = Vec::new();
	let mut description: Option<String> = None;
	let mut i = 0;

	/// Parse a required flag value.
	fn parse_required_flag<'a>(
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

	/// Parse a repeatable flag value.
	fn parse_repeatable_flag<'a>(
		flag_name: &str,
		args: &'a [String],
		i: &mut usize,
	) -> Option<String> {
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
			"--version" => match parse_required_flag("--version", &version, args, &mut i) {
				Some(v) => version = Some(v),
				None => return ExitCode::from(1),
			},
			"--measurement" => {
				match parse_required_flag("--measurement", &measurement, args, &mut i) {
					Some(v) => measurement = Some(v),
					None => return ExitCode::from(1),
				}
			}
			"--policy-kind" => {
				match parse_required_flag("--policy-kind", &policy_kind, args, &mut i) {
					Some(v) => policy_kind = Some(v),
					None => return ExitCode::from(1),
				}
			}
			"--threshold" => match parse_required_flag("--threshold", &threshold, args, &mut i) {
				Some(v) => threshold = Some(v),
				None => return ExitCode::from(1),
			},
			"--severity" => match parse_required_flag("--severity", &severity, args, &mut i) {
				Some(v) => severity = Some(v),
				None => return ExitCode::from(1),
			},
			"--scope-clause" => match parse_repeatable_flag("--scope-clause", args, &mut i) {
				Some(v) => scope_clauses_raw.push(v),
				None => return ExitCode::from(1),
			},
			"--description" => {
				match parse_required_flag("--description", &description, args, &mut i) {
					Some(v) => description = Some(v),
					None => return ExitCode::from(1),
				}
			}
			other if other.starts_with('-') => {
				eprintln!("error: unknown flag: {}", other);
				eprintln!("{}", DECLARE_QUALITY_POLICY_USAGE);
				return ExitCode::from(1);
			}
			_ => positional.push(&args[i]),
		}
		i += 1;
	}

	// Validate positional args: db_path, repo_uid, policy_id.
	if positional.len() != 3 {
		eprintln!("{}", DECLARE_QUALITY_POLICY_USAGE);
		return ExitCode::from(1);
	}

	let db_path = Path::new(positional[0].as_str());
	let repo_uid = positional[1].as_str();
	let policy_id = positional[2].as_str();

	if policy_id.trim().is_empty() {
		eprintln!("error: policy_id must be non-empty");
		return ExitCode::from(1);
	}

	// Version defaults to 1.
	let version_num: i64 = match version {
		Some(v) => match v.parse() {
			Ok(n) => n,
			Err(_) => {
				eprintln!("error: --version must be an integer, got: {}", v);
				return ExitCode::from(1);
			}
		},
		None => 1,
	};

	// Validate and parse measurement kind.
	let measurement_str = match measurement {
		Some(v) => v,
		None => {
			eprintln!("error: --measurement is required");
			eprintln!(
				"supported kinds: {}",
				SupportedMeasurementKind::supported_kinds_display()
			);
			return ExitCode::from(1);
		}
	};
	if let Err(e) = parse_measurement_kind(&measurement_str) {
		eprintln!("error: {}", e);
		eprintln!(
			"supported kinds: {}",
			SupportedMeasurementKind::supported_kinds_display()
		);
		return ExitCode::from(1);
	}

	// Validate and parse policy kind.
	let policy_kind_str = match policy_kind {
		Some(v) => v,
		None => {
			eprintln!("error: --policy-kind is required");
			return ExitCode::from(1);
		}
	};
	let policy_kind_enum = match QualityPolicyKind::from_str(&policy_kind_str) {
		Some(k) => k,
		None => {
			eprintln!(
				"error: invalid --policy-kind: '{}'; valid values: absolute_max, absolute_min, no_new, no_worsened",
				policy_kind_str
			);
			return ExitCode::from(1);
		}
	};

	// Validate and parse threshold.
	let threshold_str = match threshold {
		Some(v) => v,
		None => {
			eprintln!("error: --threshold is required");
			return ExitCode::from(1);
		}
	};
	let threshold_num: f64 = match threshold_str.parse() {
		Ok(v) => v,
		Err(_) => {
			eprintln!(
				"error: --threshold must be a number, got: {}",
				threshold_str
			);
			return ExitCode::from(1);
		}
	};

	// Parse severity (default: fail).
	let severity_enum = match severity.as_deref() {
		None | Some("fail") => QualityPolicySeverity::Fail,
		Some("advisory") => QualityPolicySeverity::Advisory,
		Some(other) => {
			eprintln!(
				"error: invalid --severity: '{}'; valid values: fail, advisory",
				other
			);
			return ExitCode::from(1);
		}
	};

	// Parse scope clauses from <type>:<selector> format.
	let mut scope_clauses = Vec::new();
	for clause_str in &scope_clauses_raw {
		let parts: Vec<&str> = clause_str.splitn(2, ':').collect();
		if parts.len() != 2 {
			eprintln!(
				"error: invalid --scope-clause format: '{}'; expected <type>:<selector>",
				clause_str
			);
			return ExitCode::from(1);
		}
		let clause_type = parts[0].trim();
		let selector = parts[1].trim();
		if selector.is_empty() {
			eprintln!(
				"error: --scope-clause selector is empty in '{}'",
				clause_str
			);
			return ExitCode::from(1);
		}
		let clause_kind = match ScopeClauseKind::from_str(clause_type) {
			Some(k) => k,
			None => {
				eprintln!(
					"error: invalid scope clause type: '{}'; valid types: module, file, symbol_kind",
					clause_type
				);
				return ExitCode::from(1);
			}
		};
		scope_clauses.push(ScopeClause::new(clause_kind, selector));
	}

	// Build the payload.
	let payload = QualityPolicyPayload {
		policy_id: policy_id.to_string(),
		version: version_num,
		scope_clauses,
		measurement_kind: measurement_str.clone(),
		policy_kind: policy_kind_enum,
		threshold: threshold_num,
		severity: severity_enum,
		description,
	};

	// Validate payload using the quality-policy domain crate.
	let errors = validate_quality_policy_payload(&payload);
	if !errors.is_empty() {
		for e in errors {
			eprintln!("error: {}", e);
		}
		return ExitCode::from(1);
	}

	// Open storage.
	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	// Build the declaration.
	let target_stable_key = format!("{}:REPO", repo_uid);
	let now = utc_now_iso8601();

	let value_json = match serde_json::to_string(&payload) {
		Ok(j) => j,
		Err(e) => {
			eprintln!("error: failed to serialize payload: {}", e);
			return ExitCode::from(2);
		}
	};

	let decl = DeclarationInsert {
		identity_key: quality_policy_identity_key(repo_uid, policy_id, version_num),
		repo_uid: repo_uid.to_string(),
		target_stable_key,
		kind: "quality_policy".to_string(),
		value_json,
		created_at: now,
		created_by: Some("cli".to_string()),
		supersedes_uid: None,
		authored_basis_json: None,
	};

	match storage.insert_declaration(&decl) {
		Ok(result) => {
			let output = serde_json::json!({
				"declaration_uid": result.declaration_uid,
				"kind": "quality_policy",
				"policy_id": policy_id,
				"version": version_num,
				"measurement": measurement_str,
				"policy_kind": policy_kind_str,
				"threshold": threshold_num,
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

// ── modules command ──────────────────────────────────────────────

fn run_modules(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage:");
		eprintln!("  rmap modules list <db_path> <repo_uid>");
		eprintln!("  rmap modules show <db_path> <repo_uid> <module>");
		eprintln!("  rmap modules files <db_path> <repo_uid> <module>");
		eprintln!("  rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
		eprintln!("  rmap modules violations <db_path> <repo_uid>");
		eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"list" => run_modules_list(&args[1..]),
		"show" => run_modules_show(&args[1..]),
		"files" => run_modules_files(&args[1..]),
		"deps" => run_modules_deps(&args[1..]),
		"violations" => run_modules_violations(&args[1..]),
		"boundary" => run_modules_boundary(&args[1..]),
		other => {
			eprintln!("unknown modules subcommand: {}", other);
			eprintln!("usage:");
			eprintln!("  rmap modules list <db_path> <repo_uid>");
			eprintln!("  rmap modules show <db_path> <repo_uid> <module>");
			eprintln!("  rmap modules files <db_path> <repo_uid> <module>");
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
///
/// RS-MG-12b: Extended with rollup fields for per-module stats.
#[derive(serde::Serialize)]
struct ModuleListEntry {
	// Identity fields
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
	display_name: Option<String>,
	confidence: f64,
	// Rollup fields (RS-MG-12b)
	owned_file_count: u64,
	owned_test_file_count: u64,
	outbound_dependency_count: u64,
	outbound_import_count: u64,
	inbound_dependency_count: u64,
	inbound_import_count: u64,
	/// `None` when policy-derived rollups are unavailable (parse failure).
	/// `Some(0)` means zero violations; `None` means unknown.
	violation_count: Option<u64>,
	dead_symbol_count: u64,
	dead_test_symbol_count: u64,
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

	// ── Step 1: Load module context (with fallback) ──────────────
	// ModuleQueryContext handles fallback from TS tables to Rust nodes/edges.
	use crate::module_resolution::ModuleQueryContext;
	let ctx = match ModuleQueryContext::load(&storage, &snapshot.snapshot_uid) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: failed to load module context: {}", e);
			return ExitCode::from(2);
		}
	};
	let modules = ctx.modules;

	// If still no modules, return early with empty result
	if modules.is_empty() {
		// Empty modules → no degradation, no warnings
		let mut empty_extra = serde_json::Map::new();
		empty_extra.insert("rollups_degraded".to_string(), serde_json::Value::Bool(false));
		empty_extra.insert("warnings".to_string(), serde_json::Value::Array(vec![]));

		let output = match build_envelope(
			&storage,
			"modules list",
			repo_uid,
			&snapshot,
			serde_json::Value::Array(vec![]),
			0,
			empty_extra,
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
				return ExitCode::SUCCESS;
			}
			Err(e) => {
				eprintln!("error: {}", e);
				return ExitCode::from(2);
			}
		}
	}

	// ── Step 2: Load data for rollup computation ──────────────────

	// 2a. Owned files with is_test flag (from context, fallback already applied)
	let owned_files = ctx.owned_files;

	// 2b. Resolved imports for edge derivation
	let imports = match storage.get_resolved_imports_for_snapshot(&snapshot.snapshot_uid) {
		Ok(i) => i,
		Err(e) => {
			eprintln!("error: failed to load imports: {}", e);
			return ExitCode::from(2);
		}
	};

	// 2c. File ownership for edge derivation (from context, fallback already applied)
	let ownership = ctx.ownership;

	// 2d. Dead nodes (SYMBOL kind only)
	let dead_nodes = match storage.find_dead_nodes(&snapshot.snapshot_uid, repo_uid, Some("SYMBOL"))
	{
		Ok(d) => d,
		Err(e) => {
			eprintln!("error: failed to load dead nodes: {}", e);
			return ExitCode::from(2);
		}
	};

	// 2e. Module boundary violations (advisory — catalog survives policy corruption)
	let (violations_eval, violations_warning): (Option<ModuleBoundaryEvaluation>, Option<String>) =
		match evaluate_discovered_module_violations(&storage, repo_uid, &snapshot.snapshot_uid) {
			Ok(r) => (Some(r.evaluation), None),
			Err(msg) => (
				None,
				Some(format!("discovered-module violation rollups unavailable: {}", msg)),
			),
		};

	// ── Step 3: Derive module edges ───────────────────────────────
	use repo_graph_classification::module_edges::{
		derive_module_dependency_edges, FileOwnershipFact, ModuleEdgeDerivationInput,
		ModuleRef, ResolvedImportFact,
	};

	let module_refs: Vec<ModuleRef> = modules
		.iter()
		.map(|m| ModuleRef {
			module_uid: m.module_candidate_uid.clone(),
			canonical_path: m.canonical_root_path.clone(),
		})
		.collect();

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

	let edge_input = ModuleEdgeDerivationInput {
		imports: import_facts,
		ownership: ownership_facts,
		modules: module_refs.clone(),
	};

	let edge_result = match derive_module_dependency_edges(edge_input) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: failed to derive module edges: {}", e);
			return ExitCode::from(2);
		}
	};
	let edges = edge_result.edges;

	// ── Step 4: Compute rollups ───────────────────────────────────
	use repo_graph_classification::module_rollup::{
		compute_module_rollups, DeadNodeFact, ModuleRollupInput, OwnedFileFact,
	};

	let owned_file_facts: Vec<OwnedFileFact> = owned_files
		.into_iter()
		.map(|f| OwnedFileFact {
			file_path: f.file_path,
			module_uid: f.module_candidate_uid,
			is_test: f.is_test,
		})
		.collect();

	let dead_node_facts: Vec<DeadNodeFact> = dead_nodes
		.into_iter()
		.filter_map(|d| {
			d.file.map(|file_path| DeadNodeFact {
				file_path,
				is_test: d.is_test,
			})
		})
		.collect();

	// When violations are unavailable, pass empty vec — rollups will compute
	// violation_count as 0, but we'll override to None in the output.
	let violations_for_rollup = violations_eval
		.as_ref()
		.map(|e| e.violations.clone())
		.unwrap_or_default();

	let rollup_input = ModuleRollupInput {
		modules: module_refs,
		owned_files: owned_file_facts,
		edges,
		violations: violations_for_rollup,
		dead_nodes: dead_node_facts,
	};

	let rollups = match compute_module_rollups(&rollup_input) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: failed to compute rollups: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 5: Build rollup lookup by module_uid ─────────────────
	use std::collections::HashMap;
	let rollup_map: HashMap<&str, &repo_graph_classification::module_rollup::ModuleRollup> =
		rollups.iter().map(|r| (r.module_uid.as_str(), r)).collect();

	// ── Step 6: Merge module identity with rollup stats ───────────
	// violation_count is None when violations_eval failed (policy unavailable)
	let violations_available = violations_eval.is_some();

	let results: Vec<ModuleListEntry> = modules
		.into_iter()
		.map(|m| {
			let rollup = rollup_map.get(m.module_candidate_uid.as_str());
			ModuleListEntry {
				module_uid: m.module_candidate_uid,
				module_key: m.module_key,
				canonical_root_path: m.canonical_root_path,
				module_kind: m.module_kind,
				display_name: m.display_name,
				confidence: m.confidence,
				// Rollup fields — default to 0 if rollup missing (shouldn't happen)
				owned_file_count: rollup.map_or(0, |r| r.owned_file_count),
				owned_test_file_count: rollup.map_or(0, |r| r.owned_test_file_count),
				outbound_dependency_count: rollup.map_or(0, |r| r.outbound_dependency_count),
				outbound_import_count: rollup.map_or(0, |r| r.outbound_import_count),
				inbound_dependency_count: rollup.map_or(0, |r| r.inbound_dependency_count),
				inbound_import_count: rollup.map_or(0, |r| r.inbound_import_count),
				// None when policy parsing failed; Some(count) when available
				violation_count: if violations_available {
					Some(rollup.map_or(0, |r| r.violation_count))
				} else {
					None
				},
				dead_symbol_count: rollup.map_or(0, |r| r.dead_symbol_count),
				dead_test_symbol_count: rollup.map_or(0, |r| r.dead_test_symbol_count),
			}
		})
		.collect();

	let count = results.len();

	// Build extra envelope fields for degradation status
	let mut extra_fields = serde_json::Map::new();
	extra_fields.insert(
		"rollups_degraded".to_string(),
		serde_json::Value::Bool(!violations_available),
	);

	let warnings: Vec<String> = violations_warning.into_iter().collect();
	extra_fields.insert(
		"warnings".to_string(),
		serde_json::to_value(&warnings).unwrap(),
	);

	let output = match build_envelope(
		&storage,
		"modules list",
		repo_uid,
		&snapshot,
		serde_json::to_value(&results).unwrap(),
		count,
		extra_fields,
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

// ── modules show command ─────────────────────────────────────────

/// Module identity DTO for `modules show` output.
#[derive(serde::Serialize, Clone)]
struct ModuleIdentity {
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
	display_name: Option<String>,
	confidence: f64,
}

/// Rollups DTO for `modules show` output.
/// Matches `modules list` rollup fields, with `violation_count` nullable.
#[derive(serde::Serialize)]
struct ModuleShowRollups {
	owned_file_count: u64,
	owned_test_file_count: u64,
	outbound_dependency_count: u64,
	outbound_import_count: u64,
	inbound_dependency_count: u64,
	inbound_import_count: u64,
	violation_count: Option<u64>,
	dead_symbol_count: u64,
	dead_test_symbol_count: u64,
}

/// Weighted neighbor DTO with full identity.
#[derive(serde::Serialize)]
struct EnrichedNeighbor {
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
	import_count: u64,
	source_file_count: u64,
}

/// Target module identity for violation output.
#[derive(serde::Serialize)]
struct ViolationTargetIdentity {
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
}

/// Violation DTO with enriched target identity.
#[derive(serde::Serialize)]
struct EnrichedViolation {
	declaration_uid: String,
	target: ViolationTargetIdentity,
	import_count: u64,
	source_file_count: u64,
	reason: Option<String>,
}

fn run_modules_show(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap modules show <db_path> <repo_uid> <module>");
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
			return ExitCode::from(2);
		}
	};

	// ── Step 1: Load module context (with fallback) ──────────────
	use crate::module_resolution::ModuleQueryContext;
	let ctx = match ModuleQueryContext::load(&storage, &snapshot.snapshot_uid) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: failed to load module context: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 2: Resolve module argument ───────────────────────────
	// Resolution: canonical_root_path exact → module_key exact → module_uid exact → exit 1
	let resolved_module = match ctx.resolve_module(module_arg) {
		Some(m) => m.clone(),
		None => {
			eprintln!("error: module not found: {}", module_arg);
			return ExitCode::from(1); // Exit 1 for resolution failure
		}
	};
	let modules = ctx.modules;

	// Build module identity lookup for enrichment
	let module_identity_map: std::collections::HashMap<&str, ModuleIdentity> = modules
		.iter()
		.map(|m| {
			(
				m.canonical_root_path.as_str(),
				ModuleIdentity {
					module_uid: m.module_candidate_uid.clone(),
					module_key: m.module_key.clone(),
					canonical_root_path: m.canonical_root_path.clone(),
					module_kind: m.module_kind.clone(),
					display_name: m.display_name.clone(),
					confidence: m.confidence,
				},
			)
		})
		.collect();

	// ── Step 3: Load data for computation ─────────────────────────
	// Owned files and ownership from context (fallback already applied)
	let owned_files = ctx.owned_files;
	let ownership = ctx.ownership;

	let imports = match storage.get_resolved_imports_for_snapshot(&snapshot.snapshot_uid) {
		Ok(i) => i,
		Err(e) => {
			eprintln!("error: failed to load imports: {}", e);
			return ExitCode::from(2);
		}
	};

	let dead_nodes = match storage.find_dead_nodes(&snapshot.snapshot_uid, repo_uid, Some("SYMBOL"))
	{
		Ok(d) => d,
		Err(e) => {
			eprintln!("error: failed to load dead nodes: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 4: Evaluate violations (advisory) ────────────────────
	let (violations_eval, violations_warning): (Option<ModuleBoundaryEvaluation>, Option<String>) =
		match evaluate_discovered_module_violations(&storage, repo_uid, &snapshot.snapshot_uid) {
			Ok(r) => (Some(r.evaluation), None),
			Err(msg) => (
				None,
				Some(format!(
					"discovered-module violation rollups unavailable: {}",
					msg
				)),
			),
		};

	// ── Step 5: Derive module edges ───────────────────────────────
	use repo_graph_classification::module_edges::{
		derive_module_dependency_edges, FileOwnershipFact, ModuleEdgeDerivationInput, ModuleRef,
		ResolvedImportFact,
	};

	let module_refs: Vec<ModuleRef> = modules
		.iter()
		.map(|m| ModuleRef {
			module_uid: m.module_candidate_uid.clone(),
			canonical_path: m.canonical_root_path.clone(),
		})
		.collect();

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

	let edge_input = ModuleEdgeDerivationInput {
		imports: import_facts,
		ownership: ownership_facts,
		modules: module_refs.clone(),
	};

	let edge_result = match derive_module_dependency_edges(edge_input) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: failed to derive module edges: {}", e);
			return ExitCode::from(2);
		}
	};
	let edges = edge_result.edges;

	// ── Step 6: Compute rollup for this module ────────────────────
	use repo_graph_classification::module_rollup::{
		compute_module_rollups, DeadNodeFact, ModuleRollupInput, OwnedFileFact,
	};

	let violations_for_rollup = violations_eval
		.as_ref()
		.map(|e| e.violations.clone())
		.unwrap_or_default();

	let owned_file_facts: Vec<OwnedFileFact> = owned_files
		.into_iter()
		.map(|f| OwnedFileFact {
			file_path: f.file_path,
			module_uid: f.module_candidate_uid,
			is_test: f.is_test,
		})
		.collect();

	let dead_node_facts: Vec<DeadNodeFact> = dead_nodes
		.into_iter()
		.filter_map(|d| {
			d.file.map(|file_path| DeadNodeFact {
				file_path,
				is_test: d.is_test,
			})
		})
		.collect();

	let rollup_input = ModuleRollupInput {
		modules: module_refs,
		owned_files: owned_file_facts,
		edges: edges.clone(),
		violations: violations_for_rollup,
		dead_nodes: dead_node_facts,
	};

	let rollups = match compute_module_rollups(&rollup_input) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: failed to compute rollups: {}", e);
			return ExitCode::from(2);
		}
	};

	// Find this module's rollup
	let module_rollup = rollups
		.iter()
		.find(|r| r.module_uid == resolved_module.module_candidate_uid);

	let violations_available = violations_eval.is_some();

	let rollups_output = ModuleShowRollups {
		owned_file_count: module_rollup.map_or(0, |r| r.owned_file_count),
		owned_test_file_count: module_rollup.map_or(0, |r| r.owned_test_file_count),
		outbound_dependency_count: module_rollup.map_or(0, |r| r.outbound_dependency_count),
		outbound_import_count: module_rollup.map_or(0, |r| r.outbound_import_count),
		inbound_dependency_count: module_rollup.map_or(0, |r| r.inbound_dependency_count),
		inbound_import_count: module_rollup.map_or(0, |r| r.inbound_import_count),
		violation_count: if violations_available {
			Some(module_rollup.map_or(0, |r| r.violation_count))
		} else {
			None
		},
		dead_symbol_count: module_rollup.map_or(0, |r| r.dead_symbol_count),
		dead_test_symbol_count: module_rollup.map_or(0, |r| r.dead_test_symbol_count),
	};

	// ── Step 7: Compute weighted neighbors ────────────────────────
	use repo_graph_classification::weighted_neighbors::compute_weighted_neighbors;

	let weighted = compute_weighted_neighbors(&resolved_module.module_candidate_uid, &edges);

	// Enrich outbound neighbors with identity
	let outbound_dependencies: Vec<EnrichedNeighbor> = weighted
		.outbound
		.iter()
		.filter_map(|n| {
			// Find module by UID, then get identity from path lookup
			let module_path = edges
				.iter()
				.find(|e| e.target_module_uid == n.module_uid)
				.map(|e| e.target_canonical_path.as_str())?;
			let identity = module_identity_map.get(module_path)?;
			Some(EnrichedNeighbor {
				module_uid: identity.module_uid.clone(),
				module_key: identity.module_key.clone(),
				canonical_root_path: identity.canonical_root_path.clone(),
				module_kind: identity.module_kind.clone(),
				import_count: n.import_count,
				source_file_count: n.source_file_count,
			})
		})
		.collect();

	// Enrich inbound neighbors with identity
	let inbound_dependencies: Vec<EnrichedNeighbor> = weighted
		.inbound
		.iter()
		.filter_map(|n| {
			let module_path = edges
				.iter()
				.find(|e| e.source_module_uid == n.module_uid)
				.map(|e| e.source_canonical_path.as_str())?;
			let identity = module_identity_map.get(module_path)?;
			Some(EnrichedNeighbor {
				module_uid: identity.module_uid.clone(),
				module_key: identity.module_key.clone(),
				canonical_root_path: identity.canonical_root_path.clone(),
				module_kind: identity.module_kind.clone(),
				import_count: n.import_count,
				source_file_count: n.source_file_count,
			})
		})
		.collect();

	// ── Step 8: Filter and enrich violations ──────────────────────
	// Only source-side violations (where this module is the source)
	let violations_output: Option<Vec<EnrichedViolation>> = if violations_available {
		let source_violations: Vec<EnrichedViolation> = violations_eval
			.as_ref()
			.unwrap()
			.violations
			.iter()
			.filter(|v| v.source_canonical_path == resolved_module.canonical_root_path)
			.filter_map(|v| {
				let target_identity = module_identity_map.get(v.target_canonical_path.as_str())?;
				Some(EnrichedViolation {
					declaration_uid: v.declaration_uid.clone(),
					target: ViolationTargetIdentity {
						module_uid: target_identity.module_uid.clone(),
						module_key: target_identity.module_key.clone(),
						canonical_root_path: target_identity.canonical_root_path.clone(),
						module_kind: target_identity.module_kind.clone(),
					},
					import_count: v.import_count,
					source_file_count: v.source_file_count,
					reason: v.reason.clone(),
				})
			})
			.collect();
		Some(source_violations)
	} else {
		None // null when policy unavailable
	};

	// ── Step 9: Build output ──────────────────────────────────────
	let module_identity = ModuleIdentity {
		module_uid: resolved_module.module_candidate_uid.clone(),
		module_key: resolved_module.module_key.clone(),
		canonical_root_path: resolved_module.canonical_root_path.clone(),
		module_kind: resolved_module.module_kind.clone(),
		display_name: resolved_module.display_name.clone(),
		confidence: resolved_module.confidence,
	};

	let warnings: Vec<String> = violations_warning.into_iter().collect();

	let mut extra_fields = serde_json::Map::new();
	extra_fields.insert(
		"module".to_string(),
		serde_json::to_value(&module_identity).unwrap(),
	);
	extra_fields.insert(
		"rollups".to_string(),
		serde_json::to_value(&rollups_output).unwrap(),
	);
	extra_fields.insert(
		"outbound_dependencies".to_string(),
		serde_json::to_value(&outbound_dependencies).unwrap(),
	);
	extra_fields.insert(
		"inbound_dependencies".to_string(),
		serde_json::to_value(&inbound_dependencies).unwrap(),
	);
	extra_fields.insert(
		"violations".to_string(),
		serde_json::to_value(&violations_output).unwrap(),
	);
	extra_fields.insert(
		"rollups_degraded".to_string(),
		serde_json::Value::Bool(!violations_available),
	);
	extra_fields.insert(
		"warnings".to_string(),
		serde_json::to_value(&warnings).unwrap(),
	);

	// Trust overlay (Option A: only when repo has degradations).
	// Module dependencies are import-based, so graph_basis = "IMPORTS".
	if let Some(trust) = compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "IMPORTS") {
		if trust.has_degradation() || !trust.caveats.is_empty() {
			extra_fields.insert("trust".to_string(), serde_json::to_value(&trust).unwrap());
		}
	}

	// Build envelope (no results array for show — module is the main content)
	let output = match build_envelope(
		&storage,
		"modules show",
		repo_uid,
		&snapshot,
		serde_json::Value::Null, // No results array
		0,                       // count not applicable
		extra_fields,
	) {
		Ok(mut v) => {
			// Remove the results/count fields since show doesn't use them
			if let serde_json::Value::Object(ref mut map) = v {
				map.remove("results");
				map.remove("count");
			}
			v
		}
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

fn run_modules_files(args: &[String]) -> ExitCode {
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
			return ExitCode::from(2);
		}
	};

	// Load module context (with fallback)
	use crate::module_resolution::ModuleQueryContext;
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

	// Load module context (with fallback)
	use crate::module_resolution::ModuleQueryContext;
	let ctx = match ModuleQueryContext::load(&storage, &snapshot.snapshot_uid) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: failed to load module context: {}", e);
			return ExitCode::from(2);
		}
	};

	// Resolve module filter argument against discovered modules.
	// Resolution precedence: canonical_root_path exact → module_key exact → module_uid exact.
	// Unknown module → error (not empty results).
	let resolved_module_path: Option<String> = match module_filter {
		Some(filter) => match ctx.resolve_module(filter) {
			Some(m) => Some(m.canonical_root_path.clone()),
			None => {
				eprintln!("error: module not found: {}", filter);
				eprintln!("hint: use canonical path (e.g., 'packages/app') or module key");
				return ExitCode::from(1);
			}
		},
		None => None,
	};
	let modules = ctx.modules;

	// Load resolved imports
	let imports = match storage.get_resolved_imports_for_snapshot(&snapshot.snapshot_uid) {
		Ok(i) => i,
		Err(e) => {
			eprintln!("error: failed to load imports: {}", e);
			return ExitCode::from(2);
		}
	};

	// File ownership from context (fallback already applied)
	let ownership = ctx.ownership;

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
// Returns the evaluation result plus derivation diagnostics.

use repo_graph_classification::boundary_evaluator::ModuleBoundaryEvaluation;
use repo_graph_classification::module_edges::ModuleEdgeDiagnostics;

/// Result of discovered-module violations evaluation.
///
/// Bundles the boundary evaluation with edge derivation diagnostics so callers
/// can report graph degradation (e.g., missing module ownership) alongside
/// violation results.
struct DiscoveredModuleViolationsResult {
	evaluation: ModuleBoundaryEvaluation,
	diagnostics: ModuleEdgeDiagnostics,
}

fn evaluate_discovered_module_violations(
	storage: &repo_graph_storage::StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
) -> Result<DiscoveredModuleViolationsResult, String> {
	// 1. Load module context (with fallback)
	use crate::module_resolution::ModuleQueryContext;
	let ctx = ModuleQueryContext::load(storage, snapshot_uid)
		.map_err(|e| format!("failed to load module context: {}", e))?;
	let modules = ctx.modules;

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

	// 4. Load imports for edge derivation (separate query)
	// File ownership from context (fallback already applied)
	let imports = storage
		.get_resolved_imports_for_snapshot(snapshot_uid)
		.map_err(|e| format!("failed to load imports: {}", e))?;

	let ownership = ctx.ownership;

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

	Ok(DiscoveredModuleViolationsResult {
		evaluation,
		diagnostics: derivation_result.diagnostics,
	})
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
	let result =
		match evaluate_discovered_module_violations(&storage, repo_uid, &snapshot.snapshot_uid) {
			Ok(r) => r,
			Err(msg) => {
				eprintln!("error: {}", msg);
				return ExitCode::from(2);
			}
		};

	use repo_graph_classification::boundary_evaluator::StaleSide;

	// 8. Build JSON output — preserve evaluator order exactly
	let violations_json: Vec<serde_json::Value> = result
		.evaluation
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

	let stale_json: Vec<serde_json::Value> = result
		.evaluation
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

	let violation_count = result.evaluation.violations.len();
	let stale_count = result.evaluation.stale_declarations.len();

	// Build diagnostics JSON
	// Note: imports_source_no_file and imports_target_no_file are always 0 in Rust
	// because the storage query (get_resolved_imports_for_snapshot) pre-filters
	// edges where nodes lack file_uid. The TS implementation tracks these separately.
	let diagnostics_json = serde_json::json!({
		"imports_edges_total": result.diagnostics.imports_total,
		"imports_source_no_file": 0,
		"imports_target_no_file": 0,
		"imports_source_no_module": result.diagnostics.imports_source_unowned,
		"imports_target_no_module": result.diagnostics.imports_target_unowned,
		"imports_intra_module": result.diagnostics.imports_intra_module,
		"imports_cross_module": result.diagnostics.imports_cross_module,
	});

	let results = serde_json::json!({
		"violations": violations_json,
		"stale_declarations": stale_json,
	});

	// Build envelope with count, stale_count, and diagnostics
	let mut extra = serde_json::Map::new();
	extra.insert(
		"stale_count".to_string(),
		serde_json::Value::Number(stale_count.into()),
	);
	extra.insert("diagnostics".to_string(), diagnostics_json);

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

	// Load module context (with fallback)
	use crate::module_resolution::ModuleQueryContext;
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

/// Extract module path from a MODULE stable key: `{repo}:{path}:MODULE`
fn extract_module_path_from_key(stable_key: &str) -> Option<String> {
	if !stable_key.ends_with(":MODULE") {
		return None;
	}
	let without_suffix = &stable_key[..stable_key.len() - ":MODULE".len()];
	let colon_pos = without_suffix.find(':')?;
	Some(without_suffix[colon_pos + 1..].to_string())
}

// ── surfaces command ─────────────────────────────────────────────

fn run_surfaces(args: &[String]) -> ExitCode {
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


