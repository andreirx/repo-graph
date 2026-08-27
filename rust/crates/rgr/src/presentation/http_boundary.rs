//! HTTP-BOUNDARY-1: presentation for the HTTP/REST boundary map.
//!
//! Crate-private module holding the HTTP-specific DTO + rendering that the
//! `surfaces list` and `modules list` presenters need, so those two already
//! large files (`surfaces.rs`, `modules_list.rs`) do not grow an HTTP
//! responsibility inline (operator refactor ruling 2026-08-24 + review-5 item 4;
//! the crate-private-module allowance is pre-ratified for this slice).
//!
//! Abstraction record — module: `presentation::http_boundary`; concrete current
//! users: `SurfacesListResponse::render_human` (surface section + degraded read)
//! and `ModulesListResponse::render_human` (the Layer-3 boundary note); axis:
//! one cohesive HTTP-render concern, two presenters share it across the empty/
//! degraded honesty rules; rejected simpler alternative: leaving the ~300 lines
//! inline in the two 500+-line presentation files (violates the guardrail the
//! ruling enforces).

use serde::Deserialize;

/// One HTTP/REST provider or consumer surface from the boundary-interaction
/// store (`channel_kind='http'`). Rendered as a distinct section from
/// `project_surfaces`, never mixed into them.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct HttpBoundarySurfaceEntry {
    #[serde(default)]
    pub(crate) direction: String,
    #[serde(default, rename = "httpMethod")]
    pub(crate) http_method: String,
    /// `None` = dynamic/unreadable URL — shown as `<dynamic>`, never fabricated.
    #[serde(default)]
    pub(crate) route: Option<String>,
    #[serde(default, rename = "sourceFile")]
    pub(crate) source_file: String,
    /// §2.5 — `files.is_test` for this surface's file. `Some(true)` → `[test]`;
    /// `Some(false)`/`None` → no label (never asserts non-test). A read failure
    /// degrades upstream, so `None` here is data absence, not a swallowed error.
    #[serde(default, rename = "isTest")]
    pub(crate) is_test: Option<bool>,
    /// §2.1 — framework label (`spring` = REST, `spring_mvc` = MVC/view-render,
    /// `nextjs_app_router`, `axios`, …). Drives the REST-vs-MVC basis note.
    #[serde(default)]
    pub(crate) framework: Option<String>,
    /// §3 — when `route` is unknown, the recorded reason the URL is not statically
    /// derivable; rendered beside `<dynamic>` so it is never a silent gap.
    #[serde(default, rename = "routeUnknownReason")]
    pub(crate) route_unknown_reason: Option<String>,
    /// §2.5 — the REAL owning module (from `module_file_ownership`), for the
    /// dual-implementation note. `None` = ownership unavailable → the note states
    /// the module is unknown, never a fabricated path-segment proxy.
    #[serde(default)]
    pub(crate) module: Option<String>,
    /// §2.3 (Option B) — set when the read-time union found this (method, route,
    /// file) recorded with a CONFLICTING direction in the other family; rendered
    /// as a labeled conflict so divergence is surfaced, never silently dropped.
    #[serde(default)]
    pub(crate) conflict: Option<String>,
}

/// The ONE shared provider/consumer count for the HTTP surface section (§2.3).
///
/// Both the section HEADLINE and the section FOOTER derive from this single
/// count of the SAME rows being printed, so a headline/footer contradiction is
/// impossible by construction — enforced by `count_coherence` parsing tests.
///
/// Abstraction record — type: `HttpSurfaceAggregation`; concrete current users:
/// `render_surfaces` (headline + footer); axis: one count, two print sites that
/// must never disagree; rejected simpler alternative: counting inline at each
/// print site (the exact drift the audit measured — headline 0 above N rows).
pub(crate) struct HttpSurfaceAggregation {
    pub(crate) providers: usize,
    pub(crate) consumers: usize,
}

