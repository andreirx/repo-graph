//! Core extractor implementation.

use std::collections::BTreeMap;

use repo_graph_classification::types::{
	ImportBinding, RuntimeBuiltinsSet, SourceLocation,
};
use repo_graph_indexer::extractor_port::{ExtractorError, ExtractorPort};
use repo_graph_indexer::types::{
	Arg0Payload, EdgeType, ExtractionResult, ExtractedEdge, ExtractedNode,
	NodeKind, NodeSubtype, Resolution, ResolvedCallsite, Visibility,
};

use crate::builtins::ts_js_runtime_builtins;

/// Extractor name and version. Mirrors `EXTRACTOR_VERSIONS.typescript`
/// from `src/version.ts`. The version string is stamped on every
/// node and edge this extractor produces.
const EXTRACTOR_NAME: &str = "ts-core:0.2.0";

/// The full language surface this extractor handles.
const LANGUAGES: &[&str] = &["typescript", "tsx", "javascript", "jsx"];

/// Concrete `ExtractorPort` adapter for TypeScript/TSX/JS/JSX.
///
/// Uses native tree-sitter with compiled-in grammars from
/// `tree-sitter-typescript`. The TS grammar handles `.ts`/`.js`
/// files; the TSX grammar handles `.tsx`/`.jsx` files (includes
/// JSX syntax support).
pub struct TsExtractor {
	languages: Vec<String>,
	builtins: RuntimeBuiltinsSet,
	/// tree-sitter parser instance. Created in `initialize()`.
	parser: Option<tree_sitter::Parser>,
	/// TypeScript grammar (for .ts/.js files).
	ts_language: tree_sitter::Language,
	/// TSX grammar (for .tsx/.jsx files).
	tsx_language: tree_sitter::Language,
}

impl TsExtractor {
	/// Create a new extractor. Call `initialize()` before `extract()`.
	pub fn new() -> Self {
		Self {
			languages: LANGUAGES.iter().map(|s| s.to_string()).collect(),
			builtins: ts_js_runtime_builtins(),
			parser: None,
			ts_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
			tsx_language: tree_sitter_typescript::LANGUAGE_TSX.into(),
		}
	}

	/// Select the appropriate grammar for a file path.
	fn language_for_file(&self, file_path: &str) -> tree_sitter::Language {
		if file_path.ends_with(".tsx") || file_path.ends_with(".jsx") {
			self.tsx_language.clone()
		} else {
			self.ts_language.clone()
		}
	}
}

impl ExtractorPort for TsExtractor {
	fn name(&self) -> &str {
		EXTRACTOR_NAME
	}

	fn languages(&self) -> &[String] {
		&self.languages
	}

	fn runtime_builtins(&self) -> &RuntimeBuiltinsSet {
		&self.builtins
	}

	fn initialize(&mut self) -> Result<(), ExtractorError> {
		let mut parser = tree_sitter::Parser::new();
		// Verify both grammars can be set (catches ABI mismatches).
		parser.set_language(&self.ts_language).map_err(|e| ExtractorError {
			message: format!("failed to set TypeScript grammar: {}", e),
		})?;
		parser.set_language(&self.tsx_language).map_err(|e| ExtractorError {
			message: format!("failed to set TSX grammar: {}", e),
		})?;
		// Reset to TS grammar as default.
		parser.set_language(&self.ts_language).map_err(|e| ExtractorError {
			message: format!("failed to reset to TypeScript grammar: {}", e),
		})?;
		self.parser = Some(parser);
		Ok(())
	}

	fn extract(
		&self,
		source: &str,
		file_path: &str,
		file_uid: &str,
		repo_uid: &str,
		snapshot_uid: &str,
	) -> Result<ExtractionResult, ExtractorError> {
		let _parser = self.parser.as_ref().ok_or_else(|| ExtractorError {
			message: "extractor not initialized — call initialize() first".into(),
		})?;

		// Select grammar and parse.
		let language = self.language_for_file(file_path);
		let mut parser_clone = tree_sitter::Parser::new();
		parser_clone.set_language(&language).map_err(|e| ExtractorError {
			message: format!("failed to set grammar for {}: {}", file_path, e),
		})?;

		let tree = parser_clone.parse(source, None).ok_or_else(|| ExtractorError {
			message: format!("tree-sitter returned null tree for {}", file_path),
		})?;

		let root = tree.root_node();

		// ── FILE node ────────────────────────────────────────
		// TS uses `source.split("\n").length` which counts a trailing
		// newline as an extra empty line. Rust's `lines()` does not.
		// Mirror the TS behavior exactly.
		let line_count = source.split('\n').count().max(1) as i64;
		let file_node_uid = uuid::Uuid::new_v4().to_string();
		let file_name = file_path.rsplit('/').next().unwrap_or(file_path);

		let src = source.as_bytes();

		let mut ctx = ExtractionCtx {
			file_path,
			file_uid,
			file_node_uid: &file_node_uid,
			repo_uid,
			snapshot_uid,
			nodes: vec![ExtractedNode {
				node_uid: file_node_uid.clone(),
				snapshot_uid: snapshot_uid.into(),
				repo_uid: repo_uid.into(),
				stable_key: format!("{}:{}:FILE", repo_uid, file_path),
				kind: NodeKind::File,
				subtype: Some(NodeSubtype::Source),
				name: file_name.into(),
				qualified_name: Some(file_path.into()),
				file_uid: Some(file_uid.into()),
				parent_node_uid: None,
				location: Some(SourceLocation {
					line_start: 1,
					col_start: 0,
					line_end: line_count,
					col_end: 0,
				}),
				signature: None,
				visibility: None,
				doc_comment: None,
				metadata_json: None,
			}],
			edges: Vec::new(),
			import_bindings: Vec::new(),
			resolved_callsites: Vec::new(),
			metrics: BTreeMap::new(),
			exported_names: std::collections::HashSet::new(),
			file_scope_bindings: std::collections::HashMap::new(),
			class_bindings: None,
			enclosing_class_name: None,
			member_types: std::collections::HashMap::new(),
		};

		// ── Walk top-level statements ─────────────────────────
		let mut cursor = root.walk();
		for child in root.children(&mut cursor) {
			let exported = is_exported(&child);
			match child.kind() {
				"import_statement" => {
					extract_import(
						&child, src, file_path, &file_node_uid,
						repo_uid, snapshot_uid,
						&mut ctx.edges, &mut ctx.import_bindings,
					);
				}
				"export_statement" => {
					// Re-exports with a source field.
					if child.child_by_field_name("source").is_some() {
						extract_import(
							&child, src, file_path, &file_node_uid,
							repo_uid, snapshot_uid,
							&mut ctx.edges, &mut ctx.import_bindings,
						);
					}
					// Exported declarations: `export function f() {}`
					if let Some(decl) = child.child_by_field_name("declaration") {
						match decl.kind() {
							"function_declaration" => {
								extract_function(&decl, src, true, &mut ctx);
							}
							"class_declaration" => {
								extract_class(&decl, src, true, &mut ctx);
							}
							"lexical_declaration" | "variable_declaration" => {
								extract_lexical_declaration(&decl, src, true, &mut ctx);
							}
							"interface_declaration" => {
								extract_interface(&decl, src, true, &mut ctx);
							}
							"type_alias_declaration" => {
								extract_type_alias(&decl, src, true, &mut ctx);
							}
							"enum_declaration" => {
								extract_enum(&decl, src, true, &mut ctx);
							}
							_ => {}
						}
					}
					// Plain export list: `export { x, y }` — collect names
					// for second-pass visibility update.
					collect_export_names(&child, src, &mut ctx.exported_names);
				}
				"function_declaration" => {
					extract_function(&child, src, exported, &mut ctx);
				}
				"class_declaration" => {
					extract_class(&child, src, exported, &mut ctx);
				}
				"lexical_declaration" | "variable_declaration" => {
					extract_lexical_declaration(&child, src, exported, &mut ctx);
				}
				"interface_declaration" => {
					extract_interface(&child, src, exported, &mut ctx);
				}
				"type_alias_declaration" => {
					extract_type_alias(&child, src, exported, &mut ctx);
				}
				"enum_declaration" => {
					extract_enum(&child, src, exported, &mut ctx);
				}
				"expression_statement" => {
					// Top-level calls: e.g., `app.listen(3000);`
					extract_calls_from_node(&child, src, &file_node_uid, &mut ctx, None, None);
				}
				_ => {}
			}
		}

		// Second pass: update visibility for names in `export { x, y }`.
		for node in &mut ctx.nodes {
			if node.kind == NodeKind::Symbol {
				if ctx.exported_names.contains(&node.name) {
					node.visibility = Some(Visibility::Export);
				}
			}
		}

		Ok(ExtractionResult {
			nodes: ctx.nodes,
			edges: ctx.edges,
			metrics: ctx.metrics,
			import_bindings: ctx.import_bindings,
			resolved_callsites: ctx.resolved_callsites,
		})
	}
}

// ── Extraction context ───────────────────────────────────────────

struct ExtractionCtx<'a> {
	file_path: &'a str,
	file_uid: &'a str,
	#[allow(dead_code)] // Used when ctx is passed to call extraction via file_node_uid local
	file_node_uid: &'a str,
	repo_uid: &'a str,
	snapshot_uid: &'a str,
	nodes: Vec<ExtractedNode>,
	edges: Vec<ExtractedEdge>,
	import_bindings: Vec<ImportBinding>,
	resolved_callsites: Vec<repo_graph_indexer::types::ResolvedCallsite>,
	metrics: BTreeMap<String, repo_graph_indexer::types::ExtractedMetrics>,
	exported_names: std::collections::HashSet<String>,
	// Receiver type binding state:
	/// File-scope variable type bindings (top-level const).
	file_scope_bindings: std::collections::HashMap<String, String>,
	/// Class-scope property type bindings (for this.prop.method()).
	class_bindings: Option<std::collections::HashMap<String, String>>,
	/// Enclosing class name (for this.method() → ClassName.method).
	enclosing_class_name: Option<String>,
	/// Interface/class member type map for 3-part chain resolution.
	member_types: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
}

// ── Common helpers ───────────────────────────────────────────────

/// Check if a node has a direct child of the given kind.
fn node_has_child_kind(node: &tree_sitter::Node, kind: &str) -> bool {
	for i in 0..node.child_count() {
		if let Some(child) = node.child(i) {
			if child.kind() == kind {
				return true;
			}
		}
	}
	false
}

/// Check if a node is inside an export_statement.
fn is_exported(node: &tree_sitter::Node) -> bool {
	node.parent().map(|p| p.kind() == "export_statement").unwrap_or(false)
}

/// Extract preceding `/** ... */` doc comment.
fn extract_doc_comment(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
	fn find_preceding(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
		let mut prev = node.prev_sibling();
		while let Some(p) = prev {
			if p.kind() == "comment" {
				let text = p.utf8_text(source).unwrap_or("");
				if text.starts_with("/**") {
					return Some(text.to_string());
				}
			}
			if p.kind() != "comment" {
				break;
			}
			prev = p.prev_sibling();
		}
		None
	}
	find_preceding(node, source)
		.or_else(|| node.parent().and_then(|p| find_preceding(&p, source)))
}

/// Build a SYMBOL node with the standard stable_key format.
fn make_symbol_node(
	name: &str,
	subtype: NodeSubtype,
	visibility: Visibility,
	signature: Option<String>,
	node: &tree_sitter::Node,
	source: &[u8],
	ctx: &ExtractionCtx,
) -> ExtractedNode {
	// Subtype must be SCREAMING_SNAKE_CASE in the stable_key,
	// matching the TS format. Use serde serialization.
	let subtype_str = serde_json::to_value(&subtype)
		.ok()
		.and_then(|v| v.as_str().map(|s| s.to_string()))
		.unwrap_or_else(|| format!("{:?}", subtype));
	let stable_key = format!(
		"{}:{}#{}:SYMBOL:{}",
		ctx.repo_uid, ctx.file_path, name, subtype_str
	);
	ExtractedNode {
		node_uid: uuid::Uuid::new_v4().to_string(),
		snapshot_uid: ctx.snapshot_uid.into(),
		repo_uid: ctx.repo_uid.into(),
		stable_key,
		kind: NodeKind::Symbol,
		subtype: Some(subtype),
		name: name.into(),
		qualified_name: Some(name.into()),
		file_uid: Some(ctx.file_uid.into()),
		parent_node_uid: None,
		location: Some(location_from_node(node)),
		signature,
		visibility: Some(visibility),
		doc_comment: extract_doc_comment(node, source),
		metadata_json: None,
	}
}

