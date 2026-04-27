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

// ── Mixed-language isolation ─────────────────────────────────────

fn mixed_lang_fixture_path() -> PathBuf {
	let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest
		.join("..")
		.join("..")
		.join("..")
		.join("test")
		.join("fixtures")
		.join("mixed-lang")
}

#[test]
fn mixed_lang_language_isolation() {
	// Language isolation regression test: in a repo containing Rust, TypeScript,
	// and Java files alongside both Cargo.toml and package.json, each file must
	// receive only the dependency signals that match its own language.
	//
	// Failure mode (the bug this guards against): the compose layer's else-branch
	// was returning package.json deps for all non-Rust files, meaning Java files
	// would inherit Node dependency signals from a nearby package.json.
	//
	// Fixture layout:
	//   Cargo.toml    → deps: serde, wgpu
	//   package.json  → deps: express
	//   src/engine.rs → must receive serde + wgpu, NOT express
	//   src/server.ts → must receive express, NOT serde/wgpu
	//   src/App.java  → must receive empty signals (no Java manifest reader yet)
	//   build.gradle  → not a recognized source extension, not indexed
	let repo_path = mixed_lang_fixture_path();
	assert!(
		repo_path.join("Cargo.toml").exists(),
		"mixed-lang fixture not found at {:?}",
		repo_path
	);
	assert!(
		repo_path.join("package.json").exists(),
		"mixed-lang fixture missing package.json at {:?}",
		repo_path
	);

	let mut storage = StorageConnection::open_in_memory().unwrap();
	let result = index_into_storage(
		&repo_path,
		&mut storage,
		"mixed-lang",
		&ComposeOptions::default(),
	)
	.unwrap();

	assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

	// 3 source files: engine.rs (Rust), server.ts (TS), App.java (Java).
	// build.gradle, Cargo.toml, package.json are not source extensions.
	assert_eq!(result.files_total, 3, "expected engine.rs + server.ts + App.java");

	use repo_graph_indexer::storage_port::FileSignalPort;

	// ── server.ts: package.json deps only ────────────────────────
	let ts_signals = FileSignalPort::query_file_signals_batch(
		&storage,
		&result.snapshot_uid,
		&["mixed-lang:src/server.ts".into()],
	)
	.unwrap();

	assert!(!ts_signals.is_empty(), "expected file signals for src/server.ts");
	let ts_sig = &ts_signals[0];

	if let Some(ref deps_json) = ts_sig.package_dependencies_json {
		let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
		let names = deps["names"].as_array().unwrap();
		let dep_names: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();

		assert!(
			dep_names.contains(&"express"),
			"server.ts must receive express from package.json, got: {:?}",
			dep_names
		);
		// Cargo deps must NOT bleed into TS files.
		assert!(
			!dep_names.contains(&"serde"),
			"server.ts must NOT receive Cargo.toml deps (serde), got: {:?}",
			dep_names
		);
		assert!(
			!dep_names.contains(&"wgpu"),
			"server.ts must NOT receive Cargo.toml deps (wgpu), got: {:?}",
			dep_names
		);
	} else {
		panic!("expected package_dependencies_json for server.ts");
	}

	// ── engine.rs: Cargo.toml deps only ──────────────────────────
	let rs_signals = FileSignalPort::query_file_signals_batch(
		&storage,
		&result.snapshot_uid,
		&["mixed-lang:src/engine.rs".into()],
	)
	.unwrap();

	assert!(!rs_signals.is_empty(), "expected file signals for src/engine.rs");
	let rs_sig = &rs_signals[0];

	if let Some(ref deps_json) = rs_sig.package_dependencies_json {
		let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
		let names = deps["names"].as_array().unwrap();
		let dep_names: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();

		assert!(
			dep_names.contains(&"serde"),
			"engine.rs must receive serde from Cargo.toml, got: {:?}",
			dep_names
		);
		assert!(
			dep_names.contains(&"wgpu"),
			"engine.rs must receive wgpu from Cargo.toml, got: {:?}",
			dep_names
		);
		// package.json deps must NOT bleed into Rust files.
		assert!(
			!dep_names.contains(&"express"),
			"engine.rs must NOT receive package.json deps (express), got: {:?}",
			dep_names
		);
	} else {
		panic!("expected package_dependencies_json for engine.rs (from Cargo.toml)");
	}

	// ── App.java: empty signals (no Java manifest reader) ────────
	// Java files fall into the wildcard arm of the language dispatch
	// match, which returns empty PackageDependencySet and empty
	// TsconfigAliases. Since the empty set is converted to None in
	// FileInput, package_dependencies_json on the file signal must be
	// None — no contamination from either package.json or Cargo.toml.
	let java_signals = FileSignalPort::query_file_signals_batch(
		&storage,
		&result.snapshot_uid,
		&["mixed-lang:src/App.java".into()],
	)
	.unwrap();

	if !java_signals.is_empty() {
		let java_sig = &java_signals[0];
		assert!(
			java_sig.package_dependencies_json.is_none(),
			"App.java must NOT receive any dependency signals; \
			 Java has no manifest reader yet. Got: {:?}",
			java_sig.package_dependencies_json
		);
	}
	// If java_signals is empty: no signal row exists for App.java,
	// which is also correct — no signals at all is still isolation.
}

