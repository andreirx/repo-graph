//! Edge resolution policy — pure deterministic resolution of
//! symbolic edge targets to concrete node UIDs.
//!
//! Mirror of the resolution logic in
//! `src/adapters/indexer/repo-indexer.ts` (lines 2087–2366).
//!
//! All functions are PURE. No I/O, no storage access. The caller
//! builds the `ResolverIndex` from data fetched through the
//! storage port and passes it in.
//!
//! Resolution strategies by edge type:
//!   - IMPORTS: stable-key lookup → file-resolution map → C/C++
//!     per-TU includes → repo-prefix fallback
//!   - CALLS: dotted-name method extraction → name lookup →
//!     import-binding-assisted file narrowing
//!   - INSTANTIATES: name lookup filtered to CLASS subtype
//!   - IMPLEMENTS: name lookup filtered to INTERFACE subtype
//!   - Other: unfiltered name lookup

use std::collections::HashMap;

use repo_graph_classification::types::{ImportBinding, SourceLocation};

use crate::types::{EdgeType, ExtractedEdge, Resolution};

// ── Resolver types ───────────────────────────────────────────────

/// Slim node for resolution and affinity filtering. Mirrors
/// `ResolverNode` from `src/core/ports/storage.ts:1115`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverNode {
	pub node_uid: String,
	pub stable_key: String,
	pub name: String,
	pub qualified_name: Option<String>,
	pub kind: String,
	pub subtype: Option<String>,
	pub file_uid: Option<String>,
}

/// The in-memory index used for edge resolution. Built from
/// fetched data before resolution begins.
///
/// All maps use `HashMap` for O(1) lookup in the resolution hot
/// path. These are internal algorithm state, not public API
/// surfaces (the no-HashMap rule applies to public DTOs only).
pub struct ResolverIndex {
	/// Direct stable-key → node lookup.
	pub nodes_by_stable_key: HashMap<String, ResolverNode>,
	/// Name → all nodes with that name (may be ambiguous).
	pub nodes_by_name: HashMap<String, Vec<ResolverNode>>,
	/// Source node UID → file UID (for import-binding and
	/// include-path resolution).
	pub node_uid_to_file_uid: HashMap<String, String>,
	/// Extensionless stable-key → full stable-key with extension.
	pub file_resolution: HashMap<String, String>,
	/// Per-TU C/C++ include → header stable-key.
	/// Outer key is source file UID.
	pub per_file_include_resolution: HashMap<String, HashMap<String, String>>,
	/// Stable-key → node UID (for module-edge creation).
	pub stable_key_to_uid: HashMap<String, String>,
	/// File node UID → module stable-key (for module-edge creation).
	pub file_to_module: HashMap<String, String>,
}

/// A resolved edge — the symbolic `target_key` has been replaced
/// with a concrete `target_node_uid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEdge {
	pub edge_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub source_node_uid: String,
	pub target_node_uid: String,
	pub edge_type: EdgeType,
	pub resolution: Resolution,
	pub extractor: String,
	pub location: Option<SourceLocation>,
	pub metadata_json: Option<String>,
}

/// An unresolved edge with its assigned failure category.
#[derive(Debug, Clone)]
pub struct CategorizedUnresolvedEdge {
	pub edge: ExtractedEdge,
	pub category: repo_graph_classification::types::UnresolvedEdgeCategory,
	pub source_file_uid: Option<String>,
}

/// Result of resolving a batch of edges.
pub struct ResolutionResult {
	pub resolved: Vec<ResolvedEdge>,
	pub still_unresolved: Vec<CategorizedUnresolvedEdge>,
	/// (source_node_uid, target_node_uid) pairs for resolved
	/// IMPORTS edges. Used by the orchestrator to derive
	/// MODULE → MODULE import edges.
	pub resolved_import_pairs: Vec<(String, String)>,
}

// ── Subtypes that are type-only (not value-space) ────────────────

/// Mirror of `TYPE_ONLY_SUBTYPES` from `repo-indexer.ts:3028`.
fn is_type_only_subtype(subtype: Option<&str>) -> bool {
	matches!(subtype, Some("TYPE_ALIAS") | Some("INTERFACE"))
}

// ── Resolution entry point ───────────────────────────────────────

