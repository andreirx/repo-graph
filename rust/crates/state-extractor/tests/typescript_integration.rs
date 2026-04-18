//! End-to-end integration test for the TS pipeline:
//!
//!   synthetic TS source
//!     → `ts-extractor::extract`
//!     → `ExtractionResult.resolved_callsites`
//!     → `languages::typescript::emit_from_resolved_callsites`
//!     → `StateBoundaryEmitter`
//!     → `EmittedFacts` with nodes + edges
//!
//! Scope: SB-3. No real corpus (that is SB-4). Every source
//! string is hand-crafted to exercise a specific path in the
//! pipeline.

use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::types::{EdgeType, NodeKind, NodeSubtype, Resolution};
use repo_graph_state_bindings::{BindingTable, Language, RepoUid};
use repo_graph_state_extractor::languages::typescript::emit_from_resolved_callsites;
use repo_graph_state_extractor::{EmitterContext, StateBoundaryEmitter};
use repo_graph_ts_extractor::TsExtractor;

const REPO_UID: &str = "myservice";
const SNAPSHOT_UID: &str = "snap-1";
const EXTRACTOR: &str = "state-extractor:0.1.0";

fn run_extractor(source: &str, path: &str) -> repo_graph_indexer::types::ExtractionResult {
	let mut ext = TsExtractor::new();
	ext.initialize().expect("ts-extractor initialize");
	let file_uid = format!("{}:{}", REPO_UID, path);
	ext.extract(source, path, &file_uid, REPO_UID, SNAPSHOT_UID)
		.expect("extract should succeed")
}

fn new_emitter(table: &BindingTable) -> StateBoundaryEmitter<'_> {
	StateBoundaryEmitter::new(
		table,
		EmitterContext {
			repo_uid: RepoUid::new(REPO_UID).unwrap(),
			snapshot_uid: SNAPSHOT_UID.to_string(),
			language: Language::Typescript,
			extractor_name: EXTRACTOR.to_string(),
		},
	)
}

// ── Named-import read path ─────────────────────────────────────────

#[test]
fn fs_read_file_named_import_emits_reads_edge_and_fs_path_node() {
	let source = r#"
import { readFile } from "fs";
export function load() {
  readFile("/etc/app.yaml", () => {});
}
"#;
	let result = run_extractor(source, "src/load.ts");
	assert_eq!(result.resolved_callsites.len(), 1);

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	let count = emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter)
		.expect("emit must succeed");
	assert_eq!(count, 1);

	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(facts.edges.len(), 1);

	let node = &facts.nodes[0];
	assert_eq!(node.kind, NodeKind::FsPath);
	assert_eq!(node.subtype, Some(NodeSubtype::FilePath));
	assert_eq!(node.stable_key, "myservice:fs:/etc/app.yaml:FS_PATH");
	assert_eq!(node.name, "/etc/app.yaml");

	let edge = &facts.edges[0];
	assert_eq!(edge.edge_type, EdgeType::Reads);
	assert_eq!(edge.target_key, node.stable_key);
	assert_eq!(edge.resolution, Resolution::Static);
	let md = edge.metadata_json.as_ref().expect("edge must have metadata");
	assert!(md.contains("\"basis\":\"stdlib_api\""));
	assert!(md.contains("\"binding_key\":\"typescript:fs:readFile:read\""));
}

// ── Default-import write path ──────────────────────────────────────

#[test]
fn fs_write_file_default_import_member_call() {
	let source = r#"
import fs from "fs";
export function save(data: Buffer) {
  fs.writeFileSync("/var/log/out.bin", data);
}
"#;
	let result = run_extractor(source, "src/save.ts");
	assert_eq!(result.resolved_callsites.len(), 1);

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	let count =
		emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();
	assert_eq!(count, 1);

	let facts = emitter.drain();
	assert_eq!(facts.edges.len(), 1);
	assert_eq!(facts.edges[0].edge_type, EdgeType::Writes);
	assert_eq!(
		facts.nodes[0].stable_key,
		"myservice:fs:/var/log/out.bin:FS_PATH"
	);
}

// ── Namespace import + env-key read ────────────────────────────────

