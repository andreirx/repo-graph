//! Rust half of the ts-extractor parity harness.
//!
//! Walks `ts-extractor-parity-fixtures/` at the repo root, runs each
//! fixture through the Rust `TsExtractor`, normalizes the output to
//! the canonical comparison shape, and compares against `expected.json`.
//!
//! Canonical comparison shape (locked at R6 design):
//!   - nodes: keyed by stable_key, fields: kind, subtype, name,
//!     qualified_name, signature, visibility, doc_comment
//!   - edges: keyed by (source_stable_key, edge_type, target_key),
//!     fields: metadata_json
//!   - import_bindings: by (identifier, specifier, is_relative, is_type_only)
//!   - metrics: keyed by stable_key, all 3 fields
//!   - Stripped: nodeUid, edgeUid, snapshotUid, repoUid, fileUid,
//!     extractor version
//!   - Location INCLUDED on nodes and edges (stable for same source+grammar)

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_ts_extractor::TsExtractor;
use serde_json::Value;
use walkdir::WalkDir;

fn fixtures_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("..").join("..").join("..")
		.join("ts-extractor-parity-fixtures")
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
	for entry in WalkDir::new(&root).min_depth(1).max_depth(1).sort_by_file_name() {
		let entry = entry.unwrap();
		if !entry.file_type().is_dir() { continue; }
		let path = entry.path().to_path_buf();
		let name = path.file_name().unwrap().to_str().unwrap().to_string();
		let input_path = path.join("input.json");
		if !input_path.exists() { continue; }
		let input: Value = serde_json::from_str(&fs::read_to_string(&input_path).unwrap()).unwrap();
		let expected_path = path.join("expected.json");
		let expected = if expected_path.exists() {
			Some(serde_json::from_str(&fs::read_to_string(&expected_path).unwrap()).unwrap())
		} else { None };
		fixtures.push(Fixture { name, dir: path, input, expected });
	}
	fixtures
}

/// Normalize extraction result to the canonical comparison shape.
fn normalize_result(result: &repo_graph_indexer::types::ExtractionResult, nodes_list: &[repo_graph_indexer::types::ExtractedNode]) -> Value {
	// Build stable_key → node_uid map for edge source resolution.
	let uid_to_key: BTreeMap<String, String> = nodes_list.iter()
		.map(|n| (n.node_uid.clone(), n.stable_key.clone()))
		.collect();

	// Nodes: keyed by stable_key, stripped of volatile fields.
	let mut nodes = serde_json::Map::new();
	for n in nodes_list {
		let subtype_val = n.subtype.as_ref()
			.map(|st| serde_json::to_value(st).unwrap_or(Value::Null))
			.unwrap_or(Value::Null);
		let kind_val = serde_json::to_value(&n.kind).unwrap_or(Value::Null);
		let vis_val = n.visibility.as_ref()
			.map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
			.unwrap_or(Value::Null);
		let loc_val = n.location.map(|l| serde_json::json!({
			"lineStart": l.line_start,
			"colStart": l.col_start,
			"lineEnd": l.line_end,
			"colEnd": l.col_end,
		}));
		nodes.insert(n.stable_key.clone(), serde_json::json!({
			"kind": kind_val,
			"subtype": subtype_val,
			"name": n.name,
			"qualified_name": n.qualified_name,
			"location": loc_val,
			"signature": n.signature,
			"visibility": vis_val,
			"doc_comment": n.doc_comment,
		}));
	}

	// Edges: sorted list of (source_stable_key, edge_type, target_key, metadata).
	let mut edges: Vec<Value> = result.edges.iter().map(|e| {
		let source_key = uid_to_key.get(&e.source_node_uid)
			.cloned().unwrap_or_else(|| e.source_node_uid.clone());
		let edge_type = serde_json::to_value(&e.edge_type).unwrap_or(Value::Null);
		let metadata: Value = e.metadata_json.as_ref()
			.and_then(|s| serde_json::from_str(s).ok())
			.unwrap_or(Value::Null);
		let loc_val = e.location.map(|l| serde_json::json!({
			"lineStart": l.line_start,
			"colStart": l.col_start,
			"lineEnd": l.line_end,
			"colEnd": l.col_end,
		}));
		serde_json::json!({
			"source": source_key,
			"type": edge_type,
			"target": e.target_key,
			"location": loc_val,
			"metadata": metadata,
		})
	}).collect();
	edges.sort_by(|a, b| {
		let ka = format!("{}|{}|{}", a["type"], a["source"], a["target"]);
		let kb = format!("{}|{}|{}", b["type"], b["source"], b["target"]);
		ka.cmp(&kb)
	});

	// Import bindings: sorted by (identifier, specifier).
	let mut bindings: Vec<Value> = result.import_bindings.iter().map(|b| {
		let loc_val = b.location.map(|l| serde_json::json!({
			"lineStart": l.line_start,
			"colStart": l.col_start,
			"lineEnd": l.line_end,
			"colEnd": l.col_end,
		}));
		serde_json::json!({
			"identifier": b.identifier,
			"specifier": b.specifier,
			"is_relative": b.is_relative,
			"is_type_only": b.is_type_only,
			"location": loc_val,
		})
	}).collect();
	bindings.sort_by(|a, b| {
		let ka = format!("{}|{}", a["identifier"], a["specifier"]);
		let kb = format!("{}|{}", b["identifier"], b["specifier"]);
		ka.cmp(&kb)
	});

	// Metrics: keyed by stable_key.
	let mut metrics = serde_json::Map::new();
	for (key, m) in &result.metrics {
		metrics.insert(key.clone(), serde_json::json!({
			"cyclomatic_complexity": m.cyclomatic_complexity,
			"parameter_count": m.parameter_count,
			"max_nesting_depth": m.max_nesting_depth,
		}));
	}

	serde_json::json!({
		"nodes": nodes,
		"edges": edges,
		"import_bindings": bindings,
		"metrics": metrics,
	})
}

