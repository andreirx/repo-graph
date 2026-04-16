//! Deterministic tests for the `declare boundary` command (Rust-33).
//!
//! Test matrix:
//!   1. Missing subcommand => usage error, exit 1
//!   2. Missing --forbids => usage error, exit 1
//!   3. Repeated --forbids => usage error, exit 1
//!   4. Missing DB => storage error, exit 2
//!   5. Insert boundary success => JSON output, exit 0
//!   6. Idempotent repeated insert => inserted=false, exit 0
//!   7. Reason does not affect identity => same UID regardless of reason
//!   8. Inserted boundary visible to violations command
//!   9. Exact JSON output shape
//!  10. Flag token as --forbids value => usage error
//!  11. Flag token as --reason value => usage error
//!  12. Empty --forbids value => usage error

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a fixture for declare boundary testing.
/// Same structure as gate tests: src/core, src/adapters, src/util.
fn build_declare_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src/core")).unwrap();
	std::fs::create_dir_all(root.join("src/util")).unwrap();
	std::fs::create_dir_all(root.join("src/adapters")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/core/service.ts"),
		"import { helper } from \"../util/helper\";\nexport function serve() { helper(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/util/helper.ts"),
		"export function helper() {}\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/adapters/store.ts"),
		"import { serve } from \"../core/service\";\nexport function store() { serve(); }\n",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();
	assert_eq!(result.files_total, 3);

	(repo_dir, db_dir, db_path)
}

fn run_cmd(args: &[&str]) -> std::process::Output {
	Command::new(binary_path())
		.args(args)
		.output()
		.unwrap()
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
	let stdout = String::from_utf8_lossy(&output.stdout);
	serde_json::from_str(&stdout).unwrap_or_else(|e| {
		panic!("invalid JSON: {}\nstdout: {}", e, stdout)
	})
}

// -- 1. Missing subcommand => usage error ----------------------------

#[test]
fn declare_missing_subcommand() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["declare"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());

	// Also test unknown subcommand.
	let output2 = run_cmd(&["declare", "unknown", db_str, "r1", "src/core", "--forbids", "src/adapters"]);
	assert_eq!(output2.status.code(), Some(1));
}

// -- 2. Missing --forbids => usage error -----------------------------

#[test]
fn declare_boundary_missing_forbids() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["declare", "boundary", db_str, "r1", "src/core"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--forbids"), "stderr: {}", stderr);
}

// -- 3. Repeated --forbids => usage error ----------------------------

#[test]
fn declare_boundary_repeated_forbids() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters", "--forbids", "src/util",
	]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("more than once"), "stderr: {}", stderr);
}

// -- 4. Missing DB => storage error ----------------------------------

#[test]
fn declare_boundary_missing_db() {
	let output = run_cmd(&[
		"declare", "boundary", "/nonexistent/path.db", "r1", "src/core",
		"--forbids", "src/adapters",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// -- 5. Insert boundary success --------------------------------------

#[test]
fn declare_boundary_success() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters",
	]);
	assert_eq!(output.status.code(), Some(0), "success => exit 0");

	let result = parse_json(&output);
	assert!(result["declaration_uid"].is_string());
	assert_eq!(result["kind"], "boundary");
	assert_eq!(result["target"], "src/core");
	assert_eq!(result["forbids"], "src/adapters");
	assert_eq!(result["inserted"], true);
}

// -- 6. Idempotent repeated insert -----------------------------------

#[test]
fn declare_boundary_idempotent() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let first = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters",
	]);
	assert_eq!(first.status.code(), Some(0));
	let first_result = parse_json(&first);
	assert_eq!(first_result["inserted"], true);

	let second = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters",
	]);
	assert_eq!(second.status.code(), Some(0));
	let second_result = parse_json(&second);
	assert_eq!(second_result["inserted"], false);
	assert_eq!(
		first_result["declaration_uid"],
		second_result["declaration_uid"],
	);
}

// -- 7. Reason does not affect identity ------------------------------

#[test]
fn declare_boundary_reason_does_not_affect_identity() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let first = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters",
	]);
	assert_eq!(first.status.code(), Some(0));
	let first_result = parse_json(&first);
	assert_eq!(first_result["inserted"], true);

	// Same boundary with a different reason — should be idempotent.
	let second = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters",
		"--reason", "clean architecture enforcement",
	]);
	assert_eq!(second.status.code(), Some(0));
	let second_result = parse_json(&second);
	assert_eq!(second_result["inserted"], false, "reason change must not create new declaration");
	assert_eq!(
		first_result["declaration_uid"],
		second_result["declaration_uid"],
	);
}

// -- 8. Inserted boundary visible to violations ----------------------

#[test]
fn declare_boundary_visible_to_violations() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	// Declare: adapters --forbids--> core.
	// store.ts imports from core/service.ts → should produce a violation.
	let declare_out = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/adapters",
		"--forbids", "src/core",
	]);
	assert_eq!(declare_out.status.code(), Some(0));

	let violations_out = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(violations_out.status.code(), Some(0));

	let violations = parse_json(&violations_out);
	let count = violations["count"].as_i64().unwrap();
	assert!(count > 0, "boundary should produce at least one violation");

	let results = violations["results"].as_array().unwrap();
	assert!(
		results.iter().any(|v| {
			v["source_file"].as_str().unwrap().contains("adapters/store")
				&& v["target_file"].as_str().unwrap().contains("core/service")
		}),
		"should find adapters/store.ts -> core/service.ts violation"
	);
}

// -- 9. Exact JSON output shape --------------------------------------

#[test]
fn declare_boundary_json_shape() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters",
		"--reason", "dependency rule",
	]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);

	// Exactly these keys, no more.
	let obj = result.as_object().unwrap();
	let keys: Vec<&String> = obj.keys().collect();
	assert!(keys.contains(&&"declaration_uid".to_string()));
	assert!(keys.contains(&&"kind".to_string()));
	assert!(keys.contains(&&"target".to_string()));
	assert!(keys.contains(&&"forbids".to_string()));
	assert!(keys.contains(&&"inserted".to_string()));
	assert_eq!(keys.len(), 5, "exactly 5 keys in output, got: {:?}", keys);

	assert_eq!(result["kind"], "boundary");
	assert_eq!(result["target"], "src/core");
	assert_eq!(result["forbids"], "src/adapters");
	assert_eq!(result["inserted"], true);
}

// -- 10. Flag token as --forbids value => usage error ----------------

#[test]
fn declare_boundary_flag_as_forbids_value() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	// --forbids followed by --reason (a flag, not a value).
	let output = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "--reason", "some text",
	]);
	assert_eq!(
		output.status.code(),
		Some(1),
		"flag token as --forbids value must be usage error, stderr: {}",
		String::from_utf8_lossy(&output.stderr),
	);
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--forbids requires a"), "stderr: {}", stderr);
}

// -- 11. Flag token as --reason value => usage error -----------------

#[test]
fn declare_boundary_flag_as_reason_value() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	// --reason followed by --forbids (a flag, not a value).
	let output = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--reason", "--forbids", "src/adapters",
	]);
	assert_eq!(
		output.status.code(),
		Some(1),
		"flag token as --reason value must be usage error, stderr: {}",
		String::from_utf8_lossy(&output.stderr),
	);
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--reason requires a"), "stderr: {}", stderr);
}

// -- 12. Empty --forbids value => usage error ------------------------

#[test]
fn declare_boundary_empty_forbids_value() {
	let (_r, _d, db) = build_declare_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "", "--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(1), "empty --forbids => usage error");
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("non-empty"), "stderr: {}", stderr);
}
