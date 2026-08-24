//! Render tests for the dense `orient` command (ORIENT-DENSITY-1).
//!
//! Split out of `orient.rs` (via `#[path]`, the `trust_tests.rs` idiom) to respect
//! the >500-line structural guardrail. Still a child module of `orient`, so
//! `use super::*` reaches the response structs, `OrientDepth`, and the
//! `render_orient_envelope`/`render_human` entry points exactly as before; the split
//! is pure relocation. Pins the dense headline (named structure, complexity centers,
//! cycles/docs, one reliability caveat), the budget→depth trade (small ⊂ full), and
//! the preserved honesty (no overclaim, no withdrawn dead-code surface).

use super::*;

fn minimal_response() -> OrientResponse {
    OrientResponse {
        repo: "test-repo".to_string(),
        display_name: None,
        snapshot: "snap-123".to_string(),
        focus: Focus {
            input: None,
            resolved: true,
            resolved_kind: Some("repo".to_string()),
            resolved_path: None,
            reason: None,
        },
        confidence: "high".to_string(),
        documentation: None,
        signals: vec![],
        limits: vec![],
        next: vec![],
        truncated: false,
        trust_briefing: None,
        relationship_next_action: None,
        measurement_coverage: None,
        witnesses: None,
        index_drift: None,
        parse_status: None,
    }
}

/// Wrap a presentation `Signal` as a minimal leaf envelope (sqlite-sourced, Fresh) for render tests.
/// The renderer only reads `.value`, so the sibling metadata is incidental here.
fn leaf(sig: Signal) -> CoherenceEnvelope<Signal> {
    CoherenceEnvelope::sqlite_leaf(sig, false)
}

/// Build a presentation `Signal` leaf with optional evidence.
fn sig(
    code: &str,
    severity: &str,
    summary: &str,
    evidence: serde_json::Value,
) -> CoherenceEnvelope<Signal> {
    leaf(Signal {
        code: code.to_string(),
        severity: severity.to_string(),
        category: "structure".to_string(),
        summary: summary.to_string(),
        scope: None,
        evidence: if evidence.is_null() {
            None
        } else {
            Some(evidence)
        },
    })
}

/// A realistic nginx-shaped repo response: NAMED modules, named complexity
/// centers, cycles, docs, and a degraded call-graph. `pub(super)` so the sibling
/// `density_tests` module reuses this fixture (the E2E Usefulness Protocol shape).
pub(super) fn nginx_like() -> OrientResponse {
    use serde_json::json;
    let mut r = minimal_response();
    r.repo = "nginx".to_string();
    r.confidence = "medium".to_string();
    r.documentation = Some(DocumentationSection {
        relevant_files: vec![
            RelevantDoc {
                path: "README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                reason: "repo_root_doc".to_string(),
            },
            RelevantDoc {
                path: "CHANGES".to_string(),
                kind: "changelog".to_string(),
                generated: false,
                reason: "repo_root_doc".to_string(),
            },
        ],
        count: 2,
    });
    r.signals = vec![
        sig(
            "MODULE_SUMMARY",
            "low",
            "397 files, 5000 symbols indexed; 6 discovered modules.",
            json!({
                "file_count": 397, "symbol_count": 5000, "discovered_module_count": 6,
                "top_modules": [
                    {"path": "src/http", "file_count": 89},
                    {"path": "src/core", "file_count": 54},
                    {"path": "src/event", "file_count": 20},
                    {"path": "src/os", "file_count": 18},
                    {"path": "src/stream", "file_count": 12},
                    {"path": "src/mail", "file_count": 8}
                ],
                "package_groups": [
                    {"name": "http", "file_count": 89, "test_file_count": 0},
                    {"name": "core", "file_count": 54, "test_file_count": 0},
                    {"name": "event", "file_count": 20, "test_file_count": 0},
                    {"name": "os", "file_count": 18, "test_file_count": 0},
                    {"name": "stream", "file_count": 12, "test_file_count": 0},
                    {"name": "mail", "file_count": 8, "test_file_count": 0}
                ]
            }),
        ),
        sig(
            "HIGH_COMPLEXITY",
            "medium",
            "342 symbols exceed complexity threshold of 20.",
            json!({
                "high_complexity_count": 342, "threshold": 20,
                "top_complex": [
                    {"symbol": "ngx_http_upstream_process_header", "file": "src/http/ngx_http_upstream.c", "complexity": 89},
                    {"symbol": "ngx_resolver_process_response", "file": "src/core/ngx_resolver.c", "complexity": 67},
                    {"symbol": "ngx_http_v2_state_headers", "file": "src/http/v2/ngx_http_v2.c", "complexity": 61},
                    {"symbol": "ngx_http_parse_complex_uri", "file": "src/http/ngx_http_parse.c", "complexity": 54},
                    {"symbol": "ngx_conf_parse", "file": "src/core/ngx_conf_file.c", "complexity": 48}
                ]
            }),
        ),
        sig(
            "IMPORT_CYCLES",
            "medium",
            "3 import cycles detected at the module level.",
            json!({ "cycle_count": 3, "cycles": [{"length": 3, "modules": ["http", "core", "event"]}] }),
        ),
    ];
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "LOW".to_string(),
                reasons: vec!["call_resolution_rate=42.0%_below_50%".to_string()],
            }),
            import_graph: Some(ReliabilityAxis {
                level: "LOW".to_string(),
                reasons: vec!["unresolved_imports=944".to_string()],
            }),
            change_impact: Some(ReliabilityAxis {
                level: "MEDIUM".to_string(),
                reasons: vec!["alias_resolution_suspicion".to_string()],
            }),
        }),
        call_coverage: None,
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });
    r
}

// ── Focus handling (preserved) ──────────────────────────────────

#[test]
fn repo_focus_omits_focus_line() {
    // Repo-level focus adds no information — the headline names the repo.
    let out = minimal_response().render_human(OrientDepth::Small);
    assert!(!out.contains("Focus:"));
}

#[test]
fn path_focus_shows_focus_line() {
    let mut r = minimal_response();
    r.focus = Focus {
        input: Some("src/core".to_string()),
        resolved: true,
        resolved_kind: Some("module".to_string()),
        resolved_path: Some("src/core".to_string()),
        reason: None,
    };
    let out = r.render_human(OrientDepth::Small);
    assert!(out.contains("Focus: src/core (module)"));
}