fn run_fixture(fixture: &Fixture) -> Result<Value, String> {
	let source = fixture.input["source"].as_str().ok_or("missing source")?;
	let file_path = fixture.input["filePath"].as_str().ok_or("missing filePath")?;
	let file_uid = fixture.input["fileUid"].as_str().ok_or("missing fileUid")?;
	let repo_uid = fixture.input["repoUid"].as_str().ok_or("missing repoUid")?;
	let snapshot_uid = fixture.input["snapshotUid"].as_str().ok_or("missing snapshotUid")?;

	let mut ext = TsExtractor::new();
	ext.initialize().map_err(|e| format!("init: {}", e))?;
	let result = ext.extract(source, file_path, file_uid, repo_uid, snapshot_uid)
		.map_err(|e| format!("extract: {}", e))?;

	Ok(normalize_result(&result, &result.nodes))
}

#[test]
fn parity_against_shared_ts_extractor_fixture_corpus() {
	let fixtures = discover_fixtures();
	assert!(!fixtures.is_empty(), "no fixtures found");

	let mut failures = Vec::new();
	let mut generated = Vec::new();

	for fixture in &fixtures {
		let actual = match run_fixture(fixture) {
			Ok(v) => v,
			Err(e) => { failures.push(format!("-- {} -- error: {}", fixture.name, e)); continue; }
		};
		match &fixture.expected {
			Some(expected) if actual == *expected => {}
			Some(expected) => {
				let ap = serde_json::to_string_pretty(&actual).unwrap_or_default();
				let ep = serde_json::to_string_pretty(expected).unwrap_or_default();
				if std::env::var("RGR_TS_EXTRACTOR_PARITY_UPDATE").is_ok() {
					fs::write(fixture.dir.join("expected.json"), format!("{ap}\n")).unwrap();
					generated.push(fixture.name.clone());
				} else {
					failures.push(format!("-- {} -- mismatch\n--- expected ---\n{ep}\n--- actual ---\n{ap}", fixture.name));
				}
			}
			None => {
				let ap = serde_json::to_string_pretty(&actual).unwrap_or_default();
				fs::write(fixture.dir.join("expected.json"), format!("{ap}\n")).unwrap();
				generated.push(fixture.name.clone());
			}
		}
	}
	if !generated.is_empty() { eprintln!("Generated {}: {:?}", generated.len(), generated); }
	if !failures.is_empty() { panic!("{} failed:{}", failures.len(), failures.join("\n")); }
	if !generated.is_empty() { panic!("Generated {}. Re-run.", generated.len()); }
}