// ── Rust extraction ──────────────────────────────────────────────

fn rust_fixture_path() -> PathBuf {
	let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest
		.join("..")
		.join("..")
		.join("..")
		.join("test")
		.join("fixtures")
		.join("rust")
		.join("simple-crate")
}

#[test]
fn index_rust_crate_extracts_symbols() {
	let repo_path = rust_fixture_path();
	assert!(
		repo_path.join("Cargo.toml").exists(),
		"simple-crate fixture not found at {:?}",
		repo_path
	);

	let mut storage = StorageConnection::open_in_memory().unwrap();
	let result = index_into_storage(
		&repo_path,
		&mut storage,
		"rust-simple",
		&ComposeOptions::default(),
	)
	.unwrap();

	assert_eq!(snap_status(&storage, &result.snapshot_uid), "ready");

	// ── File count ───────────────────────────────────────────
	// src/lib.rs and src/utils.rs are Rust source files.
	// Cargo.toml is NOT a source extension.
	assert_eq!(result.files_total, 2, "expected lib.rs + utils.rs");

	// ── Nodes ────────────────────────────────────────────────
	use repo_graph_indexer::storage_port::NodeStorePort;
	let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
	let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

	// FILE nodes for both Rust files.
	assert!(
		stable_keys.contains(&"rust-simple:src/lib.rs:FILE"),
		"missing lib.rs FILE node, keys: {:?}",
		stable_keys
	);
	assert!(
		stable_keys.contains(&"rust-simple:src/utils.rs:FILE"),
		"missing utils.rs FILE node, keys: {:?}",
		stable_keys
	);

	// SYMBOL nodes extracted by Rust extractor.
	// Struct: Config
	assert!(
		stable_keys.iter().any(|k| k.contains("#Config:SYMBOL:CLASS")),
		"missing Config struct node, keys: {:?}",
		stable_keys
	);

	// Enum: Status
	assert!(
		stable_keys.iter().any(|k| k.contains("#Status:SYMBOL:ENUM")),
		"missing Status enum node, keys: {:?}",
		stable_keys
	);

	// Trait: Processor
	assert!(
		stable_keys.iter().any(|k| k.contains("#Processor:SYMBOL:INTERFACE")),
		"missing Processor trait node, keys: {:?}",
		stable_keys
	);

	// Impl method: Config.new
	assert!(
		stable_keys.iter().any(|k| k.contains("#Config.new:SYMBOL:METHOD")),
		"missing Config.new method node, keys: {:?}",
		stable_keys
	);

	// Impl method: Config.get_value
	assert!(
		stable_keys.iter().any(|k| k.contains("#Config.get_value:SYMBOL:METHOD")),
		"missing Config.get_value method node, keys: {:?}",
		stable_keys
	);

	// Trait impl method: Config.process (impl Processor for Config)
	assert!(
		stable_keys.iter().any(|k| k.contains("#Config.process:SYMBOL:METHOD")),
		"missing Config.process trait impl method, keys: {:?}",
		stable_keys
	);

	// Free function: create_config
	assert!(
		stable_keys.iter().any(|k| k.contains("#create_config:SYMBOL:FUNCTION")),
		"missing create_config function node, keys: {:?}",
		stable_keys
	);

	// Const: MAX_SIZE
	assert!(
		stable_keys.iter().any(|k| k.contains("#MAX_SIZE:SYMBOL:CONSTANT")),
		"missing MAX_SIZE const node, keys: {:?}",
		stable_keys
	);

	// Utils module function: describe_config
	assert!(
		stable_keys.iter().any(|k| k.contains("utils.rs") && k.contains("#describe_config:SYMBOL:FUNCTION")),
		"missing describe_config from utils.rs, keys: {:?}",
		stable_keys
	);

	// ── Edges ────────────────────────────────────────────────
	// Verify edges exist via result counts.
	// edges_total includes resolved OWNS edges (module->file).
	// The Rust fixture should produce at least a few resolved edges.
	assert!(
		result.edges_total > 0,
		"expected at least one resolved edge (e.g., OWNS from module to file)"
	);

	// Verify import bindings were extracted via file signals.
	use repo_graph_indexer::storage_port::FileSignalPort;
	let signals = FileSignalPort::query_file_signals_batch(
		&storage,
		&result.snapshot_uid,
		&["rust-simple:src/lib.rs".into()],
	)
	.unwrap();

	assert!(
		!signals.is_empty(),
		"expected file signals for src/lib.rs"
	);

	let sig = &signals[0];
	assert!(
		sig.import_bindings_json.is_some(),
		"expected import_bindings_json for Rust file"
	);

	// Verify import binding contains HashMap.
	if let Some(ref bindings_json) = sig.import_bindings_json {
		let bindings: serde_json::Value = serde_json::from_str(bindings_json).unwrap();
		let arr = bindings.as_array().unwrap();
		let identifiers: Vec<&str> = arr
			.iter()
			.filter_map(|b| b["identifier"].as_str())
			.collect();
		assert!(
			identifiers.contains(&"HashMap"),
			"expected HashMap in import bindings, got: {:?}",
			identifiers
		);
	}
}