#[test]
fn unresolved_focus_shows_reason() {
    let mut r = minimal_response();
    r.focus = Focus {
        input: Some("nonexistent/path".to_string()),
        resolved: false,
        resolved_kind: None,
        resolved_path: None,
        reason: Some("no_match".to_string()),
    };
    let out = r.render_human(OrientDepth::Small);
    assert!(out.contains("Focus: nonexistent/path (unresolved: no_match)"));
}

#[test]
fn hides_internal_fields() {
    let out = nginx_like().render_human(OrientDepth::Full);
    assert!(!out.contains("snap-123"), "snapshot uid should be hidden");
    assert!(!out.contains("\"schema\""), "schema should be hidden");
}

// ── ORIENT-DENSITY-1: the dense headline (named, load-bearing) ──

#[test]
fn small_structure_line_names_modules() {
    // §3.1 + §4 + MODULE-MODEL-1 D2(i): small budget LEADS with the NAMED
    // package-group topology, with the declared/inferred module count as a
    // separately-labelled secondary fact (never collapsed).
    let out = nginx_like().render_human(OrientDepth::Small);
    assert!(
        out.contains(
            "nginx · 397 files, 5000 symbols · 6 package groups: http, core, event, os, stream, mail · 6 modules"
        ),
        "small must NAME the package groups + label the module count:\n{out}"
    );
}

#[test]
fn small_complexity_centers_are_named() {
    // §3.2: NAME the top complex files, not "342 exceed threshold".
    let out = nginx_like().render_human(OrientDepth::Small);
    assert!(
        out.contains("Complexity centers: src/http/ngx_http_upstream.c (cx 89)"),
        "small must NAME the complexity centers:\n{out}"
    );
    // top-3 at small; the rest are pointed to (honest, not dropped).
    assert!(out.contains("src/core/ngx_resolver.c (cx 67)"));
    assert!(out.contains("src/http/v2/ngx_http_v2.c (cx 61)"));
    assert!(
        out.contains("+339 more above threshold — rmap hotspots"),
        "must honestly point to the remaining 342-3 centers:\n{out}"
    );
}

#[test]
fn small_cycles_and_docs_line() {
    let out = nginx_like().render_human(OrientDepth::Small);
    assert!(
        out.contains("3 import cycles (http -> core -> event -> http). Docs: README.md, CHANGES."),
        "cycles (named anchor) + docs on one dense line:\n{out}"
    );
}

#[test]
fn small_reliability_is_one_compressed_line() {
    // §3.5: ONE honest caveat line, not the three-axis Degradation block.
    let out = nginx_like().render_human(OrientDepth::Small);
    assert!(
        out.contains(
            "Reliability: your code's calls 42% resolved (LOW) — verify call/dead claims against source."
        ),
        "one compressed reliability caveat with the real %:\n{out}"
    );
    // The verbose per-axis Degradation section is depth, NOT in the small headline.
    assert!(
        !out.contains("Degradation"),
        "no 3-line degradation at small:\n{out}"
    );
    assert!(!out.contains("Import-graph reliability is LOW"));
}

#[test]
fn orient_renders_external_coverage_map_from_shared_view() {
    // RELIABILITY-REFRAME-1 (review-0 defect / REVISE): orient CONSUMES the ONE shared
    // projection built from the overlay's call-coverage COUNTS — the in-scope rate + band
    // (compressed line) AND the external share + NAMED coverage map (--full) — the same
    // `CallReliabilityView` trust/check render, not a bespoke per-surface number.
    let mut r = minimal_response();
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "LOW".to_string(),
                reasons: vec!["call_resolution_rate=42.0%_below_50%".to_string()],
            }),
            import_graph: None,
            change_impact: None,
        }),
        // 42 resolved / (42 + 58) in-scope = 42%; 100 external of 200 total calls = 50%.
        call_coverage: Some(repo_graph_trust::CallCoverage {
            resolved_calls: 42,
            unresolved_calls: 158,
            unresolved_calls_external: 100,
            unresolved_calls_internal_like: 58,
            unresolved_calls_unknown: 0,
            external_targets: vec![
                repo_graph_trust::types::EnrichmentTopType {
                    type_name: "Value".to_string(),
                    count: 30,
                    is_external: true,
                },
                repo_graph_trust::types::EnrichmentTopType {
                    type_name: "Vec".to_string(),
                    count: 12,
                    is_external: true,
                },
            ],
        }),
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });

    // Compressed headline (Small): in-scope rate + band from the view's COUNTS.
    let small = r.render_human(OrientDepth::Small);
    assert!(
        small.contains("Reliability: your code's calls 42% resolved (LOW)"),
        "compressed in-scope rate from the shared view:\n{small}"
    );

    // Full Degradation: the external SHARE + the NAMED coverage map, reader-frame.
    let full = r.render_human(OrientDepth::Full);
    assert!(
        full.contains("50% of calls go into external libraries — follow to their crates/docs"),
        "external share named as reader context:\n{full}"
    );
    assert!(
        full.contains(
            "External coverage (heuristic): `Value` (30), `Vec` (12) — follow to their crates/docs"
        ),
        "top external receiver targets named, count-desc:\n{full}"
    );
    // review-1 §2: the map carries BOTH distinct EY1-A heuristic bases — the receiver TYPE
    // (language-server hover) and the EXTERNAL classification (static name-set, NOT
    // compiler-verified) — compactly, as honest as trust's detailed section.
    assert!(
        full.contains("receiver types inferred from a language-server type hover"),
        "receiver-type basis on the orient coverage map:\n{full}"
    );
    assert!(
        full.contains("static std/library name-set, not compiler-verified"),
        "external-classification basis (not compiler-verified) on the orient coverage map:\n{full}"
    );
    // Never the grades-us pipeline frame on any orient reader line.
    assert!(!full.contains("Call graph reliability is"));
    assert!(!full.to_lowercase().contains("call resolution rate"));
    // review-2 §1: the map is reader CONTEXT — it renders under its own `External calls`
    // heading, decoupled from the band-gated `Degradation` block.
    assert!(
        full.contains("External calls"),
        "coverage map lives in its own context section:\n{full}"
    );
}