/// Collect names from `export { x, y }` plain export lists.
fn collect_export_names(
	export_node: &tree_sitter::Node,
	source: &[u8],
	names: &mut std::collections::HashSet<String>,
) {
	// Skip if this has a source (it's a re-export, not a plain list).
	if export_node.child_by_field_name("source").is_some() {
		return;
	}
	// Skip if this has a declaration (it's `export function ...`).
	if export_node.child_by_field_name("declaration").is_some() {
		return;
	}
	// Look for export_clause → export_specifier → name
	let mut cursor = export_node.walk();
	for child in export_node.children(&mut cursor) {
		if child.kind() != "export_clause" {
			continue;
		}
		let mut clause_cursor = child.walk();
		for spec in child.children(&mut clause_cursor) {
			if spec.kind() != "export_specifier" {
				continue;
			}
			if let Some(name_node) = spec.child_by_field_name("name") {
				names.insert(name_node.utf8_text(source).unwrap_or("").to_string());
			}
		}
	}
}

// ── Function extraction ──────────────────────────────────────────

/// Extract a function_declaration as a SYMBOL:FUNCTION node.
fn extract_function(
	node: &tree_sitter::Node,
	source: &[u8],
	exported: bool,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let name = name_node.utf8_text(source).unwrap_or("");

	let params = node.child_by_field_name("parameters");
	let return_type = node.child_by_field_name("return_type");
	let sig = params.map(|p| {
		let params_text = p.utf8_text(source).unwrap_or("()");
		let ret = return_type
			.map(|r| format!(": {}", r.utf8_text(source).unwrap_or("")))
			.unwrap_or_default();
		format!("{}{}{}", name, params_text, ret)
	});

	let visibility = if exported {
		Visibility::Export
	} else {
		Visibility::Private
	};

	let graph_node = make_symbol_node(
		name, NodeSubtype::Function, visibility, sig, node, source, ctx,
	);

	let fn_node_uid = graph_node.node_uid.clone();
	let fn_stable_key = graph_node.stable_key.clone();
	ctx.nodes.push(graph_node);

	// Extract calls and metrics from function body.
	if let Some(body) = node.child_by_field_name("body") {
		let params = node.child_by_field_name("parameters");
		let param_bindings = collect_parameter_bindings(params, source);
		let mut var_bindings: BindingMap = BindingMap::new();
		collect_var_bindings(&body, source, &mut var_bindings);
		extract_calls_from_node(&body, source, &fn_node_uid, ctx, Some(&param_bindings), Some(&mut var_bindings));

		ctx.metrics.insert(
			fn_stable_key,
			crate::metrics::compute_function_metrics(&body, params.as_ref()),
		);
	}
}

// ── Variable/constant extraction ─────────────────────────────────

/// Extract a lexical_declaration or variable_declaration.
/// Detects arrow/function expressions to emit FUNCTION instead of
/// CONSTANT/VARIABLE.
fn extract_lexical_declaration(
	node: &tree_sitter::Node,
	source: &[u8],
	exported: bool,
	ctx: &mut ExtractionCtx,
) {
	let is_const = node_has_child_kind(node, "const");

	let mut decl_cursor = node.walk();
	for child in node.children(&mut decl_cursor) {
		if child.kind() != "variable_declarator" {
			continue;
		}
		let name_node = match child.child_by_field_name("name") {
			Some(n) => n,
			None => continue,
		};
		let name = name_node.utf8_text(source).unwrap_or("");

		let value = child.child_by_field_name("value");
		let is_function_like = value
			.map(|v| {
				matches!(
					v.kind(),
					"arrow_function" | "function_expression" | "function"
				)
			})
			.unwrap_or(false);

		let subtype = if is_function_like {
			NodeSubtype::Function
		} else if is_const {
			NodeSubtype::Constant
		} else {
			NodeSubtype::Variable
		};

		let visibility = if exported {
			Visibility::Export
		} else {
			Visibility::Private
		};

		let graph_node = make_symbol_node(
			name, subtype, visibility, None, &child, source, ctx,
		);

		let var_node_uid = graph_node.node_uid.clone();
		ctx.nodes.push(graph_node);

		// Extract calls from initializer.
		if let Some(value) = child.child_by_field_name("value") {
			extract_calls_from_node(&value, source, &var_node_uid, ctx, None, None);
		}

		// Build file-scope type binding for this variable.
		if !is_function_like {
			if let Some(type_name) = extract_simple_type_name(&child, source) {
				ctx.file_scope_bindings.insert(name.to_string(), type_name);
			} else if let Some(val) = child.child_by_field_name("value") {
				if val.kind() == "new_expression" {
					if let Some(ctor) = val.child_by_field_name("constructor") {
						ctx.file_scope_bindings.insert(
							name.to_string(),
							ctor.utf8_text(source).unwrap_or("").to_string(),
						);
					}
				}
			}
		}
	}
}

// ── Class extraction ─────────────────────────────────────────────

/// Extract a class_declaration as a SYMBOL:CLASS node with members.
fn extract_class(
	node: &tree_sitter::Node,
	source: &[u8],
	exported: bool,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let class_name = name_node.utf8_text(source).unwrap_or("");

	let visibility = if exported {
		Visibility::Export
	} else {
		Visibility::Private
	};

	let class_node = make_symbol_node(
		class_name, NodeSubtype::Class, visibility, None, node, source, ctx,
	);
	let class_node_uid = class_node.node_uid.clone();
	let class_node_name = class_node.name.clone();
	ctx.nodes.push(class_node);

	// IMPLEMENTS edges from class_heritage.
	extract_implements(node, source, &class_node_uid, ctx);

	// Extract members from class body.
	let body = match node.child_by_field_name("body") {
		Some(b) => b,
		None => return,
	};

	// Collect member types for 3-part chain resolution.
	collect_member_types(class_name, &body, source, &mut ctx.member_types);

	// Set class context for receiver type binding.
	ctx.class_bindings = Some(build_class_bindings(&body, source));
	ctx.enclosing_class_name = Some(class_name.to_string());

	for i in 0..body.child_count() {
		if let Some(member) = body.child(i) {
			match member.kind() {
				"method_definition" => {
					extract_method(&member, source, &class_node_uid, &class_node_name, ctx);
				}
				"public_field_definition" => {
					extract_property(&member, source, &class_node_uid, &class_node_name, ctx);
				}
				_ => {}
			}
		}
	}

	// Clear class context.
	ctx.class_bindings = None;
	ctx.enclosing_class_name = None;
}

/// Extract a method_definition as a SYMBOL node (METHOD, CONSTRUCTOR,
/// GETTER, or SETTER).
fn extract_method(
	node: &tree_sitter::Node,
	source: &[u8],
	parent_node_uid: &str,
	parent_class_name: &str,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let name = name_node.utf8_text(source).unwrap_or("");
	let qualified_name = format!("{}.{}", parent_class_name, name);

	let mut subtype = NodeSubtype::Method;
	if name == "constructor" {
		subtype = NodeSubtype::Constructor;
	}
	// Check for getter/setter.
	if node_has_child_kind(node, "get") {
		subtype = NodeSubtype::Getter;
	} else if node_has_child_kind(node, "set") {
		subtype = NodeSubtype::Setter;
	}

	let visibility = get_method_visibility(node, source);

	let params = node.child_by_field_name("parameters");
	let sig = params.map(|p| {
		format!("{}{}", name, p.utf8_text(source).unwrap_or("()"))
	});

	let subtype_str = serde_json::to_value(&subtype)
		.ok()
		.and_then(|v| v.as_str().map(|s| s.to_string()))
		.unwrap_or_else(|| format!("{:?}", subtype));
	let stable_key = format!(
		"{}:{}#{}:SYMBOL:{}",
		ctx.repo_uid, ctx.file_path, qualified_name, subtype_str
	);

	let method_node = ExtractedNode {
		node_uid: uuid::Uuid::new_v4().to_string(),
		snapshot_uid: ctx.snapshot_uid.into(),
		repo_uid: ctx.repo_uid.into(),
		stable_key,
		kind: NodeKind::Symbol,
		subtype: Some(subtype),
		name: name.into(),
		qualified_name: Some(qualified_name),
		file_uid: Some(ctx.file_uid.into()),
		parent_node_uid: Some(parent_node_uid.into()),
		location: Some(location_from_node(node)),
		signature: sig,
		visibility: Some(visibility),
		doc_comment: extract_doc_comment(node, source),
		metadata_json: None,
	};

	let method_node_uid = method_node.node_uid.clone();
	let method_stable_key = method_node.stable_key.clone();
	ctx.nodes.push(method_node);

	// Extract calls and metrics from method body.
	if let Some(body) = node.child_by_field_name("body") {
		let params = node.child_by_field_name("parameters");
		let param_bindings = collect_parameter_bindings(params, source);
		let mut var_bindings: BindingMap = BindingMap::new();
		collect_var_bindings(&body, source, &mut var_bindings);
		extract_calls_from_node(&body, source, &method_node_uid, ctx, Some(&param_bindings), Some(&mut var_bindings));

		ctx.metrics.insert(
			method_stable_key,
			crate::metrics::compute_function_metrics(&body, params.as_ref()),
		);
	}
}

/// Extract a public_field_definition as a SYMBOL:PROPERTY node.
fn extract_property(
	node: &tree_sitter::Node,
	source: &[u8],
	parent_node_uid: &str,
	parent_class_name: &str,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let name = name_node.utf8_text(source).unwrap_or("");
	let qualified_name = format!("{}.{}", parent_class_name, name);

	let subtype_str = serde_json::to_value(&NodeSubtype::Property)
		.ok()
		.and_then(|v| v.as_str().map(|s| s.to_string()))
		.unwrap_or("PROPERTY".into());
	let stable_key = format!(
		"{}:{}#{}:SYMBOL:{}",
		ctx.repo_uid, ctx.file_path, qualified_name, subtype_str
	);

	ctx.nodes.push(ExtractedNode {
		node_uid: uuid::Uuid::new_v4().to_string(),
		snapshot_uid: ctx.snapshot_uid.into(),
		repo_uid: ctx.repo_uid.into(),
		stable_key,
		kind: NodeKind::Symbol,
		subtype: Some(NodeSubtype::Property),
		name: name.into(),
		qualified_name: Some(qualified_name),
		file_uid: Some(ctx.file_uid.into()),
		parent_node_uid: Some(parent_node_uid.into()),
		location: Some(location_from_node(node)),
		signature: None,
		visibility: Some(get_method_visibility(node, source)),
		doc_comment: None,
		metadata_json: None,
	});
}

/// Extract IMPLEMENTS edges from class_heritage.
fn extract_implements(
	class_node: &tree_sitter::Node,
	source: &[u8],
	class_node_uid: &str,
	ctx: &mut ExtractionCtx,
) {
	// Find class_heritage child.
	for i in 0..class_node.child_count() {
		let child = match class_node.child(i) {
			Some(c) => c,
			None => continue,
		};
		if child.kind() != "class_heritage" {
			continue;
		}
		// Look for implements_clause.
		for j in 0..child.child_count() {
			let clause = match child.child(j) {
				Some(c) => c,
				None => continue,
			};
			if clause.kind() != "implements_clause" {
				continue;
			}
			// Each type_identifier or identifier in the clause is an implemented interface.
			for k in 0..clause.child_count() {
				let type_node = match clause.child(k) {
					Some(n) => n,
					None => continue,
				};
				if type_node.kind() != "type_identifier" && type_node.kind() != "identifier" {
					continue;
				}
				let iface_name = type_node.utf8_text(source).unwrap_or("");
				ctx.edges.push(ExtractedEdge {
					edge_uid: uuid::Uuid::new_v4().to_string(),
					snapshot_uid: ctx.snapshot_uid.into(),
					repo_uid: ctx.repo_uid.into(),
					source_node_uid: class_node_uid.into(),
					target_key: iface_name.into(),
					edge_type: EdgeType::Implements,
					resolution: Resolution::Static,
					extractor: EXTRACTOR_NAME.into(),
					location: Some(location_from_node(&type_node)),
					metadata_json: Some(
						serde_json::json!({"targetName": iface_name}).to_string(),
					),
				});
			}
		}
	}
}