/// Resolve a batch of unresolved edges against the resolver index.
///
/// For each edge, attempts to resolve the symbolic `target_key` to
/// a concrete `target_node_uid`. Successfully resolved edges are
/// returned as `ResolvedEdge`; failures are categorized and
/// returned as `CategorizedUnresolvedEdge`.
///
/// Mirror of `resolveEdges` from `repo-indexer.ts:2087`.
pub fn resolve_edges(
	edges: &[ExtractedEdge],
	index: &ResolverIndex,
	import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
) -> ResolutionResult {
	let mut resolved = Vec::new();
	let mut still_unresolved = Vec::new();
	let mut resolved_import_pairs = Vec::new();

	for edge in edges {
		let target_node_uid = resolve_target(edge, index, import_bindings_by_file);

		if let Some(uid) = target_node_uid {
			if edge.edge_type == EdgeType::Imports {
				resolved_import_pairs
					.push((edge.source_node_uid.clone(), uid.clone()));
			}
			resolved.push(ResolvedEdge {
				edge_uid: edge.edge_uid.clone(),
				snapshot_uid: edge.snapshot_uid.clone(),
				repo_uid: edge.repo_uid.clone(),
				source_node_uid: edge.source_node_uid.clone(),
				target_node_uid: uid,
				edge_type: edge.edge_type,
				resolution: edge.resolution,
				extractor: edge.extractor.clone(),
				location: edge.location,
				metadata_json: edge.metadata_json.clone(),
			});
		} else {
			let category = categorize_unresolved_edge(edge);
			let source_file_uid = index
				.node_uid_to_file_uid
				.get(&edge.source_node_uid)
				.cloned();
			still_unresolved.push(CategorizedUnresolvedEdge {
				edge: edge.clone(),
				category,
				source_file_uid,
			});
		}
	}

	ResolutionResult {
		resolved,
		still_unresolved,
		resolved_import_pairs,
	}
}

// ── Per-edge dispatch ────────────────────────────────────────────

fn resolve_target(
	edge: &ExtractedEdge,
	index: &ResolverIndex,
	import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
) -> Option<String> {
	match edge.edge_type {
		EdgeType::Imports => {
			let source_file_uid = index
				.node_uid_to_file_uid
				.get(&edge.source_node_uid);
			let tu_includes = source_file_uid
				.and_then(|fuid| index.per_file_include_resolution.get(fuid));
			resolve_import_target(
				&edge.target_key,
				&index.nodes_by_stable_key,
				&index.file_resolution,
				&edge.repo_uid,
				tu_includes,
			)
		}
		EdgeType::Calls => resolve_call_target(
			&edge.target_key,
			&edge.source_node_uid,
			&index.nodes_by_stable_key,
			&index.nodes_by_name,
			&index.file_resolution,
			import_bindings_by_file,
			&index.node_uid_to_file_uid,
		),
		EdgeType::Instantiates => {
			resolve_named_target(&edge.target_key, &index.nodes_by_name, edge.edge_type)
		}
		EdgeType::Implements => {
			resolve_named_target(&edge.target_key, &index.nodes_by_name, edge.edge_type)
		}
		_ => resolve_named_target(&edge.target_key, &index.nodes_by_name, edge.edge_type),
	}
}

// ── IMPORTS resolution ───────────────────────────────────────────

fn resolve_import_target(
	target_key: &str,
	nodes_by_stable_key: &HashMap<String, ResolverNode>,
	file_resolution: &HashMap<String, String>,
	repo_uid: &str,
	tu_include_resolution: Option<&HashMap<String, String>>,
) -> Option<String> {
	// Stage 1: direct stable-key lookup.
	if let Some(node) = nodes_by_stable_key.get(target_key) {
		return Some(node.node_uid.clone());
	}

	// Stage 2: extensionless → with-extension via file resolution map.
	if let Some(resolved_key) = file_resolution.get(target_key) {
		if let Some(node) = nodes_by_stable_key.get(resolved_key) {
			return Some(node.node_uid.clone());
		}
	}

	// Stage 3: C/C++ per-TU include resolution.
	if let Some(tu_map) = tu_include_resolution {
		if let Some(resolved_header) = tu_map.get(target_key) {
			if let Some(node) = nodes_by_stable_key.get(resolved_header) {
				return Some(node.node_uid.clone());
			}
		}
	}

	// Stage 4: repo-prefix fallback for bare header names.
	if !target_key.contains(':') {
		let constructed_key = format!("{}:{}:FILE", repo_uid, target_key);
		if let Some(resolved) = file_resolution.get(&constructed_key) {
			if let Some(node) = nodes_by_stable_key.get(resolved) {
				return Some(node.node_uid.clone());
			}
		}
		if let Some(node) = nodes_by_stable_key.get(&constructed_key) {
			return Some(node.node_uid.clone());
		}
	}

	None
}

