//! Gate command family.
//!
//! Requirement/obligation gate evaluation for CI integration.
//!
//! # Boundary rules
//!
//! This module owns gate command-family behavior:
//! - `run_gate` handler
//! - gate-local argument parsing (--strict, --advisory)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - gate domain logic (belongs in `repo-graph-gate` crate)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{format_gate_error, open_storage, utc_now_iso8601};

// ── gate command ─────────────────────────────────────────────────

pub fn run_gate(args: &[String]) -> ExitCode {
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