/// Get visibility from accessibility_modifier (private/protected/public).
/// Defaults to PUBLIC for class members (TS convention).
fn get_method_visibility(node: &tree_sitter::Node, source: &[u8]) -> Visibility {
	for i in 0..node.child_count() {
		if let Some(child) = node.child(i) {
			if child.kind() == "accessibility_modifier" {
				let text = child.utf8_text(source).unwrap_or("");
				return match text {
					"private" => Visibility::Private,
					"protected" => Visibility::Protected,
					"public" => Visibility::Public,
					_ => Visibility::Public,
				};
			}
		}
	}
	Visibility::Public
}

// ── Interface extraction ─────────────────────────────────────────

/// Extract an interface_declaration with method/property members.
fn extract_interface(
	node: &tree_sitter::Node,
	source: &[u8],
	exported: bool,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let iface_name = name_node.utf8_text(source).unwrap_or("");

	let visibility = if exported { Visibility::Export } else { Visibility::Private };
	let iface_node = make_symbol_node(
		iface_name, NodeSubtype::Interface, visibility, None, node, source, ctx,
	);
	let iface_node_uid = iface_node.node_uid.clone();
	let iface_node_name = iface_node.name.clone();
	ctx.nodes.push(iface_node);

	// Find interface_body.
	let body = match find_child_by_kind(node, "interface_body") {
		Some(b) => b,
		None => return,
	};

	// Collect member types for 3-part chain resolution.
	collect_member_types(iface_name, &body, source, &mut ctx.member_types);

	// Overload dedup: track seen method names.
	let mut seen_methods = std::collections::HashSet::new();

	for i in 0..body.child_count() {
		let member = match body.child(i) {
			Some(m) => m,
			None => continue,
		};
		match member.kind() {
			"method_signature" => {
				let mn = match member.child_by_field_name("name") {
					Some(n) => n,
					None => continue,
				};
				let key = format!("{}:method", mn.utf8_text(source).unwrap_or(""));
				if seen_methods.contains(&key) {
					continue; // Skip overload.
				}
				seen_methods.insert(key);
				extract_interface_method(&member, source, &iface_node_uid, &iface_node_name, ctx);
			}
			"property_signature" => {
				extract_interface_property(&member, source, &iface_node_uid, &iface_node_name, ctx);
			}
			_ => {}
		}
	}
}

fn extract_interface_method(
	node: &tree_sitter::Node,
	source: &[u8],
	parent_uid: &str,
	parent_name: &str,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let name = name_node.utf8_text(source).unwrap_or("");
	let qualified_name = format!("{}.{}", parent_name, name);

	let mut subtype = NodeSubtype::Method;
	if node_has_child_kind(node, "get") {
		subtype = NodeSubtype::Getter;
	} else if node_has_child_kind(node, "set") {
		subtype = NodeSubtype::Setter;
	}

	let params = node.child_by_field_name("parameters");
	let sig = params.map(|p| {
		format!("{}{}", name, p.utf8_text(source).unwrap_or("()"))
	});

	let subtype_str = serde_json::to_value(&subtype)
		.ok()
		.and_then(|v| v.as_str().map(|s| s.to_string()))
		.unwrap_or_else(|| format!("{:?}", subtype));

	ctx.nodes.push(ExtractedNode {
		node_uid: uuid::Uuid::new_v4().to_string(),
		snapshot_uid: ctx.snapshot_uid.into(),
		repo_uid: ctx.repo_uid.into(),
		stable_key: format!(
			"{}:{}#{}:SYMBOL:{}",
			ctx.repo_uid, ctx.file_path, qualified_name, subtype_str
		),
		kind: NodeKind::Symbol,
		subtype: Some(subtype),
		name: name.into(),
		qualified_name: Some(qualified_name),
		file_uid: Some(ctx.file_uid.into()),
		parent_node_uid: Some(parent_uid.into()),
		location: Some(location_from_node(node)),
		signature: sig,
		visibility: Some(Visibility::Public), // Interface members always PUBLIC.
		doc_comment: extract_doc_comment(node, source),
		metadata_json: None,
	});
}

fn extract_interface_property(
	node: &tree_sitter::Node,
	source: &[u8],
	parent_uid: &str,
	parent_name: &str,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let name = name_node.utf8_text(source).unwrap_or("");
	let qualified_name = format!("{}.{}", parent_name, name);

	let subtype_str = serde_json::to_value(&NodeSubtype::Property)
		.ok()
		.and_then(|v| v.as_str().map(|s| s.to_string()))
		.unwrap_or("PROPERTY".into());

	ctx.nodes.push(ExtractedNode {
		node_uid: uuid::Uuid::new_v4().to_string(),
		snapshot_uid: ctx.snapshot_uid.into(),
		repo_uid: ctx.repo_uid.into(),
		stable_key: format!(
			"{}:{}#{}:SYMBOL:{}",
			ctx.repo_uid, ctx.file_path, qualified_name, subtype_str
		),
		kind: NodeKind::Symbol,
		subtype: Some(NodeSubtype::Property),
		name: name.into(),
		qualified_name: Some(qualified_name),
		file_uid: Some(ctx.file_uid.into()),
		parent_node_uid: Some(parent_uid.into()),
		location: Some(location_from_node(node)),
		signature: None,
		visibility: Some(Visibility::Public),
		doc_comment: None,
		metadata_json: None,
	});
}

/// Find the first child of a specific kind.
fn find_child_by_kind<'a>(node: &'a tree_sitter::Node, kind: &str) -> Option<tree_sitter::Node<'a>> {
	for i in 0..node.child_count() {
		if let Some(child) = node.child(i) {
			if child.kind() == kind {
				return Some(child);
			}
		}
	}
	None
}

// ── Receiver type binding ────────────────────────────────────────

/// Extract a simple type name from a node's type annotation.
/// Returns `Some("TypeName")` for `: TypeName`, `None` for complex types.
fn extract_simple_type_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
	let type_ann = node.child_by_field_name("type")?;
	for i in 0..type_ann.child_count() {
		if let Some(child) = type_ann.child(i) {
			if child.kind() == "type_identifier" {
				return Some(child.utf8_text(source).unwrap_or("").to_string());
			}
		}
	}
	None
}

/// Build class-scope property type bindings from field declarations
/// and constructor parameter properties.
fn build_class_bindings(class_body: &tree_sitter::Node, source: &[u8]) -> std::collections::HashMap<String, String> {
	let mut bindings = std::collections::HashMap::new();
	for i in 0..class_body.child_count() {
		let member = match class_body.child(i) {
			Some(m) => m,
			None => continue,
		};
		// Field declarations: `private storage: StoragePort;`
		if member.kind() == "public_field_definition" {
			if let (Some(name_node), Some(type_name)) = (
				member.child_by_field_name("name"),
				extract_simple_type_name(&member, source),
			) {
				bindings.insert(
					name_node.utf8_text(source).unwrap_or("").to_string(),
					type_name,
				);
			}
		}
		// Constructor parameter properties.
		if member.kind() == "method_definition" {
			let mn = member.child_by_field_name("name");
			if mn.map(|n| n.utf8_text(source).unwrap_or("")) != Some("constructor") {
				continue;
			}
			let params = match member.child_by_field_name("parameters") {
				Some(p) => p,
				None => continue,
			};
			for j in 0..params.child_count() {
				let param = match params.child(j) {
					Some(p) if p.kind() == "required_parameter" => p,
					_ => continue,
				};
				let has_accessor = (0..param.child_count()).any(|k| {
					param.child(k).map(|c| {
						c.kind() == "accessibility_modifier" || c.kind() == "readonly"
					}).unwrap_or(false)
				});
				if !has_accessor { continue; }
				if let (Some(pattern), Some(type_name)) = (
					param.child_by_field_name("pattern"),
					extract_simple_type_name(&param, source),
				) {
					bindings.insert(
						pattern.utf8_text(source).unwrap_or("").to_string(),
						type_name,
					);
				}
			}
		}
	}
	bindings
}

/// Collect member type annotations from interface/class body for
/// 3-part chain resolution.
fn collect_member_types(
	type_name: &str,
	body: &tree_sitter::Node,
	source: &[u8],
	member_types: &mut std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) {
	let mut members = std::collections::HashMap::new();
	for i in 0..body.child_count() {
		let member = match body.child(i) {
			Some(m) => m,
			None => continue,
		};
		if member.kind() != "property_signature" && member.kind() != "public_field_definition" {
			continue;
		}
		if let (Some(name_node), Some(mt)) = (
			member.child_by_field_name("name"),
			extract_simple_type_name(&member, source),
		) {
			members.insert(name_node.utf8_text(source).unwrap_or("").to_string(), mt);
		}
	}
	if !members.is_empty() {
		member_types.insert(type_name.to_string(), members);
	}
}

/// Collect parameter bindings from a formal_parameters node.
fn collect_parameter_bindings(params_node: Option<tree_sitter::Node>, source: &[u8]) -> std::collections::HashMap<String, Option<String>> {
	let mut bindings = std::collections::HashMap::new();
	let params = match params_node {
		Some(p) => p,
		None => return bindings,
	};
	for i in 0..params.child_count() {
		let param = match params.child(i) {
			Some(p) => p,
			None => continue,
		};
		if param.kind() != "required_parameter" && param.kind() != "optional_parameter" {
			continue;
		}
		if let Some(pattern) = param.child_by_field_name("pattern") {
			let type_name = extract_simple_type_name(&param, source);
			bindings.insert(
				pattern.utf8_text(source).unwrap_or("").to_string(),
				type_name,
			);
		}
	}
	bindings
}

/// Resolve receiver type for a member call target.
/// 4-pattern resolution matching TS `resolveReceiverType`.
fn resolve_receiver_type(
	raw_name: &str,
	ctx: &ExtractionCtx,
	local_bindings: Option<&std::collections::HashMap<String, Option<String>>>,
	fn_bindings: Option<&std::collections::HashMap<String, Option<String>>>,
) -> String {
	if !raw_name.contains('.') {
		return raw_name.to_string();
	}

	let parts: Vec<&str> = raw_name.split('.').collect();
	let method = parts[parts.len() - 1];

	// Pattern 1: this.property.method() — class bindings.
	if parts[0] == "this" && parts.len() >= 3 {
		if let Some(ref cb) = ctx.class_bindings {
			if let Some(type_name) = cb.get(parts[1]) {
				return format!("{}.{}", type_name, method);
			}
		}
	}

	// Pattern 2: this.method() → ClassName.method.
	if parts[0] == "this" && parts.len() == 2 {
		if let Some(ref cn) = ctx.enclosing_class_name {
			return format!("{}.{}", cn, method);
		}
		return raw_name.to_string();
	}

	// Three-level variable type lookup: local → function → file.
	let resolve_var = |var_name: &str| -> Option<Result<String, ()>> {
		// Block-local
		if let Some(lb) = local_bindings {
			if let Some(entry) = lb.get(var_name) {
				return Some(match entry {
					Some(t) => Ok(t.clone()),
					None => Err(()), // shadowed
				});
			}
		}
		// Function-level
		if let Some(fb) = fn_bindings {
			if let Some(entry) = fb.get(var_name) {
				return Some(match entry {
					Some(t) => Ok(t.clone()),
					None => Err(()),
				});
			}
		}
		// File-scope
		if let Some(t) = ctx.file_scope_bindings.get(var_name) {
			return Some(Ok(t.clone()));
		}
		None
	};

	// Pattern 3: variable.method() — 2-part chain.
	if parts.len() == 2 {
		if let Some(result) = resolve_var(parts[0]) {
			match result {
				Err(()) => return raw_name.to_string(), // shadowed
				Ok(type_name) => return format!("{}.{}", type_name, method),
			}
		}
		return raw_name.to_string();
	}

	// Pattern 4: variable.property.method() — 3-part chain.
	if parts.len() >= 3 && parts[0] != "this" {
		if let Some(result) = resolve_var(parts[0]) {
			match result {
				Err(()) => return raw_name.to_string(),
				Ok(type_name) => {
					let prop_name = parts[1];
					if let Some(member_map) = ctx.member_types.get(&type_name) {
						if let Some(prop_type) = member_map.get(prop_name) {
							return format!("{}.{}", prop_type, method);
						}
					}
					return format!("{}.{}.{}", type_name, prop_name, method);
				}
			}
		}
	}

	raw_name.to_string()
}

