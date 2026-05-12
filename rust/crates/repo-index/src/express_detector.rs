//! FD-1A: Express route detection for TS/JS files.
//!
//! Detects Express route registrations (`app.get()`, `router.post()`, etc.)
//! and produces `CreateProjectSurfaceInput` records for persistence.
//!
//! Detection patterns:
//! - `app.get('/path', handler)` — route registration
//! - `router.post('/path', handler)` — router route
//! - `app.use('/prefix', middleware)` — middleware mount
//!
//! Receiver validation:
//! - Requires conventional receiver names: `app`, `router`, `server`
//! - Requires file to contain Express import indicator
//!
//! Limitations (first-cut scope):
//! - Does not handle dynamic paths (template strings with variables)
//! - Does not compose router mount prefixes
//! - Does not perform deep middleware analysis

use repo_graph_indexer::jsts_extensions::{
    get_extension, is_jsts_extension, is_jsts_jsx_extension,
};
use repo_graph_indexer::orchestrator::FileInput;
use repo_graph_storage::types::CreateProjectSurfaceInput;
use tree_sitter::{Node, Parser};

// ── Constants ─────────────────────────────────────────────────────────

/// HTTP methods recognized as Express route registrations.
const EXPRESS_ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "options", "head", "all",
];

/// Methods recognized as Express middleware/lifecycle.
const EXPRESS_MIDDLEWARE_METHODS: &[&str] = &["use"];

/// Conventional receiver names for Express app/router instances.
const EXPRESS_RECEIVERS: &[&str] = &["app", "router", "server"];

// ── Detection output ──────────────────────────────────────────────────

/// A detected Express route registration.
#[derive(Debug, Clone)]
pub struct ExpressRouteDetection {
    /// HTTP method (uppercase): "GET", "POST", "USE", etc.
    pub http_method: String,
    /// Route path: "/api/users", "/api/users/:id", etc.
    pub path: String,
    /// Receiver name: "app", "router", "server".
    pub receiver: String,
    /// Source line (1-based).
    pub line_start: i64,
    /// Detection confidence (0.0 to 1.0).
    pub confidence: f64,
    /// Source file path (repo-relative).
    pub file_path: String,
}

// ── Public API ────────────────────────────────────────────────────────

/// Detect Express routes in TS/JS files.
///
/// Filters to TS/JS files, checks for Express import, parses AST,
/// and extracts route registrations.
///
/// Returns raw detections with file paths. Use `routes_to_surfaces` to
/// convert to persistable surface inputs (requires module resolution).
pub fn detect_express_routes(file_inputs: &[FileInput]) -> Vec<ExpressRouteDetection> {
    let mut routes = Vec::new();

    // Initialize tree-sitter parsers.
    let mut parser = Parser::new();
    let ts_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();

    for file in file_inputs {
        // Filter to JS/TS family files (core + extended per FD-SUPPORT-EXT-JSTS).
        let ext = get_extension(&file.rel_path);
        if !is_jsts_extension(ext) {
            continue;
        }

        // Check for Express import (simple string check, mirrors TS prototype).
        if !has_express_import(&file.content) {
            continue;
        }

        // Select grammar based on extension (TSX for .tsx/.jsx, TS for others).
        let language = if is_jsts_jsx_extension(ext) {
            &tsx_language
        } else {
            &ts_language
        };

        if parser.set_language(language).is_err() {
            continue; // Skip file if language setup fails.
        }

        // Parse AST.
        let tree = match parser.parse(&file.content, None) {
            Some(t) => t,
            None => continue, // Parse failed, skip.
        };

        // Detect routes in this file.
        let file_routes =
            detect_routes_in_file(&tree.root_node(), file.content.as_bytes(), &file.rel_path);
        routes.extend(file_routes);
    }

    routes
}

/// Module resolution function type.
///
/// Given a file path, returns the module_candidate_uid that owns that file.
/// Returns None if no module covers the path.
pub type ModuleResolver = dyn Fn(&str) -> Option<String>;

/// Convert detected routes to persistable surface inputs.
///
/// Requires a module resolver function to determine the module_candidate_uid
/// for each detected route. Routes without a resolvable module are skipped.
pub fn routes_to_surfaces(
    routes: &[ExpressRouteDetection],
    snapshot_uid: &str,
    repo_uid: &str,
    resolve_module: &ModuleResolver,
) -> Vec<CreateProjectSurfaceInput> {
    routes
        .iter()
        .filter_map(|route| {
            route_to_surface_with_resolver(route, snapshot_uid, repo_uid, resolve_module)
        })
        .collect()
}

// ── Import detection ──────────────────────────────────────────────────

/// Check if source contains Express import indicators.
///
/// Simple string-based check, mirrors TS prototype behavior.
fn has_express_import(source: &str) -> bool {
    source.contains("'express'")
        || source.contains("\"express\"")
        || source.contains("from 'express'")
        || source.contains("from \"express\"")
        || source.contains("require('express')")
        || source.contains("require(\"express\")")
}

