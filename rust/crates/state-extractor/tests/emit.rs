//! Integration tests for `StateBoundaryEmitter`.
//!
//! Exercise the full emission pipeline against synthetic fixtures:
//! binding-table loader → Form-A match → stable-key builder →
//! node dedup → edge emission → evidence serialization.
//!
//! Scope is SB-2: no language integration, no real extractor. The
//! callsite inputs are constructed directly to pin the emitter's
//! contract behavior.

use repo_graph_classification::types::SourceLocation;
use repo_graph_indexer::types::{EdgeType, NodeKind, NodeSubtype, Resolution};
use repo_graph_state_bindings::{
	table::ResourceKind, BindingTable, CalleePath, FsPathOrLogical, ImportView, Language,
	LogicalName, RepoUid,
};
use repo_graph_state_extractor::{
	CallsiteLogicalName, EmitError, EmitterContext, LogicalNameSource,
	StateBoundaryCallsite, StateBoundaryEmitter, STATE_BOUNDARY_EVIDENCE_VERSION,
};

const SOURCE_EXTRACTOR_NAME: &str = "state-extractor:0.1.0";

// ── Helpers ──────────────────────────────────────────────────────

fn synthetic_table() -> BindingTable {
	// Four bindings covering Read, Write, ReadWrite across the
	// four resource kinds in slice 1.
	BindingTable::load_str(
		r#"
[[binding]]
language      = "typescript"
module        = "fs"
symbol_path   = "readFile"
resource_kind = "fs"
driver        = "node-fs"
direction     = "read"
basis         = "stdlib_api"

[[binding]]
language      = "typescript"
module        = "@aws-sdk/client-s3"
symbol_path   = "PutObjectCommand"
resource_kind = "blob"
driver        = "s3"
direction     = "write"
basis         = "sdk_call"

[[binding]]
language      = "typescript"
module        = "pg"
symbol_path   = "Client.query"
resource_kind = "db"
driver        = "postgres"
direction     = "read_write"
basis         = "sdk_call"

[[binding]]
language      = "typescript"
module        = "redis"
symbol_path   = "createClient"
resource_kind = "cache"
driver        = "redis"
direction     = "read_write"
basis         = "sdk_call"
"#,
	)
	.expect("synthetic table must parse")
}

fn loc(line: i64) -> SourceLocation {
	SourceLocation {
		line_start: line,
		col_start: 0,
		line_end: line,
		col_end: 1,
	}
}

/// Callsite with a `Generic(LogicalName)` payload. Appropriate
/// for DB / Cache / Blob / colon-free FS cases.
fn callsite_generic(
	source_node_uid: &str,
	module: &str,
	symbol: &str,
	logical: &str,
	source: LogicalNameSource,
	line: i64,
) -> StateBoundaryCallsite {
	StateBoundaryCallsite {
		source_node_uid: source_node_uid.to_string(),
		file_uid: "myservice:src/handler.ts".to_string(),
		source_location: loc(line),
		imports_in_file: vec![ImportView {
			module_path: module.to_string(),
			imported_symbol: symbol.to_string(),
			import_alias: None,
		}],
		callee: CalleePath {
			resolved_module: Some(module.to_string()),
			resolved_symbol: symbol.to_string(),
		},
		logical_name: CallsiteLogicalName::Generic(
			LogicalName::new(logical.to_string()).unwrap(),
		),
		logical_name_source: source,
	}
}

/// Callsite with an `Fs(FsPathOrLogical)` payload. Appropriate
/// for FS cases that may contain `:` (Windows paths, URI-style
/// references).
fn callsite_fs(
	source_node_uid: &str,
	module: &str,
	symbol: &str,
	fs_payload: &str,
	source: LogicalNameSource,
	line: i64,
) -> StateBoundaryCallsite {
	StateBoundaryCallsite {
		source_node_uid: source_node_uid.to_string(),
		file_uid: "myservice:src/handler.ts".to_string(),
		source_location: loc(line),
		imports_in_file: vec![ImportView {
			module_path: module.to_string(),
			imported_symbol: symbol.to_string(),
			import_alias: None,
		}],
		callee: CalleePath {
			resolved_module: Some(module.to_string()),
			resolved_symbol: symbol.to_string(),
		},
		logical_name: CallsiteLogicalName::Fs(
			FsPathOrLogical::new(fs_payload.to_string()).unwrap(),
		),
		logical_name_source: source,
	}
}

