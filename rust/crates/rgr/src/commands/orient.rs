//! Orient command family.
//!
//! Agent-facing discovery surfaces: orient, check, explain.
//!
//! # Boundary rules
//!
//! This module owns orient/check/explain command-family behavior:
//! - `run_orient`, `run_check_cmd`, `run_explain_cmd` handlers
//! - family-local usage helpers
//! - family-local budget/focus parsing
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - cross-family helpers (belong in `crate::cli`)
//! - domain/application logic (belongs in support crates)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{compute_trust_overlay_for_snapshot, open_storage, utc_now_iso8601};

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

pub fn run_orient(args: &[String]) -> ExitCode {
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

	// ── Trust overlay (Option A: only when degraded) ─────────
	// Orient uses CALLS+IMPORTS for comprehensive graph analysis.
	// Use the exact snapshot from the result to ensure consistency
	// (avoid race with concurrent indexing).
	let snapshot = match storage.get_snapshot(&result.snapshot) {
		Ok(Some(snap)) => snap,
		Ok(None) | Err(_) => {
			// Snapshot unavailable — emit result without trust overlay.
			match serde_json::to_string_pretty(&result) {
				Ok(json) => {
					println!("{}", json);
					return ExitCode::from(0);
				}
				Err(e) => {
					eprintln!("error: {}", e);
					return ExitCode::from(2);
				}
			}
		}
	};

	// Convert result to mutable JSON Value to add trust field.
	let mut output = match serde_json::to_value(&result) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Add trust section if degraded (briefing surface pattern).
	if let Some(trust) = compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "CALLS+IMPORTS") {
		if trust.has_degradation() || !trust.caveats.is_empty() {
			if let serde_json::Value::Object(ref mut map) = output {
				map.insert("trust".to_string(), serde_json::to_value(&trust).unwrap());
			}
		}
	}

	// ── Serialize and emit ───────────────────────────────────
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

fn print_orient_usage() {
	eprintln!(
		"usage: rmap orient <db_path> <repo_uid> \
		 [--budget small|medium|large] [--focus <string>]"
	);
}

// ── check command ────────────────────────────────────────────────

pub fn run_check_cmd(args: &[String]) -> ExitCode {
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

pub fn run_explain_cmd(args: &[String]) -> ExitCode {
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

	// ── Trust overlay (Option A: only when degraded) ─────────
	// Explain uses CALLS+IMPORTS for comprehensive graph analysis.
	// Use the exact snapshot from the result to ensure consistency
	// (avoid race with concurrent indexing).
	let snapshot = match storage.get_snapshot(&result.snapshot) {
		Ok(Some(snap)) => snap,
		Ok(None) | Err(_) => {
			// Snapshot unavailable — emit result without trust overlay.
			match serde_json::to_string_pretty(&result) {
				Ok(json) => {
					println!("{}", json);
					return ExitCode::from(0);
				}
				Err(e) => {
					eprintln!("error: {}", e);
					return ExitCode::from(2);
				}
			}
		}
	};

	// Convert result to mutable JSON Value to add trust field.
	let mut output = match serde_json::to_value(&result) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// Add trust section if degraded (briefing surface pattern).
	if let Some(trust) = compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "CALLS+IMPORTS") {
		if trust.has_degradation() || !trust.caveats.is_empty() {
			if let serde_json::Value::Object(ref mut map) = output {
				map.insert("trust".to_string(), serde_json::to_value(&trust).unwrap());
			}
		}
	}

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

fn print_explain_usage() {
	eprintln!(
		"usage: rmap explain <db_path> <repo_uid> <target> \
		 [--budget medium|large]"
	);
}
