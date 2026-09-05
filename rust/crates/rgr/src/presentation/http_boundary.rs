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

use crate::presentation::anchor;

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
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the surface's start line for the `path:line` anchor
    /// on this individual row. `None` (older daemon, project family, or absent line) → the
    /// bare file path renders, never a fabricated line.
    #[serde(default, rename = "lineStart")]
    pub(crate) line: Option<u64>,
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
    /// HEADLINE providers = NON-test-fixture providers (`is_test != Some(true)` — production plus
    /// unknown). COHERENCE-3 (§2.2): the SAME count the `boundaries summary` HTTP line shows (it
    /// too subtracts test-only), so the two surfaces cannot state a different provider count.
    pub(crate) providers: usize,
    /// HEADLINE consumers = NON-test-fixture consumers.
    pub(crate) consumers: usize,
    /// COHERENCE-3 (§2.2): test-FIXTURE surfaces (`files.is_test == Some(true)`) EXCLUDED from the
    /// headline — the same demotion `cycles` applies to test-only cycles. Disclosed as
    /// "(+M test-fixture excluded)"; the fixture ROWS still render below, labeled `[test]`.
    test_fixture_excluded: usize,
    /// COHERENCE-3 (§2.2 / RULE #1): surfaces whose `is_test` is UNKNOWN (`None` — no `files` row,
    /// never asserted non-test). KEPT in the headline (never demoted) but disclosed as
    /// "test-status unknown for K" so an unknown is never counted invisibly.
    unknown: usize,
}