fn context() -> EmitterContext {
	EmitterContext {
		repo_uid: RepoUid::new("myservice".to_string()).unwrap(),
		snapshot_uid: "snap-1".to_string(),
		language: Language::Typescript,
		extractor_name: SOURCE_EXTRACTOR_NAME.to_string(),
	}
}

// ── Read binding: one READS edge, one resource node ──────────────

#[test]
fn read_binding_emits_single_reads_edge_and_resource_node() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_generic(
		"sym-1",
		"fs",
		"readFile",
		"/etc/app/settings.yaml",
		LogicalNameSource::NormalizedPath,
		10,
	);

	let emitted = emitter.emit_for_callsite(&cs).unwrap();
	assert_eq!(emitted, 1);

	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(facts.edges.len(), 1);

	let node = &facts.nodes[0];
	assert_eq!(
		node.stable_key,
		"myservice:fs:/etc/app/settings.yaml:FS_PATH"
	);
	assert_eq!(node.kind, NodeKind::FsPath);
	assert_eq!(node.subtype, Some(NodeSubtype::FilePath));
	assert_eq!(node.name, "/etc/app/settings.yaml");

	let edge = &facts.edges[0];
	assert_eq!(edge.source_node_uid, "sym-1");
	assert_eq!(edge.target_key, node.stable_key);
	assert_eq!(edge.edge_type, EdgeType::Reads);
	assert_eq!(edge.resolution, Resolution::Static);
	assert_eq!(edge.extractor, SOURCE_EXTRACTOR_NAME);
	assert_eq!(edge.location, Some(loc(10)));

	let md = edge.metadata_json.as_ref().unwrap();
	assert!(md.contains("\"state_boundary_version\":1"));
	assert!(md.contains("\"basis\":\"stdlib_api\""));
	assert!(md.contains("\"binding_key\":\"typescript:fs:readFile:read\""));
	assert!(md.contains("\"direction\":\"read\""));
	assert!(md.contains("\"logical_name_source\":\"normalized_path\""));
}

// ── Write binding: one WRITES edge ────────────────────────────────

#[test]
fn write_binding_emits_single_writes_edge() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_generic(
		"sym-2",
		"@aws-sdk/client-s3",
		"PutObjectCommand",
		"artifacts-bucket",
		LogicalNameSource::LiteralIdentifier,
		20,
	);

	assert_eq!(emitter.emit_for_callsite(&cs).unwrap(), 1);
	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(facts.edges.len(), 1);

	let node = &facts.nodes[0];
	assert_eq!(node.stable_key, "myservice:blob:s3:artifacts-bucket:BLOB");
	assert_eq!(node.kind, NodeKind::Blob);
	assert_eq!(node.subtype, Some(NodeSubtype::Bucket));

	let edge = &facts.edges[0];
	assert_eq!(edge.edge_type, EdgeType::Writes);
	assert_eq!(edge.resolution, Resolution::Static);
}

// ── ReadWrite binding: two edges, same resource node ─────────────

#[test]
fn read_write_binding_emits_both_edges_for_same_resource() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_generic(
		"sym-3",
		"pg",
		"Client.query",
		"DATABASE_URL",
		LogicalNameSource::EnvKey,
		30,
	);

	assert_eq!(emitter.emit_for_callsite(&cs).unwrap(), 2);
	let facts = emitter.drain();

	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(facts.edges.len(), 2);

	let node = &facts.nodes[0];
	assert_eq!(
		node.stable_key,
		"myservice:db:postgres:DATABASE_URL:DB_RESOURCE"
	);
	assert_eq!(node.kind, NodeKind::DbResource);
	assert_eq!(node.subtype, Some(NodeSubtype::Connection));

	let edge_types: Vec<EdgeType> = facts.edges.iter().map(|e| e.edge_type).collect();
	assert!(edge_types.contains(&EdgeType::Reads));
	assert!(edge_types.contains(&EdgeType::Writes));

	for e in &facts.edges {
		assert_eq!(e.target_key, node.stable_key);
		assert_eq!(e.source_node_uid, "sym-3");
		assert_eq!(e.resolution, Resolution::Inferred);
	}

	let reads_edge = facts
		.edges
		.iter()
		.find(|e| e.edge_type == EdgeType::Reads)
		.unwrap();
	let writes_edge = facts
		.edges
		.iter()
		.find(|e| e.edge_type == EdgeType::Writes)
		.unwrap();
	assert!(reads_edge
		.metadata_json
		.as_ref()
		.unwrap()
		.contains("\"direction\":\"read\""));
	assert!(writes_edge
		.metadata_json
		.as_ref()
		.unwrap()
		.contains("\"direction\":\"write\""));

	for e in &facts.edges {
		assert!(e
			.metadata_json
			.as_ref()
			.unwrap()
			.contains("\"binding_key\":\"typescript:pg:Client.query:read_write\""));
	}
}