#[test]
fn orient_external_coverage_visible_when_call_graph_band_is_high() {
    // RELIABILITY-REFRAME-1 (review-2 §1): the external coverage map is CONTEXT, not a grade, so
    // it must stay visible even when the in-scope band is HIGH. It used to be nested inside the
    // `cg.level != "HIGH"` degradation branch, which hid reader context behind a good grade. Here
    // the in-scope rate is a GENUINE HIGH (90%), and the external context still renders.
    let mut r = minimal_response();
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "HIGH".to_string(), // 90 / (90 + 10) in-scope = 90% → genuine HIGH
                reasons: vec![],
            }),
            import_graph: None,
            change_impact: None,
        }),
        // 90 resolved, 10 in-scope unresolved; 40 external of 140 total calls = 28.6% → 29%.
        call_coverage: Some(repo_graph_trust::CallCoverage {
            resolved_calls: 90,
            unresolved_calls: 50,
            unresolved_calls_external: 40,
            unresolved_calls_internal_like: 10,
            unresolved_calls_unknown: 0,
            external_targets: vec![repo_graph_trust::types::EnrichmentTopType {
                type_name: "Arc".to_string(),
                count: 25,
                is_external: true,
            }],
        }),
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });

    let full = r.render_human(OrientDepth::Full);
    // The External calls context section renders DESPITE the HIGH band.
    assert!(
        full.contains("External calls"),
        "external coverage section present at HIGH band:\n{full}"
    );
    assert!(
        full.contains("29% of calls go into external libraries — follow to their crates/docs"),
        "external share visible regardless of grade:\n{full}"
    );
    assert!(
        full.contains("External coverage (heuristic): `Arc` (25)"),
        "named external target visible at HIGH band:\n{full}"
    );
    // A HIGH band is not restated as a degradation line — the context above is enough.
    assert!(
        !full.contains("your code's calls 90% resolved"),
        "a HIGH in-scope band is not surfaced as a degradation line:\n{full}"
    );
}

#[test]
fn orient_zero_in_scope_calls_is_honest_no_fabricated_rate() {
    // RELIABILITY-REFRAME-1 (iteration-5 §2): a repo whose calls are ALL external has zero
    // in-scope calls (nothing to grade). `compute_call_graph_reliability(0,0)` is a vacuous HIGH;
    // orient must NOT fabricate an in-scope "100% resolved" rate. The prior contract let it stay
    // SILENT on the in-scope rate ("honest by omission") — the ratified rule now REQUIRES the
    // explicit "no in-scope calls measured" (unknown, not silence, not a fabricated 100%) at
    // every budget, while still naming WHERE the external calls go — context, not a grade.
    let mut r = minimal_response();
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "HIGH".to_string(), // the vacuous 0-of-0 band
                reasons: vec![],
            }),
            import_graph: None,
            change_impact: None,
        }),
        // 0 resolved, 0 in-scope unresolved — every call is external (50 of 50 = 100%).
        call_coverage: Some(repo_graph_trust::CallCoverage {
            resolved_calls: 0,
            unresolved_calls: 50,
            unresolved_calls_external: 50,
            unresolved_calls_internal_like: 0,
            unresolved_calls_unknown: 0,
            external_targets: vec![repo_graph_trust::types::EnrichmentTopType {
                type_name: "Buffer".to_string(),
                count: 40,
                is_external: true,
            }],
        }),
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });

    let full = r.render_human(OrientDepth::Full);
    // The explicit honest unknown renders — NOT silence (fixes the prior "blesses silence" test).
    assert!(
        full.contains("Reliability: no in-scope calls measured"),
        "zero in-scope renders the explicit honest unknown, not silence:\n{full}"
    );
    // Context still named: 100% external + the named target.
    assert!(
        full.contains("100% of calls go into external libraries"),
        "all-external repo names the external share as context:\n{full}"
    );
    assert!(
        full.contains("External coverage (heuristic): `Buffer` (40)"),
        "named external target for an all-external repo:\n{full}"
    );
    // No fabricated in-scope rate: never "your code's calls N% resolved" / "100% resolved".
    assert!(
        !full.contains("your code's calls"),
        "zero in-scope calls must not fabricate a resolved rate:\n{full}"
    );
    assert!(
        !full.contains("100% resolved"),
        "vacuous HIGH must not read as a fabricated 100% resolved:\n{full}"
    );
}

#[test]
fn orient_small_headline_carries_material_unclassified_caveat() {
    // iteration-5 §2: the material-unclassified qualification (review-3 §2) rides the DEFAULT
    // (small) headline, not only `--full` — the ratified unknown rules apply at EVERY budget.
    // The caveat comes from the SAME shared helper the `--full` External calls section uses.
    let mut r = minimal_response();
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "LOW".to_string(),
                reasons: vec!["call_resolution_rate=42.0%_below_50%".to_string()],
            }),
            import_graph: None,
            change_impact: None,
        }),
        // in-scope = 42 / (42 + 58) = 42%; unclassified 30 of 100 in-scope = 30% ≥ 20% material.
        call_coverage: Some(repo_graph_trust::CallCoverage {
            resolved_calls: 42,
            unresolved_calls: 158,
            unresolved_calls_external: 100,
            unresolved_calls_internal_like: 58,
            unresolved_calls_unknown: 30,
            external_targets: vec![],
        }),
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });
    let small = r.render_human(OrientDepth::Small);
    assert!(
        small.contains("Reliability: your code's calls 42% resolved (LOW)"),
        "in-scope rate on the small headline:\n{small}"
    );
    assert!(
        small.contains("30 of these 100 calls are unclassified"),
        "material-unclassified caveat rides the DEFAULT surface, not only --full:\n{small}"
    );
    assert!(
        small.contains("true resolved share may be higher"),
        "the caveat states the rate is a lower bound:\n{small}"
    );
    // An IMMATERIAL unclassified share (< 20%) stays silent — the caveat is not noise.
    r.trust_briefing.as_mut().unwrap().call_coverage = Some(repo_graph_trust::CallCoverage {
        resolved_calls: 42,
        unresolved_calls: 158,
        unresolved_calls_external: 100,
        unresolved_calls_internal_like: 58,
        unresolved_calls_unknown: 10, // 10 of 100 = 10% < 20%
        external_targets: vec![],
    });
    let immaterial = r.render_human(OrientDepth::Small);
    assert!(
        !immaterial.contains("unclassified"),
        "immaterial unclassified share is silent at small:\n{immaterial}"
    );
}