#[test]
fn fs_namespace_import_with_env_key_logical_name() {
	let source = r#"
import * as fs from "fs";
export function load() {
  fs.readFile(process.env.CACHE_DIR, () => {});
}
"#;
	let result = run_extractor(source, "src/load.ts");
	assert_eq!(result.resolved_callsites.len(), 1);

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();

	let facts = emitter.drain();
	let node = &facts.nodes[0];
	// Env-key derived → Logical subtype.
	assert_eq!(node.subtype, Some(NodeSubtype::Logical));
	assert_eq!(node.stable_key, "myservice:fs:CACHE_DIR:FS_PATH");

	let edge = &facts.edges[0];
	// Env-key source → Inferred resolution (per contract §7).
	assert_eq!(edge.resolution, Resolution::Inferred);
	let md = edge.metadata_json.as_ref().unwrap();
	assert!(md.contains("\"logical_name_source\":\"env_key\""));
}

// ── Aliased named import ──────────────────────────────────────────

#[test]
fn fs_aliased_named_import_resolves_original_symbol_end_to_end() {
	let source = r#"
import { readFile as rf } from "fs";
export function load() {
  rf("/etc/config", () => {});
}
"#;
	let result = run_extractor(source, "src/load.ts");
	assert_eq!(result.resolved_callsites.len(), 1);
	// ResolvedCallsite must carry the ORIGINAL symbol, not the
	// local alias.
	assert_eq!(result.resolved_callsites[0].resolved_symbol, "readFile");

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();

	let facts = emitter.drain();
	assert_eq!(facts.edges.len(), 1);
	assert_eq!(facts.edges[0].edge_type, EdgeType::Reads);
	// Binding key uses the canonical symbol, not the alias.
	assert!(facts.edges[0]
		.metadata_json
		.as_ref()
		.unwrap()
		.contains("\"binding_key\":\"typescript:fs:readFile:read\""));
}

// ── node: protocol specifier variants ─────────────────────────────

#[test]
fn node_fs_specifier_matches() {
	let source = r#"
import { readFile } from "node:fs";
export function load() {
  readFile("/etc/x", () => {});
}
"#;
	let result = run_extractor(source, "src/load.ts");
	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	let count =
		emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();
	assert_eq!(count, 1, "node:fs:readFile must match");
}

#[test]
fn node_fs_promises_specifier_matches() {
	let source = r#"
import { writeFile } from "node:fs/promises";
export async function save() {
  await writeFile("/etc/out", "data");
}
"#;
	let result = run_extractor(source, "src/save.ts");
	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	let count =
		emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();
	assert_eq!(count, 1, "node:fs/promises:writeFile must match");
	let facts = emitter.drain();
	assert_eq!(facts.edges[0].edge_type, EdgeType::Writes);
}

// ── fs/promises (non-prefixed) ────────────────────────────────────

#[test]
fn fs_promises_specifier_matches() {
	let source = r#"
import { appendFile } from "fs/promises";
export async function log(line: string) {
  await appendFile("/var/log/app.log", line);
}
"#;
	let result = run_extractor(source, "src/log.ts");
	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	let count =
		emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();
	assert_eq!(count, 1);
	let facts = emitter.drain();
	assert_eq!(facts.edges[0].edge_type, EdgeType::Writes);
}

// ── Dedup: two reads to the same path → one resource node ─────────

#[test]
fn two_reads_to_same_path_dedup_to_one_fs_path_node() {
	let source = r#"
import { readFile, readFileSync } from "fs";
export function a() {
  readFile("/etc/settings", () => {});
}
export function b() {
  readFileSync("/etc/settings");
}
"#;
	let result = run_extractor(source, "src/double.ts");
	assert_eq!(result.resolved_callsites.len(), 2);

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();

	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 1, "same path → single node");
	assert_eq!(facts.edges.len(), 2, "two call sites → two edges");
	for edge in &facts.edges {
		assert_eq!(edge.target_key, facts.nodes[0].stable_key);
	}
}

// ── Unmatched module produces no state-boundary edge ──────────────

#[test]
fn non_fs_module_produces_no_state_boundary_edge() {
	let source = r#"
import { map } from "lodash";
export function transform(xs: number[]) {
  map(xs, (x) => x * 2);
}
"#;
	let result = run_extractor(source, "src/xf.ts");
	// lodash is not in the binding table; ts-extractor still
	// emits a ResolvedCallsite (callee resolves via import
	// bindings), but state-extractor's matcher returns no match.
	// The arg 0 is a variable reference, not a string literal,
	// so ts-extractor should suppress the ResolvedCallsite
	// entirely.
	assert_eq!(result.resolved_callsites.len(), 0);

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	let count =
		emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();
	assert_eq!(count, 0);
	let facts = emitter.drain();
	assert!(facts.nodes.is_empty());
	assert!(facts.edges.is_empty());
}