impl HttpSurfaceAggregation {
    /// Partition providers/consumers off the rows that WILL be printed by the stored `is_test`
    /// fact — the single source of truth for the headline, footer, and the exclusion clause.
    /// Test-fixture surfaces are excluded from the headline counts; unknown-`is_test` surfaces stay
    /// counted (never demoted) but are tallied for the disclosure. NEVER classifies from a path —
    /// only the stored fact (STANDING HONESTY RULE #2).
    pub(crate) fn from_entries(entries: &[HttpBoundarySurfaceEntry]) -> Self {
        let mut providers = 0;
        let mut consumers = 0;
        let mut test_fixture_excluded = 0;
        let mut unknown = 0;
        for s in entries {
            if s.is_test == Some(true) {
                test_fixture_excluded += 1;
                continue; // a test-fixture surface is demoted out of the headline
            }
            if s.is_test.is_none() {
                unknown += 1; // kept in the headline, disclosed — never invisible
            }
            match s.direction.as_str() {
                "provider" => providers += 1,
                "consumer" => consumers += 1,
                _ => {}
            }
        }
        HttpSurfaceAggregation {
            providers,
            consumers,
            test_fixture_excluded,
            unknown,
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

    /// COHERENCE-3 (§2.2): the exclusion/unknown disclosure appended to the headline — the SAME
    /// shape `cycles` uses ("+M test-fixture excluded; test-status unknown for K"), so the reader
    /// sees the demoted and unknown surfaces even though they leave the headline count. `None` when
    /// there is nothing to disclose (no fixtures, no unknowns) — then byte-identical to pre-slice.
    fn exclusion_clause(&self) -> Option<String> {
        // The SHARED clause — the SAME wording `orient`'s HTTP headline appends — so the two
        // surfaces cannot phrase the partition differently.
        crate::presentation::surface_exclusion_clause(
            self.test_fixture_excluded as u64,
            self.unknown as u64,
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
    // COHERENCE-3 (§2.2): the headline counts EXCLUDE test-fixture surfaces (matching `cycles` and
    // the `boundaries summary` HTTP line) and DISCLOSE the excluded + unknown counts, so no two
    // surfaces state a different provider/consumer count for one snapshot.
    match agg.exclusion_clause() {
        Some(clause) => out.push_str(&format!(
            "\nHTTP/REST API surfaces: {} ({})\n",
            agg.phrase(),
            clause
        )),
        None => out.push_str(&format!("\nHTTP/REST API surfaces: {}\n", agg.phrase())),
    }

    // §2.5 dual-implementation (review-3 item 3): a (method, route) served by ≥2
    // DISTINCT real owning MODULES is a stated dual implementation, noted ONCE.
    // When ownership is unavailable for a provider file, duality across modules
    // cannot be confirmed — the note states that honestly instead of asserting two
    // modules. Computed over the FULL entry set (before the display collapse) so a
    // dual across two files is still detected.
    let dual = dual_providers(surfaces);

    // SURFACES-DEDUP-1 (§2.1): collapse rows identical in every rendered field to a
    // single `×N` row (the `boundaries list` pattern) — amodx prints 46 verbatim
    // `GET <dynamic — …> …/index.ts [consumer]` rows otherwise. HUMAN-render only:
    // the `--json` path serializes the daemon envelope directly (every row kept), so
    // no machine consumer loses a row. The headline/footer aggregation still counts
    // every surface (`agg` is over all `surfaces`), so ×N sums back to the totals.
    let groups = collapse_identical(surfaces);

    let mut noted_dual: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for (s, count) in &groups {
        let route = match &s.route {
            Some(r) => r.clone(),
            // §3: an unknown route shows its recorded reason, never a bare
            // `<dynamic>` that hides WHY.
            None => match &s.route_unknown_reason {
                Some(reason) => format!("<dynamic — {}>", reason),
                None => "<dynamic>".to_string(),
            },
        };
        // ANCHORS-EVERYWHERE-1 (Tier 1): anchor the individual surface row at
        // `source_file:line` (uniform with find). Absent line → bare path, never fabricated.
        let mut line = format!(
            "  {:6} {}  {}  [{}]",
            s.http_method,
            route,
            anchor(&s.source_file, s.line),
            s.direction
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
        // SURFACES-DEDUP-1: the ×N collapse count (only when >1, so a single row is
        // byte-identical to the pre-slice output).
        if *count > 1 {
            line.push_str(&format!("  ×{}", count));
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

    // §2.3 + COHERENCE-3 §2.2 (review-0 item 3): the footer repeats the SAME aggregation
    // AND the SAME exclusion clause as the headline — so headline == footer verbatim. Its
    // count is the PRODUCTION (non-fixture) partition (`agg.total()` counts the same rows the
    // phrase names); the test-fixture rows it still lists above (labeled `[test]`) are disclosed
    // here too, so the footer is never a bare count silently sitting below listed fixtures.
    let footer_plural = if agg.total() == 1 { "" } else { "s" };
    match agg.exclusion_clause() {
        Some(clause) => out.push_str(&format!(
            "— {} HTTP surface{}: {} ({}) —\n",
            agg.total(),
            footer_plural,
            agg.phrase(),
            clause,
        )),
        None => out.push_str(&format!(
            "— {} HTTP surface{}: {} —\n",
            agg.total(),
            footer_plural,
            agg.phrase(),
        )),
    }
    out
}

/// SURFACES-DEDUP-1 (§2.1): collapse surfaces that are IDENTICAL in every field the
/// human row renders — `(direction, method, route, file, is_test, framework,
/// route_unknown_reason, conflict)` — into `(representative, count)` pairs, in the same
/// deterministic `(direction, method, route, file)` order the pre-slice loop used.
///
/// `module` is deliberately NOT part of the collapse key: it never appears on the row (it
/// feeds only the per-route dual note, computed over the FULL entry set), and a single file
/// belongs to a single module — so two rows sharing `source_file` share it, and collapsing
/// them loses nothing. The JSON path is untouched (every row kept there).
///
/// - what: the human-render row de-duplicator for the HTTP-surface section.
/// - concrete current user: [`render_surfaces`] (sole caller).
/// - axis: FIXED collapse operation over a growing row set — a pure fold, not an interface.
/// - rejected simpler: printing one line per entry (the amodx 46-verbatim-row wall this fixes).
fn collapse_identical(
    surfaces: &[HttpBoundarySurfaceEntry],
) -> Vec<(&HttpBoundarySurfaceEntry, usize)> {
    let mut refs: Vec<&HttpBoundarySurfaceEntry> = surfaces.iter().collect();
    refs.sort_by(|a, b| line_key(a).cmp(&line_key(b)));

    let mut groups: Vec<(&HttpBoundarySurfaceEntry, usize)> = Vec::new();
    for s in refs {
        match groups.last_mut() {
            Some((rep, count)) if line_key(rep) == line_key(s) => *count += 1,
            _ => groups.push((s, 1)),
        }
    }
    groups
}

/// A borrowed identity of every field a surface row renders, for the SURFACES-DEDUP-1 collapse
/// (deterministic sort + adjacency fold). Everything the human line prints is included, so two
/// rows collapse ONLY when their rendered lines would be byte-identical. `module` is excluded —
/// it never appears on the row (see [`collapse_identical`]).
type LineKey<'a> = (
    &'a str,         // direction
    &'a str,         // http_method
    Option<&'a str>, // route
    &'a str,         // source_file
    Option<u64>,     // line (ANCHORS-EVERYWHERE-1: rows at different lines render differently)
    Option<bool>,    // is_test
    Option<&'a str>, // framework
    Option<&'a str>, // route_unknown_reason
    Option<&'a str>, // conflict
);

fn line_key(s: &HttpBoundarySurfaceEntry) -> LineKey<'_> {
    (
        s.direction.as_str(),
        s.http_method.as_str(),
        s.route.as_deref(),
        s.source_file.as_str(),
        // ANCHORS-EVERYWHERE-1: the rendered anchor line is part of the row identity, so two
        // surfaces at DIFFERENT lines never collapse into one `×N` (that would hide a line).
        s.line,
        s.is_test,
        s.framework.as_deref(),
        s.route_unknown_reason.as_deref(),
        s.conflict.as_deref(),
    )
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
            line: None,
            // COHERENCE-3: default to a KNOWN production surface (`Some(false)`) — the common real
            // case (a surface in a file the tracked-files table knows is non-test). Tests that
            // exercise the test-fixture partition set `is_test = Some(true)`; the unknown case sets
            // `None` explicitly.
            is_test: Some(false),
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
    fn headline_excludes_test_fixtures_and_discloses_but_still_lists_them() {
        // COHERENCE-3 §2.2: a test-fixture surface (is_test == Some(true)) is EXCLUDED from the
        // headline provider/consumer counts (matching cycles + the boundaries-summary HTTP line),
        // DISCLOSED as "(+M test-fixture excluded)", and STILL rendered below, labeled [test].
        let mut fixture = entry("provider", "GET", Some("/fx"), "test/fixtures/server.js");
        fixture.is_test = Some(true);
        let surfaces = vec![
            entry("provider", "GET", Some("/a"), "backend/A.java"), // production
            fixture,                                                // test-fixture (excluded)
            entry("consumer", "GET", Some("/a"), "web/a.ts"),       // production
        ];
        let out = render_surfaces(&surfaces);
        assert!(
            out.contains(
                "HTTP/REST API surfaces: 1 provider, 1 consumer (+1 test-fixture excluded)"
            ),
            "headline excludes the fixture and discloses it:\n{out}"
        );
        // The fixture row still renders, labeled [test] (fixtures labeled in lists).
        assert!(
            out.contains("[test]"),
            "fixture still listed, labeled:\n{out}"
        );
        // COHERENCE-3 (review-0 item 3): the footer shows the SAME production phrase AND the SAME
        // exclusion clause as the headline — byte-identical, never a bare "2 surfaces" sitting
        // silently below the listed `[test]` fixture row.
        assert!(
            out.contains("— 2 HTTP surfaces: 1 provider, 1 consumer (+1 test-fixture excluded) —"),
            "footer matches the headline production phrase AND discloses the exclusion:\n{out}"
        );
    }

    #[test]
    fn footer_count_is_the_production_partition_and_discloses_excluded_fixtures() {
        // COHERENCE-3 (review-0 item 3): with fixtures present, MORE rows print than the footer
        // counts (the footer is the production partition), so the footer MUST disclose the gap —
        // never a bare "N surfaces" sitting silently below listed `[test]` rows. This pins the
        // footer's count against the rows it does NOT include.
        let mut fx1 = entry("provider", "GET", Some("/fx1"), "test/fixtures/a.js");
        fx1.is_test = Some(true);
        let mut fx2 = entry("consumer", "GET", Some("/fx2"), "test/fixtures/b.js");
        fx2.is_test = Some(true);
        let surfaces = vec![
            entry("provider", "GET", Some("/a"), "backend/A.java"), // production
            fx1,                                                    // fixture (excluded)
            fx2,                                                    // fixture (excluded)
        ];
        let out = render_surfaces(&surfaces);
        // Three rows render (incl. both `[test]` fixtures); the footer counts only the 1 production
        // surface but discloses the 2 excluded — so the reader can reconcile rows vs count.
        let row_count = out
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains('['))
            .count();
        assert_eq!(row_count, 3, "all rows incl. fixtures print:\n{out}");
        let footer = out.lines().find(|l| l.starts_with("—")).expect("footer");
        let (fp, fc) = parse_phrase(footer);
        assert_eq!((fp, fc), (1, 0), "footer counts only production:\n{out}");
        assert!(
            footer.contains("(+2 test-fixture excluded)"),
            "footer discloses the excluded fixtures it does not count:\n{out}"
        );
    }

    #[test]
    fn unknown_is_test_kept_in_headline_but_disclosed_never_invisible() {
        // is_test None (no files row) → UNKNOWN: KEPT in the headline count (never demoted) but
        // DISCLOSED, never silently treated as production (RULE #1).
        let mut unknown = entry("provider", "GET", Some("/u"), "gen/x.ts");
        unknown.is_test = None;
        let out = render_surfaces(&[
            entry("provider", "GET", Some("/a"), "backend/A.java"),
            unknown,
        ]);
        assert!(
            out.contains("2 providers, 0 consumers (test-status unknown for 1)"),
            "unknown stays counted but is disclosed:\n{out}"
        );
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

    /// ANCHORS-EVERYWHERE-1 (§4): an individual surface row anchors `source_file:line` when a line
    /// is present; absent → bare path. And two surfaces at the SAME (method,route,file) but
    /// DIFFERENT lines must NOT collapse into one `×N` (that would hide a distinct anchor).
    #[test]
    fn surface_row_anchors_line_and_distinct_lines_do_not_collapse() {
        let mut a = entry("consumer", "GET", Some("/api/x"), "web/a.ts");
        a.line = Some(12);
        let out = render_surfaces(&[a.clone()]);
        assert!(out.contains("web/a.ts:12"), "row anchors path:line:\n{out}");

        // Absent line → bare path (byte-identical to the pre-anchor row).
        let bare = render_surfaces(&[entry("consumer", "GET", Some("/api/x"), "web/a.ts")]);
        assert!(
            bare.contains("web/a.ts  [consumer]"),
            "absent line → bare path:\n{bare}"
        );

        // Same identity, different lines → two separate rows (no ×N collapse).
        let mut b = entry("consumer", "GET", Some("/api/x"), "web/a.ts");
        b.line = Some(30);
        let two = render_surfaces(&[a, b]);
        let rows: Vec<&str> = two
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains('['))
            .collect();
        assert_eq!(rows.len(), 2, "distinct lines must stay separate:\n{two}");
        assert!(!two.contains('×'), "no ×N collapse across lines:\n{two}");
    }

    #[test]
    fn identical_rows_collapse_to_one_with_count() {
        // SURFACES-DEDUP-1 (§2.1): amodx's shape — 46 verbatim-identical consumer rows for the
        // same (method, dynamic-route, file). They collapse to ONE row with `×46`, yet the
        // headline/footer still count all 46 (the collapse is display-only).
        let surfaces: Vec<HttpBoundarySurfaceEntry> = (0..46)
            .map(|_| entry("consumer", "GET", None, "tools/mcp-server/src/index.ts"))
            .collect();
        let out = render_surfaces(&surfaces);
        // Exactly one indented surface row is printed.
        let row_lines: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains('['))
            .collect();
        assert_eq!(row_lines.len(), 1, "rows not collapsed:\n{out}");
        assert!(row_lines[0].contains("×46"), "missing ×N count:\n{out}");
        // The footer still counts every collapsed surface.
        assert!(
            out.contains("46 HTTP surfaces: 0 providers, 46 consumers"),
            "footer must count all rows:\n{out}"
        );
    }

    #[test]
    fn distinct_rows_do_not_collapse_and_single_row_has_no_count() {
        // Rows differing in any rendered field stay distinct; a lone row carries NO ×N (byte-
        // identical to the pre-slice output for non-duplicated surfaces).
        let surfaces = vec![
            entry("consumer", "GET", Some("/a"), "web/a.ts"),
            entry("consumer", "GET", Some("/b"), "web/a.ts"),
        ];
        let out = render_surfaces(&surfaces);
        let row_lines: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains('['))
            .collect();
        assert_eq!(
            row_lines.len(),
            2,
            "distinct rows must not collapse:\n{out}"
        );
        assert!(!out.contains('×'), "single rows must carry no ×N:\n{out}");
    }

    #[test]
    fn rows_differing_only_in_test_flag_stay_separate() {
        // The collapse key includes every rendered field: a `[test]` row must NOT merge with a
        // non-test row of the same (method, route, file) — that would hide the test/prod split.
        let mut a = entry("consumer", "GET", Some("/a"), "web/a.ts");
        a.is_test = Some(true);
        let b = entry("consumer", "GET", Some("/a"), "web/a.ts"); // is_test None
        let out = render_surfaces(&[a, b]);
        let row_lines: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains('['))
            .collect();
        assert_eq!(row_lines.len(), 2, "test/prod rows must not merge:\n{out}");
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
