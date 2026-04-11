//! Rust half of the storage parity harness.
//!
//! Walks the shared `storage-parity-fixtures/` corpus at the repo
//! root, runs each fixture's `operations.json` through the Rust
//! `StorageConnection` API, and compares the captured results plus
//! post-state dump against the fixture's `expected.json`.
//!
//! This is the Rust side of the R2-F cross-runtime parity check.
//! The TypeScript half (`test/storage-parity/storage-parity.test.ts`)
//! runs the same fixtures through the TS `SqliteStorage` adapter
//! and compares against the same `expected.json`. If one side
//! passes and the other fails, the storage contract has drifted.
//!
//! Contract notes: see `storage-parity-fixtures/README.md` for
//! the fixture format, supported operations, symbolic binding
//! syntax (`@binding.field`), normalization rules, and canonical
//! ordering requirements. This file implements exactly what the
//! README specifies.
//!
//! ── Single integration test, many fixtures ────────────────────
//!
//! One `#[test]` function iterates the fixture corpus, collects
//! failures, and panics at the end with the full list. Per-fixture
//! `#[test]` generation would require a proc macro or a third-
//! party crate; neither is in the Rust-2 dep budget. The
//! collected-failures pattern gives adequate diagnostics for
//! acceptance.

use std::fs;
use std::path::{Path, PathBuf};