// ── Type alias / enum extraction ─────────────────────────────────

fn extract_type_alias(
	node: &tree_sitter::Node,
	source: &[u8],
	exported: bool,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let name = name_node.utf8_text(source).unwrap_or("");
	let visibility = if exported { Visibility::Export } else { Visibility::Private };
	ctx.nodes.push(make_symbol_node(
		name, NodeSubtype::TypeAlias, visibility, None, node, source, ctx,
	));
}

fn extract_enum(
	node: &tree_sitter::Node,
	source: &[u8],
	exported: bool,
	ctx: &mut ExtractionCtx,
) {
	let name_node = match node.child_by_field_name("name") {
		Some(n) => n,
		None => return,
	};
	let name = name_node.utf8_text(source).unwrap_or("");
	let visibility = if exported { Visibility::Export } else { Visibility::Private };
	ctx.nodes.push(make_symbol_node(
		name, NodeSubtype::Enum, visibility, None, node, source, ctx,
	));
}

// ── Import extraction ────────────────────────────────────────────

/// Extract IMPORTS edges and ImportBinding records from an
/// `import_statement` node.
///
/// Mirror of `TypeScriptExtractor.extractImport` from
/// `ts-extractor.ts:186`.
fn extract_import(
	node: &tree_sitter::Node,
	source: &[u8],
	file_path: &str,
	file_node_uid: &str,
	repo_uid: &str,
	snapshot_uid: &str,
	edges: &mut Vec<ExtractedEdge>,
	import_bindings: &mut Vec<ImportBinding>,
) {
	// Get the import source string (the module specifier).
	let source_node = match node.child_by_field_name("source") {
		Some(n) => n,
		None => return,
	};
	let raw_text = source_node.utf8_text(source).unwrap_or("");
	let raw_path = raw_text.trim_matches(|c| c == '\'' || c == '"');

	let is_relative = raw_path.starts_with('.');
	let location = location_from_node(node);

	// Statement-level `import type` detection.
	let node_text = node.utf8_text(source).unwrap_or("");
	let is_type_only = node_text.starts_with("import type ")
		|| node_text.starts_with("import type\t")
		|| node_text.starts_with("import type{");

	// Collect import bindings from the import clause.
	let mut cursor = node.walk();
	for child in node.children(&mut cursor) {
		if child.kind() != "import_clause" {
			continue;
		}
		for (ident, imported) in collect_local_identifiers(&child, source) {
			import_bindings.push(ImportBinding {
				identifier: ident,
				specifier: raw_path.to_string(),
				is_relative,
				location: Some(location),
				is_type_only,
				imported_name: imported,
			});
		}
	}

	// Only relative imports produce IMPORTS edges.
	if !is_relative {
		return;
	}

	let resolved_path = match resolve_import_path(raw_path, file_path) {
		Some(p) => p,
		None => return,
	};

	let target_key = format!("{}:{}:FILE", repo_uid, resolved_path);

	edges.push(ExtractedEdge {
		edge_uid: uuid::Uuid::new_v4().to_string(),
		snapshot_uid: snapshot_uid.into(),
		repo_uid: repo_uid.into(),
		source_node_uid: file_node_uid.into(),
		target_key,
		edge_type: EdgeType::Imports,
		resolution: Resolution::Static,
		extractor: EXTRACTOR_NAME.into(),
		location: Some(location),
		metadata_json: Some(
			serde_json::json!({
				"rawPath": raw_path,
				"resolvedPath": resolved_path,
			})
			.to_string(),
		),
	});
}

/// Collect local identifier names from an `import_clause` node,
/// paired with the original exported symbol name (if any).
///
/// Returns `(identifier, imported_name)` tuples:
///
/// - Default import `import X from "m"` → `("X", None)`.
/// - Namespace import `import * as X from "m"` → `("X", None)`.
/// - Named import `import { X } from "m"` → `("X", Some("X"))`.
/// - Named import with alias `import { X as Y } from "m"` →
///   `("Y", Some("X"))`.
///
/// Default and namespace imports carry `None` because they bring
/// in the whole module surface; the actual symbol comes from the
/// member expression at the call site.
///
/// Mirror of `collectLocalIdentifiers` from `ts-extractor.ts:1691`.
fn collect_local_identifiers(
	import_clause: &tree_sitter::Node,
	source: &[u8],
) -> Vec<(String, Option<String>)> {
	let mut identifiers = Vec::new();
	let mut cursor = import_clause.walk();
	for child in import_clause.children(&mut cursor) {
		match child.kind() {
			"identifier" => {
				// Default import: `import X from "m"`.
				let ident = child.utf8_text(source).unwrap_or("").to_string();
				identifiers.push((ident, None));
			}
			"namespace_import" => {
				// `import * as ns from "m"`.
				let mut ns_cursor = child.walk();
				for n in child.children(&mut ns_cursor) {
					if n.kind() == "identifier" {
						let ident = n.utf8_text(source).unwrap_or("").to_string();
						identifiers.push((ident, None));
						break;
					}
				}
			}
			"named_imports" => {
				// `import { a, b as c } from "m"`.
				let mut named_cursor = child.walk();
				for spec in child.children(&mut named_cursor) {
					if spec.kind() != "import_specifier" {
						continue;
					}
					let name_node = spec.child_by_field_name("name");
					let alias_node = spec.child_by_field_name("alias");
					let exported_name = name_node
						.map(|n| n.utf8_text(source).unwrap_or("").to_string());
					// Local name: alias if present, otherwise the
					// exported name.
					let local = alias_node.or(name_node).map(|n| {
						n.utf8_text(source).unwrap_or("").to_string()
					});
					if let Some(ident) = local {
						identifiers.push((ident, exported_name));
					}
				}
			}
			_ => {}
		}
	}
	identifiers
}

/// Resolve a relative import path to a repo-relative path.
///
/// Strips `.js`/`.jsx` extensions (TS files commonly import with
/// `.js` but the actual source is `.ts`/`.tsx`). Normalizes
/// `../` and `./` segments.
///
/// Mirror of `resolveImportPath` from `ts-extractor.ts:1575`.
fn resolve_import_path(raw_path: &str, current_file_path: &str) -> Option<String> {
	// Get current directory from file path.
	let current_dir = match current_file_path.rfind('/') {
		Some(pos) => &current_file_path[..pos],
		None => "",
	};

	// Join and normalize path segments.
	let mut parts: Vec<&str> = Vec::new();
	if !current_dir.is_empty() {
		parts.extend(current_dir.split('/'));
	}
	for seg in raw_path.split('/') {
		if seg == "." || seg.is_empty() {
			continue;
		} else if seg == ".." {
			parts.pop();
		} else {
			parts.push(seg);
		}
	}
	let mut result = parts.join("/");

	// Strip .js/.jsx extension.
	if result.ends_with(".js") {
		result.truncate(result.len() - 3);
	} else if result.ends_with(".jsx") {
		result.truncate(result.len() - 4);
	}

	Some(result)
}

/// Convert a tree-sitter node position to a SourceLocation.
///
/// tree-sitter uses 0-based rows; we convert to 1-based lines.
/// Columns stay 0-based.
///
/// Mirror of `locationFromNode` from `ts-extractor.ts:1666`.
// ── Call extraction ──────────────────────────────────────────────

/// Recursively walk a subtree extracting CALLS and INSTANTIATES edges.
/// Stops at scope boundaries (nested functions, classes, arrow functions).
///
/// Implements the full receiver type binding model:
///   - `statement_block` → `walk_block_sequentially` with TDZ + var hoisting
///   - `for_statement` / `for_in_statement` → `walk_loop_with_header_scope`
///   - Binding resolution via `resolve_receiver_type` (4-pattern: class,
///     this.method, 2-part chain, 3-part chain)
///
/// Matches the TS extractor's `extractCallsFromNode` + `walkBlockSequentially`
/// + `walkLoopWithHeaderScope` semantics.
type BindingMap = std::collections::HashMap<String, Option<String>>;

fn extract_calls_from_node(
	node: &tree_sitter::Node,
	source: &[u8],
	caller_node_uid: &str,
	ctx: &mut ExtractionCtx,
	local_bindings: Option<&BindingMap>,
	mut fn_bindings: Option<&mut BindingMap>,
) {
	// Scope boundary: statement_block → sequential walk with TDZ.
	if node.kind() == "statement_block" {
		walk_block_sequentially(node, source, caller_node_uid, ctx, local_bindings, fn_bindings);
		return;
	}

	// Scope boundary: for/for-in loops introduce header scope.
	if node.kind() == "for_statement" || node.kind() == "for_in_statement" {
		walk_loop_with_header_scope(node, source, caller_node_uid, ctx, local_bindings, fn_bindings);
		return;
	}

	// Emit edges for this node.
	extract_calls_from_single_node(node, source, caller_node_uid, ctx, local_bindings, fn_bindings.as_deref());

	// Recurse into children, skipping scope boundaries.
	for i in 0..node.child_count() {
		if let Some(child) = node.child(i) {
			if is_new_scope_node(&child) {
				continue;
			}
			extract_calls_from_node(&child, source, caller_node_uid, ctx, local_bindings, fn_bindings.as_deref_mut());
		}
	}
}

/// Walk a statement_block sequentially, modeling TDZ and var hoisting.
fn walk_block_sequentially(
	block: &tree_sitter::Node,
	source: &[u8],
	caller_node_uid: &str,
	ctx: &mut ExtractionCtx,
	parent_bindings: Option<&BindingMap>,
	mut fn_bindings: Option<&mut BindingMap>,
) {
	let mut block_bindings: BindingMap = parent_bindings.cloned().unwrap_or_default();

	// TDZ pre-scan: install shadow-only entries for all const/let names.
	for i in 0..block.child_count() {
		if let Some(child) = block.child(i) {
			if child.kind() == "lexical_declaration" {
				prescan_lexical_names(&child, source, &mut block_bindings);
			}
		}
	}

	// Sequential walk.
	for i in 0..block.child_count() {
		let child = match block.child(i) {
			Some(c) => c,
			None => continue,
		};
		if is_new_scope_node(&child) { continue; }

		if child.kind() == "lexical_declaration" {
			// Extract calls FIRST (binding not yet usable in own initializer).
			extract_calls_from_node(&child, source, caller_node_uid, ctx, Some(&block_bindings), fn_bindings.as_deref_mut());
			// Then upgrade shadow to typed binding.
			accumulate_declaration_bindings(&child, source, &mut block_bindings);
			continue;
		}

		if child.kind() == "variable_declaration" {
			extract_calls_from_node(&child, source, caller_node_uid, ctx, Some(&block_bindings), fn_bindings.as_deref_mut());
			accumulate_declaration_bindings(&child, source, &mut block_bindings);
			if let Some(ref mut fb) = fn_bindings.as_deref_mut() {
				accumulate_declaration_bindings(&child, source, fb);
			}
			continue;
		}

		extract_calls_from_node(&child, source, caller_node_uid, ctx, Some(&block_bindings), fn_bindings.as_deref_mut());
	}
}