// ── Dedup: two call sites touching the same resource ─────────────

#[test]
fn two_call_sites_to_same_resource_dedup_to_one_node() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());

	let cs1 = callsite_generic(
		"sym-A",
		"@aws-sdk/client-s3",
		"PutObjectCommand",
		"reports-prod",
		LogicalNameSource::LiteralIdentifier,
		10,
	);
	let cs2 = callsite_generic(
		"sym-B",
		"@aws-sdk/client-s3",
		"PutObjectCommand",
		"reports-prod",
		LogicalNameSource::LiteralIdentifier,
		42,
	);

	assert_eq!(emitter.emit_for_callsite(&cs1).unwrap(), 1);
	assert_eq!(emitter.emit_for_callsite(&cs2).unwrap(), 1);

	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(
		facts.nodes[0].stable_key,
		"myservice:blob:s3:reports-prod:BLOB"
	);
	assert_eq!(facts.edges.len(), 2);
	assert_eq!(facts.edges[0].target_key, facts.edges[1].target_key);
	let sources: Vec<&str> = facts.edges.iter().map(|e| e.source_node_uid.as_str()).collect();
	assert!(sources.contains(&"sym-A"));
	assert!(sources.contains(&"sym-B"));
}

#[test]
fn different_resources_within_same_kind_are_not_deduped() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());

	let cs1 = callsite_generic(
		"sym-1",
		"@aws-sdk/client-s3",
		"PutObjectCommand",
		"bucket-a",
		LogicalNameSource::LiteralIdentifier,
		10,
	);
	let cs2 = callsite_generic(
		"sym-2",
		"@aws-sdk/client-s3",
		"PutObjectCommand",
		"bucket-b",
		LogicalNameSource::LiteralIdentifier,
		20,
	);

	emitter.emit_for_callsite(&cs1).unwrap();
	emitter.emit_for_callsite(&cs2).unwrap();
	let facts = emitter.drain();

	assert_eq!(facts.nodes.len(), 2);
	assert_eq!(facts.edges.len(), 2);
}

// ── No match: no emission ─────────────────────────────────────────

#[test]
fn unmatched_callsite_emits_nothing() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());

	let cs = StateBoundaryCallsite {
		source_node_uid: "sym-1".to_string(),
		file_uid: "myservice:src/handler.ts".to_string(),
		source_location: loc(1),
		imports_in_file: vec![ImportView {
			module_path: "lodash".to_string(),
			imported_symbol: "map".to_string(),
			import_alias: None,
		}],
		callee: CalleePath {
			resolved_module: Some("lodash".to_string()),
			resolved_symbol: "map".to_string(),
		},
		logical_name: CallsiteLogicalName::Generic(LogicalName::new("irrelevant").unwrap()),
		logical_name_source: LogicalNameSource::LiteralIdentifier,
	};

	assert_eq!(emitter.emit_for_callsite(&cs).unwrap(), 0);
	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 0);
	assert_eq!(facts.edges.len(), 0);
}

#[test]
fn cross_language_match_is_suppressed() {
	let table = synthetic_table();
	let mut ctx = context();
	ctx.language = Language::Python;
	let mut emitter = StateBoundaryEmitter::new(&table, ctx);

	let cs = callsite_generic(
		"sym-1",
		"fs",
		"readFile",
		"/etc/app/settings.yaml",
		LogicalNameSource::NormalizedPath,
		10,
	);
	assert_eq!(emitter.emit_for_callsite(&cs).unwrap(), 0);
	assert!(emitter.drain().nodes.is_empty());
}

// ── FS subtype inference ──────────────────────────────────────────