use repo_graph_storage::types::{
	CreateSnapshotInput, FileVersion, GraphEdge, GraphNode, Repo, RepoRef,
	TrackedFile, UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;
use serde_json::{json, Map, Value};

// ── Fixture loading ──────────────────────────────────────────────

fn fixtures_root() -> PathBuf {
	let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
	// rust/crates/storage/ → ../../.. → repo root → storage-parity-fixtures/
	manifest_dir
		.join("..")
		.join("..")
		.join("..")
		.join("storage-parity-fixtures")
}

struct Fixture {
	name: String,
	operations_doc: Value,
	expected: Value,
}

fn discover_fixtures() -> Vec<Fixture> {
	let root = fixtures_root();
	let mut fixtures = Vec::new();
	let entries = fs::read_dir(&root)
		.unwrap_or_else(|e| panic!("failed to read fixtures root {}: {e}", root.display()));
	for entry in entries {
		let entry = entry.expect("read_dir entry");
		let path = entry.path();
		if !path.is_dir() {
			continue;
		}
		let name = path
			.file_name()
			.and_then(|n| n.to_str())
			.expect("fixture dir name utf-8")
			.to_string();
		let ops_path = path.join("operations.json");
		let exp_path = path.join("expected.json");
		if !ops_path.exists() || !exp_path.exists() {
			continue;
		}
		let ops_raw = fs::read_to_string(&ops_path)
			.unwrap_or_else(|e| panic!("read {}: {e}", ops_path.display()));
		let exp_raw = fs::read_to_string(&exp_path)
			.unwrap_or_else(|e| panic!("read {}: {e}", exp_path.display()));
		let operations_doc: Value = serde_json::from_str(&ops_raw)
			.unwrap_or_else(|e| panic!("parse operations.json for {name}: {e}"));
		let expected: Value = serde_json::from_str(&exp_raw)
			.unwrap_or_else(|e| panic!("parse expected.json for {name}: {e}"));
		fixtures.push(Fixture {
			name,
			operations_doc,
			expected,
		});
	}
	fixtures.sort_by(|a, b| a.name.cmp(&b.name));
	fixtures
}

// ── Harness state ────────────────────────────────────────────────

#[derive(Default)]
struct HarnessState {
	/// Binding-name → captured return value (JSON).
	bindings: Map<String, Value>,
	/// Dynamic-value substitutions to apply during normalization.
	/// Each entry is (actual_value, placeholder). Applied globally
	/// to every string in the dump and the results map.
	substitutions: Vec<(String, String)>,
}

// ── Reference resolution ─────────────────────────────────────────

/// Walk a JSON value and replace any string of the form
/// `@<binding>.<field>` with the corresponding field from the
/// bindings map. Leaves other values untouched.
fn resolve_refs(value: &Value, bindings: &Map<String, Value>) -> Result<Value, String> {
	match value {
		Value::String(s) => {
			if let Some(rest) = s.strip_prefix('@') {
				let (binding_name, field) = rest
					.split_once('.')
					.ok_or_else(|| format!("bad ref syntax: {}", s))?;
				let binding = bindings
					.get(binding_name)
					.ok_or_else(|| format!("unknown binding: {}", binding_name))?;
				binding.get(field).cloned().ok_or_else(|| {
					format!("binding '{}' has no field '{}'", binding_name, field)
				})
			} else {
				Ok(Value::String(s.clone()))
			}
		}
		Value::Array(arr) => {
			let mut out = Vec::with_capacity(arr.len());
			for item in arr {
				out.push(resolve_refs(item, bindings)?);
			}
			Ok(Value::Array(out))
		}
		Value::Object(map) => {
			let mut out = Map::new();
			for (k, v) in map {
				out.insert(k.clone(), resolve_refs(v, bindings)?);
			}
			Ok(Value::Object(out))
		}
		_ => Ok(value.clone()),
	}
}

// ── Operation dispatch ───────────────────────────────────────────
//
// One match arm per operation kind. Each arm:
//   1. Extracts and type-checks the arguments from the resolved op.
//   2. Invokes the corresponding StorageConnection method.
//   3. If the op has `as`, captures the return value into bindings.
//   4. If the op generates dynamic values (currently only
//      `createSnapshot`), records substitution rules.

fn dispatch_op(
	storage: &mut StorageConnection,
	state: &mut HarnessState,
	raw_op: &Value,
) -> Result<(), String> {
	let op_kind = raw_op
		.get("op")
		.and_then(|v| v.as_str())
		.ok_or_else(|| "operation missing 'op' field".to_string())?
		.to_string();

	// Resolve @binding.field references in the operation's argument
	// fields. The 'op' and 'as' fields don't contain refs themselves
	// but the rest of the object might.
	let resolved = resolve_refs(raw_op, &state.bindings)
		.map_err(|e| format!("{}: {}", op_kind, e))?;

	let binding_name: Option<String> = resolved
		.get("as")
		.and_then(|v| v.as_str())
		.map(String::from);

	let get_field = |key: &str| -> Result<Value, String> {
		resolved
			.get(key)
			.cloned()
			.ok_or_else(|| format!("{}: missing '{}' field", op_kind, key))
	};

	match op_kind.as_str() {
		"addRepo" => {
			let repo: Repo = from_json("addRepo.repo", get_field("repo")?)?;
			storage.add_repo(&repo).map_err(|e| format!("addRepo: {}", e))?;
		}

		"getRepo" => {
			let ref_val = get_field("ref")?;
			let repo_ref = parse_repo_ref(&ref_val)?;
			let result = storage
				.get_repo(&repo_ref)
				.map_err(|e| format!("getRepo: {}", e))?;
			capture_option_dto(state, binding_name.as_deref(), result)?;
		}

		"listRepos" => {
			let repos = storage
				.list_repos()
				.map_err(|e| format!("listRepos: {}", e))?;
			capture_vec_dto(state, binding_name.as_deref(), repos)?;
		}

		"removeRepo" => {
			let uid: String = from_json("removeRepo.repoUid", get_field("repoUid")?)?;
			storage
				.remove_repo(&uid)
				.map_err(|e| format!("removeRepo: {}", e))?;
		}

		"createSnapshot" => {
			let input: CreateSnapshotInput =
				from_json("createSnapshot.input", get_field("input")?)?;
			let snap = storage
				.create_snapshot(&input)
				.map_err(|e| format!("createSnapshot: {}", e))?;

			// Register dynamic-value substitutions. Only createSnapshot
			// introduces new dynamic values (snapshot_uid, created_at).
			// Bindings are required here because without one the
			// generated values could not be referenced later OR
			// normalized in the dump.
			let name = binding_name.as_deref().ok_or_else(|| {
				"createSnapshot must have an 'as' binding so the generated \
				 snapshot_uid and created_at can be normalized"
					.to_string()
			})?;
			state.substitutions.push((
				snap.snapshot_uid.clone(),
				format!("<SNAP:{}>", name),
			));
			state.substitutions.push((
				snap.created_at.clone(),
				format!("<TS:{}:createdAt>", name),
			));

			let snap_json = serde_json::to_value(&snap)
				.map_err(|e| format!("createSnapshot: serialize: {}", e))?;
			state.bindings.insert(name.to_string(), snap_json);
		}

		"getSnapshot" => {
			let uid: String = from_json("getSnapshot.snapshotUid", get_field("snapshotUid")?)?;
			let result = storage
				.get_snapshot(&uid)
				.map_err(|e| format!("getSnapshot: {}", e))?;
			capture_option_dto(state, binding_name.as_deref(), result)?;
		}

		"getLatestSnapshot" => {
			let repo_uid: String =
				from_json("getLatestSnapshot.repoUid", get_field("repoUid")?)?;
			let result = storage
				.get_latest_snapshot(&repo_uid)
				.map_err(|e| format!("getLatestSnapshot: {}", e))?;
			capture_option_dto(state, binding_name.as_deref(), result)?;
		}

		"updateSnapshotStatus" => {
			let input: UpdateSnapshotStatusInput =
				from_json("updateSnapshotStatus.input", get_field("input")?)?;
			if input.completed_at.is_none() {
				return Err(
					"updateSnapshotStatus fixtures MUST provide explicit 'completedAt' \
					 (the auto-generation path is unsupported in R2-F v1 because it \
					 introduces a new dynamic timestamp the binding machinery cannot \
					 capture)"
						.to_string(),
				);
			}
			storage
				.update_snapshot_status(&input)
				.map_err(|e| format!("updateSnapshotStatus: {}", e))?;
		}

		"updateSnapshotCounts" => {
			let uid: String =
				from_json("updateSnapshotCounts.snapshotUid", get_field("snapshotUid")?)?;
			storage
				.update_snapshot_counts(&uid)
				.map_err(|e| format!("updateSnapshotCounts: {}", e))?;
		}

		"upsertFiles" => {
			let files: Vec<TrackedFile> = from_json("upsertFiles.files", get_field("files")?)?;
			storage
				.upsert_files(&files)
				.map_err(|e| format!("upsertFiles: {}", e))?;
		}

		"upsertFileVersions" => {
			let versions: Vec<FileVersion> =
				from_json("upsertFileVersions.fileVersions", get_field("fileVersions")?)?;
			storage
				.upsert_file_versions(&versions)
				.map_err(|e| format!("upsertFileVersions: {}", e))?;
		}

		"getFilesByRepo" => {
			let repo_uid: String =
				from_json("getFilesByRepo.repoUid", get_field("repoUid")?)?;
			let files = storage
				.get_files_by_repo(&repo_uid)
				.map_err(|e| format!("getFilesByRepo: {}", e))?;
			capture_vec_dto(state, binding_name.as_deref(), files)?;
		}

		"getStaleFiles" => {
			let snapshot_uid: String =
				from_json("getStaleFiles.snapshotUid", get_field("snapshotUid")?)?;
			let files = storage
				.get_stale_files(&snapshot_uid)
				.map_err(|e| format!("getStaleFiles: {}", e))?;
			capture_vec_dto(state, binding_name.as_deref(), files)?;
		}

		"queryFileVersionHashes" => {
			let snapshot_uid: String = from_json(
				"queryFileVersionHashes.snapshotUid",
				get_field("snapshotUid")?,
			)?;
			let hashes = storage
				.query_file_version_hashes(&snapshot_uid)
				.map_err(|e| format!("queryFileVersionHashes: {}", e))?;
			if let Some(name) = binding_name.as_deref() {
				// HashMap has no defined iteration order. Sort keys
				// for deterministic serialization.
				let mut sorted: Vec<(String, String)> = hashes.into_iter().collect();
				sorted.sort_by(|a, b| a.0.cmp(&b.0));
				let mut map = Map::new();
				for (k, v) in sorted {
					map.insert(k, Value::String(v));
				}
				state.bindings.insert(name.to_string(), Value::Object(map));
			}
		}

		"insertNodes" => {
			let nodes: Vec<GraphNode> = from_json("insertNodes.nodes", get_field("nodes")?)?;
			storage
				.insert_nodes(&nodes)
				.map_err(|e| format!("insertNodes: {}", e))?;
		}

		"queryAllNodes" => {
			let snapshot_uid: String =
				from_json("queryAllNodes.snapshotUid", get_field("snapshotUid")?)?;
			let mut nodes = storage
				.query_all_nodes(&snapshot_uid)
				.map_err(|e| format!("queryAllNodes: {}", e))?;
			// Sort by node_uid for deterministic result ordering
			// (the underlying SQL has no ORDER BY; both runtimes
			// must produce the same order).
			nodes.sort_by(|a, b| a.node_uid.cmp(&b.node_uid));
			capture_vec_dto(state, binding_name.as_deref(), nodes)?;
		}

		"deleteNodesByFile" => {
			let snapshot_uid: String =
				from_json("deleteNodesByFile.snapshotUid", get_field("snapshotUid")?)?;
			let file_uid: String =
				from_json("deleteNodesByFile.fileUid", get_field("fileUid")?)?;
			storage
				.delete_nodes_by_file(&snapshot_uid, &file_uid)
				.map_err(|e| format!("deleteNodesByFile: {}", e))?;
		}

		"insertEdges" => {
			let edges: Vec<GraphEdge> = from_json("insertEdges.edges", get_field("edges")?)?;
			storage
				.insert_edges(&edges)
				.map_err(|e| format!("insertEdges: {}", e))?;
		}

		"deleteEdgesByUids" => {
			let uids: Vec<String> =
				from_json("deleteEdgesByUids.edgeUids", get_field("edgeUids")?)?;
			storage
				.delete_edges_by_uids(&uids)
				.map_err(|e| format!("deleteEdgesByUids: {}", e))?;
		}

		other => {
			return Err(format!("unknown operation: {}", other));
		}
	}

	Ok(())
}

fn parse_repo_ref(value: &Value) -> Result<RepoRef, String> {
	let obj = value
		.as_object()
		.ok_or_else(|| "getRepo.ref must be an object".to_string())?;
	if let Some(uid) = obj.get("uid").and_then(|v| v.as_str()) {
		Ok(RepoRef::Uid(uid.to_string()))
	} else if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
		Ok(RepoRef::Name(name.to_string()))
	} else if let Some(root_path) = obj.get("rootPath").and_then(|v| v.as_str()) {
		Ok(RepoRef::RootPath(root_path.to_string()))
	} else {
		Err("getRepo.ref must have one of {uid, name, rootPath}".to_string())
	}
}