impl HttpSurfaceAggregation {
    /// Count providers/consumers off the rows that WILL be printed — the single
    /// source of truth for both the headline and the footer.
    pub(crate) fn from_entries(entries: &[HttpBoundarySurfaceEntry]) -> Self {
        HttpSurfaceAggregation {
            providers: entries.iter().filter(|s| s.direction == "provider").count(),
            consumers: entries.iter().filter(|s| s.direction == "consumer").count(),
        }
    }

    fn total(&self) -> usize {
        self.providers + self.consumers
    }

    /// The canonical "`P` provider(s), `C` consumer(s)" phrase both the headline
    /// and footer print verbatim — one format, so they cannot diverge.
    fn phrase(&self) -> String {
        format!(
            "{} provider{}, {} consumer{}",
            self.providers,
            if self.providers == 1 { "" } else { "s" },
            self.consumers,
            if self.consumers == 1 { "" } else { "s" },
        )
    }
}

/// Render the HTTP/REST provider & consumer map as a distinct section. Each
/// surface reads `METHOD /route  file  [provider|consumer]` plus honest labels
/// (`[test]`, a Spring REST/MVC basis note, a dual-implementation note, an
/// unknown-route reason). A dynamic URL shows `<dynamic>`, never fabricated.
/// Empty input → empty string (caller decides the empty/degraded messaging).
pub(crate) fn render_surfaces(surfaces: &[HttpBoundarySurfaceEntry]) -> String {
    if surfaces.is_empty() {
        return String::new();
    }
    // §2.3: ONE aggregation feeds BOTH the headline and the footer.
    let agg = HttpSurfaceAggregation::from_entries(surfaces);

    let mut out = String::new();
    out.push_str(&format!("\nHTTP/REST API surfaces: {}\n", agg.phrase()));

    let mut entries = surfaces.to_vec();
    entries.sort_by(|a, b| {
        (&a.direction, &a.http_method, &a.route, &a.source_file).cmp(&(
            &b.direction,
            &b.http_method,
            &b.route,
            &b.source_file,
        ))
    });

    // §2.5 dual-implementation (review-3 item 3): a (method, route) served by ≥2
    // DISTINCT real owning MODULES is a stated dual implementation, noted ONCE.
    // When ownership is unavailable for a provider file, duality across modules
    // cannot be confirmed — the note states that honestly instead of asserting two
    // modules. Computed before the loop so the note attaches to the first
    // occurrence.
    let dual = dual_providers(&entries);

    let mut noted_dual: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for s in &entries {
        let route = match &s.route {
            Some(r) => r.clone(),
            // §3: an unknown route shows its recorded reason, never a bare
            // `<dynamic>` that hides WHY.
            None => match &s.route_unknown_reason {
                Some(reason) => format!("<dynamic — {}>", reason),
                None => "<dynamic>".to_string(),
            },
        };
        let mut line = format!(
            "  {:6} {}  {}  [{}]",
            s.http_method, route, s.source_file, s.direction
        );
        // §2.5: `[test]` for a surface in a test file (files.is_test = true).
        if s.is_test == Some(true) {
            line.push_str(" [test]");
        }
        // §2.1: the Spring REST-vs-MVC basis note (only Spring distinguishes them).
        if let Some(note) = basis_note(s.framework.as_deref()) {
            line.push(' ');
            line.push_str(note);
        }
        // §2.3 (Option B): a cross-family direction conflict at this identity is
        // labeled inline — the union surfaces divergence, never a silent drop.
        if let Some(conflict) = &s.conflict {
            line.push_str(&format!(" [conflict: {}]", conflict));
        }
        out.push_str(&line);
        out.push('\n');

        // §2.5: the dual-implementation note, once per (method, route). A CONFIRMED
        // dual names the OTHER owning module(s); an UNDETERMINED one (ownership
        // unavailable for ≥1 provider file) states that without asserting two
        // modules (review-3 item 3).
        if s.direction == "provider" {
            if let Some(r) = &s.route {
                let key = (s.http_method.clone(), r.clone());
                if let Some(dual_impl) = dual.get(&key) {
                    if let Some(note) = dual_impl.note_for(s.module.as_deref(), &s.source_file) {
                        if noted_dual.insert(key) {
                            out.push_str(&note);
                        }
                    }
                }
            }
        }
    }

    // §2.3: the footer repeats the SAME aggregation — headline == footer by
    // construction, and both == the rows above.
    out.push_str(&format!(
        "— {} HTTP surface{}: {} —\n",
        agg.total(),
        if agg.total() == 1 { "" } else { "s" },
        agg.phrase(),
    ));
    out
}