#[test]
fn orient_small_headline_empty_call_graph_still_reads_no_in_scope_calls_measured() {
    // review-6 §1: an EMPTY call graph (all coverage counts zero, the vacuous HIGH band)
    // is still a zero-in-scope measurement — the default surface must render the honest
    // "no in-scope calls measured", NOT fall silent behind a `total_calls > 0` gate. With
    // zero total calls the external share is genuinely UNKNOWN (ExternalShare = None), so
    // no share line renders either — unknown stays unknown, never a fabricated 0%.
    let mut r = minimal_response();
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "HIGH".to_string(), // vacuous band over an empty graph
                reasons: vec![],
            }),
            import_graph: None,
            change_impact: None,
        }),
        call_coverage: Some(repo_graph_trust::CallCoverage {
            resolved_calls: 0,
            unresolved_calls: 0,
            unresolved_calls_external: 0,
            unresolved_calls_internal_like: 0,
            unresolved_calls_unknown: 0,
            external_targets: vec![],
        }),
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });
    let small = r.render_human(OrientDepth::Small);
    assert!(
        small.contains("Reliability: no in-scope calls measured"),
        "an empty call graph renders the honest unknown at the DEFAULT budget, not silence:\n{small}"
    );
    assert!(
        !small.contains("of calls go into external libraries"),
        "zero total calls = UNKNOWN external share — no fabricated share line:\n{small}"
    );
    assert!(
        !small.contains("your code's calls"),
        "no fabricated rate on an empty call graph:\n{small}"
    );
}

#[test]
fn orient_small_headline_zero_in_scope_reads_no_in_scope_calls_measured() {
    // iteration-5 §2: a repo whose calls are ALL external (the vacuous 0-of-0 HIGH band) must
    // NOT fall silent on the DEFAULT surface — it renders the honest "no in-scope calls
    // measured" (unknown, never a fabricated 100%) plus the external share as compact context.
    let mut r = minimal_response();
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "HIGH".to_string(), // the vacuous 0-of-0 band
                reasons: vec![],
            }),
            import_graph: None,
            change_impact: None,
        }),
        call_coverage: Some(repo_graph_trust::CallCoverage {
            resolved_calls: 0,
            unresolved_calls: 50,
            unresolved_calls_external: 50,
            unresolved_calls_internal_like: 0,
            unresolved_calls_unknown: 0,
            external_targets: vec![repo_graph_trust::types::EnrichmentTopType {
                type_name: "Buffer".to_string(),
                count: 40,
                is_external: true,
            }],
        }),
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });
    let small = r.render_human(OrientDepth::Small);
    assert!(
        small.contains("Reliability: no in-scope calls measured"),
        "zero in-scope renders the honest unknown at the DEFAULT budget, not silence:\n{small}"
    );
    assert!(
        small.contains("100% of calls go into external libraries"),
        "the external share is compact context at small:\n{small}"
    );
    assert!(
        !small.contains("your code's calls"),
        "no fabricated in-scope rate:\n{small}"
    );
    assert!(
        !small.contains("100% resolved"),
        "no fabricated 100% resolved:\n{small}"
    );
}

#[test]
fn small_is_dense_not_thin_meta() {
    // The headline finding: small must be dense load-bearing orientation,
    // not the old severity-grouped meta list.
    let out = nginx_like().render_human(OrientDepth::Small);
    // Dense, NAMED facts present.
    assert!(out.contains("package groups: http, core"));
    assert!(out.contains("Complexity centers: src/http"));
    assert!(out.contains("Reliability: your code's calls"));
    // The old thin-meta surface is gone: no "Signals / High / Medium / Low" grouping.
    assert!(
        !out.contains("\n  High\n"),
        "no severity grouping at small:\n{out}"
    );
    assert!(
        !out.contains("Signals\n"),
        "no Signals heading at small:\n{out}"
    );
}

#[test]
fn small_points_to_full_for_depth() {
    let out = nginx_like().render_human(OrientDepth::Small);
    assert!(out.contains("[--full for the complete breakdown"));
}

// ── ORIENT-DENSITY-1 §5: budget trades DEPTH (small ⊂ large) ────

#[test]
fn budget_trades_depth_small_subset_of_full() {
    let r = nginx_like();
    let small = r.render_human(OrientDepth::Small);
    let full = r.render_human(OrientDepth::Full);

    // The dense headline is present at BOTH tiers (budget never strips it).
    assert!(small.contains("package groups: http, core, event, os, stream, mail"));
    assert!(full.contains("package groups: http, core, event, os, stream, mail"));
    assert!(small.contains("src/http/ngx_http_upstream.c (cx 89)"));
    assert!(full.contains("src/http/ngx_http_upstream.c (cx 89)"));

    // FULL adds DEPTH small does not have: the package-group topology breakdown
    // AND the (separately-labelled) declared/inferred module breakdown, the
    // per-axis reliability, and the full certainty/provenance block.
    assert!(
        full.contains("Package groups (directory/package topology"),
        "full adds the topology breakdown:\n{full}"
    );
    assert!(full.contains("http — 89 files"), "{full}");
    assert!(
        full.contains("Modules (declared/inferred, by size)"),
        "full adds the labelled module breakdown:\n{full}"
    );
    assert!(full.contains("src/http — 89 files"));
    assert!(
        full.contains("Degradation"),
        "full expands per-axis reliability"
    );
    assert!(full.contains("Import-graph reliability is LOW"));

    // SMALL does NOT carry the depth sections (they are the trade).
    assert!(!small.contains("Package groups (directory/package topology"));
    assert!(!small.contains("Modules (declared/inferred, by size)"));
    assert!(!small.contains("Degradation"));
    // FULL is complete → no "--full" pointer; SMALL has it.
    assert!(small.contains("[--full for the complete breakdown"));
    assert!(!full.contains("[--full for the complete breakdown"));
}

#[test]
fn medium_adds_limits_and_next_but_small_does_not() {
    let mut r = nginx_like();
    r.limits = vec![Limit {
        code: "GATE_NOT_CONFIGURED".to_string(),
        summary: "No active requirement declarations.".to_string(),
    }];
    r.next = vec![NextAction {
        kind: "check".to_string(),
        repo: "nginx".to_string(),
        target: None,
        reason: "Verify current state.".to_string(),
    }];

    let small = r.render_human(OrientDepth::Small);
    assert!(
        !small.contains("Limits"),
        "limits are depth, not headline:\n{small}"
    );
    assert!(!small.contains("Next steps"));

    let medium = r.render_human(OrientDepth::Medium);
    assert!(medium.contains("Limits"));
    assert!(medium.contains("No active requirement declarations."));
    assert!(medium.contains("Next steps"));
    assert!(medium.contains("rmap check"));
    // The dense headline is still there at medium.
    assert!(medium.contains("package groups: http, core"));
}