fn from_json<T: for<'de> serde::Deserialize<'de>>(
	label: &str,
	value: Value,
) -> Result<T, String> {
	serde_json::from_value(value).map_err(|e| format!("{}: {}", label, e))
}

fn capture_option_dto<T: serde::Serialize>(
	state: &mut HarnessState,
	binding_name: Option<&str>,
	value: Option<T>,
) -> Result<(), String> {
	if let Some(name) = binding_name {
		let json = match value {
			Some(v) => serde_json::to_value(&v).map_err(|e| e.to_string())?,
			None => Value::Null,
		};
		state.bindings.insert(name.to_string(), json);
	}
	Ok(())
}

fn capture_vec_dto<T: serde::Serialize>(
	state: &mut HarnessState,
	binding_name: Option<&str>,
	values: Vec<T>,
) -> Result<(), String> {
	if let Some(name) = binding_name {
		let json = serde_json::to_value(&values).map_err(|e| e.to_string())?;
		state.bindings.insert(name.to_string(), json);
	}
	Ok(())
}

// ── State dump ───────────────────────────────────────────────────
//
// The harness does NOT build the dump locally. It calls
// `StorageConnection::diagnostic_dump()`, a narrow public method
// that returns the canonical `{schema, tables}` JSON. The dump
// logic lives inside the crate at `src/diagnostic.rs`. See the
// R2-F correction note in `connection.rs`'s `connection()`
// accessor docs for the architectural rationale (it used to be
// in this file, plumbed through a widened `pub fn connection()`
// escape hatch; R2-F P2 review caught that as an API-surface
// regression and the dump logic was re-encapsulated inside the
// crate with only a narrow public entry point exposed).