// ── CALLS resolution ─────────────────────────────────────────────

fn resolve_call_target(
	target_key: &str,
	source_node_uid: &str,
	_nodes_by_stable_key: &HashMap<String, ResolverNode>,
	nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
	file_resolution: &HashMap<String, String>,
	import_bindings_by_file: Option<&HashMap<String, Vec<ImportBinding>>>,
	node_uid_to_file_uid: &HashMap<String, String>,
) -> Option<String> {
	// Dotted name: extract method name from "obj.method" or
	// "this.field.method" patterns.
	if target_key.contains('.') {
		let parts: Vec<&str> = target_key.split('.').collect();
		let method_name = parts[parts.len() - 1];

		// "this.repo.findById" style (3+ parts starting with "this")
		if parts[0] == "this" && parts.len() >= 3 {
			if let Some(uid) =
				pick_unambiguous(nodes_by_name.get(method_name), EdgeType::Calls)
			{
				return Some(uid);
			}
		}

		// "obj.method()" — try method name.
		if let Some(uid) =
			pick_unambiguous(nodes_by_name.get(method_name), EdgeType::Calls)
		{
			return Some(uid);
		}
	}

	// Simple function call: "classifyMedia".
	if let Some(uid) =
		pick_unambiguous(nodes_by_name.get(target_key), EdgeType::Calls)
	{
		return Some(uid);
	}

	// Import-binding-assisted resolution: if the identifier was
	// imported in the source file, narrow lookup to that module.
	if let Some(bindings_map) = import_bindings_by_file {
		if let Some(source_file_uid) = node_uid_to_file_uid.get(source_node_uid) {
			if let Some(bindings) = bindings_map.get(source_file_uid) {
				if let Some(binding) = bindings.iter().find(|b| b.identifier == target_key) {
					if let Some(resolved_file_uid) = resolve_import_specifier_to_file(
						&binding.specifier,
						source_file_uid,
						file_resolution,
					) {
						if let Some(candidates) = nodes_by_name.get(target_key) {
							let in_file: Vec<ResolverNode> = candidates
								.iter()
								.filter(|n| {
									n.file_uid.as_deref() == Some(resolved_file_uid.as_str())
								})
								.cloned()
								.collect();
							if let Some(uid) = pick_unambiguous(
								Some(&in_file),
								EdgeType::Calls,
							) {
								return Some(uid);
							}
						}
					}
				}
			}
		}
	}

	None
}

/// Resolve a relative import specifier to a file UID.
///
/// Mirror of `resolveImportSpecifierToFile` from
/// `repo-indexer.ts:2327`.
fn resolve_import_specifier_to_file(
	specifier: &str,
	source_file_uid: &str,
	file_resolution: &HashMap<String, String>,
) -> Option<String> {
	if !specifier.starts_with('.') {
		return None;
	}

	let colon_idx = source_file_uid.find(':')?;
	let repo_uid = &source_file_uid[..colon_idx];
	let source_path = &source_file_uid[colon_idx + 1..];
	let source_dir = match source_path.rfind('/') {
		Some(pos) => &source_path[..pos],
		None => "",
	};

	let resolved_path = resolve_relative_path(source_dir, specifier);
	let target_file_key = format!("{}:{}:FILE", repo_uid, resolved_path);

	if let Some(resolved_stable_key) = file_resolution.get(&target_file_key) {
		// Extract file_uid: "repoUid:path.ts:FILE" → "repoUid:path.ts"
		let file_uid = resolved_stable_key
			.strip_suffix(":FILE")
			.unwrap_or(resolved_stable_key);
		return Some(file_uid.to_string());
	}

	None
}

// ── Named target resolution (INSTANTIATES/IMPLEMENTS/other) ──────

fn resolve_named_target(
	target_key: &str,
	nodes_by_name: &HashMap<String, Vec<ResolverNode>>,
	edge_type: EdgeType,
) -> Option<String> {
	pick_unambiguous(nodes_by_name.get(target_key), edge_type)
}

// ── Affinity filtering + singleton check ─────────────────────────

/// Apply declaration-space affinity filtering then check for an
/// unambiguous singleton result.
///
/// Mirror of `pickUnambiguous` from `repo-indexer.ts:2854`.
fn pick_unambiguous(
	candidates: Option<&Vec<ResolverNode>>,
	edge_type: EdgeType,
) -> Option<String> {
	let candidates = candidates?;
	if candidates.is_empty() {
		return None;
	}

	let filtered = filter_by_edge_affinity(candidates, edge_type);
	if filtered.len() == 1 {
		return Some(filtered[0].node_uid.clone());
	}

	None
}