#[test]
fn fs_env_derived_logical_name_gets_logical_subtype() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());

	let cs = callsite_generic(
		"sym-1",
		"fs",
		"readFile",
		"CACHE_DIR",
		LogicalNameSource::EnvKey,
		10,
	);
	emitter.emit_for_callsite(&cs).unwrap();
	let facts = emitter.drain();

	let node = &facts.nodes[0];
	assert_eq!(node.kind, NodeKind::FsPath);
	assert_eq!(node.subtype, Some(NodeSubtype::Logical));
	assert_eq!(facts.edges[0].resolution, Resolution::Inferred);
}

#[test]
fn fs_literal_identifier_also_gets_logical_subtype() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_generic(
		"sym-1",
		"fs",
		"readFile",
		"stable-cache-name",
		LogicalNameSource::LiteralIdentifier,
		10,
	);
	emitter.emit_for_callsite(&cs).unwrap();
	let facts = emitter.drain();
	assert_eq!(facts.nodes[0].subtype, Some(NodeSubtype::Logical));
	assert_eq!(facts.edges[0].resolution, Resolution::Static);
}

// ── Cache ReadWrite: both edges point at same resource ───────────

#[test]
fn cache_read_write_binding_dedups_target() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_generic(
		"sym-1",
		"redis",
		"createClient",
		"REDIS_URL",
		LogicalNameSource::EnvKey,
		10,
	);
	emitter.emit_for_callsite(&cs).unwrap();
	let facts = emitter.drain();

	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(facts.nodes[0].kind, NodeKind::State);
	assert_eq!(facts.nodes[0].subtype, Some(NodeSubtype::Cache));
	assert_eq!(
		facts.nodes[0].stable_key,
		"myservice:cache:redis:REDIS_URL:STATE"
	);

	assert_eq!(facts.edges.len(), 2);
	for e in &facts.edges {
		assert_eq!(e.target_key, facts.nodes[0].stable_key);
	}
}

// ── Evidence version pin ─────────────────────────────────────────

#[test]
fn evidence_version_is_one() {
	assert_eq!(STATE_BOUNDARY_EVIDENCE_VERSION, 1);
}

// ══════════════════════════════════════════════════════════════════
//  P2-a coverage: CallsiteLogicalName variant handling
// ══════════════════════════════════════════════════════════════════
//
// The Fs / Generic enum variants enable colon-bearing FS payloads
// (Windows drive letters, URI-style references) which the contract
// §5.1 parsing-semantics note explicitly permits. Non-FS kinds
// remain naive-split-safe: `Fs(..)` payloads that contain `:` and
// match a non-FS binding must fail with a typed EmitError.

#[test]
fn fs_callsite_with_windows_drive_letter_succeeds() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_fs(
		"sym-1",
		"fs",
		"readFile",
		"C:\\Windows\\path",
		LogicalNameSource::NormalizedPath,
		10,
	);
	emitter.emit_for_callsite(&cs).unwrap();
	let facts = emitter.drain();

	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(facts.edges.len(), 1);

	let node = &facts.nodes[0];
	assert_eq!(node.kind, NodeKind::FsPath);
	assert_eq!(node.subtype, Some(NodeSubtype::FilePath));
	assert_eq!(node.name, "C:\\Windows\\path");
	assert_eq!(node.stable_key, "myservice:fs:C:\\Windows\\path:FS_PATH");

	let edge = &facts.edges[0];
	assert_eq!(edge.target_key, node.stable_key);
	assert_eq!(edge.edge_type, EdgeType::Reads);
}

#[test]
fn fs_callsite_with_uri_style_reference_succeeds() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_fs(
		"sym-1",
		"fs",
		"readFile",
		"file:///etc/config",
		LogicalNameSource::NormalizedUrl,
		10,
	);
	emitter.emit_for_callsite(&cs).unwrap();
	let facts = emitter.drain();

	let node = &facts.nodes[0];
	assert_eq!(node.stable_key, "myservice:fs:file:///etc/config:FS_PATH");
	assert_eq!(node.name, "file:///etc/config");
}

#[test]
fn fs_binding_accepts_generic_variant_via_upgrade() {
	// Generic(LogicalName) payload routed to an FS binding. The
	// emitter upgrades to FsPathOrLogical; since LogicalName's
	// invariants are a strict subset, the upgrade succeeds.
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_generic(
		"sym-1",
		"fs",
		"readFile",
		"/etc/colon-free/path",
		LogicalNameSource::NormalizedPath,
		10,
	);
	emitter.emit_for_callsite(&cs).unwrap();
	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(
		facts.nodes[0].stable_key,
		"myservice:fs:/etc/colon-free/path:FS_PATH"
	);
}