// ── Normalization ────────────────────────────────────────────────

/// Apply normalization rules to the dump + results in place.
/// See `storage-parity-fixtures/README.md` → "Normalization rules".
fn normalize(dump: &mut Value, results: &mut Value, substitutions: &[(String, String)]) {
	// 1. Blanket column normalization:
	//    schema_migrations.applied_at → "<TIMESTAMP>"
	if let Some(tables) = dump.get_mut("tables").and_then(|v| v.as_object_mut()) {
		if let Some(sm_rows) = tables
			.get_mut("schema_migrations")
			.and_then(|v| v.as_array_mut())
		{
			for row in sm_rows {
				if let Some(obj) = row.as_object_mut() {
					obj.insert(
						"applied_at".to_string(),
						Value::String("<TIMESTAMP>".to_string()),
					);
				}
			}
		}
	}

	// 2. Binding-based substitutions: walk every string in both
	//    the dump and the results, replace matching literal values
	//    with their placeholders.
	apply_substitutions(dump, substitutions);
	apply_substitutions(results, substitutions);
}

fn apply_substitutions(value: &mut Value, subs: &[(String, String)]) {
	match value {
		Value::String(s) => {
			for (actual, placeholder) in subs {
				if s == actual {
					*s = placeholder.clone();
					return;
				}
			}
		}
		Value::Array(arr) => {
			for item in arr {
				apply_substitutions(item, subs);
			}
		}
		Value::Object(map) => {
			for v in map.values_mut() {
				apply_substitutions(v, subs);
			}
		}
		_ => {}
	}
}