/// Filter candidates by declaration-space affinity. Returns only
/// candidates in the correct space for the edge type.
///
/// Mirror of `filterByEdgeAffinity` from `repo-indexer.ts:3049`.
pub fn filter_by_edge_affinity(
	candidates: &[ResolverNode],
	edge_type: EdgeType,
) -> Vec<&ResolverNode> {
	match edge_type {
		EdgeType::Instantiates => candidates
			.iter()
			.filter(|n| n.subtype.as_deref() == Some("CLASS"))
			.collect(),
		EdgeType::Implements => candidates
			.iter()
			.filter(|n| n.subtype.as_deref() == Some("INTERFACE"))
			.collect(),
		EdgeType::Calls => candidates
			.iter()
			.filter(|n| !is_type_only_subtype(n.subtype.as_deref()))
			.collect(),
		_ => candidates.iter().collect(),
	}
}

// ── Unresolved edge categorization ───────────────────────────────

/// Assign a failure category to an unresolved edge.
///
/// Mirror of `categorizeUnresolvedEdge` from
/// `repo-indexer.ts:3091`.
pub fn categorize_unresolved_edge(
	edge: &ExtractedEdge,
) -> repo_graph_classification::types::UnresolvedEdgeCategory {
	use repo_graph_classification::types::UnresolvedEdgeCategory;

	match edge.edge_type {
		EdgeType::Imports => UnresolvedEdgeCategory::ImportsFileNotFound,
		EdgeType::Instantiates => UnresolvedEdgeCategory::InstantiatesClassNotFound,
		EdgeType::Implements => UnresolvedEdgeCategory::ImplementsInterfaceNotFound,
		EdgeType::Calls => categorize_unresolved_call(edge),
		_ => UnresolvedEdgeCategory::Other,
	}
}

fn categorize_unresolved_call(
	edge: &ExtractedEdge,
) -> repo_graph_classification::types::UnresolvedEdgeCategory {
	use repo_graph_classification::types::UnresolvedEdgeCategory;

	// Use rawCalleeName from metadata if present (handles rewritten
	// "this.save()" → "ClassName.save" target keys).
	let key = if let Some(ref meta_str) = edge.metadata_json {
		serde_json::from_str::<serde_json::Value>(meta_str)
			.ok()
			.and_then(|v| v.get("rawCalleeName")?.as_str().map(|s| s.to_string()))
			.unwrap_or_else(|| edge.target_key.clone())
	} else {
		edge.target_key.clone()
	};

	if key.starts_with("this.") {
		let dot_count = key.chars().filter(|&c| c == '.').count();
		if dot_count > 1 {
			return UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo;
		}
		return UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext;
	}
	if key.contains('.') {
		return UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo;
	}
	UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
}

// ── Path helpers ─────────────────────────────────────────────────

/// Resolve a relative path specifier against a source directory.
///
/// Mirror of `resolveRelativePath` from `repo-indexer.ts:2945`.
pub fn resolve_relative_path(source_dir: &str, specifier: &str) -> String {
	let mut parts: Vec<&str> = if source_dir.is_empty() {
		Vec::new()
	} else {
		source_dir.split('/').collect()
	};

	for seg in specifier.split('/') {
		if seg == "." {
			continue;
		} else if seg == ".." {
			parts.pop();
		} else {
			parts.push(seg);
		}
	}

	parts.join("/")
}

/// Extract the parent directory path from a file path. Returns
/// `None` for top-level files (no `/` in path).
///
/// Mirror of `getModulePath` from `repo-indexer.ts:2998`.
pub fn get_module_path(file_path: &str) -> Option<&str> {
	let last_slash = file_path.rfind('/')?;
	if last_slash > 0 {
		Some(&file_path[..last_slash])
	} else {
		None
	}
}

// ── File resolution map builder ──────────────────────────────────