/// Walk a for/for-in/for-of loop with header-scoped bindings.
/// Mirrors TS `walkLoopWithHeaderScope`.
fn walk_loop_with_header_scope(
	node: &tree_sitter::Node,
	source: &[u8],
	caller_node_uid: &str,
	ctx: &mut ExtractionCtx,
	parent_bindings: Option<&BindingMap>,
	mut fn_bindings: Option<&mut BindingMap>,
) {
	let mut loop_bindings: BindingMap = parent_bindings.cloned().unwrap_or_default();

	if node.kind() == "for_statement" {
		// for_statement: initializer is lexical_declaration or variable_declaration.
		// 1. Shadow names, 2. Extract calls from initializer, 3. Install typed bindings.
		if let Some(initializer) = node.child_by_field_name("initializer") {
			// Step 1: shadow names.
			prescan_lexical_names(&initializer, source, &mut loop_bindings);
			// Step 2: extract calls from initializer.
			extract_calls_from_node(&initializer, source, caller_node_uid, ctx, Some(&loop_bindings), fn_bindings.as_deref_mut());
			// Step 3: install typed bindings.
			accumulate_declaration_bindings(&initializer, source, &mut loop_bindings);
			if initializer.kind() == "variable_declaration" {
				if let Some(ref mut fb) = fn_bindings.as_deref_mut() {
					accumulate_declaration_bindings(&initializer, source, fb);
				}
			}
		}

		// Extract calls from condition and increment.
		for i in 0..node.child_count() {
			if let Some(child) = node.child(i) {
				let field = node.field_name_for_child(i as u32);
				if field == Some("condition") || field == Some("increment") {
					extract_calls_from_node(&child, source, caller_node_uid, ctx, Some(&loop_bindings), fn_bindings.as_deref_mut());
				}
			}
		}
	} else if node.kind() == "for_in_statement" {
		// for_in_statement: loop variable is the "left" field.
		if let Some(left) = node.child_by_field_name("left") {
			if left.kind() == "identifier" {
				// Shadow-only: no type info for loop variable.
				loop_bindings.insert(left.utf8_text(source).unwrap_or("").to_string(), None);
			}
		}
		// Extract calls from iterable (right side).
		if let Some(right) = node.child_by_field_name("right") {
			extract_calls_from_node(&right, source, caller_node_uid, ctx, Some(&loop_bindings), fn_bindings.as_deref_mut());
		}
	}

	// Process loop body with loop-scoped bindings.
	if let Some(body) = node.child_by_field_name("body") {
		extract_calls_from_node(&body, source, caller_node_uid, ctx, Some(&loop_bindings), fn_bindings.as_deref_mut());
	}
}

/// Collect var-declared names from a function body as shadow-only entries.
/// Models JavaScript var hoisting to function scope.
fn collect_var_bindings(
	node: &tree_sitter::Node,
	source: &[u8],
	bindings: &mut BindingMap,
) {
	for i in 0..node.child_count() {
		let child = match node.child(i) {
			Some(c) => c,
			None => continue,
		};
		// Stop at nested scope boundaries.
		if is_new_scope_node(&child) { continue; }

		if child.kind() == "variable_declaration" {
			for j in 0..child.child_count() {
				if let Some(decl) = child.child(j) {
					if decl.kind() == "variable_declarator" {
						if let Some(name) = decl.child_by_field_name("name") {
							bindings.insert(
								name.utf8_text(source).unwrap_or("").to_string(),
								None,
							);
						}
					}
				}
			}
		}

		// Recurse into non-scope children.
		collect_var_bindings(&child, source, bindings);
	}
}

/// Pre-scan a block for const/let names and install shadow-only (null) entries.
fn prescan_lexical_names(
	decl: &tree_sitter::Node,
	source: &[u8],
	bindings: &mut BindingMap,
) {
	for i in 0..decl.child_count() {
		let child = match decl.child(i) {
			Some(c) if c.kind() == "variable_declarator" => c,
			_ => continue,
		};
		if let Some(name_node) = child.child_by_field_name("name") {
			bindings.insert(name_node.utf8_text(source).unwrap_or("").to_string(), None);
		}
	}
}

/// Accumulate typed bindings from a declaration into a binding map.
fn accumulate_declaration_bindings(
	decl_node: &tree_sitter::Node,
	source: &[u8],
	bindings: &mut BindingMap,
) {
	for i in 0..decl_node.child_count() {
		let child = match decl_node.child(i) {
			Some(c) if c.kind() == "variable_declarator" => c,
			_ => continue,
		};
		let name_node = match child.child_by_field_name("name") {
			Some(n) => n,
			None => continue,
		};
		let name = name_node.utf8_text(source).unwrap_or("").to_string();

		if let Some(type_name) = extract_simple_type_name(&child, source) {
			bindings.insert(name, Some(type_name));
			continue;
		}

		if let Some(value) = child.child_by_field_name("value") {
			if value.kind() == "new_expression" {
				if let Some(ctor) = value.child_by_field_name("constructor") {
					bindings.insert(name, Some(ctor.utf8_text(source).unwrap_or("").to_string()));
					continue;
				}
			}
		}

		// No type → shadow only.
		bindings.insert(name, None);
	}
}

/// Extract CALLS/INSTANTIATES edges from a single AST node.
fn extract_calls_from_single_node(
	node: &tree_sitter::Node,
	source: &[u8],
	caller_node_uid: &str,
	ctx: &mut ExtractionCtx,
	local_bindings: Option<&BindingMap>,
	fn_bindings: Option<&BindingMap>,
) {
	if node.kind() == "call_expression" {
		if let Some(fn_node) = node.child_by_field_name("function") {
			if let Some(raw_name) = get_call_target_name(&fn_node, source) {
				if !is_builtin_call(&raw_name) {
					let callee_name = resolve_receiver_type(
						&raw_name, ctx, local_bindings, fn_bindings,
					);
					let metadata = if callee_name != raw_name {
						serde_json::json!({
							"calleeName": callee_name,
							"rawCalleeName": raw_name,
						})
					} else {
						serde_json::json!({"calleeName": callee_name})
					};
					ctx.edges.push(ExtractedEdge {
						edge_uid: uuid::Uuid::new_v4().to_string(),
						snapshot_uid: ctx.snapshot_uid.into(),
						repo_uid: ctx.repo_uid.into(),
						source_node_uid: caller_node_uid.into(),
						target_key: callee_name,
						edge_type: EdgeType::Calls,
						resolution: Resolution::Static,
						extractor: EXTRACTOR_NAME.into(),
						location: Some(location_from_node(node)),
						metadata_json: Some(metadata.to_string()),
					});
				}
			}

			// SB-3-pre: emit a ResolvedCallsite alongside the
			// CALLS edge when the callee resolves to a
			// (module, symbol) via import bindings AND arg[0]
			// matches one of the slice-1 payload patterns.
			//
			// This side-channel does NOT replace the CALLS
			// edge; it is an additional structured fact
			// consumed by state-extractor. The CALLS edge's
			// shape is unchanged (preserves cross-runtime
			// parity).
			//
			// Top-level-call suppression: `ResolvedCallsite`'s
			// contract says `enclosing_symbol_node_uid` is a
			// SYMBOL node's UID. Top-level statements in the
			// call-extraction pipeline pass the FILE node UID
			// as the caller. Emitting a ResolvedCallsite in
			// that case would stamp a FILE UID into a field
			// typed as "enclosing symbol." Slice-1 choice:
			// suppress these. Top-level state touches remain
			// visible as CALLS edges; they do not produce
			// state-boundary edges. Documented in the
			// `ResolvedCallsite` docstring as a slice-1
			// limitation.
			if caller_node_uid != ctx.file_node_uid {
				if let Some(resolved) = try_resolve_callsite(
					node,
					&fn_node,
					source,
					caller_node_uid,
					&ctx.import_bindings,
				) {
					ctx.resolved_callsites.push(resolved);
				}
			}
		}
	}

	if node.kind() == "new_expression" {
		if let Some(ctor_node) = node.child_by_field_name("constructor") {
			let class_name = ctor_node.utf8_text(source).unwrap_or("");
			if !class_name.is_empty() {
				ctx.edges.push(ExtractedEdge {
					edge_uid: uuid::Uuid::new_v4().to_string(),
					snapshot_uid: ctx.snapshot_uid.into(),
					repo_uid: ctx.repo_uid.into(),
					source_node_uid: caller_node_uid.into(),
					target_key: class_name.into(),
					edge_type: EdgeType::Instantiates,
					resolution: Resolution::Static,
					extractor: EXTRACTOR_NAME.into(),
					location: Some(location_from_node(node)),
					metadata_json: Some(
						serde_json::json!({"className": class_name}).to_string(),
					),
				});
			}
		}
	}
}

/// Get the call target name from the callee AST node.
fn get_call_target_name(fn_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
	match fn_node.kind() {
		"identifier" => {
			Some(fn_node.utf8_text(source).unwrap_or("").to_string())
		}
		"member_expression" => {
			let object = fn_node.child_by_field_name("object")?;
			let property = fn_node.child_by_field_name("property")?;
			let obj_text = object.utf8_text(source).unwrap_or("");
			let prop_text = property.utf8_text(source).unwrap_or("");
			Some(format!("{}.{}", obj_text, prop_text))
		}
		_ => None,
	}
}

// ── SB-3-pre: resolved-callsite extraction ──────────────────────
//
// Produces `ResolvedCallsite` facts for call expressions whose
// callee resolves to a (module, symbol) pair via the file's
// import bindings AND whose argument 0 matches one of the
// slice-1 payload patterns (string literal OR `process.env.NAME`
// member read).
//
// Non-matching calls produce no ResolvedCallsite; they remain
// represented as plain CALLS edges only. state-extractor filters
// further via the binding table.

/// Attempt to build a `ResolvedCallsite` from a call-expression
/// AST node. Returns `None` if callee-resolution or arg-0
/// classification fails.
fn try_resolve_callsite(
	call_node: &tree_sitter::Node,
	fn_node: &tree_sitter::Node,
	source: &[u8],
	enclosing_symbol_node_uid: &str,
	import_bindings: &[ImportBinding],
) -> Option<ResolvedCallsite> {
	let (resolved_module, resolved_symbol) =
		resolve_callee_via_imports(fn_node, source, import_bindings)?;
	let arg0_payload = classify_arg0_payload(call_node, source)?;
	Some(ResolvedCallsite {
		enclosing_symbol_node_uid: enclosing_symbol_node_uid.to_string(),
		resolved_module,
		resolved_symbol,
		arg0_payload,
		source_location: location_from_node(call_node),
	})
}

/// Resolve the callee AST node to a `(module, symbol)` pair via
/// the file's import bindings.
///
/// Supported patterns:
/// - Bare identifier (`readFile(...)`): matches a named import
///   binding by local identifier; resolves via `imported_name`
///   if aliased, else the identifier is the symbol.
/// - Member expression (`fs.readFile(...)`): matches a default or
///   namespace import binding by object identifier; the property
///   becomes the symbol.
///
/// Returns `None` when no binding anchors the callee, or when
/// the callee shape is outside the supported patterns.
fn resolve_callee_via_imports(
	fn_node: &tree_sitter::Node,
	source: &[u8],
	import_bindings: &[ImportBinding],
) -> Option<(String, String)> {
	match fn_node.kind() {
		"identifier" => {
			let ident = fn_node.utf8_text(source).ok()?.to_string();
			// Look up a named-import binding whose local
			// identifier matches. Only named imports resolve
			// this way (they have imported_name populated);
			// a default / namespace binding that happens to
			// share the identifier would have imported_name =
			// None and we do NOT resolve those at the bare-
			// identifier callsite (a default import is only
			// called via member access in JS/TS).
			let binding = import_bindings
				.iter()
				.find(|b| b.identifier == ident && b.imported_name.is_some())?;
			let symbol = binding
				.imported_name
				.clone()
				.expect("checked is_some() above");
			Some((binding.specifier.clone(), symbol))
		}
		"member_expression" => {
			let object = fn_node.child_by_field_name("object")?;
			let property = fn_node.child_by_field_name("property")?;
			// The object itself must be a plain identifier
			// corresponding to an import binding. Deeper
			// member expressions (e.g., `a.b.c()`) are out of
			// slice-1 scope.
			if object.kind() != "identifier" {
				return None;
			}
			let obj_text = object.utf8_text(source).ok()?.to_string();
			let prop_text = property.utf8_text(source).ok()?.to_string();
			// The binding must be a default or namespace
			// import (imported_name = None). A named import
			// called via member access (`named.method()`)
			// would be the named symbol itself reached
			// through its own methods — not in slice-1 scope.
			let binding = import_bindings
				.iter()
				.find(|b| b.identifier == obj_text && b.imported_name.is_none())?;
			Some((binding.specifier.clone(), prop_text))
		}
		_ => None,
	}
}

