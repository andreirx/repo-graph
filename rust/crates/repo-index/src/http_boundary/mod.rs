//! HTTP-BOUNDARY-1: HTTP/REST provider + consumer detection for the
//! boundary-interaction surface family.
//!
//! Emits `BoundaryInteractionSurface` rows with `ChannelKind::Http` into the
//! same store the gRPC track uses, so `rmap boundaries` renders the HTTP API
//! map. Route + method ride in `evidence_json` (no schema change); the
//! indexer-side `http_link` post-pass reads them back and links
//! provider↔consumer by route-template match.
//!
//! ## Module layout (HTTP-BOUNDARY-1 review-0 item 6 — the 500-line split)
//!
//! Detection is split by cohesive concern, each a crate-private submodule:
//! - [`spring`] — Java Spring `@RestController` PROVIDER routes, from the
//!   java-extractor `metadata_json` annotations already on each graph node.
//! - [`typescript`] — TS/JS CONSUMER calls (`axios`/`fetch`/api-client) and the
//!   AWS-CDK apigatewayv2 serverless PROVIDER form glamCRM uses.
//! - [`java_consumer`] — Java CONSUMER call sites (`RestTemplate` / `WebClient` /
//!   `HttpClient`), parsed from `.java` source.
//!
//! All three produce a common [`HttpSurfaceDraft`]; this module owns the draft,
//! the shared route helpers, and the persistence + linking orchestration.
//!
//! Honesty: a URL that cannot be read statically (dynamic base / variable /
//! concatenation) yields a surface with an UNKNOWN route (`route: null`,
//! `routeIsDynamic: true`) — never a fabricated path.

mod imports;
mod java_consumer;
mod spring;
mod typescript;

use repo_graph_boundary_interaction::surface::SurfaceBuilder;
use repo_graph_boundary_interaction::{
    BoundaryInteractionSurface, BoundaryScope, ChannelKind, Direction, EndpointLocality,
    InteractionBasis, InteractionPattern, TransportClass,
};
use repo_graph_indexer::orchestrator::FileInput;
use repo_graph_storage::StorageConnection;
use tree_sitter::Node;

use crate::compose::ComposeError;

/// Extractor tag stamped on every HTTP boundary surface. `pub(crate)` so the
/// compose-level postpass isolation can scope its compensating cleanup to
/// exactly these facts.
pub(crate) const HTTP_EXTRACTOR: &str = "http-boundary:1.0";

// ── Common draft ──────────────────────────────────────────────────────

/// A detected HTTP boundary interaction, framework-agnostic, before it becomes
/// a `BoundaryInteractionSurface`.
#[derive(Debug, Clone, PartialEq)]
struct HttpSurfaceDraft {
    /// Provider (exposes a route) or Consumer (calls a route).
    direction: Direction,
    /// HTTP method, uppercase (e.g. "GET"). "UNKNOWN" when undecidable.
    http_method: String,
    /// Route template, e.g. "/api/v2/clients/{id}". `None` = statically
    /// unreadable (dynamic URL) — surfaced honestly, never fabricated.
    route: Option<String>,
    /// Repo-relative source file.
    source_file: String,
    /// 1-based start line of the interaction site.
    line_start: i64,
    /// 0-based start column (surface UID discriminator).
    col_start: i64,
    /// Stable key of the enclosing symbol (or `{repo}:{file}:FILE`).
    symbol_stable_key: String,
    /// Detection basis (annotation / api_call / convention).
    basis: InteractionBasis,
    /// Framework label for evidence ("spring", "aws_cdk_apigwv2", "axios",
    /// "fetch", "resttemplate", "webclient", "httpclient").
    framework: &'static str,
}