/// Build the file resolution map: extensionless stable-key →
/// full stable-key with extension. Also handles index-file
/// directory shortcuts.
///
/// Mirror of the file resolution construction at
/// `repo-indexer.ts:1879`.
pub fn build_file_resolution_map(
	file_paths: &[String],
	repo_uid: &str,
) -> HashMap<String, String> {
	let mut map = HashMap::new();

	for path in file_paths {
		let stable_key = format!("{}:{}:FILE", repo_uid, path);

		// Exact path → self (identity).
		map.insert(stable_key.clone(), stable_key.clone());

		// Extensionless → with extension.
		let without_ext = strip_extension(path);
		let extless_key = format!("{}:{}:FILE", repo_uid, without_ext);
		map.entry(extless_key).or_insert_with(|| stable_key.clone());

		// Index-file directory shortcut:
		// "src/core/index.ts" → "src/core" maps to the index file.
		if path.ends_with("/index.ts") || path.ends_with("/index.tsx") {
			let dir_path = if path.ends_with("/index.tsx") {
				&path[..path.len() - "/index.tsx".len()]
			} else {
				&path[..path.len() - "/index.ts".len()]
			};
			let dir_key = format!("{}:{}:FILE", repo_uid, dir_path);
			map.entry(dir_key).or_insert(stable_key);
		}
	}

	map
}