/// Classify the argument-0 payload of a call expression.
///
/// Supported patterns (SB-3-pre):
/// - String literal: `foo("bar")` → `StringLiteral { value: "bar" }`.
/// - `process.env.NAME` member read:
///   `foo(process.env.DATABASE_URL)` →
///   `EnvKeyRead { key_name: "DATABASE_URL" }`.
///
/// Returns `None` for any other shape (variable reference,
/// template literal, computed expression, missing argument, etc.).
fn classify_arg0_payload(
	call_node: &tree_sitter::Node,
	source: &[u8],
) -> Option<Arg0Payload> {
	let args_node = call_node.child_by_field_name("arguments")?;
	// tree-sitter's `arguments` node contains `(` + args + `)`.
	// Iterate its named children to find arg index 0.
	let mut cursor = args_node.walk();
	let mut arg0: Option<tree_sitter::Node> = None;
	for child in args_node.children(&mut cursor) {
		if child.is_named() {
			arg0 = Some(child);
			break;
		}
	}
	let arg0 = arg0?;
	match arg0.kind() {
		"string" => {
			// tree-sitter's string node wraps quoted content.
			// Strip outer quotes by reading string_fragment if
			// present, else fall back to slicing the literal.
			let literal_text = arg0.utf8_text(source).ok()?;
			let stripped = literal_text
				.trim_matches(|c| c == '\'' || c == '"' || c == '`')
				.to_string();
			Some(Arg0Payload::StringLiteral { value: stripped })
		}
		"member_expression" => {
			// Pattern: `process.env.NAME`.
			let object = arg0.child_by_field_name("object")?;
			let property = arg0.child_by_field_name("property")?;
			// The object must itself be `process.env`.
			if object.kind() != "member_expression" {
				return None;
			}
			let inner_object = object.child_by_field_name("object")?;
			let inner_property = object.child_by_field_name("property")?;
			if inner_object.utf8_text(source).ok()? != "process" {
				return None;
			}
			if inner_property.utf8_text(source).ok()? != "env" {
				return None;
			}
			let key_name = property.utf8_text(source).ok()?.to_string();
			Some(Arg0Payload::EnvKeyRead { key_name })
		}
		_ => None,
	}
}

/// Check if a node starts a new scope (don't recurse into these
/// during call extraction).
fn is_new_scope_node(node: &tree_sitter::Node) -> bool {
	matches!(
		node.kind(),
		"function_declaration"
			| "class_declaration"
			| "arrow_function"
			| "function_expression"
			| "method_definition"
	)
}

/// Check if a call target is a known JS/TS builtin that should
/// not produce a CALLS edge.
fn is_builtin_call(name: &str) -> bool {
	matches!(
		name,
		"console.log"
			| "console.error"
			| "console.warn"
			| "console.info"
			| "console.debug"
			| "JSON.parse"
			| "JSON.stringify"
			| "Object.keys"
			| "Object.values"
			| "Object.entries"
			| "Object.assign"
			| "Object.freeze"
			| "Array.isArray"
			| "Array.from"
			| "Promise.resolve"
			| "Promise.reject"
			| "Promise.all"
			| "Promise.allSettled"
			| "Math.floor"
			| "Math.ceil"
			| "Math.round"
			| "Math.max"
			| "Math.min"
			| "Math.abs"
			| "parseInt"
			| "parseFloat"
			| "setTimeout"
			| "setInterval"
			| "clearTimeout"
			| "clearInterval"
			| "require"
	)
}

