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

use cli::print_usage;
use commands::{
    run_assess, run_callers, run_callees, run_check_cmd, run_churn, run_coverage, run_cycles,
    run_dead, run_declare, run_docs, run_explain_cmd, run_gate, run_hotspots, run_imports,
    run_index, run_metrics, run_modules, run_orient, run_path, run_policy, run_refresh,
    run_resource, run_risk, run_stats, run_surfaces, run_trust, run_violations,
};
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
		"docs" => run_docs(&args[2..]),
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