impl HttpSurfaceDraft {
    /// Convert to a persistable `BoundaryInteractionSurface`. Returns `Err`
    /// only on builder invariant violation (should not happen for
    /// detector-produced drafts).
    fn into_surface(
        self,
        snapshot_uid: &str,
        repo_uid: &str,
    ) -> Result<BoundaryInteractionSurface, String> {
        let route_is_dynamic = self.route.is_none();
        let evidence = serde_json::json!({
            "version": 1,
            "httpMethod": self.http_method,
            "route": self.route,
            "routeIsDynamic": route_is_dynamic,
            "framework": self.framework,
        })
        .to_string();

        let line = self.line_start.max(0) as u32;
        let col = self.col_start.max(0) as u32;

        SurfaceBuilder::new()
            .snapshot_uid(snapshot_uid)
            .repo_uid(repo_uid)
            .boundary_scope(BoundaryScope::Unknown)
            .channel_kind(ChannelKind::Http)
            .direction(self.direction)
            .transport_class(TransportClass::CustomProtocol)
            .provenance(format!("http:{}", self.framework))
            .protocol("http")
            .interaction_pattern(InteractionPattern::RequestResponse)
            .endpoint_locality(EndpointLocality::Unknown)
            .symbol_stable_key(self.symbol_stable_key)
            .source_file(self.source_file)
            .location(line, line, col, col)
            .extractor(HTTP_EXTRACTOR)
            .basis(self.basis)
            .evidence_json(evidence)
            .build()
    }
}

// ── Persistence entry point (called from compose.rs) ──────────────────

/// Detect and persist HTTP boundary surfaces for a snapshot.
///
/// Reads Java nodes (Spring providers) from storage and re-parses TS/JS + Java
/// files (consumers + CDK providers), then inserts all HTTP surfaces. Channels
/// are empty (HTTP addressing lives in `evidence_json`, not `ChannelDetail`).
/// Returns the number of surfaces persisted.
pub(crate) fn persist_http_boundary_interactions(
    storage: &mut StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
    let nodes = storage
        .query_all_nodes(snapshot_uid)
        .map_err(ComposeError::Storage)?;

    // review-5 item 1 / review-6 item 1: Spring provider detection reads
    // annotations off graph nodes, so its IMPORT evidence must come from the file
    // set. A class is a Spring `@RestController` only if its `.java` file has an
    // actual `import` DECLARATION for the Spring web annotation package — a bare
    // `@RestController` NAME is not enough (STANDING HONESTY RULE 2), and neither
    // is the package name appearing in a comment or string literal (`imports::
    // java_imports_package` ignores both). All glamCRM controllers carry
    // `import org.springframework.web.bind.annotation.*`.
    let spring_provider_files: std::collections::HashSet<&str> = file_inputs
        .iter()
        .filter(|f| {
            f.rel_path.ends_with(".java")
                && imports::java_imports_package(
                    &f.content,
                    "org.springframework.web.bind.annotation",
                )
        })
        .map(|f| f.rel_path.as_str())
        .collect();

    let mut drafts = spring::detect_spring_http_providers(&nodes, repo_uid, &spring_provider_files);
    drafts.extend(typescript::detect_ts_http(file_inputs));
    drafts.extend(java_consumer::detect_java_http_consumers(file_inputs));

    if drafts.is_empty() {
        return Ok(0);
    }

    let mut surfaces = Vec::with_capacity(drafts.len());
    for draft in drafts {
        match draft.clone().into_surface(snapshot_uid, repo_uid) {
            Ok(surface) => surfaces.push(surface),
            Err(e) => {
                // A malformed draft is a bug, not user data — fail loudly.
                return Err(ComposeError::Index(format!(
                    "http boundary surface build failed at {}:{}: {}",
                    draft.source_file, draft.line_start, e
                )));
            }
        }
    }

    let (count, _) = storage
        .insert_boundary_surfaces_and_channels(&surfaces, &[])
        .map_err(|e| ComposeError::Index(format!("http boundary storage: {}", e)))?;

    // Link provider↔consumer by (method, route) now that ALL http surfaces for
    // the snapshot are persisted. This lives here — not in the gRPC recompute
    // dispatch — because that dispatch is proto-gated (only runs when `.proto`
    // files exist) and runs before this postpass. `run_http_link_detection`
    // never returns Err (it COLLECTS a surface-query or link-storage failure
    // into the result); its link rows FK to these surfaces (ON DELETE CASCADE),
    // so the postpass's compensating cleanup removes them too. The per-consumer
    // unlinked reasons (ambiguous/unmatched/dynamic) are recomputed honestly at
    // read time in the `boundaries_links` daemon handler (no extra write) — see
    // HttpLinkResult.
    let link_result = repo_graph_indexer::http_link::run_http_link_detection(storage, snapshot_uid);
    // A collected error means the persisted HTTP link map is INCOMPLETE. We must
    // NOT return Ok — that would leave a READY snapshot serving a false-complete
    // API map (surfaces present, links silently missing; `boundaries links` = 0
    // while read-time `http_unlinked_json` recomputes and reports them linked).
    // Propagating as a postpass error lets `isolate_postpass` drop this
    // extractor's partial surfaces/links (ON DELETE CASCADE) and record the
    // degradation via the established extraction-diagnostics channel — the
    // Mission's honest-degradation contract (review-3 item 1).
    link_result_into_postpass_error(&link_result)?;

    Ok(count)
}

