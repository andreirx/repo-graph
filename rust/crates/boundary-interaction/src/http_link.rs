//! HTTP-BOUNDARY-1: pure route-template-aware provider↔consumer matcher.
//!
//! This is the DOMAIN policy for HTTP link discovery — the sibling of the gRPC
//! contract match, but keyed on (HTTP method, route template) instead of a
//! shared proto contract element, because HTTP has no schema element by default.
//! It lives in the zero-workspace-dep policy crate (not the indexer) so BOTH
//! the index-time linker (`repo_graph_indexer::http_link::run_http_link_detection`)
//! and the read-time unlinked-counts renderer (daemon `boundaries_links` /
//! `surfaces list` handlers) call the SAME matcher — one policy, two call sites
//! across the index/serve split, with no `daemon-runtime → indexer` edge.
//!
//! ## The load-bearing heuristic (ratified in the slice)
//!
//! A link is asserted ONLY when a consumer's (method, route) matches EXACTLY
//! ONE provider surface. When a consumer route matches MULTIPLE providers (e.g.
//! a route exposed by both the Spring backend and the serverless backend), or
//! NONE, BOTH sides are left UNLINKED and the reason is counted in
//! [`UnlinkedCounts`] — never a guessed link. Route match is path-template
//! aware: `{id}` / `:id` / `*` segments are wildcards. A provider or consumer
//! whose route is `None` (dynamically-built URL) is never linked.

// ── Input DTO ─────────────────────────────────────────────────────────

/// An HTTP boundary surface read back for linking.
///
/// The raw, storage-shaped projection the matcher operates on. The storage
/// adapter produces these (method + route parsed out of `evidence_json`); the
/// matcher is pure over them.
#[derive(Debug, Clone)]
pub struct HttpSurfaceRow {
    /// Storage surface UID.
    pub surface_uid: String,
    /// "provider" or "consumer".
    pub direction: String,
    /// HTTP method, uppercase (e.g. "GET").
    pub http_method: String,
    /// Route template, e.g. "/api/v2/clients/{id}". `None` = statically
    /// unreadable (dynamic URL) — never linked, never fabricated.
    pub route: Option<String>,
    /// Source file (for evidence).
    pub source_file: String,
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the surface's start line
    /// (`boundary_interaction_surfaces.line_start`), for the `path:line` anchor on
    /// individual boundary/surface rows. Carried on the SAME read that feeds linking +
    /// rendering (no parallel SQL). `None` when the store has no line (never fabricated).
    /// The matcher IGNORES it; it is a presentation label like `is_test`.
    pub line: Option<u64>,
    /// Surface symbol stable key (for provenance).
    pub symbol_stable_key: String,
    /// HTTP-SURFACE-COHERENCE-1 §2.5 — presentation labels the matcher IGNORES.
    /// Carried here (not a second query) so the ONE read that feeds linking also
    /// feeds rendering, with no parallel SQL to drift.
    ///
    /// `files.is_test` for `source_file`: `Some(true)` = a test file (rendered
    /// `[test]`), `Some(false)` = a non-test tracked file, `None` = the file is
    /// not in the `files` table (no positive test evidence — never labelled, and
    /// never asserted non-test). Honest-degradation: a `None` here is data
    /// absence (no `files` row), NOT a read failure — a failed read Errs the whole
    /// query upstream.
    pub is_test: Option<bool>,
    /// Framework label off `evidence_json.framework` (`spring` / `spring_mvc` /
    /// `nextjs_app_router` / `axios` / …) — distinguishes REST vs MVC/view-render
    /// providers (§2.1 basis note). `None` when evidence carried no framework.
    pub framework: Option<String>,
    /// When `route` is `None`, the recorded reason (`evidence_json
    /// .routeUnknownReason`) the URL is not statically derivable (§3). Rendered
    /// beside `<dynamic>` so an unknown route is never a silent gap.
    pub route_unknown_reason: Option<String>,
}

// ── Result DTOs ───────────────────────────────────────────────────────

/// A detected HTTP provider/consumer link (route + method match).
#[derive(Debug, Clone, PartialEq)]
pub struct HttpLink {
    /// Provider surface UID.
    pub provider_surface_uid: String,
    /// Consumer surface UID.
    pub consumer_surface_uid: String,
    /// HTTP method (uppercase).
    pub http_method: String,
    /// Provider route template (e.g. "/api/v2/clients/{id}").
    pub provider_route: String,
    /// Consumer route template (e.g. "/api/v2/clients/{param}").
    pub consumer_route: String,
    /// Provider source file.
    pub provider_source_file: String,
    /// Consumer source file.
    pub consumer_source_file: String,
    /// Provider stable key (provenance).
    pub provider_stable_key: String,
    /// Consumer stable key (provenance).
    pub consumer_stable_key: String,
}

