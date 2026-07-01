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
            "Reliability: call-graph 42% resolved (LOW) — verify call/dead claims against source."
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
fn small_is_dense_not_thin_meta() {
    // The headline finding: small must be dense load-bearing orientation,
    // not the old severity-grouped meta list.
    let out = nginx_like().render_human(OrientDepth::Small);
    // Dense, NAMED facts present.
    assert!(out.contains("package groups: http, core"));
    assert!(out.contains("Complexity centers: src/http"));
    assert!(out.contains("Reliability: call-graph"));
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
    // Compressed reliability caveat (legacy fields → 78%).
    assert!(out.contains("Reliability: call-graph 78% resolved"));
    // Full per-axis Degradation present at --full.
    assert!(out.contains("Degradation"));
    assert!(out.contains("Call resolution rate: 78%"));
    // HONEST-DEGRADATION-IMPL-2 (D3): the footer is SERVING/provenance, NOT "Certainty"; the answer-class
    // is scoped ("answer basis partial"), never a bare global word; freshness + sources are preserved.
    assert!(out.contains("Serving"), "{out}");
    assert!(
        !out.contains("Certainty"),
        "the global-certainty label must be gone: {out}"
    );
    assert!(out.contains("answer basis partial"), "{out}");
    assert!(out.contains("some required inputs incomplete"), "{out}");
    assert!(out.contains("freshness precisionpending"), "{out}");
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
    assert!(
        out.contains("Serving: answer basis exact, freshness fresh · sources: sqlite"),
        "{out}"
    );
    assert!(!out.contains("Certainty"), "{out}");
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