/// Convert a collected [`HttpLinkResult`] into a postpass error when link
/// detection degraded.
///
/// `run_http_link_detection` does not fail-fast: a surface-query failure or a
/// link-write failure is collected into the result rather than returned. Either
/// means the HTTP link map persisted for this snapshot is incomplete, so the
/// boundary postpass must fail (and be isolated) rather than report success.
/// Both collected error fields are surfaced in the message so the degradation
/// diagnostic names which side failed.
///
/// Pure over the result (no I/O) so both failure classes can be covered without
/// forcing a real `StorageConnection` to fail — the query- and link-write
/// failure injection lives with the port-generic linker in
/// `repo_graph_indexer::http_link`; this seam covers the mapping to
/// `ComposeError`.
fn link_result_into_postpass_error(
    result: &repo_graph_indexer::http_link::HttpLinkResult,
) -> Result<(), ComposeError> {
    if !result.has_error() {
        return Ok(());
    }
    let mut parts = Vec::new();
    if let Some(e) = &result.surface_query_error {
        parts.push(format!("surface query failed: {}", e));
    }
    if let Some(e) = &result.link_storage_error {
        parts.push(format!("link storage failed: {}", e));
    }
    Err(ComposeError::Index(format!(
        "http boundary link detection degraded: {}",
        parts.join("; ")
    )))
}

// ── Shared route helpers (used across all three detectors) ────────────

/// Compose a class base path with a method path suffix into a route template.
fn join_route(base: &str, suffix: &str) -> String {
    let b = base.trim_end_matches('/');
    let s = suffix.trim();
    let s = s.trim_end_matches('/');
    if s.is_empty() {
        return if b.is_empty() {
            "/".to_string()
        } else {
            b.to_string()
        };
    }
    let s = if s.starts_with('/') {
        s.to_string()
    } else {
        format!("/{}", s)
    };
    if b.is_empty() {
        s
    } else {
        format!("{}{}", b, s)
    }
}

