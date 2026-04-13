//! Rust half of the indexer parity harness.
//!
//! Walks `indexer-parity-fixtures/` at the repo root, runs each
//! fixture's `input.json` through the corresponding Rust indexer
//! function, and compares against `expected.json`.
//!
//! Parity scope (R5-I lock): pure routing/resolution/invalidation
//! policies only. Orchestration workflow verification is covered by
//! the Rust unit tests in `orchestrator::tests`, not by this
//! cross-runtime harness (the TS orchestrator requires the full
//! adapter stack and cannot be exercised through mock fixtures).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use repo_graph_indexer::invalidation::{self, CurrentFileState};
use repo_graph_indexer::resolver::categorize_unresolved_edge;
use repo_graph_indexer::routing;
use repo_graph_indexer::types::{EdgeType, ExtractedEdge, Resolution};
use serde_json::Value;
use walkdir::WalkDir;

fn fixtures_root() -> PathBuf {
	let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
	manifest_dir
		.join("..")
		.join("..")
		.join("..")
		.join("indexer-parity-fixtures")
}

struct Fixture {
	name: String,
	dir: PathBuf,
	input: Value,
	expected: Option<Value>,
}

fn discover_fixtures() -> Vec<Fixture> {
	let root = fixtures_root();
	let mut fixtures = Vec::new();
	for entry in WalkDir::new(&root)
		.min_depth(1)
		.max_depth(1)
		.sort_by_file_name()
	{
		let entry = entry.unwrap();
		if !entry.file_type().is_dir() {
			continue;
		}
		let path = entry.path().to_path_buf();
		let name = path.file_name().unwrap().to_str().unwrap().to_string();
		let input_path = path.join("input.json");
		if !input_path.exists() {
			continue;
		}
		let input: Value =
			serde_json::from_str(&fs::read_to_string(&input_path).unwrap()).unwrap();
		let expected_path = path.join("expected.json");
		let expected = if expected_path.exists() {
			Some(serde_json::from_str(&fs::read_to_string(&expected_path).unwrap()).unwrap())
		} else {
			None
		};
		fixtures.push(Fixture { name, dir: path, input, expected });
	}
	fixtures
}

fn get_str(input: &Value, key: &str) -> String {
	input.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn run_fixture(fixture: &Fixture) -> Result<Value, String> {
	let fn_name = get_str(&fixture.input, "fn");
	match fn_name.as_str() {
		"detect_language" => {
			let path = get_str(&fixture.input, "filePath");
			let result = routing::detect_language(&path);
			Ok(serde_json::to_value(&result).unwrap())
		}
		"is_test_file" => {
			let path = get_str(&fixture.input, "filePath");
			let result = routing::is_test_file(&path);
			Ok(serde_json::to_value(&result).unwrap())
		}
		"categorize_unresolved_edge" => {
			let target_key = get_str(&fixture.input, "targetKey");
			let edge_type_str = get_str(&fixture.input, "edgeType");
			let edge_type: EdgeType =
				serde_json::from_value(Value::String(edge_type_str)).map_err(|e| e.to_string())?;
			let metadata_json: Option<String> = fixture
				.input
				.get("metadataJson")
				.and_then(|v| if v.is_null() { None } else { v.as_str().map(|s| s.to_string()) });
			let edge = ExtractedEdge {
				edge_uid: "parity".into(),
				snapshot_uid: "snap".into(),
				repo_uid: "r1".into(),
				source_node_uid: "src".into(),
				target_key,
				edge_type,
				resolution: Resolution::Static,
				extractor: "test:1".into(),
				location: None,
				metadata_json,
			};
			let category = categorize_unresolved_edge(&edge);
			Ok(serde_json::to_value(&category).unwrap())
		}
		"build_invalidation_plan" => {
			let parent_uid = get_str(&fixture.input, "parentSnapshotUid");
			let repo_uid = get_str(&fixture.input, "repoUid");
			let parent_hashes: BTreeMap<String, String> = fixture
				.input
				.get("parentHashes")
				.and_then(|v| serde_json::from_value(v.clone()).ok())
				.unwrap_or_default();
			let current_files: Vec<CurrentFileState> = fixture
				.input
				.get("currentFiles")
				.and_then(|v| v.as_array())
				.map(|arr| {
					arr.iter()
						.map(|v| CurrentFileState {
							file_uid: v.get("fileUid").and_then(|x| x.as_str()).unwrap_or("").into(),
							path: v.get("path").and_then(|x| x.as_str()).unwrap_or("").into(),
							content_hash: v.get("contentHash").and_then(|x| x.as_str()).unwrap_or("").into(),
						})
						.collect()
				})
				.unwrap_or_default();
			let plan = invalidation::build_invalidation_plan(
				&parent_uid, &parent_hashes, &current_files, &repo_uid,
			);
			Ok(serde_json::json!({
				"counts": {
					"unchanged": plan.counts.unchanged,
					"changed": plan.counts.changed,
					"new": plan.counts.new,
					"deleted": plan.counts.deleted,
					"config_widened": plan.counts.config_widened,
					"total": plan.counts.total,
				},
				"files_to_extract": plan.files_to_extract,
				"files_to_copy": plan.files_to_copy,
				"files_to_delete": plan.files_to_delete,
			}))
		}
		other => Err(format!("unknown fn: {other}")),
	}
}

#[test]
fn parity_against_shared_indexer_fixture_corpus() {
	let fixtures = discover_fixtures();
	assert!(!fixtures.is_empty(), "no fixtures found in indexer-parity-fixtures/");

	let mut failures = Vec::new();
	let mut generated = Vec::new();

	for fixture in &fixtures {
		let actual = match run_fixture(fixture) {
			Ok(v) => v,
			Err(e) => {
				failures.push(format!("\n-- {} --\nerror: {}", fixture.name, e));
				continue;
			}
		};

		match &fixture.expected {
			Some(expected) if actual == *expected => {}
			Some(expected) => {
				let ap = serde_json::to_string_pretty(&actual).unwrap_or_default();
				let ep = serde_json::to_string_pretty(expected).unwrap_or_default();
				if std::env::var("RGR_INDEXER_PARITY_UPDATE").is_ok() {
					fs::write(fixture.dir.join("expected.json"), format!("{ap}\n")).unwrap();
					generated.push(fixture.name.clone());
				} else {
					failures.push(format!(
						"\n-- {} --\nmismatch\n--- expected ---\n{ep}\n--- actual ---\n{ap}",
						fixture.name
					));
				}
			}
			None => {
				let ap = serde_json::to_string_pretty(&actual).unwrap_or_default();
				fs::write(fixture.dir.join("expected.json"), format!("{ap}\n")).unwrap();
				generated.push(fixture.name.clone());
			}
		}
	}

	if !generated.is_empty() {
		eprintln!("Generated expected.json for {}: {:?}", generated.len(), generated);
	}
	if !failures.is_empty() {
		panic!("{} of {} failed:{}", failures.len(), fixtures.len(), failures.join("\n"));
	}
	if !generated.is_empty() {
		panic!("Generated {} expected.json. Re-run to verify.", generated.len());
	}
}