#[test]
fn blob_binding_accepts_fs_variant_when_payload_has_no_colon() {
	// Fs(FsPathOrLogical) payload with no `:` routed to a BLOB
	// binding. The emitter downgrades to LogicalName; since the
	// payload is colon-free, the downgrade succeeds.
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_fs(
		"sym-1",
		"@aws-sdk/client-s3",
		"PutObjectCommand",
		"clean-bucket-name",
		LogicalNameSource::LiteralIdentifier,
		10,
	);
	emitter.emit_for_callsite(&cs).unwrap();
	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(
		facts.nodes[0].stable_key,
		"myservice:blob:s3:clean-bucket-name:BLOB"
	);
}

#[test]
fn blob_binding_rejects_fs_variant_with_colon_bearing_payload() {
	// Fs(FsPathOrLogical) with `:` routed to a BLOB binding. The
	// emitter attempts downgrade to LogicalName, which fails the
	// no-`:` invariant. Expected: FsPayloadInNonFsSlot error with
	// correct resource_kind and payload fields.
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_fs(
		"sym-1",
		"@aws-sdk/client-s3",
		"PutObjectCommand",
		"weird:payload:with:colons",
		LogicalNameSource::LiteralIdentifier,
		10,
	);
	match emitter.emit_for_callsite(&cs) {
		Err(EmitError::FsPayloadInNonFsSlot {
			resource_kind,
			payload,
		}) => {
			assert_eq!(resource_kind, ResourceKind::Blob);
			assert_eq!(payload, "weird:payload:with:colons");
		}
		other => panic!("expected FsPayloadInNonFsSlot, got {:?}", other),
	}
	// No state was committed to the emitter on error.
	let facts = emitter.drain();
	assert_eq!(facts.nodes.len(), 0);
	assert_eq!(facts.edges.len(), 0);
}

#[test]
fn db_binding_rejects_fs_variant_with_colon_bearing_payload() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_fs(
		"sym-1",
		"pg",
		"Client.query",
		"weird:db:name",
		LogicalNameSource::LiteralIdentifier,
		10,
	);
	match emitter.emit_for_callsite(&cs) {
		Err(EmitError::FsPayloadInNonFsSlot { resource_kind, .. }) => {
			assert_eq!(resource_kind, ResourceKind::Db);
		}
		other => panic!("expected FsPayloadInNonFsSlot, got {:?}", other),
	}
}

#[test]
fn cache_binding_rejects_fs_variant_with_colon_bearing_payload() {
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());
	let cs = callsite_fs(
		"sym-1",
		"redis",
		"createClient",
		"a:b",
		LogicalNameSource::LiteralIdentifier,
		10,
	);
	match emitter.emit_for_callsite(&cs) {
		Err(EmitError::FsPayloadInNonFsSlot { resource_kind, .. }) => {
			assert_eq!(resource_kind, ResourceKind::Cache);
		}
		other => panic!("expected FsPayloadInNonFsSlot, got {:?}", other),
	}
}

#[test]
fn dedup_works_across_variant_choices_for_the_same_fs_resource() {
	// Two FS call sites against the same path — one via Generic
	// (upgrade path), one via Fs (direct). Both should dedup to
	// the same resource node.
	let table = synthetic_table();
	let mut emitter = StateBoundaryEmitter::new(&table, context());

	let cs1 = callsite_generic(
		"sym-A",
		"fs",
		"readFile",
		"/etc/settings.yaml",
		LogicalNameSource::NormalizedPath,
		10,
	);
	let cs2 = callsite_fs(
		"sym-B",
		"fs",
		"readFile",
		"/etc/settings.yaml",
		LogicalNameSource::NormalizedPath,
		20,
	);

	emitter.emit_for_callsite(&cs1).unwrap();
	emitter.emit_for_callsite(&cs2).unwrap();
	let facts = emitter.drain();

	// Same stable key from either variant → single node.
	assert_eq!(facts.nodes.len(), 1);
	assert_eq!(
		facts.nodes[0].stable_key,
		"myservice:fs:/etc/settings.yaml:FS_PATH"
	);
	assert_eq!(facts.edges.len(), 2);
	for e in &facts.edges {
		assert_eq!(e.target_key, facts.nodes[0].stable_key);
	}
}