fn location_from_node(node: &tree_sitter::Node) -> SourceLocation {
	SourceLocation {
		line_start: node.start_position().row as i64 + 1,
		col_start: node.start_position().column as i64,
		line_end: node.end_position().row as i64 + 1,
		col_end: node.end_position().column as i64,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn initialize_succeeds() {
		let mut ext = TsExtractor::new();
		assert!(ext.initialize().is_ok());
	}

	#[test]
	fn extract_before_initialize_returns_error() {
		let ext = TsExtractor::new();
		let result = ext.extract("const x = 1;", "test.ts", "r1:test.ts", "r1", "snap1");
		match result {
			Err(e) => assert!(
				e.message.contains("not initialized"),
				"expected 'not initialized' error, got: {}",
				e.message
			),
			Ok(_) => panic!("expected error for uninitialized extractor"),
		}
	}

	#[test]
	fn extract_typescript_parses_without_error() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = ext.extract(
			"export function hello(): string { return 'world'; }",
			"src/hello.ts",
			"r1:src/hello.ts",
			"r1",
			"snap1",
		);
		assert!(result.is_ok());
	}

	#[test]
	fn extract_tsx_parses_without_error() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = ext.extract(
			"export function App() { return <div>Hello</div>; }",
			"src/App.tsx",
			"r1:src/App.tsx",
			"r1",
			"snap1",
		);
		assert!(result.is_ok());
	}

	#[test]
	fn extract_javascript_parses_without_error() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = ext.extract(
			"function greet(name) { return 'Hello ' + name; }",
			"src/greet.js",
			"r1:src/greet.js",
			"r1",
			"snap1",
		);
		assert!(result.is_ok());
	}

	#[test]
	fn extract_jsx_parses_without_error() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = ext.extract(
			"function App() { return <div>Hello</div>; }",
			"src/App.jsx",
			"r1:src/App.jsx",
			"r1",
			"snap1",
		);
		assert!(result.is_ok());
	}

	#[test]
	fn extract_malformed_source_still_returns_ok() {
		// tree-sitter produces partial trees with ERROR nodes for
		// syntactically invalid source. The extractor must not
		// reject these — it extracts whatever the visitor finds.
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = ext.extract(
			"function { broken syntax export class ;;; }}}",
			"src/broken.ts",
			"r1:src/broken.ts",
			"r1",
			"snap1",
		);
		assert!(
			result.is_ok(),
			"malformed source must produce Ok with partial extraction, got {:?}",
			result.err()
		);
	}

	#[test]
	fn languages_includes_all_four() {
		let ext = TsExtractor::new();
		let langs = ext.languages();
		assert_eq!(langs.len(), 4);
		assert!(langs.contains(&"typescript".to_string()));
		assert!(langs.contains(&"tsx".to_string()));
		assert!(langs.contains(&"javascript".to_string()));
		assert!(langs.contains(&"jsx".to_string()));
	}

	#[test]
	fn name_matches_ts_extractor_version() {
		let ext = TsExtractor::new();
		assert_eq!(ext.name(), "ts-core:0.2.0");
	}

	// ── R6-B: FILE node + imports ────────────────────────────

	fn extract_ok(ext: &TsExtractor, source: &str, path: &str) -> ExtractionResult {
		ext.extract(source, path, &format!("r1:{}", path), "r1", "snap1")
			.expect("extraction should succeed")
	}

	#[test]
	fn file_node_has_correct_stable_key_and_fields() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(&ext, "const x = 1;\n", "src/index.ts");

		// FILE node + CONSTANT node for `const x`.
		assert!(result.nodes.len() >= 1);
		let file_node = &result.nodes[0];
		assert_eq!(file_node.stable_key, "r1:src/index.ts:FILE");
		assert_eq!(file_node.kind, NodeKind::File);
		assert_eq!(file_node.subtype, Some(NodeSubtype::Source));
		assert_eq!(file_node.name, "index.ts");
		assert_eq!(file_node.qualified_name.as_deref(), Some("src/index.ts"));
		assert_eq!(file_node.file_uid.as_deref(), Some("r1:src/index.ts"));
		// Location: line 1 to line 1 (single line with newline).
		let loc = file_node.location.unwrap();
		assert_eq!(loc.line_start, 1);
		assert_eq!(loc.col_start, 0);
	}

	#[test]
	fn relative_import_produces_edge_and_binding() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import { Foo } from "./types";"#,
			"src/service.ts",
		);

		// One IMPORTS edge for relative import.
		assert_eq!(result.edges.len(), 1);
		let edge = &result.edges[0];
		assert_eq!(edge.edge_type, EdgeType::Imports);
		assert_eq!(edge.target_key, "r1:src/types:FILE");
		assert_eq!(edge.resolution, Resolution::Static);

		// Metadata has rawPath and resolvedPath.
		let meta: serde_json::Value =
			serde_json::from_str(edge.metadata_json.as_ref().unwrap()).unwrap();
		assert_eq!(meta["rawPath"], "./types");
		assert_eq!(meta["resolvedPath"], "src/types");

		// One ImportBinding.
		assert_eq!(result.import_bindings.len(), 1);
		assert_eq!(result.import_bindings[0].identifier, "Foo");
		assert_eq!(result.import_bindings[0].specifier, "./types");
		assert!(result.import_bindings[0].is_relative);
		assert!(!result.import_bindings[0].is_type_only);
		// Named import without alias: imported_name equals the
		// local identifier (original exported name).
		assert_eq!(
			result.import_bindings[0].imported_name.as_deref(),
			Some("Foo")
		);
	}

	#[test]
	fn non_relative_import_produces_binding_only() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import express from "express";"#,
			"src/app.ts",
		);

		// No IMPORTS edge (non-relative).
		assert_eq!(result.edges.len(), 0);

		// ImportBinding still produced.
		assert_eq!(result.import_bindings.len(), 1);
		assert_eq!(result.import_bindings[0].identifier, "express");
		assert_eq!(result.import_bindings[0].specifier, "express");
		assert!(!result.import_bindings[0].is_relative);
		// Default import: imported_name is None (no specific
		// exported symbol; the whole module surface is imported).
		assert_eq!(result.import_bindings[0].imported_name, None);
	}

	#[test]
	fn type_only_import_sets_is_type_only() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import type { User } from "./types";"#,
			"src/service.ts",
		);

		assert_eq!(result.import_bindings.len(), 1);
		assert!(result.import_bindings[0].is_type_only);
	}

	#[test]
	fn named_imports_with_alias() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import { foo as bar, baz } from "./utils";"#,
			"src/index.ts",
		);

		assert_eq!(result.import_bindings.len(), 2);
		// Alias takes priority for local identifier.
		assert_eq!(result.import_bindings[0].identifier, "bar");
		assert_eq!(result.import_bindings[1].identifier, "baz");
		// Aliased import preserves the original exported name.
		assert_eq!(
			result.import_bindings[0].imported_name.as_deref(),
			Some("foo"),
			"aliased import must preserve original exported name"
		);
		// Non-aliased named import: imported_name equals local name.
		assert_eq!(
			result.import_bindings[1].imported_name.as_deref(),
			Some("baz")
		);
	}

	#[test]
	fn namespace_import() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import * as utils from "./utils";"#,
			"src/index.ts",
		);

		assert_eq!(result.import_bindings.len(), 1);
		assert_eq!(result.import_bindings[0].identifier, "utils");
		// Namespace import: imported_name is None (whole module
		// surface; symbol comes from member expression at call
		// site).
		assert_eq!(result.import_bindings[0].imported_name, None);
	}

	#[test]
	fn import_strips_js_extension() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import { Foo } from "./types.js";"#,
			"src/service.ts",
		);

		assert_eq!(result.edges.len(), 1);
		assert_eq!(result.edges[0].target_key, "r1:src/types:FILE");

		let meta: serde_json::Value =
			serde_json::from_str(result.edges[0].metadata_json.as_ref().unwrap())
				.unwrap();
		assert_eq!(meta["resolvedPath"], "src/types");
	}

	#[test]
	fn import_resolves_parent_directory() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import { Config } from "../config";"#,
			"src/core/service.ts",
		);

		assert_eq!(result.edges.len(), 1);
		assert_eq!(result.edges[0].target_key, "r1:src/config:FILE");
	}

	#[test]
	fn side_effect_import_produces_no_binding() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"import "./polyfill";"#,
			"src/index.ts",
		);

		// Edge still produced (relative).
		assert_eq!(result.edges.len(), 1);
		// But no ImportBinding (no identifier).
		assert_eq!(result.import_bindings.len(), 0);
	}

	// ── resolve_import_path unit tests ───────────────────────

	#[test]
	fn resolve_path_sibling() {
		assert_eq!(
			resolve_import_path("./utils", "src/service.ts"),
			Some("src/utils".into())
		);
	}

	#[test]
	fn resolve_path_strips_jsx() {
		assert_eq!(
			resolve_import_path("./App.jsx", "src/index.ts"),
			Some("src/App".into())
		);
	}

	// ── P2 regression: re-export produces IMPORTS edge ───────

	#[test]
	fn reexport_produces_imports_edge() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"export { Foo } from "./types";"#,
			"src/index.ts",
		);

		// Re-export should produce an IMPORTS edge.
		assert_eq!(result.edges.len(), 1);
		assert_eq!(result.edges[0].edge_type, EdgeType::Imports);
		assert_eq!(result.edges[0].target_key, "r1:src/types:FILE");

		// No ImportBinding for re-exports (no import_clause).
		assert_eq!(result.import_bindings.len(), 0);
	}

	#[test]
	fn reexport_non_relative_no_edge() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"export { useState } from "react";"#,
			"src/index.ts",
		);

		// Non-relative re-export: no edge.
		assert_eq!(result.edges.len(), 0);
		assert_eq!(result.import_bindings.len(), 0);
	}

	// ── P2 regression: trailing newline line count ────────────

	#[test]
	fn file_node_line_count_matches_ts_on_trailing_newline() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		// Source with trailing newline: TS reports lineEnd = 2.
		let result = extract_ok(&ext, "const x = 1;\n", "src/a.ts");
		let loc = result.nodes[0].location.unwrap();
		assert_eq!(
			loc.line_end, 2,
			"trailing newline should produce lineEnd=2 (TS split('\\n').length)"
		);
	}

	#[test]
	fn file_node_line_count_no_trailing_newline() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(&ext, "const x = 1;", "src/a.ts");
		let loc = result.nodes[0].location.unwrap();
		assert_eq!(loc.line_end, 1);
	}

	// ── R6-C: function/variable extraction ───────────────────

	#[test]
	fn function_declaration_produces_symbol_node() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"function greet(name: string): string { return name; }",
			"src/greet.ts",
		);

		let func = result.nodes.iter().find(|n| n.name == "greet").unwrap();
		assert_eq!(func.stable_key, "r1:src/greet.ts#greet:SYMBOL:FUNCTION");
		assert_eq!(func.kind, NodeKind::Symbol);
		assert_eq!(func.subtype, Some(NodeSubtype::Function));
		assert_eq!(func.visibility, Some(Visibility::Private));
		assert!(func.signature.as_ref().unwrap().contains("(name: string)"));
	}

	#[test]
	fn exported_function_has_export_visibility() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"export function serve() {}",
			"src/server.ts",
		);

		let func = result.nodes.iter().find(|n| n.name == "serve").unwrap();
		assert_eq!(func.visibility, Some(Visibility::Export));
	}

	#[test]
	fn const_produces_constant_node() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"const MAX_SIZE = 1000;",
			"src/config.ts",
		);

		let c = result.nodes.iter().find(|n| n.name == "MAX_SIZE").unwrap();
		assert_eq!(c.stable_key, "r1:src/config.ts#MAX_SIZE:SYMBOL:CONSTANT");
		assert_eq!(c.subtype, Some(NodeSubtype::Constant));
		assert_eq!(c.visibility, Some(Visibility::Private));
	}

	#[test]
	fn let_produces_variable_node() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"let counter = 0;",
			"src/state.ts",
		);

		let v = result.nodes.iter().find(|n| n.name == "counter").unwrap();
		assert_eq!(v.subtype, Some(NodeSubtype::Variable));
	}

	#[test]
	fn arrow_function_const_produces_function_node() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"const greet = (name: string) => name;",
			"src/utils.ts",
		);

		let f = result.nodes.iter().find(|n| n.name == "greet").unwrap();
		assert_eq!(f.subtype, Some(NodeSubtype::Function));
		assert_eq!(f.stable_key, "r1:src/utils.ts#greet:SYMBOL:FUNCTION");
	}

	#[test]
	fn export_list_updates_visibility() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"function internal() {}\nfunction public_fn() {}\nexport { public_fn };",
			"src/mod.ts",
		);

		let internal = result.nodes.iter().find(|n| n.name == "internal").unwrap();
		assert_eq!(internal.visibility, Some(Visibility::Private));

		let public = result.nodes.iter().find(|n| n.name == "public_fn").unwrap();
		assert_eq!(public.visibility, Some(Visibility::Export));
	}

	#[test]
	fn doc_comment_extracted() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"/** Greets a user. */\nfunction greet() {}",
			"src/greet.ts",
		);

		let func = result.nodes.iter().find(|n| n.name == "greet").unwrap();
		assert!(
			func.doc_comment.as_ref().unwrap().contains("Greets a user"),
			"doc comment should be extracted"
		);
	}

	// ── R6-D: class extraction ───────────────────────────────

	#[test]
	fn class_produces_class_node_and_members() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"class UserService {
				private repo: any;
				constructor(repo: any) { this.repo = repo; }
				getUser(id: string) { return this.repo.find(id); }
			}"#,
			"src/service.ts",
		);

		let class = result.nodes.iter().find(|n| n.name == "UserService").unwrap();
		assert_eq!(class.stable_key, "r1:src/service.ts#UserService:SYMBOL:CLASS");
		assert_eq!(class.subtype, Some(NodeSubtype::Class));
		assert_eq!(class.visibility, Some(Visibility::Private));

		let ctor = result.nodes.iter().find(|n| n.name == "constructor").unwrap();
		assert_eq!(ctor.subtype, Some(NodeSubtype::Constructor));
		assert_eq!(
			ctor.stable_key,
			"r1:src/service.ts#UserService.constructor:SYMBOL:CONSTRUCTOR"
		);
		assert_eq!(ctor.parent_node_uid.as_deref(), Some(class.node_uid.as_str()));

		let method = result.nodes.iter().find(|n| n.name == "getUser").unwrap();
		assert_eq!(method.subtype, Some(NodeSubtype::Method));
		assert_eq!(method.qualified_name.as_deref(), Some("UserService.getUser"));
		assert_eq!(method.parent_node_uid.as_deref(), Some(class.node_uid.as_str()));
		// Methods default to PUBLIC.
		assert_eq!(method.visibility, Some(Visibility::Public));

		let prop = result.nodes.iter().find(|n| n.name == "repo").unwrap();
		assert_eq!(prop.subtype, Some(NodeSubtype::Property));
		assert_eq!(prop.visibility, Some(Visibility::Private));
	}

	#[test]
	fn exported_class() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"export class Service {}",
			"src/service.ts",
		);

		let class = result.nodes.iter().find(|n| n.name == "Service").unwrap();
		assert_eq!(class.visibility, Some(Visibility::Export));
	}

	#[test]
	fn class_implements_produces_edge() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"class Adapter implements StoragePort, LoggerPort {}",
			"src/adapter.ts",
		);

		let impl_edges: Vec<_> = result
			.edges
			.iter()
			.filter(|e| e.edge_type == EdgeType::Implements)
			.collect();
		assert_eq!(impl_edges.len(), 2);
		assert_eq!(impl_edges[0].target_key, "StoragePort");
		assert_eq!(impl_edges[1].target_key, "LoggerPort");

		// Metadata check.
		let meta: serde_json::Value =
			serde_json::from_str(impl_edges[0].metadata_json.as_ref().unwrap()).unwrap();
		assert_eq!(meta["targetName"], "StoragePort");
	}

	#[test]
	fn getter_setter_subtypes() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"class Cfg {
				get name() { return "x"; }
				set name(v: string) {}
			}"#,
			"src/cfg.ts",
		);

		let getter = result.nodes.iter().find(|n| {
			n.name == "name" && n.subtype == Some(NodeSubtype::Getter)
		});
		assert!(getter.is_some(), "should find getter");

		let setter = result.nodes.iter().find(|n| {
			n.name == "name" && n.subtype == Some(NodeSubtype::Setter)
		});
		assert!(setter.is_some(), "should find setter");
	}

	// ── R6-E: interface/type/enum extraction ─────────────────

	#[test]
	fn interface_with_members() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"export interface StoragePort {
				getUser(id: string): any;
				name: string;
			}"#,
			"src/ports.ts",
		);

		let iface = result.nodes.iter().find(|n| n.name == "StoragePort").unwrap();
		assert_eq!(iface.stable_key, "r1:src/ports.ts#StoragePort:SYMBOL:INTERFACE");
		assert_eq!(iface.subtype, Some(NodeSubtype::Interface));
		assert_eq!(iface.visibility, Some(Visibility::Export));

		let method = result.nodes.iter().find(|n| n.name == "getUser").unwrap();
		assert_eq!(method.subtype, Some(NodeSubtype::Method));
		assert_eq!(method.qualified_name.as_deref(), Some("StoragePort.getUser"));
		assert_eq!(method.parent_node_uid.as_deref(), Some(iface.node_uid.as_str()));
		assert_eq!(method.visibility, Some(Visibility::Public));

		let prop = result.nodes.iter().find(|n| n.name == "name" && n.subtype == Some(NodeSubtype::Property)).unwrap();
		assert_eq!(prop.qualified_name.as_deref(), Some("StoragePort.name"));
		assert_eq!(prop.visibility, Some(Visibility::Public));
	}

	#[test]
	fn interface_overloaded_methods_deduplicated() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"interface Api {
				fetch(id: string): any;
				fetch(id: number): any;
			}"#,
			"src/api.ts",
		);

		let methods: Vec<_> = result.nodes.iter()
			.filter(|n| n.name == "fetch" && n.subtype == Some(NodeSubtype::Method))
			.collect();
		assert_eq!(methods.len(), 1, "overloads should be deduplicated to one node");
	}

	#[test]
	fn type_alias_extraction() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"export type UserId = string;",
			"src/types.ts",
		);

		let ta = result.nodes.iter().find(|n| n.name == "UserId").unwrap();
		assert_eq!(ta.stable_key, "r1:src/types.ts#UserId:SYMBOL:TYPE_ALIAS");
		assert_eq!(ta.subtype, Some(NodeSubtype::TypeAlias));
		assert_eq!(ta.visibility, Some(Visibility::Export));
	}

	#[test]
	fn enum_extraction() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"enum Status { Active, Inactive }",
			"src/enums.ts",
		);

		let e = result.nodes.iter().find(|n| n.name == "Status").unwrap();
		assert_eq!(e.stable_key, "r1:src/enums.ts#Status:SYMBOL:ENUM");
		assert_eq!(e.subtype, Some(NodeSubtype::Enum));
		assert_eq!(e.visibility, Some(Visibility::Private));
	}

	// ── R6-F: call extraction ────────────────────────────────

	#[test]
	fn function_call_produces_calls_edge() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"function main() { doStuff(); }",
			"src/main.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "doStuff");

		let meta: serde_json::Value =
			serde_json::from_str(calls[0].metadata_json.as_ref().unwrap()).unwrap();
		assert_eq!(meta["calleeName"], "doStuff");
	}

	#[test]
	fn member_call_produces_dotted_target() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"function work() { this.repo.findById('x'); }",
			"src/work.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "this.repo.findById");
	}

	#[test]
	fn new_expression_produces_instantiates_edge() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"function create() { return new UserService(); }",
			"src/factory.ts",
		);

		let insts: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Instantiates)
			.collect();
		assert_eq!(insts.len(), 1);
		assert_eq!(insts[0].target_key, "UserService");

		let meta: serde_json::Value =
			serde_json::from_str(insts[0].metadata_json.as_ref().unwrap()).unwrap();
		assert_eq!(meta["className"], "UserService");
	}

	#[test]
	fn builtin_calls_filtered() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"function log() { console.log('hi'); JSON.stringify({}); doWork(); }",
			"src/log.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		// console.log and JSON.stringify are builtins → filtered.
		// Only doWork remains.
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "doWork");
	}

	#[test]
	fn nested_function_scope_boundary() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"function outer() {
				function inner() { innerCall(); }
				outerCall();
			}"#,
			"src/nested.ts",
		);

		// outer is a top-level declaration → extracted as a node.
		// inner is nested → NOT extracted (only top-level gets nodes).
		let outer = result.nodes.iter().find(|n| n.name == "outer").unwrap();

		// outer's body walk should find outerCall but NOT innerCall
		// (inner's function_declaration is a scope boundary → skipped).
		let outer_calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls && e.source_node_uid == outer.node_uid)
			.collect();
		assert_eq!(outer_calls.len(), 1);
		assert_eq!(outer_calls[0].target_key, "outerCall");
	}

	#[test]
	fn top_level_expression_call() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"app.listen(3000);",
			"src/index.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "app.listen");
	}

	#[test]
	fn this_method_rewritten_to_class_name() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"class UserService {
				save() { this.validate(); }
				validate() {}
			}"#,
			"src/service.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "UserService.validate");

		let meta: serde_json::Value =
			serde_json::from_str(calls[0].metadata_json.as_ref().unwrap()).unwrap();
		assert_eq!(meta["rawCalleeName"], "this.validate");
	}

	#[test]
	fn this_property_method_rewritten_via_class_binding() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"class Handler {
				private repo: UserRepository;
				handle() { this.repo.findById("x"); }
			}"#,
			"src/handler.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "UserRepository.findById");
	}

	#[test]
	fn param_binding_resolves_variable_call() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"function work(storage: StoragePort) { storage.insert(); }",
			"src/work.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "StoragePort.insert");
	}

	#[test]
	fn file_scope_binding_resolves_const() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"const db: Database = getDb();
			function query() { db.execute(); }"#,
			"src/db.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls && e.target_key == "Database.execute")
			.collect();
		assert_eq!(calls.len(), 1);
	}

	#[test]
	fn const_initializer_call() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			"const result = compute(42);",
			"src/init.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert_eq!(calls.len(), 1);
		assert_eq!(calls[0].target_key, "compute");
	}

	// ── Sequential block binding tests ───────────────────────

	#[test]
	fn block_local_const_binding_resolves_subsequent_call() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"function work() {
				const repo = new UserRepo();
				repo.find("x");
			}"#,
			"src/work.ts",
		);

		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		// repo.find should be rewritten to UserRepo.find.
		let find_call = calls.iter().find(|e| e.target_key.contains("find"));
		assert!(find_call.is_some(), "should find a find call, got: {:?}", calls.iter().map(|c| &c.target_key).collect::<Vec<_>>());
		assert_eq!(find_call.unwrap().target_key, "UserRepo.find");
	}

	#[test]
	fn for_loop_initializer_binding_resolves_in_body() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"function run() {
				for (const iter = new Iterator(); iter.hasNext(); iter.next()) {
					iter.process();
				}
			}"#,
			"src/run.ts",
		);

		// iter should resolve to Iterator in all three positions.
		let calls: Vec<_> = result.edges.iter()
			.filter(|e| e.edge_type == EdgeType::Calls && e.target_key.starts_with("Iterator."))
			.collect();
		assert!(
			calls.len() >= 2,
			"expected >=2 Iterator.* calls, got {} from {:?}",
			calls.len(),
			result.edges.iter().map(|e| &e.target_key).collect::<Vec<_>>()
		);
	}

	#[test]
	fn block_local_typed_const_binding() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"function process() {
				const db: Database = getDb();
				db.query("SELECT 1");
			}"#,
			"src/process.ts",
		);

		let query_call = result.edges.iter()
			.find(|e| e.edge_type == EdgeType::Calls && e.target_key.contains("query"));
		assert!(query_call.is_some());
		assert_eq!(query_call.unwrap().target_key, "Database.query");
	}

	// ── R6-G: metrics integration ────────────────────────────

	#[test]
	fn metrics_populated_for_function() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"function complex(a: string, b: number) {
				if (a) {
					for (const x of [1,2]) {
						if (b > 0 && x > 0) {}
					}
				}
			}"#,
			"src/complex.ts",
		);

		let func = result.nodes.iter().find(|n| n.name == "complex").unwrap();
		let m = result.metrics.get(&func.stable_key).unwrap();
		assert_eq!(m.parameter_count, 2);
		// 1 (base) + if + for_in + if + && = 5
		assert_eq!(m.cyclomatic_complexity, 5);
		// if -> for -> if = depth 3
		assert_eq!(m.max_nesting_depth, 3);
	}

	#[test]
	fn metrics_populated_for_method() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"class Svc {
				process(items: any[]) {
					for (const item of items) {
						if (item) {}
					}
				}
			}"#,
			"src/svc.ts",
		);

		let method = result.nodes.iter().find(|n| n.name == "process").unwrap();
		let m = result.metrics.get(&method.stable_key).unwrap();
		assert_eq!(m.parameter_count, 1);
		// 1 + for_in + if = 3
		assert_eq!(m.cyclomatic_complexity, 3);
		assert_eq!(m.max_nesting_depth, 2);
	}

	// ══════════════════════════════════════════════════════════════
	//  SB-3-pre: ResolvedCallsite extraction
	// ══════════════════════════════════════════════════════════════

	#[test]
	fn resolved_callsite_named_import_with_string_literal_arg0() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile } from "fs";