/// Counts of consumers left unlinked, for honest degradation reporting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnlinkedCounts {
    /// Consumers whose route matched >1 provider (ambiguous — never guessed).
    pub ambiguous: usize,
    /// Consumers whose route matched no provider.
    pub unmatched: usize,
    /// Consumers with a dynamic/unreadable route (never linkable).
    pub dynamic_route: usize,
}

// ── Matcher ───────────────────────────────────────────────────────────

/// Whether a wildcard-normalized path segment matches anything.
/// Accepts `{...}` (Spring/OpenAPI/CDK, incl. `{proxy+}`), `:name` (express),
/// and `*`.
fn is_wildcard_segment(seg: &str) -> bool {
    (seg.starts_with('{') && seg.ends_with('}')) || seg.starts_with(':') || seg == "*"
}

/// Route-template match: same segment count, each segment equal or either side
/// a wildcard. Both routes are expected to start with `/`.
pub fn route_matches(consumer_route: &str, provider_route: &str) -> bool {
    let c: Vec<&str> = consumer_route.trim_end_matches('/').split('/').collect();
    let p: Vec<&str> = provider_route.trim_end_matches('/').split('/').collect();
    if c.len() != p.len() {
        return false;
    }
    c.iter()
        .zip(p.iter())
        .all(|(cs, ps)| cs == ps || is_wildcard_segment(cs) || is_wildcard_segment(ps))
}

/// Outcome of matching one consumer against the provider set.
enum MatchOutcome<'a> {
    /// Exactly one provider matched — link it. Carries the matched provider AND
    /// its route: the route is `Some` by construction (a `None`-route provider is
    /// skipped below), so the type encodes that invariant and the link builder
    /// needs no `unwrap` on `provider.route` (review-6 item 2).
    Unique(&'a HttpSurfaceRow, &'a str),
    /// More than one provider matched — ambiguous, leave unlinked.
    Ambiguous,
    /// No provider matched — leave unlinked.
    None,
}

fn match_consumer<'a>(
    consumer: &HttpSurfaceRow,
    consumer_route: &str,
    providers: &'a [&'a HttpSurfaceRow],
) -> MatchOutcome<'a> {
    let mut matched: Vec<(&HttpSurfaceRow, &str)> = Vec::new();
    for provider in providers {
        let provider_route = match &provider.route {
            Some(r) => r.as_str(),
            None => continue,
        };
        if provider
            .http_method
            .eq_ignore_ascii_case(&consumer.http_method)
            && route_matches(consumer_route, provider_route)
        {
            matched.push((provider, provider_route));
        }
    }
    match matched.len() {
        0 => MatchOutcome::None,
        1 => MatchOutcome::Unique(matched[0].0, matched[0].1),
        _ => MatchOutcome::Ambiguous,
    }
}