// ── Top-level call is suppressed (SB-3-pre defect 1 fix) ──────────

#[test]
fn top_level_fs_call_produces_no_state_boundary_edge() {
	let source = r#"
import { readFile } from "fs";
readFile("/etc/top", () => {});
"#;
	let result = run_extractor(source, "src/top.ts");
	// Top-level call is suppressed at the ts-extractor layer
	// because the caller is the FILE node, not a symbol.
	assert!(result.resolved_callsites.is_empty());

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	let count =
		emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();
	assert_eq!(count, 0);
}

// ── Multiple distinct FS call sites in one function ───────────────

#[test]
fn mixed_read_and_write_in_same_function() {
	let source = r#"
import { readFileSync, writeFileSync } from "fs";
export function copy() {
  const data = readFileSync("/src/in");
  writeFileSync("/dst/out", data);
}
"#;
	let result = run_extractor(source, "src/copy.ts");
	assert_eq!(result.resolved_callsites.len(), 2);

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();

	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 2);
	assert_eq!(facts.edges.len(), 2);

	let reads_edge = facts
		.edges
		.iter()
		.find(|e| e.edge_type == EdgeType::Reads)
		.expect("reads edge");
	let writes_edge = facts
		.edges
		.iter()
		.find(|e| e.edge_type == EdgeType::Writes)
		.expect("writes edge");

	let read_target = facts
		.nodes
		.iter()
		.find(|n| n.stable_key == reads_edge.target_key)
		.unwrap();
	let write_target = facts
		.nodes
		.iter()
		.find(|n| n.stable_key == writes_edge.target_key)
		.unwrap();
	assert_eq!(read_target.name, "/src/in");
	assert_eq!(write_target.name, "/dst/out");
}

// ── URL-shaped FS literal → normalized_url in evidence ───────────

#[test]
fn uri_style_fs_literal_emits_normalized_url_evidence() {
	// A `file:///...` literal must be classified as
	// `normalized_url` by the adapter, not `normalized_path`.
	// This pins the evidence JSON on the emitted edge.
	let source = r#"
import { readFile } from "fs";
export function load() {
  readFile("file:///etc/config", () => {});
}
"#;
	let result = run_extractor(source, "src/load.ts");
	assert_eq!(result.resolved_callsites.len(), 1);

	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();

	let facts = emitter.drain();
	assert_eq!(facts.edges.len(), 1);

	let md = facts.edges[0]
		.metadata_json
		.as_ref()
		.expect("edge must carry evidence JSON");
	assert!(
		md.contains("\"logical_name_source\":\"normalized_url\""),
		"URI literal must be classified as normalized_url, got metadata: {}",
		md
	);
	// Sanity: stable key preserves the URL payload verbatim.
	assert_eq!(
		facts.nodes[0].stable_key,
		"myservice:fs:file:///etc/config:FS_PATH"
	);
}

#[test]
fn path_shaped_fs_literal_stays_normalized_path() {
	// Regression guard: a plain path like `/etc/config` must
	// stay `normalized_path` now that the adapter discriminates.
	let source = r#"
import { readFile } from "fs";
export function load() {
  readFile("/etc/config", () => {});
}
"#;
	let result = run_extractor(source, "src/load.ts");
	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();

	let facts = emitter.drain();
	let md = facts.edges[0].metadata_json.as_ref().unwrap();
	assert!(
		md.contains("\"logical_name_source\":\"normalized_path\""),
		"plain path must stay normalized_path, got: {}",
		md
	);
}

// ── Enclosing-symbol linkage ──────────────────────────────────────

#[test]
fn edge_source_is_enclosing_function_symbol() {
	let source = r#"
import { readFile } from "fs";
export function load() {
  readFile("/etc/x", () => {});
}
"#;
	let result = run_extractor(source, "src/load.ts");
	let table = BindingTable::load_embedded();
	let mut emitter = new_emitter(table);
	emit_from_resolved_callsites(&result.resolved_callsites, &mut emitter).unwrap();

	let load_fn = result
		.nodes
		.iter()
		.find(|n| n.name == "load")
		.expect("load function node");
	let facts = emitter.drain();
	assert_eq!(facts.edges[0].source_node_uid, load_fn.node_uid);
}
