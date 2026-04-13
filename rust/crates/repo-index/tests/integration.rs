//! End-to-end integration test: index a checked-in fixture repo
//! from disk into SQLite and verify deterministic outcomes.
//!
//! Test 1 uses `test/fixtures/typescript/classifier-repo/` for
//! exact graph counts + config signal verification.
//!
//! Test 2 uses `test/fixtures/typescript/rust-7a-fixture/` for
//! scanner exclusion proof (gitignore, node_modules, dist).
//!
//! Classifier-repo has:
//!   - package.json (dependencies: lodash, devDeps: typescript)
//!   - tsconfig.json (paths: @/* → ./src/*)
//!   - src/index.ts (imports, calls, exported function)

use std::path::PathBuf;

use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn fixture_path() -> PathBuf {
	let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest
		.join("..")
		.join("..")
		.join("..")
		.join("test")
		.join("fixtures")
		.join("typescript")
		.join("classifier-repo")
}

#[test]
fn index_classifier_repo_from_disk() {
	let repo_path = fixture_path();
	assert!(
		repo_path.join("package.json").exists(),
		"fixture not found at {:?}",
		repo_path
	);

	let mut storage = StorageConnection::open_in_memory().unwrap();
	let result = index_into_storage(
		&repo_path,
		&mut storage,
		"classifier-fixture",
		&ComposeOptions::default(),
	)
	.unwrap();

	// ── Snapshot ─────────────────────────────────────────────
	let snap = storage
		.get_snapshot(&result.snapshot_uid)
		.unwrap()
		.unwrap();
	assert_eq!(snap.status, "ready", "snapshot must be READY");

	// ── File count ───────────────────────────────────────────
	// Only src/index.ts is a source file (.ts).
	// package.json and tsconfig.json are NOT source extensions.
	assert_eq!(result.files_total, 1, "only src/index.ts is a source file");

	// ── Nodes ────────────────────────────────────────────────
	use repo_graph_indexer::storage_port::NodeStorePort;
	let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
	let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

	// FILE node for src/index.ts.
	assert!(
		stable_keys.contains(&"classifier-fixture:src/index.ts:FILE"),
		"missing FILE node, keys: {:?}",
		stable_keys
	);

	// FUNCTION node for standalone.
	assert!(
		stable_keys.iter().any(|k| k.contains("#standalone:SYMBOL:FUNCTION")),
		"missing standalone FUNCTION node, keys: {:?}",
		stable_keys
	);

	// MODULE node for src.
	assert!(
		stable_keys.iter().any(|k| k.contains("src:MODULE")),
		"missing MODULE node for src, keys: {:?}",
		stable_keys
	);

	// ── Exact edge counts ────────────────────────────────────
	// edges_total = 1: OWNS(src module → index.ts file).
	// The IMPORTS edge for ./local-nonexistent stays unresolved
	// (target file doesn't exist) so it's in edges_unresolved.
	assert_eq!(result.edges_total, 1, "edges_total");

	// edges_unresolved = 5:
	//   - debounce() → calls_function_ambiguous_or_missing
	//   - aliased() → calls_function_ambiguous_or_missing
	//   - relatively() → calls_function_ambiguous_or_missing
	//   - mysteryFunction() → calls_function_ambiguous_or_missing
	//   - import "./local-nonexistent" → imports_file_not_found
	assert_eq!(result.edges_unresolved, 5, "edges_unresolved");

	// ── Exact unresolved breakdown ───────────────────────────
	assert_eq!(
		result.unresolved_breakdown.get("calls_function_ambiguous_or_missing"),
		Some(&4),
		"breakdown: {:?}",
		result.unresolved_breakdown
	);
	assert_eq!(
		result.unresolved_breakdown.get("imports_file_not_found"),
		Some(&1),
		"breakdown: {:?}",
		result.unresolved_breakdown
	);

	// ── Config signals ───────────────────────────────────────
	// Verify package.json deps were resolved. Query file signals.
	use repo_graph_indexer::storage_port::FileSignalPort;
	let signals = FileSignalPort::query_file_signals_batch(
		&storage,
		&result.snapshot_uid,
		&["classifier-fixture:src/index.ts".into()],
	)
	.unwrap();

	// Should have at least import bindings.
	assert!(
		!signals.is_empty(),
		"expected file signals for src/index.ts"
	);
	let sig = &signals[0];
	assert!(
		sig.import_bindings_json.is_some(),
		"expected import_bindings_json"
	);

	// Package deps should include lodash and typescript.
	if let Some(ref deps_json) = sig.package_dependencies_json {
		let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
		let names = deps["names"].as_array().unwrap();
		let name_strs: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();
		assert!(
			name_strs.contains(&"lodash"),
			"expected lodash in package deps, got: {:?}",
			name_strs
		);
		assert!(
			name_strs.contains(&"typescript"),
			"expected typescript in package deps, got: {:?}",
			name_strs
		);
	} else {
		panic!("expected package_dependencies_json on file signal");
	}

	// Tsconfig aliases should include @/*.
	if let Some(ref aliases_json) = sig.tsconfig_aliases_json {
		let aliases: serde_json::Value = serde_json::from_str(aliases_json).unwrap();
		let entries = aliases["entries"].as_array().unwrap();
		let patterns: Vec<&str> = entries
			.iter()
			.filter_map(|e| e["pattern"].as_str())
			.collect();
		assert!(
			patterns.contains(&"@/*"),
			"expected @/* in tsconfig aliases, got: {:?}",
			patterns
		);
	} else {
		panic!("expected tsconfig_aliases_json on file signal");
	}
}