/// Find unambiguous provider↔consumer links; also report the unlinked reasons.
///
/// Pure over the surface set — no I/O, no storage, no persistence. The
/// index-time linker persists the [`HttpLink`]s; the read-time renderer uses
/// the same output to report honest [`UnlinkedCounts`].
pub fn find_http_links(surfaces: &[HttpSurfaceRow]) -> (Vec<HttpLink>, UnlinkedCounts) {
    let providers: Vec<&HttpSurfaceRow> = surfaces
        .iter()
        .filter(|s| s.direction == "provider")
        .collect();
    let consumers: Vec<&HttpSurfaceRow> = surfaces
        .iter()
        .filter(|s| s.direction == "consumer")
        .collect();

    let mut links = Vec::new();
    let mut counts = UnlinkedCounts::default();

    for consumer in &consumers {
        let consumer_route = match &consumer.route {
            Some(r) => r,
            None => {
                counts.dynamic_route += 1;
                continue;
            }
        };
        match match_consumer(consumer, consumer_route, &providers) {
            MatchOutcome::Unique(provider, provider_route) => links.push(HttpLink {
                provider_surface_uid: provider.surface_uid.clone(),
                consumer_surface_uid: consumer.surface_uid.clone(),
                http_method: consumer.http_method.to_uppercase(),
                provider_route: provider_route.to_string(),
                consumer_route: consumer_route.clone(),
                provider_source_file: provider.source_file.clone(),
                consumer_source_file: consumer.source_file.clone(),
                provider_stable_key: provider.symbol_stable_key.clone(),
                consumer_stable_key: consumer.symbol_stable_key.clone(),
            }),
            MatchOutcome::Ambiguous => counts.ambiguous += 1,
            MatchOutcome::None => counts.unmatched += 1,
        }
    }

    (links, counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(
        uid: &str,
        direction: &str,
        method: &str,
        route: Option<&str>,
        file: &str,
    ) -> HttpSurfaceRow {
        HttpSurfaceRow {
            surface_uid: uid.to_string(),
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(|s| s.to_string()),
            source_file: file.to_string(),
            line: None,
            symbol_stable_key: format!("r:{}:FILE", file),
            is_test: None,
            framework: None,
            route_unknown_reason: None,
        }
    }

    #[test]
    fn route_template_matching() {
        assert!(route_matches("/api/v2/clients/123", "/api/v2/clients/{id}"));
        assert!(route_matches(
            "/api/v2/clients/{param}",
            "/api/v2/clients/{id}"
        ));
        assert!(route_matches("/api/v2/clients", "/api/v2/clients"));
        assert!(route_matches("/a/1", "/a/:id"));
        assert!(route_matches("/a/1", "/a/*"));
        // Collection vs item — different segment counts, no match.
        assert!(!route_matches("/api/v2/clients", "/api/v2/clients/{id}"));
        // Different literal — no match.
        assert!(!route_matches("/api/v2/products", "/api/v2/clients"));
    }

    #[test]
    fn unique_match_links() {
        let surfaces = vec![
            surface(
                "p1",
                "provider",
                "GET",
                Some("/api/v2/clients/{id}"),
                "backend/C.java",
            ),
            surface(
                "c1",
                "consumer",
                "GET",
                Some("/api/v2/clients/{param}"),
                "frontend/api.ts",
            ),
        ];
        let (links, counts) = find_http_links(&surfaces);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].provider_surface_uid, "p1");
        assert_eq!(links[0].consumer_surface_uid, "c1");
        assert_eq!(counts, UnlinkedCounts::default());
    }

    #[test]
    fn ambiguous_two_providers_leaves_unlinked() {
        // Spring AND serverless both expose GET /api/v2/x — never guessed.
        let surfaces = vec![
            surface(
                "p_spring",
                "provider",
                "GET",
                Some("/api/v2/x"),
                "backend/X.java",
            ),
            surface(
                "p_srvless",
                "provider",
                "GET",
                Some("/api/v2/x"),
                "serverless/api.ts",
            ),
            surface(
                "c1",
                "consumer",
                "GET",
                Some("/api/v2/x"),
                "frontend/api.ts",
            ),
        ];
        let (links, counts) = find_http_links(&surfaces);
        assert!(links.is_empty(), "ambiguous consumer must not be linked");
        assert_eq!(counts.ambiguous, 1);
    }

    #[test]
    fn no_match_leaves_both_present_unlinked() {
        let surfaces = vec![
            surface(
                "p1",
                "provider",
                "GET",
                Some("/api/v2/products"),
                "backend/P.java",
            ),
            surface(
                "c1",
                "consumer",
                "GET",
                Some("/api/v2/clients"),
                "frontend/api.ts",
            ),
        ];
        let (links, counts) = find_http_links(&surfaces);
        assert!(links.is_empty());
        assert_eq!(counts.unmatched, 1);
    }

    #[test]
    fn method_mismatch_is_not_linked() {
        // Same route, different verb — module adjacency alone must not link.
        let surfaces = vec![
            surface(
                "p1",
                "provider",
                "POST",
                Some("/api/v2/clients"),
                "backend/C.java",
            ),
            surface(
                "c1",
                "consumer",
                "GET",
                Some("/api/v2/clients"),
                "frontend/api.ts",
            ),
        ];
        let (links, counts) = find_http_links(&surfaces);
        assert!(links.is_empty());
        assert_eq!(counts.unmatched, 1);
    }

    #[test]
    fn dynamic_consumer_route_never_links() {
        let surfaces = vec![
            surface(
                "p1",
                "provider",
                "GET",
                Some("/api/v2/clients"),
                "backend/C.java",
            ),
            surface("c1", "consumer", "GET", None, "frontend/api.ts"),
        ];
        let (links, counts) = find_http_links(&surfaces);
        assert!(links.is_empty());
        assert_eq!(counts.dynamic_route, 1);
    }
}