// ── AST traversal ─────────────────────────────────────────────────────

/// Detect route registrations in an AST tree for a specific file.
fn detect_routes_in_file(
    root: &Node,
    source: &[u8],
    file_path: &str,
) -> Vec<ExpressRouteDetection> {
    let mut routes = Vec::new();
    collect_routes(root, source, file_path, &mut routes);
    routes
}

/// Recursively collect route registrations from AST nodes.
fn collect_routes(
    node: &Node,
    source: &[u8],
    file_path: &str,
    routes: &mut Vec<ExpressRouteDetection>,
) {
    // Check if this node is a call expression matching Express pattern.
    if node.kind() == "call_expression" {
        if let Some(route) = try_extract_route(node, source, file_path) {
            routes.push(route);
        }
    }

    // Recurse into children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_routes(&child, source, file_path, routes);
    }
}

/// Try to extract a route from a call expression node.
///
/// Pattern: `receiver.method(path_arg, ...)`
fn try_extract_route(
    call_node: &Node,
    source: &[u8],
    file_path: &str,
) -> Option<ExpressRouteDetection> {
    // Get the callee (should be member_expression for receiver.method).
    let callee = call_node.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }

    // Extract receiver (object) and method (property).
    let receiver_node = callee.child_by_field_name("object")?;
    let method_node = callee.child_by_field_name("property")?;

    let receiver = node_text(&receiver_node, source)?;
    let method = node_text(&method_node, source)?;

    // Validate receiver is a conventional Express name.
    if !EXPRESS_RECEIVERS.contains(&receiver.as_str()) {
        return None;
    }

    // Validate method is a known route/middleware method.
    let method_lower = method.to_lowercase();
    let is_route_method = EXPRESS_ROUTE_METHODS.contains(&method_lower.as_str());
    let is_middleware_method = EXPRESS_MIDDLEWARE_METHODS.contains(&method_lower.as_str());
    if !is_route_method && !is_middleware_method {
        return None;
    }

    // Get arguments.
    let args = call_node.child_by_field_name("arguments")?;

    // First argument should be the path.
    let first_arg = get_first_argument(&args)?;
    let path = extract_path_from_arg(&first_arg, source)?;

    // Classify HTTP method.
    let http_method = if is_middleware_method {
        "USE".to_string()
    } else {
        method.to_uppercase()
    };

    // Calculate confidence.
    let confidence = if path.starts_with('/') { 0.9 } else { 0.7 };

    Some(ExpressRouteDetection {
        http_method,
        path,
        receiver,
        line_start: call_node.start_position().row as i64 + 1,
        confidence,
        file_path: file_path.to_string(),
    })
}

/// Get first argument from arguments node.
#[allow(clippy::manual_find)]
fn get_first_argument<'a>(args_node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut cursor = args_node.walk();
    for child in args_node.children(&mut cursor) {
        // Skip punctuation (parentheses, commas).
        if child.kind() != "(" && child.kind() != ")" && child.kind() != "," {
            return Some(child);
        }
    }
    None
}

/// Extract path string from argument node.
///
/// Handles:
/// - String literal: "/api/users"
/// - Template literal: `/api/users` (backticks)
///
/// Returns None for non-literal paths (variables, complex expressions).
fn extract_path_from_arg(arg_node: &Node, source: &[u8]) -> Option<String> {
    match arg_node.kind() {
        "string" => {
            // String literal: "path" or 'path'
            let text = node_text(arg_node, source)?;
            // Strip quotes.
            let path = text.trim_matches(|c| c == '"' || c == '\'');
            // Normalize Express params to OpenAPI style.
            let normalized = normalize_path(path);
            Some(normalized)
        }
        "template_string" => {
            // Template literal: `path`
            let text = node_text(arg_node, source)?;
            // Strip backticks.
            let path = text.trim_matches('`');
            // Skip complex templates with interpolations.
            if path.contains("${") {
                return None;
            }
            let normalized = normalize_path(path);
            Some(normalized)
        }
        _ => None, // Non-literal path, skip.
    }
}