// ── Fixture execution ────────────────────────────────────────────

fn run_fixture(fixture: &Fixture) -> Result<(), String> {
	let mut storage = StorageConnection::open_in_memory()
		.map_err(|e| format!("open_in_memory: {}", e))?;
	let mut state = HarnessState::default();

	let ops = fixture
		.operations_doc
		.get("operations")
		.and_then(|v| v.as_array())
		.ok_or_else(|| "operations.json must have 'operations' array".to_string())?;

	for (i, op) in ops.iter().enumerate() {
		dispatch_op(&mut storage, &mut state, op)
			.map_err(|e| format!("op[{}]: {}", i, e))?;
	}

	let mut dump = storage.diagnostic_dump();
	let mut results_val = Value::Object(state.bindings.clone());
	normalize(&mut dump, &mut results_val, &state.substitutions);

	let actual = build_actual(&results_val, &dump);
	if actual == fixture.expected {
		return Ok(());
	}

	// Mismatch: produce a diff-friendly error.
	let expected_pretty = serde_json::to_string_pretty(&fixture.expected).unwrap_or_default();
	let actual_pretty = serde_json::to_string_pretty(&actual).unwrap_or_default();

	// If the environment variable is set, print the actual output so
	// fixture authors can bootstrap expected.json. This is a
	// diagnostic aid, NOT a regeneration mechanism — the expected
	// file is always committed by hand.
	if std::env::var("RGR_STORAGE_PARITY_EMIT_ACTUAL").is_ok() {
		eprintln!("=== ACTUAL for fixture '{}' ===", fixture.name);
		eprintln!("{}", actual_pretty);
		eprintln!("=== END ACTUAL ===");
	}

	Err(format!(
		"fixture mismatch\n--- expected ---\n{}\n\n--- actual ---\n{}",
		expected_pretty, actual_pretty
	))
}

/// Assemble the full actual value from results + dump. Key order
/// is `results, schema, tables` for deterministic comparison.
fn build_actual(results: &Value, dump: &Value) -> Value {
	let schema = dump.get("schema").cloned().unwrap_or(Value::Null);
	let tables = dump.get("tables").cloned().unwrap_or(Value::Null);
	json!({
		"results": results,
		"schema": schema,
		"tables": tables,
	})
}

// ── Entry point ──────────────────────────────────────────────────

#[test]
fn parity_against_shared_storage_fixture_corpus() {
	let fixtures = discover_fixtures();
	assert!(
		!fixtures.is_empty(),
		"no fixtures found in storage-parity-fixtures/ (expected at least one)"
	);

	let mut failures: Vec<String> = Vec::new();
	for fixture in &fixtures {
		match run_fixture(fixture) {
			Ok(()) => {}
			Err(e) => failures.push(format!("\n── fixture: {} ──\n{}", fixture.name, e)),
		}
	}

	if !failures.is_empty() {
		panic!(
			"\n{} of {} fixtures failed:\n{}\n\nTip: set RGR_STORAGE_PARITY_EMIT_ACTUAL=1 to print the actual JSON for each failing fixture.",
			failures.len(),
			fixtures.len(),
			failures.join("\n")
		);
	}
}