// ── Exclusion proof ──────────────────────────────────────────────

fn exclusion_fixture_path() -> PathBuf {
	let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest
		.join("..")
		.join("..")
		.join("..")
		.join("test")
		.join("fixtures")
		.join("typescript")
		.join("rust-7a-fixture")
}

#[test]
fn index_excludes_gitignored_and_always_excluded() {
	let repo_path = exclusion_fixture_path();
	assert!(
		repo_path.join("package.json").exists(),
		"rust-7a-fixture not found at {:?}",
		repo_path
	);

	let mut storage = StorageConnection::open_in_memory().unwrap();
	let result = index_into_storage(
		&repo_path,
		&mut storage,
		"r7a",
		&ComposeOptions::default(),
	)
	.unwrap();

	assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

	// Exact file count: src/index.ts + src/server.ts = 2.
	// Excluded:
	//   - src/generated.ts (gitignored)
	//   - node_modules/pkg/index.ts (always-excluded dir)
	//   - dist/bundle.js (always-excluded dir)
	assert_eq!(result.files_total, 2, "files_total");

	// Verify excluded files are absent from nodes.
	use repo_graph_indexer::storage_port::NodeStorePort;
	let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
	let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

	assert!(
		!stable_keys.iter().any(|k| k.contains("generated")),
		"gitignored file should not appear: {:?}", stable_keys
	);
	assert!(
		!stable_keys.iter().any(|k| k.contains("node_modules")),
		"node_modules should not appear: {:?}", stable_keys
	);
	assert!(
		!stable_keys.iter().any(|k| k.contains("dist")),
		"dist should not appear: {:?}", stable_keys
	);
	assert!(
		!stable_keys.iter().any(|k| k.contains("bundle")),
		"dist/bundle.js should not appear: {:?}", stable_keys
	);

	// Included files present.
	assert!(stable_keys.contains(&"r7a:src/index.ts:FILE"));
	assert!(stable_keys.contains(&"r7a:src/server.ts:FILE"));
}

fn snap_status(storage: &StorageConnection, uid: &str) -> String {
	storage.get_snapshot(uid).unwrap().unwrap().status
}
