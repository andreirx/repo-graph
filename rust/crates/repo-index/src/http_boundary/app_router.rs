//! Next.js **App Router** HTTP PROVIDER detection (HTTP-SURFACE-COHERENCE-1 §2.2).
//!
//! A `route.ts` / `route.js` under an `app/**` directory that exports one or
//! more HTTP-verb handlers (`GET`/`POST`/`PUT`/`PATCH`/`DELETE`/`HEAD`/
//! `OPTIONS`) is a Next.js App Router **Route Handler** — the file IS the
//! server endpoint for the route its directory path names. Before this, such
//! files surfaced only as `[consumer]` (they proxy a backend with `fetch`), so
//! the server half of every Next.js app pointed the wrong way.
//!
//! ## Evidence is structural, never a name guess (STANDING HONESTY RULE 2)
//!
//! Two independent structural facts gate a provider surface, no name heuristic.
//! First, **location** — the file is `.../app/.../route.{ts,js}`; the `app`
//! segment plus the `route` basename are the Next.js framework contract for a
//! Route Handler, and nothing else in the tree is one. Second, an **exported
//! verb** — the file exports a binding whose name is EXACTLY an uppercase HTTP
//! verb; Next.js dispatches by that exact exported name, so it is a hard
//! structural signal, not a substring match. No verb exports → no surface (a
//! `route.ts` that exports only helpers is not an endpoint).
//!
//! ## Route derivation, honest about the shapes it cannot express (§3)
//!
//! The route is the app-relative directory path: `app/api/x/route.ts` → `/api/x`;
//! a dynamic segment `[param]` → `{param}` (the same route-template vocabulary
//! Spring/CDK providers use, so consumer↔provider linking stays uniform). Shapes
//! whose URL is NOT statically derivable from the path alone — route groups
//! `(group)`, parallel-route slots `@slot`, and catch-all `[...x]` /
//! `[[...x]]` — yield route `unknown` (`None`) with a recorded reason, NEVER a
//! fabricated path. `derive_route` returns that reason; the exhaustive unit
//! tests below are the record of which shapes we decline to express.

use std::ops::ControlFlow;

use repo_graph_boundary_interaction::{Direction, InteractionBasis};
use repo_graph_indexer::jsts_extensions::{get_extension, is_jsts_extension};
use repo_graph_indexer::orchestrator::FileInput;
use tree_sitter::{Node, Parser};

use super::{file_symbol_key, node_text, HttpSurfaceDraft};

/// Framework label stamped on App Router provider surfaces.
const FRAMEWORK: &str = "nextjs_app_router";