/// Strip the file extension, but ONLY for the JS/TS family
/// (`.ts`, `.tsx`, `.js`, `.jsx`). Other extensions are left
/// intact. This mirrors the TS `stripExtension` at
/// `repo-indexer.ts:2926` which only strips these four.
///
/// The narrowing is intentional: extensionless import resolution
/// (e.g., `import "./utils"` resolving to `utils.ts`) is a
/// JS/TS ecosystem convention. Other languages (Rust, Python,
/// Java, C/C++) do not use extensionless import specifiers, so
/// synthesizing extensionless keys for `.rs`, `.py`, etc. would
/// create false resolution paths that the TS indexer does not.
fn strip_extension(path: &str) -> &str {
	let dot_pos = match path.rfind('.') {
		Some(p) => p,
		None => return path,
	};
	let ext = &path[dot_pos..];
	match ext {
		".ts" | ".tsx" | ".js" | ".jsx" => &path[..dot_pos],
		_ => path,
	}
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use repo_graph_classification::types::UnresolvedEdgeCategory;

	fn make_node(uid: &str, stable_key: &str, name: &str, subtype: Option<&str>, file_uid: Option<&str>) -> ResolverNode {
		ResolverNode {
			node_uid: uid.into(),
			stable_key: stable_key.into(),
			name: name.into(),
			qualified_name: None,
			kind: "SYMBOL".into(),
			subtype: subtype.map(|s| s.into()),
			file_uid: file_uid.map(|s| s.into()),
		}
	}

	fn make_edge(uid: &str, target_key: &str, edge_type: EdgeType) -> ExtractedEdge {
		ExtractedEdge {
			edge_uid: uid.into(),
			snapshot_uid: "snap1".into(),
			repo_uid: "r1".into(),
			source_node_uid: "src1".into(),
			target_key: target_key.into(),
			edge_type,
			resolution: Resolution::Static,
			extractor: "test:1".into(),
			location: None,
			metadata_json: None,
		}
	}

	// ── filter_by_edge_affinity ──────────────────────────────

	#[test]
	fn affinity_instantiates_filters_to_class() {
		let nodes = vec![
			make_node("n1", "k1", "Foo", Some("CLASS"), None),
			make_node("n2", "k2", "Foo", Some("INTERFACE"), None),
		];
		let filtered = filter_by_edge_affinity(&nodes, EdgeType::Instantiates);
		assert_eq!(filtered.len(), 1);
		assert_eq!(filtered[0].node_uid, "n1");
	}

	#[test]
	fn affinity_implements_filters_to_interface() {
		let nodes = vec![
			make_node("n1", "k1", "Bar", Some("CLASS"), None),
			make_node("n2", "k2", "Bar", Some("INTERFACE"), None),
		];
		let filtered = filter_by_edge_affinity(&nodes, EdgeType::Implements);
		assert_eq!(filtered.len(), 1);
		assert_eq!(filtered[0].node_uid, "n2");
	}

	#[test]
	fn affinity_calls_excludes_type_only() {
		let nodes = vec![
			make_node("n1", "k1", "doStuff", Some("FUNCTION"), None),
			make_node("n2", "k2", "doStuff", Some("TYPE_ALIAS"), None),
			make_node("n3", "k3", "doStuff", Some("INTERFACE"), None),
		];
		let filtered = filter_by_edge_affinity(&nodes, EdgeType::Calls);
		assert_eq!(filtered.len(), 1);
		assert_eq!(filtered[0].node_uid, "n1");
	}

	// ── categorize_unresolved_edge ───────────────────────────

	#[test]
	fn categorize_imports() {
		let edge = make_edge("e1", "./missing", EdgeType::Imports);
		assert_eq!(
			categorize_unresolved_edge(&edge),
			UnresolvedEdgeCategory::ImportsFileNotFound
		);
	}

	#[test]
	fn categorize_calls_this_method() {
		let edge = make_edge("e1", "this.save", EdgeType::Calls);
		assert_eq!(
			categorize_unresolved_edge(&edge),
			UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext
		);
	}

	#[test]
	fn categorize_calls_this_wildcard() {
		let edge = make_edge("e1", "this.repo.findById", EdgeType::Calls);
		assert_eq!(
			categorize_unresolved_edge(&edge),
			UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo
		);
	}

	#[test]
	fn categorize_calls_obj_method() {
		let edge = make_edge("e1", "db.query", EdgeType::Calls);
		assert_eq!(
			categorize_unresolved_edge(&edge),
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo
		);
	}

	#[test]
	fn categorize_calls_function() {
		let edge = make_edge("e1", "classifyMedia", EdgeType::Calls);
		assert_eq!(
			categorize_unresolved_edge(&edge),
			UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
		);
	}

	#[test]
	fn categorize_uses_raw_callee_name_from_metadata() {
		let mut edge = make_edge("e1", "ClassName.save", EdgeType::Calls);
		edge.metadata_json = Some(r#"{"rawCalleeName":"this.save"}"#.into());
		assert_eq!(
			categorize_unresolved_edge(&edge),
			UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext
		);
	}

	// ── resolve_relative_path ────────────────────────────────

	#[test]
	fn relative_path_sibling() {
		assert_eq!(resolve_relative_path("src/core", "./utils"), "src/core/utils");
	}

	#[test]
	fn relative_path_parent() {
		assert_eq!(resolve_relative_path("src/core/api", "../utils"), "src/core/utils");
	}

	#[test]
	fn relative_path_from_root() {
		assert_eq!(resolve_relative_path("", "./src/index"), "src/index");
	}

	// ── get_module_path ──────────────────────────────────────

	#[test]
	fn module_path_nested() {
		assert_eq!(get_module_path("src/core/service.ts"), Some("src/core"));
	}

	#[test]
	fn module_path_top_level() {
		assert_eq!(get_module_path("index.ts"), None);
	}

	// ── build_file_resolution_map ────────────────────────────

	#[test]
	fn file_resolution_extensionless() {
		let paths = vec!["src/core/utils.ts".to_string()];
		let map = build_file_resolution_map(&paths, "r1");
		// Exact key.
		assert_eq!(
			map.get("r1:src/core/utils.ts:FILE"),
			Some(&"r1:src/core/utils.ts:FILE".to_string())
		);
		// Extensionless.
		assert_eq!(
			map.get("r1:src/core/utils:FILE"),
			Some(&"r1:src/core/utils.ts:FILE".to_string())
		);
	}

	#[test]
	fn file_resolution_index_shortcut() {
		let paths = vec!["src/core/index.ts".to_string()];
		let map = build_file_resolution_map(&paths, "r1");
		// Directory shortcut.
		assert_eq!(
			map.get("r1:src/core:FILE"),
			Some(&"r1:src/core/index.ts:FILE".to_string())
		);
	}

	// ── strip_extension ──────────────────────────────────────

	#[test]
	fn strip_ext_ts() {
		assert_eq!(strip_extension("src/core/utils.ts"), "src/core/utils");
	}

	#[test]
	fn strip_ext_tsx() {
		assert_eq!(strip_extension("src/App.tsx"), "src/App");
	}

	#[test]
	fn strip_ext_no_extension() {
		assert_eq!(strip_extension("Makefile"), "Makefile");
	}

	#[test]
	fn strip_ext_preserves_non_jsts_extensions() {
		// Rust, Python, Java, C/C++ extensions are NOT stripped.
		assert_eq!(strip_extension("src/main.rs"), "src/main.rs");
		assert_eq!(strip_extension("src/app.py"), "src/app.py");
		assert_eq!(strip_extension("src/Foo.java"), "src/Foo.java");
		assert_eq!(strip_extension("src/util.h"), "src/util.h");
		assert_eq!(strip_extension("src/util.cpp"), "src/util.cpp");
	}

	// ── resolve_edges integration ────────────────────────────

	#[test]
	fn resolve_import_by_stable_key() {
		let target_node = make_node(
			"file1",
			"r1:src/utils.ts:FILE",
			"utils.ts",
			None,
			None,
		);
		let mut index = ResolverIndex {
			nodes_by_stable_key: HashMap::new(),
			nodes_by_name: HashMap::new(),
			node_uid_to_file_uid: HashMap::new(),
			file_resolution: HashMap::new(),
			per_file_include_resolution: HashMap::new(),
			stable_key_to_uid: HashMap::new(),
			file_to_module: HashMap::new(),
		};
		index
			.nodes_by_stable_key
			.insert(target_node.stable_key.clone(), target_node);

		let edge = make_edge("e1", "r1:src/utils.ts:FILE", EdgeType::Imports);
		let result = resolve_edges(&[edge], &index, None);

		assert_eq!(result.resolved.len(), 1);
		assert_eq!(result.resolved[0].target_node_uid, "file1");
		assert_eq!(result.still_unresolved.len(), 0);
		assert_eq!(result.resolved_import_pairs.len(), 1);
	}

	#[test]
	fn resolve_call_singleton_by_name() {
		let func_node = make_node(
			"fn1",
			"r1:src/utils.ts:classifyMedia:SYMBOL",
			"classifyMedia",
			Some("FUNCTION"),
			Some("r1:src/utils.ts"),
		);
		let mut index = ResolverIndex {
			nodes_by_stable_key: HashMap::new(),
			nodes_by_name: HashMap::new(),
			node_uid_to_file_uid: HashMap::new(),
			file_resolution: HashMap::new(),
			per_file_include_resolution: HashMap::new(),
			stable_key_to_uid: HashMap::new(),
			file_to_module: HashMap::new(),
		};
		index
			.nodes_by_name
			.entry("classifyMedia".into())
			.or_default()
			.push(func_node);

		let edge = make_edge("e1", "classifyMedia", EdgeType::Calls);
		let result = resolve_edges(&[edge], &index, None);

		assert_eq!(result.resolved.len(), 1);
		assert_eq!(result.resolved[0].target_node_uid, "fn1");
	}

	#[test]
	fn ambiguous_name_stays_unresolved() {
		let n1 = make_node("n1", "k1", "doStuff", Some("FUNCTION"), None);
		let n2 = make_node("n2", "k2", "doStuff", Some("FUNCTION"), None);
		let mut index = ResolverIndex {
			nodes_by_stable_key: HashMap::new(),
			nodes_by_name: HashMap::new(),
			node_uid_to_file_uid: HashMap::new(),
			file_resolution: HashMap::new(),
			per_file_include_resolution: HashMap::new(),
			stable_key_to_uid: HashMap::new(),
			file_to_module: HashMap::new(),
		};
		index
			.nodes_by_name
			.entry("doStuff".into())
			.or_default()
			.extend(vec![n1, n2]);

		let edge = make_edge("e1", "doStuff", EdgeType::Calls);
		let result = resolve_edges(&[edge], &index, None);

		assert_eq!(result.resolved.len(), 0);
		assert_eq!(result.still_unresolved.len(), 1);
		assert_eq!(
			result.still_unresolved[0].category,
			UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
		);
	}

	// ── Distinctive fallback branches ────────────────────────

	#[test]
	fn resolve_import_via_extensionless_file_resolution() {
		// The target_key has no extension; file_resolution maps
		// the extensionless key to the full key.
		let target_node = make_node(
			"file1",
			"r1:src/utils.ts:FILE",
			"utils.ts",
			None,
			None,
		);
		let mut index = ResolverIndex {
			nodes_by_stable_key: HashMap::new(),
			nodes_by_name: HashMap::new(),
			node_uid_to_file_uid: HashMap::new(),
			file_resolution: HashMap::new(),
			per_file_include_resolution: HashMap::new(),
			stable_key_to_uid: HashMap::new(),
			file_to_module: HashMap::new(),
		};
		index
			.nodes_by_stable_key
			.insert("r1:src/utils.ts:FILE".into(), target_node);
		// Extensionless → with extension.
		index
			.file_resolution
			.insert("r1:src/utils:FILE".into(), "r1:src/utils.ts:FILE".into());

		let edge = make_edge("e1", "r1:src/utils:FILE", EdgeType::Imports);
		let result = resolve_edges(&[edge], &index, None);

		assert_eq!(result.resolved.len(), 1);
		assert_eq!(result.resolved[0].target_node_uid, "file1");
	}

	#[test]
	fn resolve_import_via_per_tu_include() {
		// C/C++ per-TU include resolution: bare header name
		// resolved through compile_commands.json include paths.
		let header_node = make_node(
			"hdr1",
			"r1:include/util.h:FILE",
			"util.h",
			None,
			None,
		);
		let mut index = ResolverIndex {
			nodes_by_stable_key: HashMap::new(),
			nodes_by_name: HashMap::new(),
			node_uid_to_file_uid: HashMap::new(),
			file_resolution: HashMap::new(),
			per_file_include_resolution: HashMap::new(),
			stable_key_to_uid: HashMap::new(),
			file_to_module: HashMap::new(),
		};
		index
			.nodes_by_stable_key
			.insert("r1:include/util.h:FILE".into(), header_node);
		// Source node → file UID mapping.
		index
			.node_uid_to_file_uid
			.insert("src1".into(), "r1:src/main.c".into());
		// Per-TU: main.c can resolve "util.h" → "r1:include/util.h:FILE".
		let mut tu_map = HashMap::new();
		tu_map.insert("util.h".into(), "r1:include/util.h:FILE".into());
		index
			.per_file_include_resolution
			.insert("r1:src/main.c".into(), tu_map);

		let edge = make_edge("e1", "util.h", EdgeType::Imports);
		let result = resolve_edges(&[edge], &index, None);

		assert_eq!(result.resolved.len(), 1);
		assert_eq!(result.resolved[0].target_node_uid, "hdr1");
	}

	#[test]
	fn resolve_import_via_repo_prefix_fallback() {
		// Bare header name without per-TU resolution: the
		// repo-prefix fallback constructs "repoUid:name:FILE".
		let header_node = make_node(
			"hdr1",
			"r1:util.h:FILE",
			"util.h",
			None,
			None,
		);
		let mut index = ResolverIndex {
			nodes_by_stable_key: HashMap::new(),
			nodes_by_name: HashMap::new(),
			node_uid_to_file_uid: HashMap::new(),
			file_resolution: HashMap::new(),
			per_file_include_resolution: HashMap::new(),
			stable_key_to_uid: HashMap::new(),
			file_to_module: HashMap::new(),
		};
		index
			.nodes_by_stable_key
			.insert("r1:util.h:FILE".into(), header_node);

		// No per-TU includes, no file_resolution entry.
		let edge = make_edge("e1", "util.h", EdgeType::Imports);
		let result = resolve_edges(&[edge], &index, None);

		assert_eq!(result.resolved.len(), 1);
		assert_eq!(result.resolved[0].target_node_uid, "hdr1");
	}

	#[test]
	fn resolve_call_via_import_binding_assistance() {
		// "classifyMedia" is ambiguous globally (2 functions with
		// the same name). But the source file imported it from
		// "./media", which narrows to the one in that file.
		let n1 = make_node(
			"fn_media",
			"r1:src/media.ts:classifyMedia:SYMBOL",
			"classifyMedia",
			Some("FUNCTION"),
			Some("r1:src/media.ts"),
		);
		let n2 = make_node(
			"fn_other",
			"r1:src/other.ts:classifyMedia:SYMBOL",
			"classifyMedia",
			Some("FUNCTION"),
			Some("r1:src/other.ts"),
		);
		let mut index = ResolverIndex {
			nodes_by_stable_key: HashMap::new(),
			nodes_by_name: HashMap::new(),
			node_uid_to_file_uid: HashMap::new(),
			file_resolution: HashMap::new(),
			per_file_include_resolution: HashMap::new(),
			stable_key_to_uid: HashMap::new(),
			file_to_module: HashMap::new(),
		};
		index
			.nodes_by_name
			.entry("classifyMedia".into())
			.or_default()
			.extend(vec![n1, n2]);
		// Source node lives in src/index.ts.
		index
			.node_uid_to_file_uid
			.insert("src1".into(), "r1:src/index.ts".into());
		// File resolution: "./media" from src/ → src/media.ts.
		index
			.file_resolution
			.insert("r1:src/media:FILE".into(), "r1:src/media.ts:FILE".into());

		// Import binding: "classifyMedia" imported from "./media".
		let bindings: HashMap<String, Vec<ImportBinding>> = [(
			"r1:src/index.ts".into(),
			vec![ImportBinding {
				identifier: "classifyMedia".into(),
				specifier: "./media".into(),
				is_relative: true,
				location: None,
				is_type_only: false,
			}],
		)]
		.into_iter()
		.collect();

		let edge = make_edge("e1", "classifyMedia", EdgeType::Calls);
		let result = resolve_edges(&[edge], &index, Some(&bindings));

		assert_eq!(result.resolved.len(), 1);
		assert_eq!(result.resolved[0].target_node_uid, "fn_media");
	}
}