#[test]
fn index_rust_crate_visibility_correct() {
	let repo_path = rust_fixture_path();
	let mut storage = StorageConnection::open_in_memory().unwrap();
	let result = index_into_storage(
		&repo_path,
		&mut storage,
		"rust-vis",
		&ComposeOptions::default(),
	)
	.unwrap();

	use repo_graph_indexer::storage_port::NodeStorePort;
	use repo_graph_indexer::types::Visibility;
	let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();

	// Config struct is pub -> Export
	let config_node = nodes
		.iter()
		.find(|n| n.stable_key.contains("#Config:SYMBOL:CLASS"))
		.expect("Config node not found");
	assert_eq!(
		config_node.visibility,
		Some(Visibility::Export),
		"Config should be exported (pub)"
	);

	// helper function is private -> Private
	let helper_node = nodes
		.iter()
		.find(|n| n.stable_key.contains("#helper:SYMBOL:FUNCTION"))
		.expect("helper node not found");
	assert_eq!(
		helper_node.visibility,
		Some(Visibility::Private),
		"helper should be private (no pub)"
	);

	// create_config is pub -> Export
	let create_node = nodes
		.iter()
		.find(|n| n.stable_key.contains("#create_config:SYMBOL:FUNCTION"))
		.expect("create_config node not found");
	assert_eq!(
		create_node.visibility,
		Some(Visibility::Export),
		"create_config should be exported (pub)"
	);
}

#[test]
fn index_rust_crate_receives_cargo_deps() {
	// P1 regression test: Rust files must receive Cargo.toml dependencies,
	// NOT package.json dependencies. This test verifies the language-aware
	// dependency wiring in prepare_repo_inputs().
	let repo_path = rust_fixture_path();
	let mut storage = StorageConnection::open_in_memory().unwrap();
	let result = index_into_storage(
		&repo_path,
		&mut storage,
		"rust-cargo-deps",
		&ComposeOptions::default(),
	)
	.unwrap();

	// Query file signals for lib.rs
	use repo_graph_indexer::storage_port::FileSignalPort;
	let signals = FileSignalPort::query_file_signals_batch(
		&storage,
		&result.snapshot_uid,
		&["rust-cargo-deps:src/lib.rs".into()],
	)
	.unwrap();

	assert!(!signals.is_empty(), "expected file signals for lib.rs");
	let sig = &signals[0];

	// Verify package_dependencies contains Cargo.toml deps, NOT package.json deps.
	// The fixture Cargo.toml has: serde, tokio, tempfile, but NO express/react.
	if let Some(ref deps_json) = sig.package_dependencies_json {
		let deps: serde_json::Value = serde_json::from_str(deps_json).unwrap();
		let names = deps["names"].as_array().unwrap();
		let dep_names: Vec<&str> = names.iter().filter_map(|v| v.as_str()).collect();

		// Cargo deps should be present (normalized: hyphens → underscores not needed here).
		assert!(
			dep_names.contains(&"serde"),
			"expected serde from Cargo.toml, got: {:?}",
			dep_names
		);
		assert!(
			dep_names.contains(&"tokio"),
			"expected tokio from Cargo.toml, got: {:?}",
			dep_names
		);

		// Package.json deps should NOT be present.
		assert!(
			!dep_names.contains(&"express"),
			"Rust file should NOT have package.json deps, got: {:?}",
			dep_names
		);
		assert!(
			!dep_names.contains(&"react"),
			"Rust file should NOT have package.json deps, got: {:?}",
			dep_names
		);
	} else {
		panic!("expected package_dependencies_json for Rust file (from Cargo.toml)");
	}
}