/// Normalize Express path parameters to OpenAPI style.
///
/// `:param` -> `{param}`
fn normalize_path(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();

    while let Some(c) = chars.next() {
        if c == ':' {
            // Start of parameter.
            result.push('{');
            // Collect parameter name.
            while let Some(&nc) = chars.peek() {
                if nc.is_alphanumeric() || nc == '_' {
                    result.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            result.push('}');
        } else {
            result.push(c);
        }
    }

    result
}

/// Extract text content from an AST node.
fn node_text(node: &Node, source: &[u8]) -> Option<String> {
    let start = node.start_byte();
    let end = node.end_byte();
    if end > source.len() {
        return None;
    }
    std::str::from_utf8(&source[start..end])
        .ok()
        .map(|s| s.to_string())
}

// ── Surface conversion ────────────────────────────────────────────────

/// Convert a detected route to a surface input.
///
/// Uses the module resolver to determine the module_candidate_uid.
/// Returns None if the route has no path or no owning module.
///
/// Public for use by compose.rs which needs to track converted routes
/// for evidence creation.
pub fn route_to_surface_with_resolver(
    route: &ExpressRouteDetection,
    snapshot_uid: &str,
    repo_uid: &str,
    resolve_module: &ModuleResolver,
) -> Option<CreateProjectSurfaceInput> {
    // Skip paths that don't look like routes.
    if route.path.is_empty() {
        return None;
    }

    // Resolve module candidate for this file.
    let module_candidate_uid = resolve_module(&route.file_path)?;

    // Compute stable surface key. Include module_candidate_uid to
    // disambiguate routes with the same method+path across different
    // modules (e.g., two fixtures both defining GET /api).
    let stable_key = format!(
        "surface:express_route:{}:{}:{}",
        module_candidate_uid, route.http_method, &route.path
    );

    // Extract module root (directory of the file).
    let root_path = route
        .file_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or(".");

    Some(CreateProjectSurfaceInput {
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        module_candidate_uid,
        surface_kind: "http_provider".to_string(),
        display_name: Some(format!("{} {}", route.http_method, route.path)),
        root_path: root_path.to_string(),
        entrypoint_path: Some(route.file_path.clone()),
        build_system: "npm".to_string(),
        runtime_kind: "node".to_string(),
        confidence: route.confidence,
        metadata_json: Some(
            serde_json::json!({
                "framework": "express",
                "httpMethod": route.http_method,
                "receiver": route.receiver,
                "lineStart": route.line_start,
            })
            .to_string(),
        ),
        source_type: "express_route".to_string(),
        source_specific_id: None,
        stable_surface_key: stable_key,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_detect(source: &str) -> Vec<ExpressRouteDetection> {
        let mut parser = Parser::new();
        let ts_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        detect_routes_in_file(&tree.root_node(), source.as_bytes(), "test.ts")
    }

    #[test]
    fn detects_app_get() {
        let source = r#"
import express from 'express';
const app = express();
app.get('/api/users', (req, res) => {});
"#;
        let routes = parse_and_detect(source);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].http_method, "GET");
        assert_eq!(routes[0].path, "/api/users");
        assert_eq!(routes[0].receiver, "app");
    }

    #[test]
    fn detects_router_post() {
        let source = r#"
import { Router } from 'express';
const router = Router();
router.post('/api/items', createItem);
"#;
        let routes = parse_and_detect(source);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].http_method, "POST");
        assert_eq!(routes[0].path, "/api/items");
        assert_eq!(routes[0].receiver, "router");
    }

    #[test]
    fn detects_multiple_routes() {
        let source = r#"
import express from 'express';
const app = express();
app.get('/users', getUsers);
app.post('/users', createUser);
app.delete('/users/:id', deleteUser);
"#;
        let routes = parse_and_detect(source);
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].http_method, "GET");
        assert_eq!(routes[1].http_method, "POST");
        assert_eq!(routes[2].http_method, "DELETE");
    }

    #[test]
    fn normalizes_path_params() {
        let source = r#"
import express from 'express';
const app = express();
app.get('/users/:userId/posts/:postId', handler);
"#;
        let routes = parse_and_detect(source);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/users/{userId}/posts/{postId}");
    }

    #[test]
    fn detects_app_use() {
        let source = r#"
import express from 'express';
const app = express();
app.use('/api', apiRouter);
"#;
        let routes = parse_and_detect(source);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].http_method, "USE");
        assert_eq!(routes[0].path, "/api");
    }

    #[test]
    fn ignores_non_express_receivers() {
        let source = r#"
const cache = new Map();
cache.get('/key');
"#;
        let routes = parse_and_detect(source);
        assert!(routes.is_empty());
    }

    #[test]
    fn ignores_non_route_methods() {
        let source = r#"
import express from 'express';
const app = express();
app.listen(3000);
app.set('view engine', 'ejs');
"#;
        let routes = parse_and_detect(source);
        assert!(routes.is_empty());
    }

    #[test]
    fn ignores_dynamic_paths() {
        let source = r#"
import express from 'express';
const app = express();
app.get(`${BASE_URL}/users`, handler);
"#;
        let routes = parse_and_detect(source);
        assert!(routes.is_empty());
    }

    #[test]
    fn has_express_import_detects_various_forms() {
        assert!(has_express_import("import express from 'express';"));
        assert!(has_express_import("import express from \"express\";"));
        assert!(has_express_import("const express = require('express');"));
        assert!(has_express_import("const express = require(\"express\");"));
        assert!(!has_express_import("import React from 'react';"));
    }

    #[test]
    fn normalize_path_handles_multiple_params() {
        assert_eq!(normalize_path("/users/:id"), "/users/{id}");
        assert_eq!(normalize_path("/a/:b/c/:d"), "/a/{b}/c/{d}");
        assert_eq!(normalize_path("/no/params"), "/no/params");
    }
}