/// The exact uppercase export names Next.js treats as Route Handler verbs.
const APP_ROUTER_VERBS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// Detect Next.js App Router provider surfaces across the file set. A file that
/// is not an `app/**/route.{ts,js}` is skipped; one that is but exports no HTTP
/// verb yields nothing.
pub(super) fn detect_app_router_providers(file_inputs: &[FileInput]) -> Vec<HttpSurfaceDraft> {
    let mut drafts = Vec::new();
    let mut parser = Parser::new();
    let ts_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    if parser.set_language(&ts_language).is_err() {
        return drafts;
    }

    for file in file_inputs {
        // Cheap structural pre-filter: only `.../app/.../route.{ts,js}`.
        let derivation = derive_route(&file.rel_path);
        // §3 / review-0 item 4: an inexpressible shape yields an UNKNOWN route
        // AND its recorded reason — the reason is NOT discarded here; it rides
        // with the surface so the read path can render it beside `<dynamic>`.
        let (route, route_unknown_reason): (Option<String>, Option<&'static str>) = match derivation
        {
            RouteDerivation::NotAppRoute => continue,
            RouteDerivation::Route(r) => (Some(r), None),
            RouteDerivation::Unknown(reason) => (None, Some(reason)),
        };
        // The `route.ts`/`route.js` handler is TypeScript-grammar-parseable (a
        // route handler carries no JSX). A parse failure or a too-deep tree
        // yields no verbs → no surface, never a fabricated one.
        let ext = get_extension(&file.rel_path);
        if !is_jsts_extension(ext) {
            continue;
        }
        let tree = match parser.parse(&file.content, None) {
            Some(t) => t,
            None => continue,
        };
        if crate::walk::tree_exceeds_depth(&tree.root_node(), crate::walk::MAX_POSTPASS_TREE_DEPTH)
        {
            continue;
        }
        for (verb, line, col) in exported_http_verbs(&tree.root_node(), file.content.as_bytes()) {
            drafts.push(HttpSurfaceDraft {
                direction: Direction::Provider,
                http_method: verb,
                route: route.clone(),
                route_unknown_reason,
                source_file: file.rel_path.clone(),
                line_start: line,
                col_start: col,
                symbol_stable_key: file_symbol_key(&file.rel_path),
                // Structural (location + exported verb name), not annotation.
                basis: InteractionBasis::Convention,
                framework: FRAMEWORK,
            });
        }
    }
    drafts
}

/// Outcome of deriving an App Router route from a repo-relative file path.
#[derive(Debug, PartialEq)]
enum RouteDerivation {
    /// Not an `app/**/route.{ts,js}` file — no provider here.
    NotAppRoute,
    /// A statically-derivable route template (`/api/x`, `/posts/{id}`).
    Route(String),
    /// An App Router route file whose URL is NOT statically derivable from the
    /// path — the reason names the inexpressible shape (§3).
    Unknown(&'static str),
}

/// Derive the App Router route template from a repo-relative path.
///
/// The route is the `app/`-relative directory path with the trailing
/// `route.{ts,js}` removed; `[param]` → `{param}`. Shapes whose URL is not
/// statically derivable (route groups, parallel slots, catch-alls) return
/// `Unknown` with the reason.
fn derive_route(rel_path: &str) -> RouteDerivation {
    let segments: Vec<&str> = rel_path.split('/').collect();
    let Some((basename, dirs)) = segments.split_last() else {
        return RouteDerivation::NotAppRoute;
    };
    if !matches!(*basename, "route.ts" | "route.js") {
        return RouteDerivation::NotAppRoute;
    }
    // First `app` segment is the App Router root; everything under it is a route.
    let Some(app_idx) = dirs.iter().position(|s| *s == "app") else {
        return RouteDerivation::NotAppRoute;
    };
    let route_segments = &dirs[app_idx + 1..];

    let mut out = String::new();
    for seg in route_segments {
        match classify_segment(seg) {
            SegmentKind::Literal(s) => {
                out.push('/');
                out.push_str(s);
            }
            SegmentKind::Param(name) => {
                out.push('/');
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
            SegmentKind::RouteGroup => {
                return RouteDerivation::Unknown(
                    "route group '(...)' segment — not part of the URL, path not statically derivable",
                );
            }
            SegmentKind::ParallelSlot => {
                return RouteDerivation::Unknown(
                    "parallel-route slot '@...' segment — URL not statically derivable",
                );
            }
            SegmentKind::CatchAll => {
                return RouteDerivation::Unknown(
                    "catch-all '[...]'/'[[...]]' segment — matches variable path depth, not a single route",
                );
            }
        }
    }
    if out.is_empty() {
        // `app/route.ts` serves the site root.
        out.push('/');
    }
    RouteDerivation::Route(out)
}

/// One classified App Router path segment.
enum SegmentKind<'a> {
    Literal(&'a str),
    Param(&'a str),
    RouteGroup,
    ParallelSlot,
    CatchAll,
}

fn classify_segment(seg: &str) -> SegmentKind<'_> {
    if seg.starts_with('(') && seg.ends_with(')') {
        return SegmentKind::RouteGroup;
    }
    if seg.starts_with('@') {
        return SegmentKind::ParallelSlot;
    }
    // Catch-all: `[...slug]` or optional `[[...slug]]`. Check before the plain
    // `[param]` case since both start with `[`.
    if seg.starts_with("[[") || seg.starts_with("[...") {
        return SegmentKind::CatchAll;
    }
    if let Some(inner) = seg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        // A well-formed dynamic segment `[name]`; empty/`...` handled above.
        return SegmentKind::Param(inner);
    }
    SegmentKind::Literal(seg)
}

/// Every exported HTTP-verb binding in the file, as `(verb, line_1based, col)`.
/// Handles the three export forms Next.js accepts: `export function GET`,
/// `export const GET = …`, and `export { GET }`.
fn exported_http_verbs(root: &Node, source: &[u8]) -> Vec<(String, i64, i64)> {
    let mut out: Vec<(String, i64, i64)> = Vec::new();
    crate::walk::visit_preorder(*root, |node| {
        if node.kind() == "export_statement" {
            collect_export_verbs(&node, source, &mut out);
        }
        ControlFlow::Continue(())
    });
    // Deterministic order + dedup (a verb exported twice anchors once).
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

fn collect_export_verbs(export_stmt: &Node, source: &[u8], out: &mut Vec<(String, i64, i64)>) {
    let mut cursor = export_stmt.walk();
    for child in export_stmt.children(&mut cursor) {
        match child.kind() {
            // `export [async] function GET(req) { … }`
            "function_declaration" | "generator_function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    push_if_verb(&name, source, out);
                }
            }
            // `export const GET = async (req) => { … }`
            "lexical_declaration" | "variable_declaration" => {
                let mut dc = child.walk();
                for decl in child.children(&mut dc) {
                    if decl.kind() == "variable_declarator" {
                        if let Some(name) = decl.child_by_field_name("name") {
                            push_if_verb(&name, source, out);
                        }
                    }
                }
            }
            // `export { GET, POST }` / `export { handler as DELETE }`. Next.js
            // dispatches on the EXPORTED name — the `alias` when present (`as
            // DELETE`), else the local `name`.
            "export_clause" => {
                let mut ec = child.walk();
                for spec in child.children(&mut ec) {
                    if spec.kind() == "export_specifier" {
                        let exported = spec
                            .child_by_field_name("alias")
                            .or_else(|| spec.child_by_field_name("name"));
                        if let Some(name) = exported {
                            push_if_verb(&name, source, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Push `(verb, line, col)` iff the identifier text is EXACTLY one of the
/// uppercase HTTP verbs Next.js dispatches on.
fn push_if_verb(name: &Node, source: &[u8], out: &mut Vec<(String, i64, i64)>) {
    let Some(text) = node_text(name, source) else {
        return;
    };
    if APP_ROUTER_VERBS.contains(&text.as_str()) {
        out.push((
            text,
            name.start_position().row as i64 + 1,
            name.start_position().column as i64,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_of(rel_path: &str, src: &str) -> FileInput {
        FileInput {
            rel_path: rel_path.to_string(),
            content: src.to_string(),
            content_hash: String::new(),
            size_bytes: src.len(),
            line_count: src.lines().count(),
            package_dependencies: None,
            tsconfig_aliases: None,
        }
    }

    // ── derive_route (pure) ────────────────────────────────────────────────

    #[test]
    fn static_route_from_app_dir() {
        assert_eq!(
            derive_route("renderer/src/app/api/comments/route.ts"),
            RouteDerivation::Route("/api/comments".into())
        );
    }

    #[test]
    fn dynamic_param_becomes_brace_template() {
        assert_eq!(
            derive_route("src/app/api/posts/[id]/route.ts"),
            RouteDerivation::Route("/api/posts/{id}".into())
        );
        // The amodx `[siteId]` top-level dynamic prefix.
        assert_eq!(
            derive_route("renderer/src/app/[siteId]/sitemap.xml/route.ts"),
            RouteDerivation::Route("/{siteId}/sitemap.xml".into())
        );
    }

    #[test]
    fn app_root_route_serves_slash() {
        assert_eq!(
            derive_route("app/route.ts"),
            RouteDerivation::Route("/".into())
        );
    }

    #[test]
    fn non_route_basename_is_not_app_route() {
        assert_eq!(
            derive_route("src/app/api/posts/page.ts"),
            RouteDerivation::NotAppRoute
        );
        assert_eq!(
            derive_route("src/lib/route.ts"),
            RouteDerivation::NotAppRoute
        );
    }

    #[test]
    fn inexpressible_shapes_are_unknown_not_fabricated() {
        // §3: route groups, parallel slots, and catch-alls → Unknown + reason.
        assert!(matches!(
            derive_route("src/app/(marketing)/about/route.ts"),
            RouteDerivation::Unknown(_)
        ));
        assert!(matches!(
            derive_route("src/app/@modal/photo/route.ts"),
            RouteDerivation::Unknown(_)
        ));
        assert!(matches!(
            derive_route("src/app/api/[...slug]/route.ts"),
            RouteDerivation::Unknown(_)
        ));
        assert!(matches!(
            derive_route("src/app/api/[[...slug]]/route.ts"),
            RouteDerivation::Unknown(_)
        ));
    }

    // ── detect_app_router_providers (structural) ───────────────────────────

    #[test]
    fn exported_verbs_become_providers_with_route() {
        let src = r#"
            import { NextResponse } from "next/server";
            export async function GET(req) { return NextResponse.json({}); }
            export async function POST(req) { return NextResponse.json({}); }
        "#;
        let drafts =
            detect_app_router_providers(&[file_of("renderer/src/app/api/comments/route.ts", src)]);
        assert_eq!(drafts.len(), 2, "{drafts:?}");
        assert!(drafts.iter().all(|d| d.direction == Direction::Provider));
        assert!(drafts
            .iter()
            .all(|d| d.route.as_deref() == Some("/api/comments")));
        assert!(drafts.iter().all(|d| d.framework == "nextjs_app_router"));
        let mut verbs: Vec<&str> = drafts.iter().map(|d| d.http_method.as_str()).collect();
        verbs.sort();
        assert_eq!(verbs, vec!["GET", "POST"]);
        // Distinct anchor lines so the two verbs get distinct surface identities.
        assert_ne!(drafts[0].line_start, drafts[1].line_start);
    }

    #[test]
    fn const_and_reexport_forms_are_detected() {
        let src = r#"
            const handler = async (req) => new Response("ok");
            export const PUT = handler;
            export { handler as DELETE };
        "#;
        let drafts = detect_app_router_providers(&[file_of("app/api/x/route.ts", src)]);
        let mut verbs: Vec<&str> = drafts.iter().map(|d| d.http_method.as_str()).collect();
        verbs.sort();
        assert_eq!(verbs, vec!["DELETE", "PUT"], "{drafts:?}");
    }

    #[test]
    fn route_file_without_verb_exports_emits_nothing() {
        // §2.2: a `route.ts` that exports only helpers is NOT an endpoint.
        let src = r#"
            export function helper() { return 1; }
            export const config = { runtime: "edge" };
        "#;
        let drafts = detect_app_router_providers(&[file_of("app/api/x/route.ts", src)]);
        assert!(
            drafts.is_empty(),
            "no verb exports → no surface: {drafts:?}"
        );
    }

    #[test]
    fn non_app_router_file_emits_nothing() {
        // Same verb-shaped exports, but not an app-router route file.
        let src = "export async function GET(req) { return new Response(); }";
        let drafts = detect_app_router_providers(&[file_of("src/lib/handlers.ts", src)]);
        assert!(drafts.is_empty(), "{drafts:?}");
    }

    #[test]
    fn inexpressible_route_still_emits_provider_with_unknown_route() {
        // §3: a catch-all route file is still a provider (the endpoint exists),
        // but its route is honestly UNKNOWN (None), never fabricated.
        let src = "export async function GET(req) { return new Response(); }";
        let drafts = detect_app_router_providers(&[file_of("src/app/api/[...slug]/route.ts", src)]);
        assert_eq!(drafts.len(), 1, "{drafts:?}");
        assert_eq!(drafts[0].direction, Direction::Provider);
        assert_eq!(drafts[0].http_method, "GET");
        assert_eq!(
            drafts[0].route, None,
            "inexpressible shape → unknown route, never fabricated"
        );
        // review-0 item 4: the reason is CARRIED, not discarded — it rides with
        // the surface so the read path can render it beside `<dynamic>`.
        let reason = drafts[0]
            .route_unknown_reason
            .expect("unknown route must record its reason");
        assert!(reason.contains("catch-all"), "{reason}");
    }

    #[test]
    fn lowercase_verb_export_is_not_a_handler() {
        // Next.js dispatches on the EXACT uppercase name; `get`/`getData` are
        // ordinary exports, not Route Handlers.
        let src = r#"
            export function get(req) { return new Response(); }
            export const getData = async () => 1;
        "#;
        let drafts = detect_app_router_providers(&[file_of("app/api/x/route.ts", src)]);
        assert!(drafts.is_empty(), "{drafts:?}");
    }
}