/// Extract the first double-quoted string literal from raw annotation argument
/// text like `("/api/v2/clients")` or `(value = "/x", method = ...)`.
fn first_string_literal(args_raw: &str) -> Option<String> {
    let start = args_raw.find('"')?;
    let rest = &args_raw[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Parse the repo-relative source file out of a `{repo}:{path}#...` stable key.
fn source_file_from_stable_key(repo_uid: &str, stable_key: &str) -> Option<String> {
    let prefix = format!("{}:", repo_uid);
    let rest = stable_key.strip_prefix(&prefix)?;
    let path = rest.split('#').next().unwrap_or(rest);
    // FILE nodes end ":FILE" with no '#'.
    let path = path.strip_suffix(":FILE").unwrap_or(path);
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// Extract a route template from a string/template literal argument node.
///
/// - strips the query string (`?...`),
/// - replaces `${expr}` interpolations with a `{param}` wildcard segment,
/// - normalizes express `:param` -> `{param}`.
///
/// Returns `None` when the path is not statically readable as a route (does
/// not begin with `/` after stripping) — the caller then records an UNKNOWN
/// route, never a fabricated one.
fn extract_route_literal(node: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(node, source)?;
    let inner = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    route_from_raw(inner)
}

/// Normalize a raw path/URL string into a route template, or `None` if it is
/// not statically a readable route. Shared by TS literal extraction and the
/// Java consumer URL reader — the single normalization point for BOTH sides, so
/// provider and consumer routes cannot drift.
///
/// A static absolute URL (`http(s)://host/path`) is reduced to its path: it is
/// just as statically readable as a bare `/path`, and a consumer that writes
/// `fetch("https://host/offers")` must match a `/offers` provider (review-2
/// item 1). A dynamic base (`${BASE}/x`) still yields `None` — the path does not
/// begin with `/` after interpolation, so it is honestly UNKNOWN, never
/// fabricated.
fn route_from_raw(inner: &str) -> Option<String> {
    let inner = strip_url_scheme(inner);
    // Drop query string.
    let path = inner.split('?').next().unwrap_or(inner);
    let path = replace_interpolations(path);
    if !path.starts_with('/') {
        return None;
    }
    Some(normalize_params(&path))
}

/// Reduce a static absolute `http(s)://host/path` URL to its path portion.
/// A string without a recognized scheme (a bare path, a dynamic template) is
/// returned unchanged for the caller to judge. A scheme with no path (`https://host`)
/// reduces to `/`.
fn strip_url_scheme(inner: &str) -> &str {
    match inner
        .strip_prefix("https://")
        .or_else(|| inner.strip_prefix("http://"))
    {
        Some(rest) => match rest.find('/') {
            Some(idx) => &rest[idx..],
            None => "/",
        },
        None => inner,
    }
}

/// Replace `${...}` interpolations with `{param}`.
fn replace_interpolations(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find("${") {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + 2..];
        match after.find('}') {
            Some(end) => {
                out.push_str("{param}");
                rest = &after[end + 1..];
            }
            None => {
                // Unbalanced — treat remainder literally.
                out.push_str(&rest[idx..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Normalize express `:param` path segments to `{param}`.
fn normalize_params(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ':' {
            result.push('{');
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

fn node_text(node: &Node, source: &[u8]) -> Option<String> {
    let (start, end) = (node.start_byte(), node.end_byte());
    if end > source.len() {
        return None;
    }
    std::str::from_utf8(&source[start..end])
        .ok()
        .map(|s| s.to_string())
}

fn file_symbol_key(file: &str) -> String {
    // Repo UID is prefixed by the surface builder's repo_uid separately; the
    // symbol key just anchors to the file for provenance.
    format!("{}:FILE", file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::ControlFlow;
    use tree_sitter::Parser;

    #[test]
    fn join_route_composes_base_and_suffix() {
        assert_eq!(
            join_route("/api/v2/clients", "/{id}"),
            "/api/v2/clients/{id}"
        );
        assert_eq!(join_route("/api/v2/clients", ""), "/api/v2/clients");
        assert_eq!(join_route("/api/v2/clients", "/"), "/api/v2/clients");
        assert_eq!(join_route("", "/health"), "/health");
        assert_eq!(join_route("/a/", "b"), "/a/b");
    }

    #[test]
    fn first_string_literal_from_annotation_args() {
        assert_eq!(
            first_string_literal("(\"/api/v2/clients\")").as_deref(),
            Some("/api/v2/clients")
        );
        assert_eq!(
            first_string_literal("(value = \"/x\", method = RequestMethod.GET)").as_deref(),
            Some("/x")
        );
        assert_eq!(first_string_literal("()"), None);
    }

    #[test]
    fn source_file_parses_from_stable_key() {
        assert_eq!(
            source_file_from_stable_key(
                "repo_1",
                "repo_1:backend/src/Foo.java#Foo.bar:SYMBOL:METHOD"
            )
            .as_deref(),
            Some("backend/src/Foo.java")
        );
        assert_eq!(
            source_file_from_stable_key("repo_1", "repo_1:backend/src/Foo.java:FILE").as_deref(),
            Some("backend/src/Foo.java")
        );
    }

    #[test]
    fn route_literal_strips_query_and_interpolation() {
        // helper to parse a single TS expression's first string arg
        fn route_of(src: &str) -> Option<String> {
            let mut p = Parser::new();
            let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
            p.set_language(&lang).unwrap();
            let tree = p.parse(src, None).unwrap();
            let mut found = None;
            crate::walk::visit_preorder(tree.root_node(), |n| {
                if (n.kind() == "string" || n.kind() == "template_string") && found.is_none() {
                    found = extract_route_literal(&n, src.as_bytes());
                }
                ControlFlow::Continue(())
            });
            found
        }
        assert_eq!(
            route_of("x('/api/v2/clients')"),
            Some("/api/v2/clients".into())
        );
        assert_eq!(
            route_of("x('/api/v2/clients/termene?cui=5')"),
            Some("/api/v2/clients/termene".into())
        );
        assert_eq!(
            route_of("x(`/api/v2/clients/${clientId}`)"),
            Some("/api/v2/clients/{param}".into())
        );
        // Dynamic base -> unreadable -> None (never fabricated).
        assert_eq!(route_of("x(`${BASE}/clients`)"), None);
    }

    #[test]
    fn link_result_ok_when_no_error() {
        // review-3 item 1: a clean link result completes the postpass normally.
        let result = repo_graph_indexer::http_link::HttpLinkResult {
            links_emitted: 3,
            providers_queried: 5,
            consumers_queried: 4,
            ..Default::default()
        };
        assert!(link_result_into_postpass_error(&result).is_ok());
    }

    #[test]
    fn link_result_surface_query_error_becomes_postpass_error() {
        // A surface-query failure means the link map is incomplete → the postpass
        // must fail (and be isolated), never report a false-complete API map.
        let result = repo_graph_indexer::http_link::HttpLinkResult {
            surface_query_error: Some("db locked".to_string()),
            ..Default::default()
        };
        match link_result_into_postpass_error(&result) {
            Err(ComposeError::Index(msg)) => {
                assert!(msg.contains("surface query failed: db locked"), "{msg}");
            }
            other => panic!("expected ComposeError::Index, got {other:?}"),
        }
    }

    #[test]
    fn link_result_link_storage_error_becomes_postpass_error() {
        // A link-write failure (some links may have committed) means the map is
        // incomplete → propagate so isolate_postpass drops the partial facts.
        let result = repo_graph_indexer::http_link::HttpLinkResult {
            links_emitted: 2,
            link_storage_error: Some("write failed".to_string()),
            ..Default::default()
        };
        match link_result_into_postpass_error(&result) {
            Err(ComposeError::Index(msg)) => {
                assert!(msg.contains("link storage failed: write failed"), "{msg}");
            }
            other => panic!("expected ComposeError::Index, got {other:?}"),
        }
    }

    #[test]
    fn route_from_raw_reduces_absolute_urls_to_path() {
        // review-2 item 1: static absolute URLs are readable routes.
        assert_eq!(
            route_from_raw("https://api.example.test/api/v2/offers?x=1").as_deref(),
            Some("/api/v2/offers")
        );
        assert_eq!(
            route_from_raw("http://svc.internal/clients/{id}").as_deref(),
            Some("/clients/{id}")
        );
        // Scheme with no path -> root.
        assert_eq!(route_from_raw("https://host").as_deref(), Some("/"));
        // Bare path unchanged.
        assert_eq!(route_from_raw("/health").as_deref(), Some("/health"));
        // Non-route (bare host, no scheme) stays UNKNOWN.
        assert_eq!(route_from_raw("api.example.test/x"), None);
        // Dynamic base after interpolation is still UNKNOWN, never fabricated.
        assert_eq!(route_from_raw("${BASE}/clients"), None);
    }
}
