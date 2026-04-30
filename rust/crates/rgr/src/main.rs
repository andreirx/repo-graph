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

use cli::{open_storage, print_usage, utc_now_iso8601};
use commands::{
    run_assess, run_callers, run_callees, run_check_cmd, run_churn, run_coverage, run_cycles,
    run_explain_cmd, run_gate, run_hotspots, run_imports, run_index, run_metrics, run_modules,
    run_orient, run_path, run_policy, run_refresh, run_resource, run_risk, run_stats, run_surfaces,
    run_trust, run_violations,
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


/// Extract module path from a MODULE stable key: `{repo}:{path}:MODULE`
fn extract_module_path_from_key(stable_key: &str) -> Option<String> {
	if !stable_key.ends_with(":MODULE") {
		return None;
	}
	let without_suffix = &stable_key[..stable_key.len() - ":MODULE".len()];
	let colon_pos = without_suffix.find(':')?;
	Some(without_suffix[colon_pos + 1..].to_string())
}