export function load() {
  readFile("/etc/config", () => {});
}
"#,
			"src/load.ts",
		);

		assert_eq!(result.resolved_callsites.len(), 1);
		let rc = &result.resolved_callsites[0];
		assert_eq!(rc.resolved_module, "fs");
		assert_eq!(rc.resolved_symbol, "readFile");
		match &rc.arg0_payload {
			Arg0Payload::StringLiteral { value } => {
				assert_eq!(value, "/etc/config");
			}
			other => panic!("expected StringLiteral, got {:?}", other),
		}
	}

	#[test]
	fn resolved_callsite_aliased_named_import_recovers_original_symbol() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile as rf } from "fs";
export function load() {
  rf("/etc/config", () => {});
}
"#,
			"src/load.ts",
		);

		assert_eq!(result.resolved_callsites.len(), 1);
		let rc = &result.resolved_callsites[0];
		// resolved_symbol is the ORIGINAL exported name, not the
		// local alias.
		assert_eq!(rc.resolved_module, "fs");
		assert_eq!(rc.resolved_symbol, "readFile");
	}

	#[test]
	fn resolved_callsite_default_import_member_call() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import fs from "fs";
export function load() {
  fs.readFile("/etc/config", () => {});
}
"#,
			"src/load.ts",
		);

		assert_eq!(result.resolved_callsites.len(), 1);
		let rc = &result.resolved_callsites[0];
		assert_eq!(rc.resolved_module, "fs");
		assert_eq!(rc.resolved_symbol, "readFile");
	}

	#[test]
	fn resolved_callsite_namespace_import_member_call() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import * as fs from "fs";
export function load() {
  fs.readFile("/etc/config", () => {});
}
"#,
			"src/load.ts",
		);

		assert_eq!(result.resolved_callsites.len(), 1);
		let rc = &result.resolved_callsites[0];
		assert_eq!(rc.resolved_module, "fs");
		assert_eq!(rc.resolved_symbol, "readFile");
	}

	#[test]
	fn resolved_callsite_env_key_read_arg0() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile } from "fs";
export function load() {
  readFile(process.env.CACHE_DIR, () => {});
}
"#,
			"src/load.ts",
		);

		assert_eq!(result.resolved_callsites.len(), 1);
		let rc = &result.resolved_callsites[0];
		match &rc.arg0_payload {
			Arg0Payload::EnvKeyRead { key_name } => {
				assert_eq!(key_name, "CACHE_DIR");
			}
			other => panic!("expected EnvKeyRead, got {:?}", other),
		}
	}

	#[test]
	fn no_resolved_callsite_when_callee_is_unresolved_identifier() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
export function load() {
  somethingNotImported("/etc/config");
}
"#,
			"src/load.ts",
		);
		assert!(
			result.resolved_callsites.is_empty(),
			"unresolved callee must not produce a ResolvedCallsite"
		);
	}

	#[test]
	fn no_resolved_callsite_when_arg0_is_variable_reference() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile } from "fs";
export function load(path: string) {
  readFile(path, () => {});
}
"#,
			"src/load.ts",
		);
		assert!(
			result.resolved_callsites.is_empty(),
			"non-literal, non-env arg0 must not produce a ResolvedCallsite"
		);
	}

	#[test]
	fn no_resolved_callsite_for_non_process_env_member_reads() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile } from "fs";
const config = { CACHE_DIR: "/tmp" };
export function load() {
  readFile(config.CACHE_DIR, () => {});
}
"#,
			"src/load.ts",
		);
		assert!(
			result.resolved_callsites.is_empty(),
			"non-process.env member reads must not produce a ResolvedCallsite"
		);
	}

	#[test]
	fn multiple_resolved_callsites_in_same_function() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile, writeFile } from "fs";
export function io() {
  readFile("/etc/in", () => {});
  writeFile("/etc/out", "data", () => {});
}
"#,
			"src/io.ts",
		);

		assert_eq!(result.resolved_callsites.len(), 2);
		let symbols: Vec<&str> = result
			.resolved_callsites
			.iter()
			.map(|rc| rc.resolved_symbol.as_str())
			.collect();
		assert!(symbols.contains(&"readFile"));
		assert!(symbols.contains(&"writeFile"));
	}

	#[test]
	fn top_level_call_does_not_produce_resolved_callsite() {
		// SB-3-pre slice-1 limitation: the call-extraction
		// pipeline uses the FILE node UID as the caller for
		// top-level statements. `ResolvedCallsite.enclosing_symbol_node_uid`
		// is contractually a SYMBOL node UID; to honor the
		// contract, top-level calls MUST NOT produce a
		// `ResolvedCallsite`. The CALLS edge is still emitted
		// normally (rooted at the FILE node).
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile } from "fs";
readFile("/etc/config", () => {});
"#,
			"src/top.ts",
		);

		// The CALLS edge for the top-level call still exists
		// (rooted at the FILE node).
		let calls_edges: Vec<&ExtractedEdge> = result
			.edges
			.iter()
			.filter(|e| e.edge_type == EdgeType::Calls)
			.collect();
		assert!(
			!calls_edges.is_empty(),
			"top-level call must still produce a CALLS edge"
		);

		// But no ResolvedCallsite, per slice-1 limitation.
		assert!(
			result.resolved_callsites.is_empty(),
			"top-level calls must NOT produce ResolvedCallsite facts"
		);
	}

	#[test]
	fn resolved_callsite_enclosing_symbol_is_containing_function() {
		let mut ext = TsExtractor::new();
		ext.initialize().unwrap();
		let result = extract_ok(
			&ext,
			r#"
import { readFile } from "fs";
export function load() {
  readFile("/etc/config", () => {});
}
"#,
			"src/load.ts",
		);

		// Look up the `load` function node.
		let load_fn = result
			.nodes
			.iter()
			.find(|n| n.name == "load")
			.expect("load function node");
		assert_eq!(result.resolved_callsites.len(), 1);
		assert_eq!(
			result.resolved_callsites[0].enclosing_symbol_node_uid,
			load_fn.node_uid,
			"ResolvedCallsite's enclosing_symbol_node_uid must match the containing function's node_uid"
		);
	}
}