// ── ORIENT-DENSITY-1 §3: high-severity alerts stay load-bearing ──

#[test]
fn headline_surfaces_gate_alert() {
    let mut r = nginx_like();
    r.signals.push(sig(
        "GATE_FAIL",
        "high",
        "Gate fails: 2 of 5 obligations failing.",
        serde_json::Value::Null,
    ));
    let out = r.render_human(OrientDepth::Small);
    assert!(
        out.contains("Alert: Gate fails: 2 of 5 obligations failing."),
        "gate failure is a load-bearing alert, surfaced at small:\n{out}"
    );
}

#[test]
fn honesty_named_facts_are_extracted_no_dead_claim() {
    // Density is ON TOP of the truth-audit: the named facts are the EXTRACTED
    // evidence verbatim, and no withdrawn "dead" claim is reintroduced.
    let out = nginx_like().render_human(OrientDepth::Full);
    assert!(out.contains("src/http")); // module name from evidence
    assert!(out.contains("42% resolved")); // the real call-resolution rate
    assert!(
        !out.to_lowercase().contains("dead code"),
        "the withdrawn dead-code surface must not reappear:\n{out}"
    );
}

// ── Cycle anchor formatting (preserved behavior) ────────────────

#[test]
fn cycle_anchor_full_chain_for_small_cycle() {
    let mut r = minimal_response();
    r.signals = vec![sig(
        "IMPORT_CYCLES",
        "medium",
        "1 import cycle detected.",
        serde_json::json!({
            "cycle_count": 1,
            "cycles": [{ "length": 4, "modules": ["Auth", "User", "Session", "Config"] }]
        }),
    )];
    let out = r.render_human(OrientDepth::Small);
    assert!(out.contains("1 import cycle (Auth -> User -> Session -> Config -> Auth)"));
}

#[test]
fn cycle_anchor_truncates_large_cycle() {
    let mut r = minimal_response();
    r.signals = vec![sig(
        "IMPORT_CYCLES",
        "medium",
        "1 import cycle detected.",
        serde_json::json!({
            "cycle_count": 1,
            "cycles": [{ "length": 10, "modules": ["A","B","C","D","E","F","G","H","I","J"] }]
        }),
    )];
    let out = r.render_human(OrientDepth::Small);
    assert!(out.contains("A -> B -> C -> ..."));
    assert!(out.contains("-> J -> A"));
}

#[test]
fn deserialize_from_daemon_json() {
    let json = r#"{
            "schema": "rgr.agent.v1",
            "command": "orient",
            "repo": "my-app",
            "snapshot": "snap-abc",
            "focus": { "resolved": true, "resolved_kind": "repo" },
            "confidence": "high",
            "signals": [],
            "limits": [],
            "next": [],
            "truncated": false
        }"#;
    let r: OrientResponse = serde_json::from_str(json).unwrap();
    assert_eq!(r.repo, "my-app");
    assert!(r.focus.resolved);
}

// ── The CoherenceEnvelope wrapper wire shape (preserved, depth-aware) ──

#[test]
fn wrapper_full_renders_dense_body_serving_block_and_degradation() {
    // The full daemon wire shape, rendered at --full: dense headline + the
    // expanded per-axis Degradation + the full Serving/provenance block (D3 relabel).
    let json = r#"{
            "value": {
                "schema": "rgr.agent.v1",
                "command": "orient",
                "repo": "my-app",
                "snapshot": "snap-abc",
                "focus": { "resolved": true, "resolved_kind": "repo" },
                "confidence": "medium",
                "signals": [
                    {
                        "value": {
                            "code": "IMPORT_CYCLES",
                            "severity": "medium",
                            "category": "structure",
                            "summary": "1 import cycle detected.",
                            "evidence": { "cycle_count": 1, "cycles": [{ "length": 2, "modules": ["http", "core"] }] }
                        },
                        "provenance": { "source": ["livegraph"] },
                        "trust": { "class": "Exact", "completeness": "Complete" },
                        "freshness": "Fresh"
                    }
                ],
                "limits": [],
                "next": [],
                "truncated": false,
                "trust_briefing": {
                    "call_graph_reliability": "medium",
                    "call_resolution_rate": 0.78,
                    "caveats": ["Enrichment phase did not run."]
                }
            },
            "provenance": { "source": ["livegraph", "sqlite"] },
            "trust": { "class": "Partial", "completeness": "Degraded" },
            "freshness": "PrecisionPending"
        }"#;
    let env: CoherenceEnvelope<OrientResponse> = serde_json::from_str(json).unwrap();
    let out = render_orient_envelope(&env, OrientDepth::Full);
    // Dense body: repo named in the structure line, cycle anchor rendered.
    assert!(out.contains("my-app"));
    assert!(out.contains("1 import cycle (http -> core -> http)"));
    // Compressed reliability caveat (legacy fields → 78%), reader-frame.
    assert!(out.contains("Reliability: your code's calls 78% resolved"));
    // Full per-axis Degradation present at --full.
    assert!(out.contains("Degradation"));
    assert!(out.contains("your code's calls 78% resolved"));
    // HONEST-DEGRADATION-IMPL-2 (D3): the footer is SERVING/provenance, NOT "Certainty"; the answer-class
    // is scoped ("answer basis partial"), never a bare global word; freshness + sources are preserved.
    assert!(out.contains("Serving"), "{out}");
    assert!(
        !out.contains("Certainty"),
        "the global-certainty label must be gone: {out}"
    );
    assert!(out.contains("answer basis partial"), "{out}");
    assert!(out.contains("some required inputs incomplete"), "{out}");
    // INDEX-BASIS-1 (review-0 fix #2): the coherence envelope freshness MEET keeps
    // its own name — it is NOT relabeled `parse`. This fixture attaches no
    // `parse_status`, so no `parse` clause appears (parse is a SEPARATE axis).
    assert!(out.contains("freshness precisionpending"), "{out}");
    assert!(
        !out.contains("parse "),
        "no parse clause without parse_status: {out}"
    );
    assert!(out.contains("sources: livegraph, sqlite"), "{out}");
}