/// The Spring REST-vs-MVC basis note (§2.1). Only the Spring stereotypes carry a
/// meaningful REST/view-render distinction; every other framework returns `None`
/// (no note), so App Router / axios / CDK rows are unadorned.
fn basis_note(framework: Option<&str>) -> Option<&'static str> {
    match framework {
        Some("spring") => Some("(REST)"),
        Some("spring_mvc") => Some("(MVC/view-render)"),
        _ => None,
    }
}

/// One provider of a route, for §2.5 dual classification. `module` is the REAL
/// owning module (`module_file_ownership`); `None` = ownership unavailable.
struct ProviderRef {
    source_file: String,
    module: Option<String>,
}

/// Classification of a (method, route) served by multiple providers (§2.5,
/// review-3 item 3). Dual-ness is a MODULE fact, not a file fact.
enum DualImpl {
    /// ≥2 DISTINCT real owning modules serve this route — a stated dual
    /// implementation (glamCRM's Spring/CDK duality).
    Confirmed(std::collections::BTreeSet<String>),
    /// ≥2 distinct provider FILES but ownership cannot confirm ≥2 modules
    /// (ownership absent for ≥1 file). Duality is UNDETERMINED — stated as such,
    /// never asserted as two modules (operator ruling (a)).
    Undetermined(std::collections::BTreeSet<String>),
}

impl DualImpl {
    /// The note to attach at the first provider row for this route, given that
    /// row's module + file so the note names the OTHERS. `None` if nothing else to
    /// state.
    fn note_for(&self, this_module: Option<&str>, this_file: &str) -> Option<String> {
        match self {
            DualImpl::Confirmed(modules) => {
                let others: Vec<String> = modules
                    .iter()
                    .filter(|m| Some(m.as_str()) != this_module)
                    .cloned()
                    .collect();
                let list = if others.is_empty() {
                    modules.iter().cloned().collect::<Vec<_>>()
                } else {
                    others
                };
                if list.is_empty() {
                    return None;
                }
                Some(format!(
                    "         also provided by {} (dual implementation)\n",
                    list.join(", ")
                ))
            }
            DualImpl::Undetermined(files) => {
                let others: Vec<String> = files
                    .iter()
                    .filter(|f| f.as_str() != this_file)
                    .cloned()
                    .collect();
                if others.is_empty() {
                    return None;
                }
                Some(format!(
                    "         also served from {}; owning module(s) unavailable — dual implementation undetermined\n",
                    others.join(", ")
                ))
            }
        }
    }
}

/// Classify each (method, route) served by multiple providers (§2.5). A route
/// served by ≥2 DISTINCT real owning MODULES is a CONFIRMED dual implementation;
/// ≥2 distinct provider FILES whose ownership cannot confirm two modules is
/// UNDETERMINED (never asserted as dual). A route from a single file — or from
/// multiple files all in the SAME known module — is absent from the map.
fn dual_providers(
    entries: &[HttpBoundarySurfaceEntry],
) -> std::collections::HashMap<(String, String), DualImpl> {
    let mut by_route: std::collections::HashMap<(String, String), Vec<ProviderRef>> =
        std::collections::HashMap::new();
    for s in entries {
        if s.direction != "provider" {
            continue;
        }
        if let Some(route) = &s.route {
            by_route
                .entry((s.http_method.clone(), route.clone()))
                .or_default()
                .push(ProviderRef {
                    source_file: s.source_file.clone(),
                    module: s.module.clone(),
                });
        }
    }
    let mut out = std::collections::HashMap::new();
    for (key, providers) in by_route {
        let files: std::collections::BTreeSet<String> =
            providers.iter().map(|p| p.source_file.clone()).collect();
        if files.len() < 2 {
            continue; // single file → not a dual implementation
        }
        let known_modules: std::collections::BTreeSet<String> =
            providers.iter().filter_map(|p| p.module.clone()).collect();
        let has_unknown = providers.iter().any(|p| p.module.is_none());
        if known_modules.len() >= 2 {
            // ≥2 distinct real modules → confirmed dual (a stated fact).
            out.insert(key, DualImpl::Confirmed(known_modules));
        } else if has_unknown {
            // ≤1 known module + ≥1 unowned file → cannot confirm two modules.
            out.insert(key, DualImpl::Undetermined(files));
        }
        // else: exactly one known module, no unknowns → all one module → not dual.
    }
    out
}