#[test]
fn wrapper_small_has_compressed_serving_no_degradation() {
    // Not degraded, small budget: one-line compressed Serving posture (D3 relabel), no Degradation block.
    let json = r#"{
            "value": {
                "schema": "rgr.agent.v1",
                "command": "orient",
                "repo": "clean-app",
                "snapshot": "snap-1",
                "focus": { "resolved": true, "resolved_kind": "repo" },
                "confidence": "high",
                "signals": [],
                "limits": [],
                "next": [],
                "truncated": false
            },
            "provenance": { "source": ["sqlite"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        }"#;
    let env: CoherenceEnvelope<OrientResponse> = serde_json::from_str(json).unwrap();
    assert!(env.value.trust_briefing.is_none());
    let out = render_orient_envelope(&env, OrientDepth::Small);
    assert!(out.contains("clean-app"));
    assert!(!out.contains("Degradation"));
    // HONEST-DEGRADATION-IMPL-2 (D3): compressed one-line Serving posture — scoped answer-basis, NOT a
    // bare global "exact" under "Certainty".
    // INDEX-BASIS-1 (review-0 fix #2): the envelope freshness keeps its own name;
    // no `parse_status` attached here → the pre-slice compressed footer, unchanged.
    // No `index_drift` in this fixture → no basis/drift line.
    assert!(
        out.contains("Serving: answer basis exact, freshness fresh · sources: sqlite"),
        "{out}"
    );
    assert!(
        !out.contains("parse "),
        "no parse clause without parse_status: {out}"
    );
    assert!(!out.contains("Certainty"), "{out}");
}

// ── INDEX-BASIS-1 — the index basis / working-tree drift footer line ───────────────────────────────

#[test]
fn wrapper_renders_index_basis_and_drift_line() {
    // When the daemon attaches `value.index_drift`, the footer carries the honest
    // basis + drift line (from `IndexDrift::describe`, the one wording home).
    let json = r#"{
            "value": {
                "schema": "rgr.agent.v1",
                "command": "orient",
                "repo": "moved-app",
                "snapshot": "snap-1",
                "focus": { "resolved": true, "resolved_kind": "repo" },
                "confidence": "high",
                "signals": [], "limits": [], "next": [], "truncated": false,
                "index_drift": {
                    "state": "drifted",
                    "basis": "abcdef0123456789",
                    "commits_ahead": 1,
                    "files_changed": 3,
                    "indexed_changed": 3,
                    "modules": ["src"]
                }
            },
            "provenance": { "source": ["sqlite"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        }"#;
    let env: CoherenceEnvelope<OrientResponse> = serde_json::from_str(json).unwrap();
    let out = render_orient_envelope(&env, OrientDepth::Small);
    assert!(out.contains("index basis: abcdef0"), "sha7 basis: {out}");
    assert!(out.contains("1 commit ahead"), "{out}");
    assert!(out.contains("3 files changed"), "{out}");
    assert!(out.contains("(3 indexed, modules src)"), "{out}");
    assert!(out.contains("rmap refresh"), "next action: {out}");
}

#[test]
fn wrapper_renders_basis_unknown_for_pre_slice_snapshot() {
    // A snapshot indexed before basis tracking → honest "unknown, run refresh".
    let json = r#"{
            "value": {
                "schema": "rgr.agent.v1", "command": "orient", "repo": "old-app",
                "snapshot": "snap-1",
                "focus": { "resolved": true, "resolved_kind": "repo" },
                "confidence": "high",
                "signals": [], "limits": [], "next": [], "truncated": false,
                "index_drift": { "state": "basis_unknown" }
            },
            "provenance": { "source": ["sqlite"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        }"#;
    let env: CoherenceEnvelope<OrientResponse> = serde_json::from_str(json).unwrap();
    let out = render_orient_envelope(&env, OrientDepth::Small);
    assert!(
        out.contains("index basis: unknown (indexed before basis tracking)"),
        "{out}"
    );
    assert!(out.contains("rmap refresh"), "{out}");
}

#[test]
fn wrapper_renders_parse_status_beside_freshness() {
    // review-0 fix #2: the honest `parse` axis (from value.parse_status) renders
    // BESIDE the coherence envelope `freshness` (which keeps its own name) — the two
    // are distinct axes, never conflated. Here freshness=fresh, parse=2 unparsed.
    let json = r#"{
            "value": {
                "schema": "rgr.agent.v1", "command": "orient", "repo": "app",
                "snapshot": "snap-1",
                "focus": { "resolved": true, "resolved_kind": "repo" },
                "confidence": "high",
                "signals": [], "limits": [], "next": [], "truncated": false,
                "parse_status": { "state": "unparsed", "count": 2 }
            },
            "provenance": { "source": ["sqlite"] },
            "trust": { "class": "Exact", "completeness": "Complete" },
            "freshness": "Fresh"
        }"#;
    let env: CoherenceEnvelope<OrientResponse> = serde_json::from_str(json).unwrap();
    let out = render_orient_envelope(&env, OrientDepth::Small);
    assert!(
        out.contains("Serving: answer basis exact, freshness fresh, parse: 2 unparsed"),
        "freshness kept its name; parse is its own honest value with the ratified `parse:` label: {out}"
    );

    // parse: ok when nothing failed to parse.
    let json_ok = json.replace(
        r#"{ "state": "unparsed", "count": 2 }"#,
        r#"{ "state": "ok" }"#,
    );
    let env_ok: CoherenceEnvelope<OrientResponse> = serde_json::from_str(&json_ok).unwrap();
    let out_ok = render_orient_envelope(&env_ok, OrientDepth::Small);
    assert!(out_ok.contains("freshness fresh, parse: ok"), "{out_ok}");
}

// ── HONEST-DEGRADATION-IMPL-2 (D5) — toolchain-aware next-action line renders ──────────────────────

#[test]
fn relationship_next_action_renders_in_headline() {
    // D5: the daemon-supplied next-action renders in the dense headline (present at every budget). The
    // daemon decides WHICH line (here the C no-path case); the renderer surfaces it verbatim.
    let mut resp = minimal_response();
    resp.relationship_next_action = Some(
        "no semantic-resolution path exists for C on this build; these relationship facts remain \
         low-confidence"
            .to_string(),
    );
    let out = resp.render_human(OrientDepth::Small);
    assert!(
        out.contains("no semantic-resolution path exists for C"),
        "{out}"
    );
}

#[test]
fn relationship_next_action_absent_renders_nothing() {
    // None (resolved repo / no honest statement) → no line, no noise.
    let resp = minimal_response(); // relationship_next_action: None
    let out = resp.render_human(OrientDepth::Small);
    assert!(!out.contains("semantic-resolution"), "{out}");
    assert!(!out.contains("rmap enrich"), "{out}");
}

// ── ORIENT-DENSITY-1 review-1 #1: docs are a load-bearing headline fact ──

#[test]
fn docs_render_in_headline_without_cycles() {
    // The real nginx run (review-1 #1) had docs but NO import cycles, yet showed
    // no Docs line. The ROOT cause was the daemon-side root_path resolution (fixed
    // + covered by the storage `doc_inventory_resolves_db_relative_root_path`
    // test); this asserts the PRESENTATION surfaces docs whenever the daemon
    // supplies them, even with no cycles to share the line with.
    let mut r = minimal_response();
    r.documentation = Some(DocumentationSection {
        relevant_files: vec![
            RelevantDoc {
                path: "README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                reason: "repo_root_doc".to_string(),
            },
            RelevantDoc {
                path: "CONTRIBUTING.md".to_string(),
                kind: "architecture".to_string(),
                generated: false,
                reason: "repo_root_doc".to_string(),
            },
        ],
        count: 2,
    });
    // No IMPORT_CYCLES signal in this fixture.
    let out = r.render_human(OrientDepth::Small);
    assert!(
        out.contains("Docs: README.md, CONTRIBUTING.md"),
        "docs render at small even without cycles:\n{out}"
    );
    assert!(
        !out.contains("import cycle"),
        "no cycles in this fixture:\n{out}"
    );
}

// ── ORIENT-DENSITY-1 review-1 #2: --full is COMPLETE, headline stays bounded ──

#[test]
fn full_complexity_breakdown_complete_headline_bounded() {
    use serde_json::json;
    // 8 centers, ALL carried in the evidence (count == len) — i.e. the agent sent
    // everything at `--full` (the agent-layer test proves the evidence scales).
    // The HEADLINE names a bounded top-5; the dedicated SECTION is the COMPLETE
    // set with NO "+N more" pointer (the review-1 #2 "(+338 more)" failure).
    let top: Vec<_> = (0..8)
        .map(|i| {
            json!({
                "symbol": format!("fn{i}"),
                "file": format!("src/f{i}.c"),
                "complexity": 90 - i
            })
        })
        .collect();
    let mut r = minimal_response();
    r.signals = vec![sig(
        "HIGH_COMPLEXITY",
        "medium",
        "8 symbols exceed complexity threshold of 20.",
        json!({ "high_complexity_count": 8, "threshold": 20, "top_complex": top }),
    )];

    let full = r.render_human(OrientDepth::Full);

    // The headline LINE is a bounded top-5 — never a dump of every center.
    let headline = full
        .lines()
        .find(|l| l.starts_with("Complexity centers:"))
        .expect("headline complexity line present");
    assert!(headline.contains("src/f0.c (cx 90)"));
    assert!(headline.contains("src/f4.c (cx 86)"));
    assert!(
        !headline.contains("src/f5.c"),
        "headline is bounded at top-5, not all 8:\n{headline}"
    );
    // At --full there is NO "+N more" pointer — the complete section follows.
    assert!(
        !full.contains("more above threshold"),
        "no truncation pointer at --full:\n{full}"
    );
    // The dedicated section is the COMPLETE list (all 8, incl. the tail f5..f7).
    assert!(full.contains("Complexity centers (by cyclomatic complexity)"));
    assert!(
        full.contains("src/f7.c — fn7 (cx 83)"),
        "the breakdown section lists EVERY center, named with its symbol:\n{full}"
    );

    // At small the headline DOES point to the rest (honest) and there is NO section.
    let small = r.render_human(OrientDepth::Small);
    assert!(
        small.contains("more above threshold — rmap hotspots"),
        "small honestly points to the centers it does not name:\n{small}"
    );
    assert!(
        !small.contains("Complexity centers (by cyclomatic"),
        "the full breakdown section is depth, absent at small"
    );
}

// ── METRIC-LANG-COVERAGE-1 (part A): coverage caveat on the primary surface ──

#[test]
fn measurement_coverage_caveat_rides_headline_at_every_tier() {
    use repo_graph_classification::measurement_coverage::{
        LanguageFunctionCount, MeasurementCoverageBlock,
    };
    let mut r = nginx_like();
    // Rust unmeasured (72%), TS measured — the repo-graph self-index shape.
    r.measurement_coverage = Some(MeasurementCoverageBlock::from_counts(vec![
        LanguageFunctionCount {
            language: "rust".to_string(),
            function_count: 72,
            measured_count: 0,
        },
        LanguageFunctionCount {
            language: "typescript".to_string(),
            function_count: 28,
            measured_count: 28,
        },
    ]));
    // The caveat is load-bearing at every depth — present at small AND full.
    for depth in [OrientDepth::Small, OrientDepth::Full] {
        let out = r.render_human(depth);
        assert!(
            out.contains("Rust (72% of functions)") && out.contains("not yet measured"),
            "coverage caveat must ride the headline at {depth:?}:\n{out}"
        );
    }
}

#[test]
fn no_measurement_coverage_caveat_when_all_measured() {
    use repo_graph_classification::measurement_coverage::{
        LanguageFunctionCount, MeasurementCoverageBlock,
    };
    let mut r = nginx_like();
    r.measurement_coverage = Some(MeasurementCoverageBlock::from_counts(vec![
        LanguageFunctionCount {
            language: "rust".to_string(),
            function_count: 100,
            measured_count: 100,
        },
    ]));
    let out = r.render_human(OrientDepth::Full);
    assert!(
        !out.contains("not yet measured"),
        "no caveat when every significant language is measured:\n{out}"
    );
}

#[test]
fn measurement_coverage_unavailable_is_stated_not_silent() {
    // review-6 item 2 (orient human surface): a coverage read failure must SAY SO on the
    // headline, never render as if the complexity centers covered the whole repo.
    use repo_graph_classification::measurement_coverage::MeasurementCoverageBlock;
    let mut r = nginx_like();
    r.measurement_coverage = Some(MeasurementCoverageBlock::unavailable());
    let out = r.render_human(OrientDepth::Full);
    assert!(
        out.contains("could not be read"),
        "unavailable coverage must be stated on the orient headline:\n{out}"
    );
}

// ── RELIABILITY-REFRAME-1 review-3 §4 / slice §1.4: the ONE shared projection ─────────────────
//
// The binding proof that orient, trust, AND check consume the SAME complete projection — the same
// in-scope rate (EXCLUDING external), the same external share, and the same named target from ONE
// `CallReliabilityView` derivation — NOT merely equivalent wording from partial inputs (review-0's
// objection to the earlier `one_shared_computation` test). Each surface is driven by the SAME
// counts through its REAL render entry point; the assertion is that the identical projection-derived
// strings appear in all three, and that the external-INCLUSIVE rate (21%) appears in NONE of them.

/// A trust report carrying the shared binding counts (resolved 42 / in-scope-unresolved 58 /
/// external 100 / total 200) plus the two external receiver targets.
fn binding_report() -> repo_graph_trust::types::TrustReport {
    use repo_graph_trust::types::*;
    let axis = |level, reasons: Vec<&str>| ReliabilityAxisScore {
        level,
        reasons: reasons.into_iter().map(String::from).collect(),
    };
    let no_dg = || DowngradeTrigger {
        triggered: false,
        reasons: vec![],
    };
    let ext = |name: &str, count| EnrichmentTopType {
        type_name: name.into(),
        count,
        is_external: true,
    };
    TrustReport {
        snapshot_uid: "snap_binding".into(),
        display_name: Some("binding".into()),
        basis_commit: None,
        toolchain: None,
        diagnostics_version: Some(1),
        summary: TrustSummary {
            edges_total: 200,
            edges_resolved: 200,
            unresolved_total: 158,
            resolved_calls: 42,
            unresolved_calls: 158,
            unresolved_calls_external: 100,
            unresolved_calls_internal_like: 58,
            call_resolution_rate: 0.42,
            reliability: TrustReliability {
                import_graph: axis(ReliabilityLevel::HIGH, vec![]),
                call_graph: axis(
                    ReliabilityLevel::LOW,
                    vec!["call_resolution_rate=42.0%_below_50%"],
                ),
                dead_code: axis(ReliabilityLevel::HIGH, vec![]),
                change_impact: axis(ReliabilityLevel::HIGH, vec![]),
            },
            triggered_downgrades: TrustDowngrades {
                framework_heavy_suspicion: no_dg(),
                registry_pattern_suspicion: no_dg(),
                missing_entrypoint_declarations: no_dg(),
                alias_resolution_suspicion: no_dg(),
            },
        },
        categories: vec![],
        classifications: vec![],
        basis_classifications: vec![],
        external_dependencies: Default::default(),
        unknown_calls_blast_radius: None,
        enrichment_status: Some(EnrichmentStatus {
            eligible: 42,
            enriched: 42,
            top_types: vec![],
            top_external_types: vec![ext("Value", 30), ext("Vec", 12)],
        }),
        modules: vec![],
        caveats: vec![],
        diagnostics_available: true,
        enrichment_eligible_count: 42,
        unresolved_calls_unknown: 0,
    }
}

#[test]
fn one_shared_projection_reaches_orient_trust_and_check() {
    use repo_graph_agent::check::{evaluate_conditions, CheckInput, ConditionCode};
    use repo_graph_agent::reliability::ExternalTarget;
    use repo_graph_agent::storage_port::{AgentReliabilityLevel, EnrichmentState};

    // ── trust surface ──
    let trust_out =
        crate::presentation::trust::render_trust_envelope(&repo_graph_trust::trust_to_coherent(
            binding_report(),
            repo_graph_trust::LiveGraphPosture::unavailable_leaf(),
            false,
        ));

    // ── orient surface (same counts on the overlay `call_coverage`) ──
    let mut r = minimal_response();
    r.trust_briefing = Some(TrustOverlay {
        reliability: Some(ReliabilitySection {
            call_graph: Some(ReliabilityAxis {
                level: "LOW".to_string(),
                reasons: vec!["call_resolution_rate=42.0%_below_50%".to_string()],
            }),
            import_graph: None,
            change_impact: None,
        }),
        call_coverage: Some(repo_graph_trust::CallCoverage {
            resolved_calls: 42,
            unresolved_calls: 158,
            unresolved_calls_external: 100,
            unresolved_calls_internal_like: 58,
            unresolved_calls_unknown: 0,
            external_targets: vec![
                repo_graph_trust::types::EnrichmentTopType {
                    type_name: "Value".into(),
                    count: 30,
                    is_external: true,
                },
                repo_graph_trust::types::EnrichmentTopType {
                    type_name: "Vec".into(),
                    count: 12,
                    is_external: true,
                },
            ],
        }),
        call_graph_reliability: None,
        call_resolution_rate: None,
        caveats: vec![],
    });
    let orient_out = r.render_human(OrientDepth::Full);

    // ── check surface (same counts on CheckInput) ──
    let check_out = evaluate_conditions(&CheckInput {
        snapshot_exists: true,
        files_total: 1,
        stale_file_count: 0,
        call_graph_reliability: Some(AgentReliabilityLevel::Low),
        resolved_calls: 42,
        unresolved_calls_internal_like: 58,
        unresolved_calls: 158,
        unresolved_calls_unknown: 0,
        external_targets: vec![
            ExternalTarget {
                type_name: "Value".into(),
                count: 30,
            },
            ExternalTarget {
                type_name: "Vec".into(),
                count: 12,
            },
        ],
        enrichment_state: Some(EnrichmentState::Ran),
        gate_outcome: None,
        index_drift: None,
    })
    .into_iter()
    .find(|c| c.code == ConditionCode::CallGraphReliability)
    .expect("CALL_GRAPH_RELIABILITY present")
    .summary;

    // Each surface renders the SAME three projection facts, from the SAME derivation.
    for (name, out) in [
        ("trust", &trust_out),
        ("orient", &orient_out),
        ("check", &check_out),
    ] {
        assert!(
            out.contains("42% resolved"),
            "{name} must render the shared IN-SCOPE rate (42%, external-excluded):\n{out}"
        );
        assert!(
            out.contains("50% of calls go into external libraries"),
            "{name} must render the shared external share (50%):\n{out}"
        );
        assert!(
            out.contains("`Value`"),
            "{name} must render the shared named external target:\n{out}"
        );
        // The external-INCLUSIVE rate (42 / 200 = 21%) must appear in NONE of them — proof they
        // consume the shared EXTERNAL-EXCLUDING projection, not a per-surface partial number.
        assert!(
            !out.contains("21% resolved"),
            "{name} must NOT use the external-inclusive rate (21%):\n{out}"
        );
    }
}