/// Render a FAILED HTTP-surface read as UNKNOWN (review-4 item 2) — never an
/// empty REST map presented as fact.
pub(crate) fn render_surfaces_degraded(reason: &str) -> String {
    format!(
        "\nHTTP/REST API surfaces: unknown — {} (not reporting 0; rerun after reindex).\n",
        reason
    )
}

/// The `modules list` Layer-3 boundary note for the "no cross-module imports,
/// >1 module" case (review-0 item 2 + review-1 honesty + review-4 item 2).
///
/// Returns the note to append given the persisted HTTP-link count and any
/// reader-framed degradation:
/// - a degraded read OR a `None` count (both signal a failed read) → UNKNOWN,
///   which must NOT restore "boundaries may not be meaningful";
/// - `Some(n>0)` → the modules are LIKELY connected via HTTP route match, spoken
///   as the Layer-3 heuristic it is (route match, not runtime-proven);
/// - `Some(0)` → genuinely no boundaries → the original "may not be meaningful"
///   hint.
pub(crate) fn render_modules_note(link_count: Option<usize>, degraded: Option<&str>) -> String {
    match (degraded.is_some(), link_count) {
        (true, _) | (_, None) => {
            "\nnote: whether these modules talk over HTTP/REST is UNKNOWN — the \
             HTTP boundary link read degraded (not asserting the import graph is \
             complete); rerun after reindex, or see `rmap boundaries links`.\n"
                .to_string()
        }
        (false, Some(n)) if n > 0 => {
            let plural = if n == 1 { "" } else { "s" };
            format!(
                "\nnote: imports are intra-module, but these modules are likely \
                 connected via HTTP route match (heuristic, {} link{}; Layer-3 \
                 discovery, not runtime-proven) — see `rmap boundaries links`.\n",
                n, plural,
            )
        }
        (false, Some(_)) => {
            "\nhint: all imports are intra-module. Module boundaries may not be meaningful yet.\n"
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        direction: &str,
        method: &str,
        route: Option<&str>,
        file: &str,
    ) -> HttpBoundarySurfaceEntry {
        HttpBoundarySurfaceEntry {
            direction: direction.to_string(),
            http_method: method.to_string(),
            route: route.map(str::to_string),
            source_file: file.to_string(),
            is_test: None,
            framework: None,
            route_unknown_reason: None,
            module: None,
            conflict: None,
        }
    }

    /// Parse the "P providers, C consumers" phrase out of a rendered line.
    fn parse_phrase(line: &str) -> (usize, usize) {
        let after = line.split(':').next_back().unwrap_or(line);
        let p = after
            .split("provider")
            .next()
            .and_then(|s| s.split_whitespace().next_back())
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no provider count in {line:?}"));
        let c = after
            .split("consumer")
            .next()
            .and_then(|s| s.trim_end_matches(", ").split_whitespace().next_back())
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no consumer count in {line:?}"));
        (p, c)
    }

    /// §2.3 count-coherence: parse the rendered output and cross-check the
    /// headline count == the footer count == the actual number of surface rows.
    /// A contradiction (the audit's "headline 0 above 244 rows") is impossible if
    /// this passes.
    #[test]
    fn count_coherence_headline_equals_footer_equals_rows() {
        let surfaces = vec![
            entry("provider", "GET", Some("/a"), "backend/A.java"),
            entry("provider", "POST", Some("/b"), "backend/B.java"),
            entry("provider", "GET", None, "backend/C.java"),
            entry("consumer", "GET", Some("/a"), "web/a.ts"),
        ];
        let out = render_surfaces(&surfaces);
        let lines: Vec<&str> = out.lines().collect();
        let headline = lines
            .iter()
            .find(|l| l.starts_with("HTTP/REST API surfaces:"))
            .expect("headline");
        let footer = lines.iter().find(|l| l.starts_with("—")).expect("footer");
        // Count actual surface rows (indented `  METHOD ...  [dir]` lines).
        let row_count = lines
            .iter()
            .filter(|l| l.starts_with("  ") && l.contains('['))
            .count();
        let (hp, hc) = parse_phrase(headline);
        let (fp, fc) = parse_phrase(footer);
        assert_eq!((hp, hc), (fp, fc), "headline vs footer:\n{out}");
        assert_eq!(hp + hc, row_count, "headline count vs rows printed:\n{out}");
        assert_eq!((hp, hc), (3, 1), "3 providers, 1 consumer:\n{out}");
    }

    #[test]
    fn test_file_consumer_is_labelled() {
        let mut surfaces = vec![entry("consumer", "GET", Some("/a"), "src/test/ApiIT.java")];
        surfaces[0].is_test = Some(true);
        let out = render_surfaces(&surfaces);
        assert!(out.contains("[test]"), "{out}");
        // A non-test surface (is_test None/false) carries no [test].
        let prod = render_surfaces(&[entry("provider", "GET", Some("/a"), "backend/A.java")]);
        assert!(!prod.contains("[test]"), "{prod}");
    }

    #[test]
    fn spring_rest_vs_mvc_basis_note() {
        let mut rest = entry("provider", "GET", Some("/api/x"), "backend/RestC.java");
        rest.framework = Some("spring".to_string());
        let mut mvc = entry("provider", "GET", Some("/owners"), "backend/OwnerC.java");
        mvc.framework = Some("spring_mvc".to_string());
        let out = render_surfaces(&[rest, mvc]);
        assert!(out.contains("(REST)"), "{out}");
        assert!(out.contains("(MVC/view-render)"), "{out}");
    }

    #[test]
    fn dual_implementation_noted_once_with_real_modules() {
        // Same (GET, /api/offers) served by a Spring backend file AND a CDK
        // serverless file → the dual implementation is a stated fact, once. The
        // note names the REAL owning modules (module_file_ownership), not a path
        // proxy.
        let mut spring = entry(
            "provider",
            "GET",
            Some("/api/offers"),
            "backend/OfferC.java",
        );
        spring.framework = Some("spring".to_string());
        spring.module = Some("core-api".to_string());
        let mut cdk = entry("provider", "GET", Some("/api/offers"), "serverless/api.ts");
        cdk.framework = Some("aws_cdk_apigwv2".to_string());
        cdk.module = Some("edge-serverless".to_string());
        let out = render_surfaces(&[spring, cdk]);
        let note_count = out.matches("also provided by").count();
        assert_eq!(note_count, 1, "dual note exactly once:\n{out}");
        // The note names the OTHER real module.
        assert!(
            out.contains("core-api") || out.contains("edge-serverless"),
            "{out}"
        );
    }

    #[test]
    fn dual_implementation_absent_ownership_is_undetermined_not_asserted() {
        // review-3 item 3: two provider FILES for one route, but ownership is absent
        // (module None on both): we CANNOT assert two modules, so duality is stated
        // as UNDETERMINED — never "dual implementation" and never two module names.
        let a = entry(
            "provider",
            "GET",
            Some("/api/offers"),
            "backend/OfferC.java",
        );
        let b = entry("provider", "GET", Some("/api/offers"), "serverless/api.ts");
        let out = render_surfaces(&[a, b]);
        assert!(
            out.contains("dual implementation undetermined"),
            "undetermined stated: {out}"
        );
        assert!(
            out.contains("owning module(s) unavailable"),
            "ownership unavailable stated: {out}"
        );
        // It must NOT assert a confirmed dual implementation off unknown ownership.
        assert_eq!(
            out.matches("(dual implementation)").count(),
            0,
            "no confirmed-dual assertion without module evidence: {out}"
        );
    }

    #[test]
    fn dual_implementation_same_module_two_files_not_noted() {
        // review-3 item 3: two provider FILES for one route but BOTH owned by the
        // SAME real module → NOT a dual implementation (one module, two files). No
        // note of either kind.
        let mut a = entry("provider", "GET", Some("/api/offers"), "backend/A.java");
        a.module = Some("core-api".to_string());
        let mut b = entry("provider", "GET", Some("/api/offers"), "backend/B.java");
        b.module = Some("core-api".to_string());
        let out = render_surfaces(&[a, b]);
        assert_eq!(out.matches("also provided by").count(), 0, "{out}");
        assert!(!out.contains("undetermined"), "{out}");
    }

    #[test]
    fn direction_conflict_row_is_labeled() {
        // §2.3 (Option B): a union direction-conflict is labeled inline.
        let mut s = entry("provider", "GET", Some("/api/x"), "svc.ts");
        s.conflict = Some("identity also recorded as consumer".to_string());
        let out = render_surfaces(&[s]);
        assert!(out.contains("[conflict:"), "{out}");
        assert!(out.contains("also recorded as consumer"), "{out}");
    }

    #[test]
    fn unknown_route_shows_recorded_reason() {
        // §3: an unknown route renders its reason, never a bare `<dynamic>`.
        let mut s = entry("provider", "GET", None, "src/app/api/[...slug]/route.ts");
        s.route_unknown_reason = Some("catch-all segment".to_string());
        let out = render_surfaces(&[s]);
        assert!(out.contains("<dynamic — catch-all segment>"), "{out}");
    }

    #[test]
    fn render_surfaces_shows_providers_consumers_and_dynamic() {
        let surfaces = vec![
            entry(
                "provider",
                "GET",
                Some("/api/v2/offers/{id}"),
                "backend/OfferController.java",
            ),
            entry("consumer", "GET", None, "frontend/api.ts"),
        ];
        let out = render_surfaces(&surfaces);
        assert!(
            out.contains("HTTP/REST API surfaces: 1 provider, 1 consumer"),
            "{out}"
        );
        assert!(out.contains("GET    /api/v2/offers/{id}"), "{out}");
        assert!(
            out.contains("<dynamic>"),
            "dynamic route shown honestly: {out}"
        );
    }

    #[test]
    fn render_surfaces_empty_is_empty_string() {
        assert!(render_surfaces(&[]).is_empty());
    }

    #[test]
    fn render_surfaces_degraded_is_unknown() {
        let out = render_surfaces_degraded("db locked");
        assert!(out.contains("HTTP/REST API surfaces: unknown"), "{out}");
        assert!(out.contains("db locked"), "{out}");
    }

    #[test]
    fn modules_note_heuristic_is_honest_layer3() {
        let out = render_modules_note(Some(3), None);
        assert!(
            out.contains("likely connected via HTTP route match"),
            "{out}"
        );
        assert!(out.contains("heuristic, 3 links"), "{out}");
        assert!(out.contains("not runtime-proven"), "{out}");
        assert!(
            !out.contains("at runtime"),
            "must not claim runtime connection: {out}"
        );
    }

    #[test]
    fn modules_note_zero_links_keeps_meaningless_hint() {
        let out = render_modules_note(Some(0), None);
        assert!(
            out.contains("Module boundaries may not be meaningful"),
            "{out}"
        );
    }

    #[test]
    fn modules_note_degraded_or_none_is_unknown() {
        let out = render_modules_note(None, Some("db locked"));
        assert!(out.contains("UNKNOWN") && out.contains("degraded"), "{out}");
        assert!(
            !out.contains("Module boundaries may not be meaningful"),
            "{out}"
        );
        // A `None` count with no reason string is also a failed read → unknown.
        let out2 = render_modules_note(None, None);
        assert!(out2.contains("UNKNOWN"), "{out2}");
    }
}
