//! Unit tests for the ORIENT-SEGMENT-2 presentation renderers.
//!
//! Fixtures are built by DESERIALIZING an `OrientResponse` from JSON (the struct
//! derives `Deserialize`), which also exercises the wire shape the daemon injects
//! — the additive `directory_group_fallback` / `http_surfaces` / `module_identity`
//! fields. A response that trips none of these features renders exactly as today.

use super::super::orient::{OrientDepth, OrientResponse};
use serde_json::{json, Value};

/// Deserialize an `OrientResponse` from a JSON object, filling the required
/// identity fields so each test only states the fields it exercises.
fn response(mut extra: Value) -> OrientResponse {
    let base = extra.as_object_mut().expect("test json must be an object");
    base.entry("repo").or_insert(json!("test-repo"));
    base.entry("snapshot").or_insert(json!("snap-1"));
    base.entry("confidence").or_insert(json!("high"));
    base.entry("focus")
        .or_insert(json!({"resolved": true, "resolved_kind": "repo"}));
    serde_json::from_value(extra).expect("valid OrientResponse json")
}

// ── §2.5 HTTP surfaces headline ────────────────────────────────────────────────

#[test]
fn http_headline_renders_when_present() {
    let r = response(json!({
        "http_surfaces": {"total": 244, "providers": 222, "consumers": 23}
    }));
    let line = r.http_surfaces_line(OrientDepth::Small).unwrap();
    assert_eq!(
        line,
        "244 HTTP surfaces (222 providers / 23 consumers) — rmap surfaces"
    );
}

#[test]
fn http_headline_discloses_test_fixture_and_unknown_partition() {
    // COHERENCE-3 §2.2: the production counts head the line; the excluded test-fixture surfaces
    // and the unknown-is_test surfaces are disclosed in the SAME shape the `surfaces` command and
    // `cycles` use — so orient states the SAME production count as those surfaces, never inflated
    // by fixtures.
    let r = response(json!({
        "http_surfaces": {
            "total": 3, "providers": 3, "consumers": 0,
            "test_fixture_excluded": 6, "test_status_unknown": 1
        }
    }));
    let line = r.http_surfaces_line(OrientDepth::Small).unwrap();
    assert_eq!(
        line,
        "3 HTTP surfaces (3 providers / 0 consumers) (+6 test-fixture excluded; test-status \
         unknown for 1) — rmap surfaces"
    );
}

#[test]
fn http_headline_no_clause_when_no_fixtures_or_unknown() {
    // Zero excluded/unknown ⇒ no clause ⇒ byte-identical to the pre-slice headline.
    let r = response(json!({
        "http_surfaces": {"total": 5, "providers": 3, "consumers": 2,
                          "test_fixture_excluded": 0, "test_status_unknown": 0}
    }));
    assert_eq!(
        r.http_surfaces_line(OrientDepth::Small).unwrap(),
        "5 HTTP surfaces (3 providers / 2 consumers) — rmap surfaces"
    );
}

#[test]
fn http_headline_fixture_only_still_discloses_when_production_zero() {
    // COHERENCE-3 §2.2 (review-0 item 2): a repo whose ONLY HTTP surface is a test fixture
    // (storybook) — the daemon attaches the block with production `total == 0` but
    // `test_fixture_excluded > 0`. The pre-fix code returned `None` on `total == 0`, hiding the
    // exclusion ENTIRELY (a partition made invisible — standing honesty rule #1). Now the zero
    // production count renders WITH the exclusion clause, so the excluded fixtures are never
    // silent.
    let r = response(json!({
        "http_surfaces": {
            "total": 0, "providers": 0, "consumers": 0,
            "test_fixture_excluded": 1, "test_status_unknown": 0
        }
    }));
    assert_eq!(
        r.http_surfaces_line(OrientDepth::Small).unwrap(),
        "0 HTTP surfaces (0 providers / 0 consumers) (+1 test-fixture excluded) — rmap surfaces"
    );
}

#[test]
fn http_headline_clean_zero_with_nothing_to_disclose_is_none() {
    // A block attached with a genuine clean zero (no production surfaces, no fixtures, no
    // unknowns) has NOTHING to disclose → no line, byte-identical. The fixture-only disclosure
    // above must NOT leak into this case (it would fabricate a "0 HTTP surfaces" headline on a
    // repo that has none).
    let r = response(json!({
        "http_surfaces": {
            "total": 0, "providers": 0, "consumers": 0,
            "test_fixture_excluded": 0, "test_status_unknown": 0
        }
    }));
    assert!(r.http_surfaces_line(OrientDepth::Small).is_none());
    assert!(r.http_surfaces_line(OrientDepth::Full).is_none());
}

#[test]
fn http_headline_absent_when_field_missing() {
    // Non-HTTP repo: the daemon attached nothing → no line (byte-identical).
    let r = response(json!({}));
    assert!(r.http_surfaces_line(OrientDepth::Full).is_none());
}

#[test]
fn http_headline_unavailable_only_at_full_detail() {
    let r = response(json!({ "http_surfaces": {"unavailable": "union read failed"} }));
    // Small/medium headline stays clean; the unknown-with-reason rides the detail tiers.
    assert!(r.http_surfaces_line(OrientDepth::Small).is_none());
    assert!(r.http_surfaces_line(OrientDepth::Medium).is_none());
    let full = r.http_surfaces_line(OrientDepth::Full).unwrap();
    assert!(full.contains("unavailable"), "{full}");
    assert!(full.contains("union read failed"), "{full}");
}

#[test]
fn http_headline_partial_counts_are_malformed_not_silent() {
    // review-4 §2: an attached SUCCESS block with only some counts (`total`/`providers`
    // present, `consumers` absent) is a malformed shape. It must NOT silently drop a repo
    // that carries the field, nor fabricate a count from the partial — unknown-with-reason
    // at the detail tiers, clean at the headline tiers.
    let r = response(json!({
        "http_surfaces": {"total": 5, "providers": 2}
    }));
    assert!(r.http_surfaces_line(OrientDepth::Small).is_none());
    assert!(r.http_surfaces_line(OrientDepth::Medium).is_none());
    let full = r.http_surfaces_line(OrientDepth::Full).unwrap();
    assert!(full.contains("unavailable (malformed"), "{full}");
    // Never a fabricated headline from the present partial count.
    assert!(!full.contains("HTTP surface ("), "{full}");
}

// ── MODULE-EDGES-1 §2.3 top cross-module edges headline ────────────────────────

#[test]
fn module_edges_headline_renders_when_present() {
    // VCMI-shaped: client → lib, server → lib. The top-3 join the headline at EVERY
    // depth (a load-bearing architecture fact), in the daemon's ref-count-DESC order.
    let r = response(json!({
        "top_module_edges": {"edges": [
            {"source": "client", "target": "lib", "import_count": 14},
            {"source": "server", "target": "lib", "import_count": 9}
        ]}
    }));
    let line = r.top_module_edges_line(OrientDepth::Small).unwrap();
    assert_eq!(
        line,
        "Module edges: client \u{2192} lib (14), server \u{2192} lib (9)"
    );
}

#[test]
fn module_edges_headline_absent_when_field_missing() {
    // Repo without cross-module edges: the daemon attached nothing → no line.
    let r = response(json!({}));
    assert!(r.top_module_edges_line(OrientDepth::Full).is_none());
}

#[test]
fn module_edges_headline_unavailable_only_at_full_detail() {
    let r = response(json!({
        "top_module_edges": {"unavailable": "duplicate ownership: a.ts"}
    }));
    // Failed graph read: unknown-with-reason on the detail tiers, clean headline.
    assert!(r.top_module_edges_line(OrientDepth::Small).is_none());
    assert!(r.top_module_edges_line(OrientDepth::Medium).is_none());
    let full = r.top_module_edges_line(OrientDepth::Full).unwrap();
    assert!(full.contains("unavailable"), "{full}");
    assert!(full.contains("duplicate ownership: a.ts"), "{full}");
}

#[test]
fn module_edges_headline_malformed_row_is_unknown_not_fabricated() {
    // A row missing an endpoint is UNKNOWN — never a fabricated `→ (unknown)` edge.
    let r = response(json!({
        "top_module_edges": {"edges": [
            {"source": "client", "import_count": 14}
        ]}
    }));
    assert!(r.top_module_edges_line(OrientDepth::Small).is_none());
    let full = r.top_module_edges_line(OrientDepth::Full).unwrap();
    assert!(full.contains("unavailable (malformed edge row)"), "{full}");
    assert!(
        !full.contains("\u{2192}"),
        "no fabricated edge arrow:\n{full}"
    );
}

// ── §2.1 Directory-group topology fallback ─────────────────────────────────────

#[test]
fn directory_groups_section_renders_fan_in() {
    let r = response(json!({
        "directory_group_fallback": {
            "groups": [
                {"name": "django/db", "fan_in": 340, "fan_out": 12, "file_count": 60},
                {"name": "django/test", "fan_in": 242, "fan_out": 8, "file_count": 90}
            ],
            "total": 685
        }
    }));
    let out = r.directory_groups_section();
    assert!(
        out.contains("Directory groups (no manifest topology at this depth)"),
        "{out}"
    );
    assert!(out.contains("django/db — fan-in 340"), "{out}");
    assert!(out.contains("django/test — fan-in 242"), "{out}");
    // Honest omission line — the complete set is 685.
    assert!(out.contains("and 683 more"), "{out}");
}

#[test]
fn directory_groups_section_absent_is_empty() {
    // Non-collapsed repo → nothing injected → byte-identical (empty section).
    assert!(response(json!({})).directory_groups_section().is_empty());
}

#[test]
fn directory_groups_section_surfaces_unavailable_reason() {
    let r = response(json!({
        "directory_group_fallback": {"unavailable": "read cancelled"}
    }));
    let out = r.directory_groups_section();
    assert!(out.contains("unavailable (read cancelled)"), "{out}");
}

#[test]
fn directory_groups_section_groups_without_total_is_malformed_not_silent() {
    // review-4 §2: an attached SUCCESS block missing `total` cannot state an honest
    // omission count. Rendering the groups without it would silently hide the defect — so
    // the block is unknown-WITH-REASON, not a partial section.
    let r = response(json!({
        "directory_group_fallback": {
            "groups": [{"name": "a", "fan_in": 1, "fan_out": 0, "file_count": 1}]
        }
    }));
    let out = r.directory_groups_section();
    assert!(out.contains("unavailable (malformed"), "{out}");
    // No fabricated group row leaks through.
    assert!(!out.contains("fan-in 1"), "{out}");
}

#[test]
fn directory_groups_section_total_without_groups_is_malformed_not_silent() {
    // review-4 §2: `total` without `groups` and without `unavailable` — previously
    // rendered NOTHING (silent absence). It is an attached-but-malformed shape → reason.
    let r = response(json!({
        "directory_group_fallback": {"total": 5}
    }));
    let out = r.directory_groups_section();
    assert!(out.contains("unavailable (malformed"), "{out}");
}

#[test]
fn directory_groups_section_empty_block_is_malformed_not_silent() {
    // An attached but empty object (neither success nor failure shape) → reason, not
    // silent empty.
    let r = response(json!({ "directory_group_fallback": {} }));
    let out = r.directory_groups_section();
    assert!(out.contains("unavailable (malformed"), "{out}");
}

// ── §2.4 Saturated ladder / --full completeness ────────────────────────────────

/// Wrap a bare `Signal` value in the `CoherenceEnvelope<Signal>` leaf wire shape
/// (`value` + `provenance` / `trust` / `freshness`) the `signals` list deserializes.
fn signal_leaf(value: Value) -> Value {
    json!({
        "value": value,
        "provenance": { "source": ["sqlite"] },
        "trust": { "class": "Exact", "completeness": "Complete" },
        "freshness": "Fresh"
    })
}

/// A MODULE_SUMMARY signal whose sub-fields the test can fill — the affirmative
/// structural evidence `budget_saturated` now REQUIRES before it may claim
/// completeness. `evidence` defaults to a fully-shown, non-eliding shape.
fn with_module_summary(mut extra: Value, evidence: Value) -> Value {
    extra.as_object_mut().unwrap().insert(
        "signals".into(),
        json!([signal_leaf(json!({
            "code": "MODULE_SUMMARY", "severity": "low",
            "category": "structure", "summary": "s", "evidence": evidence
        }))]),
    );
    extra
}

#[test]
fn budget_saturated_true_when_nothing_elided() {
    // Affirmative evidence (a MODULE_SUMMARY with all groups/modules shown) + a
    // fully-shown directory fallback + a PRESENT, within-cap documentation section →
    // complete. (review-1 finding 1: the docs section must be PRESENT for a completeness
    // claim — an absent section is UNKNOWN, not "nothing to elide".)
    let r = response(with_docs(
        with_module_summary(
            json!({
                "directory_group_fallback": {
                    "groups": [{"name": "a", "fan_in": 1, "fan_out": 0, "file_count": 1}],
                    "total": 1
                }
            }),
            json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
        ),
        1,
    ));
    assert!(r.budget_saturated());
}

/// A `documentation` section carrying `n` relevant docs (each with the required
/// path/kind/reason fields) — the headline names at most DOC_HEADLINE_CAP=6.
fn with_docs(mut extra: Value, n: usize) -> Value {
    let files: Vec<Value> = (0..n)
        .map(|i| json!({"path": format!("docs/d{i}.md"), "kind": "doc", "reason": "relevant"}))
        .collect();
    extra.as_object_mut().unwrap().insert(
        "documentation".into(),
        json!({"relevant_files": files, "count": n}),
    );
    extra
}

#[test]
fn budget_saturated_false_when_docs_exceed_the_headline_cap() {
    // ECONOMY-2 §2.2 (review-0 gap c): the Docs headline caps at DOC_HEADLINE_CAP=6; a repo
    // with more relevant docs ELIDES the rest, so "output complete" must be REFUSED even when
    // every other section is whole (module summary complete, no fallback).
    let r = response(with_docs(
        with_module_summary(
            json!({}),
            json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
        ),
        7,
    ));
    assert!(!r.budget_saturated());
}

#[test]
fn budget_saturated_true_when_docs_within_the_headline_cap() {
    // Exactly DOC_HEADLINE_CAP docs → nothing elided → the completeness claim is allowed.
    let r = response(with_docs(
        with_module_summary(
            json!({}),
            json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
        ),
        6,
    ));
    assert!(r.budget_saturated());
}

#[test]
fn budget_saturated_false_when_documentation_absent() {
    // ECONOMY-2 review-1 finding 1 (STANDING HONESTY RULE #1): the daemon's
    // `build_documentation_section` collapses a `get_doc_inventory` READ FAILURE into the
    // SAME absent `documentation` (None) as a genuinely doc-less repo, so an absent section is
    // UNKNOWN — not evidence that "nothing was elided". Even with EVERY other dimension
    // provably complete (module summary whole, no fallback, no over-cap package groups or
    // complexity centers), the terminal "output complete" claim must be REFUSED while the
    // documentation axis is unknown. The prior `.unwrap_or(0)` read this None as a known zero
    // and let the claim through — the exact defect this slice removes.
    let r = response(with_module_summary(
        json!({}),
        json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
    ));
    // `documentation` is intentionally NOT injected → deserializes to None.
    assert!(r.documentation.is_none());
    assert!(!r.budget_saturated());
}

#[test]
fn docs_headline_names_the_elided_remainder() {
    // The Docs headline names the elided remainder + the exact command (STANDING HONESTY 2),
    // and the render carries no false "output complete" while docs are unshown.
    let r = response(with_docs(
        with_module_summary(
            json!({}),
            json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
        ),
        9,
    ));
    let out = r.render_human(OrientDepth::Full);
    assert!(
        out.contains("… and 3 more docs — rmap docs list"),
        "docs elision names N + command:\n{out}"
    );
    assert!(
        !out.contains("output complete"),
        "no false 'output complete' while docs elide:\n{out}"
    );
}

#[test]
fn budget_saturated_false_when_fallback_elides() {
    let r = response(with_module_summary(
        json!({
            "directory_group_fallback": {
                "groups": [{"name": "a", "fan_in": 1, "fan_out": 0, "file_count": 1}],
                "total": 42
            }
        }),
        json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
    ));
    assert!(!r.budget_saturated());
}

#[test]
fn budget_saturated_true_with_many_package_groups_under_the_cap() {
    // ECONOMY-2 (§2.3): `--full` caps the package-group section at 200 (was uncapped). A
    // repo with MORE than `large`'s 50 groups but WITHIN the cap is still COMPLETE at full
    // (nothing elided). All modules shown, no fallback → complete.
    // Rows carry all three REQUIRED contract fields (name/file_count/test_file_count) —
    // the shape both producers actually emit; a row is well-formed only with all three
    // (review-2: `test_file_count` is required, not optional).
    let groups: Vec<Value> = (0..60)
        .map(|i| json!({"name": format!("g{i}"), "file_count": 1, "test_file_count": 0}))
        .collect();
    // A PRESENT, within-cap documentation section (review-1 finding 1) so the ONLY
    // dimension under test is the package-group cap, not the docs gate.
    let r = response(with_docs(
        with_module_summary(
            json!({}),
            json!({
                "package_groups": groups,
                "discovered_module_count": 0,
                "top_modules": []
            }),
        ),
        1,
    ));
    assert!(r.budget_saturated());
}

#[test]
fn budget_saturated_false_when_package_groups_exceed_the_full_cap() {
    // ECONOMY-2 (§2.2/§2.3): a package-group count ABOVE the `--full` cap ELIDES
    // (`… and N more group`), so "output complete" must be REFUSED — the truthful-ladder
    // tie. 201 well-formed groups > the 200-row cap.
    let groups: Vec<Value> = (0..201)
        .map(|i| json!({"name": format!("g{i}"), "file_count": 1, "test_file_count": 0}))
        .collect();
    let r = response(with_module_summary(
        json!({}),
        json!({
            "package_groups": groups,
            "discovered_module_count": 0,
            "top_modules": []
        }),
    ));
    assert!(!r.budget_saturated());
    let full = r.render_human(OrientDepth::Full);
    assert!(!full.contains("budget not reached"));
    // review-1 finding 2b (§2.3): the `--full` package-group elision STATES the 200-row bound.
    // 201 groups − the 200 cap = 1 omitted; the line names the count AND where the cap fell.
    assert!(
        full.contains("… and 1 more group (showing 200) — see `stats --json` / `modules`"),
        "the --full package-group elision must STATE the 200-row bound:\n{full}"
    );
}

#[test]
fn budget_saturated_false_when_complexity_centers_exceed_the_full_cap() {
    // ECONOMY-2 (§2.2/§2.3): a complexity count ABOVE the `--full` cap ELIDES
    // (`+N more above threshold`), so "output complete" must be REFUSED.
    let top: Vec<Value> = (0..201)
        .map(|i| json!({"complexity": 9, "file": format!("f{i}.rs")}))
        .collect();
    let mut extra = json!({});
    extra.as_object_mut().unwrap().insert(
        "signals".into(),
        json!([
            signal_leaf(json!({
                "code": "MODULE_SUMMARY", "severity": "low", "category": "structure",
                "summary": "s",
                "evidence": {"package_groups": [], "discovered_module_count": 0, "top_modules": []}
            })),
            signal_leaf(json!({
                "code": "HIGH_COMPLEXITY", "severity": "medium", "category": "quality",
                "summary": "c",
                "evidence": {"high_complexity_count": 201, "top_complex": top}
            }))
        ]),
    );
    let r = response(extra);
    assert!(!r.budget_saturated());
    let full = r.render_human(OrientDepth::Full);
    assert!(!full.contains("budget not reached"));
    // review-1 finding 2b (§2.3): the `--full` complexity elision STATES the 200-row bound.
    // 201 centers − the 200 cap = 1 omitted.
    assert!(
        full.contains("+1 more above threshold (showing 200) — rmap hotspots"),
        "the --full complexity elision must STATE the 200-row bound:\n{full}"
    );
}

#[test]
fn budget_saturated_false_without_structural_evidence() {
    // review-1 §2: a signal-less response must NOT read as "output complete" — an
    // absent MODULE_SUMMARY is UNKNOWN, never a fabricated positive.
    assert!(!response(json!({})).budget_saturated());
}

#[test]
fn budget_saturated_false_when_fallback_read_failed() {
    // review-1 §2: a FAILED directory-group read (`unavailable`) is an unknown
    // section, so completeness cannot be claimed even with structural evidence.
    let r = response(with_module_summary(
        json!({ "directory_group_fallback": {"unavailable": "read cancelled"} }),
        json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
    ));
    assert!(!r.budget_saturated());
}

#[test]
fn budget_saturated_false_when_complexity_count_unknown() {
    // review-1 §2: a PRESENT HIGH_COMPLEXITY signal whose true count cannot be read
    // cannot affirm completeness — refuse rather than assume complete.
    // The MODULE_SUMMARY carries affirmative, well-formed module evidence so the module
    // gate passes and control reaches the complexity check — the HIGH_COMPLEXITY signal
    // then lacks a readable `high_complexity_count`, which is what must refuse the claim.
    let mut extra = json!({});
    extra.as_object_mut().unwrap().insert(
        "signals".into(),
        json!([
            signal_leaf(json!({ "code": "MODULE_SUMMARY", "severity": "low", "category": "structure",
                         "summary": "s", "evidence": {
                             "package_groups": [], "discovered_module_count": 0, "top_modules": []
                         } })),
            signal_leaf(json!({ "code": "HIGH_COMPLEXITY", "severity": "medium", "category": "quality",
                         "summary": "c", "evidence": {"top_complex": [{"file": "a", "complexity": 9}]} }))
        ]),
    );
    assert!(!response(extra).budget_saturated());
}

// ── §2.4 review-3 §1: malformed MODULE_SUMMARY must never read as "output complete" ──
//
// Each test DESERIALIZES a specific malformed structural shape and asserts `--full` does
// NOT claim completeness. `budget_saturated` may claim completeness ONLY from affirmative,
// well-formed positive evidence; every one of these shapes is UNKNOWN, and unknown is never
// "complete" (standing honesty rule #1). The `--full` render is checked end-to-end so the
// guard is proven at the point the reader would see the line.

#[test]
fn budget_saturated_false_when_module_count_missing() {
    // Missing `discovered_module_count` (module discovery unavailable): the true module
    // total is UNKNOWN, so the shown list cannot be proven to cover it.
    let r = response(with_module_summary(
        json!({}),
        json!({"package_groups": [], "top_modules": []}),
    ));
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

#[test]
fn budget_saturated_false_when_top_modules_missing_even_at_zero_total() {
    // Missing `top_modules` with `discovered_module_count == 0`: review-3 removes the
    // zero-total exception — an absent module list is a malformed block, not a vacuously
    // complete zero.
    let r = response(with_module_summary(
        json!({}),
        json!({"package_groups": [], "discovered_module_count": 0}),
    ));
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

#[test]
fn budget_saturated_false_when_package_groups_missing() {
    // Missing `package_groups`: the structural section's completeness input is absent →
    // UNKNOWN → the section cannot be affirmed whole.
    let r = response(with_module_summary(
        json!({}),
        json!({"discovered_module_count": 0, "top_modules": []}),
    ));
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

#[test]
fn budget_saturated_false_when_fallback_has_groups_but_no_total() {
    // A collapse fallback with `groups` but no `total`: the COMPLETE directory-group count
    // is UNKNOWN, so the shown groups cannot be proven to cover it (review-3 §1).
    let r = response(with_module_summary(
        json!({
            "directory_group_fallback": {
                "groups": [{"name": "a", "fan_in": 1, "fan_out": 0, "file_count": 1}]
            }
        }),
        json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
    ));
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

// ── §2.4 review-4 §1: a malformed RENDERED ROW must never read as "output complete" ──
//
// A list whose LENGTH covers the total can still hold a row the renderer would degrade to a
// false `0` / `(unknown)` via its `unwrap_or` defaults. Completeness must see well-formed
// ROWS, not merely a covering count. Each test drives `render_human(Full)` end-to-end so the
// guard holds at the point the reader would see the line.

#[test]
fn budget_saturated_false_when_a_module_row_is_malformed() {
    // `discovered_module_count == 1` and a length-1 `top_modules`, but the row lacks `path`
    // (the renderer would mint `(unknown)`). Covering length is NOT enough — refuse.
    let r = response(with_module_summary(
        json!({}),
        json!({
            "package_groups": [],
            "discovered_module_count": 1,
            "top_modules": [{"file_count": 3}]
        }),
    ));
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

#[test]
fn budget_saturated_false_when_a_package_group_row_is_malformed() {
    // A present `package_groups` whose row lacks `name` (the renderer would mint
    // `(unknown) — 0 files`). Presence is NOT enough — refuse.
    let r = response(with_module_summary(
        json!({}),
        json!({
            "package_groups": [{"file_count": 2}],
            "discovered_module_count": 0,
            "top_modules": []
        }),
    ));
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

#[test]
fn budget_saturated_false_when_a_package_group_row_lacks_test_file_count() {
    // review-2: `test_file_count` is a REQUIRED contract field (both producers emit it
    // unconditionally). A row with a valid `name`+`file_count` but NO `test_file_count` is
    // schema drift the renderer now shows as `(test count unavailable)` — a degraded row, so
    // completeness must be REFUSED (never "complete" over an unknown; standing honesty #1).
    let r = response(with_module_summary(
        json!({}),
        json!({
            "package_groups": [{"name": "core", "file_count": 2}],
            "discovered_module_count": 0,
            "top_modules": []
        }),
    ));
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

#[test]
fn budget_saturated_false_when_a_complexity_row_is_malformed() {
    // MODULE_SUMMARY is affirmatively complete, so control reaches the complexity gate.
    // The HIGH_COMPLEXITY row has a count of 1 and a length-1 `top_complex`, but the row
    // carries NEITHER `file` NOR `symbol` (the renderer would silently SKIP it) — refuse.
    let mut extra = json!({});
    extra.as_object_mut().unwrap().insert(
        "signals".into(),
        json!([
            signal_leaf(
                json!({ "code": "MODULE_SUMMARY", "severity": "low", "category": "structure",
                         "summary": "s", "evidence": {
                             "package_groups": [], "discovered_module_count": 0, "top_modules": []
                         } })
            ),
            signal_leaf(
                json!({ "code": "HIGH_COMPLEXITY", "severity": "medium", "category": "quality",
                         "summary": "c", "evidence": {
                             "high_complexity_count": 1,
                             "top_complex": [{"complexity": 9}]
                         } })
            )
        ]),
    );
    let r = response(extra);
    assert!(!r.budget_saturated());
    assert!(!r
        .render_human(OrientDepth::Full)
        .contains("budget not reached"));
}

#[test]
fn saturated_line_renders_only_at_full_when_complete() {
    // review-2 finding 1: `[budget not reached — output complete]` renders ONLY when the repo
    // is saturated AND `--full` expanded a long tail BEYOND `--budget large` (full != large) —
    // otherwise the identical-to-large notice takes precedence (proven in the sibling test).
    // 60 package groups make this a GENUINE output-complete case: `large` caps at 50 (elides
    // 10), `--full` shows all 60 within the 200 cap (nothing elided) → saturated AND full !=
    // large. A PRESENT, within-cap docs section (review-1 finding 1) keeps the documentation
    // axis KNOWN-complete (absent is UNKNOWN and would correctly refuse the terminal line).
    let groups: Vec<Value> = (0..60)
        .map(|i| json!({"name": format!("g{i}"), "file_count": 1, "test_file_count": 0}))
        .collect();
    let r = response(with_docs(
        with_module_summary(
            json!({}),
            json!({"package_groups": groups, "discovered_module_count": 0, "top_modules": []}),
        ),
        1,
    ));
    assert!(
        r.budget_saturated(),
        "60 groups < 200 cap, all else complete → saturated"
    );
    let full = r.render_human(OrientDepth::Full);
    // Full + affirmatively-complete + expanded beyond large → the honest terminal line.
    assert!(
        full.contains("budget not reached — output complete"),
        "saturated + full-beyond-large → the completeness line:\n{full}"
    );
    // NOT the identical notice — `large` elided 10 groups that `--full` shows, so full != large.
    assert!(
        !full.contains("identical to --budget large"),
        "full != large here, so the identical notice must NOT render:\n{full}"
    );
    // Large never prints it (still under the detailed tables).
    assert!(!r
        .render_human(OrientDepth::Large)
        .contains("budget not reached"));
    // Small/medium print the --full pointer instead.
    assert!(r
        .render_human(OrientDepth::Small)
        .contains("--full for the complete breakdown"));
}

#[test]
fn saturated_and_identical_to_large_prefers_identical_notice() {
    // review-2 finding 1: when `--full` is BYTE-IDENTICAL to `--budget large` AND the repo is
    // otherwise complete (budget_saturated), the identical-to-large notice takes PRECEDENCE
    // over `[budget not reached — output complete]` — the spec's "`--full` identical to `large`
    // → one-line notice" is UNCONDITIONAL. A small complete repo (0 package groups, no
    // complexity long tail) renders the same bytes at `large` and `--full`, so full == large;
    // docs present within cap keeps the documentation axis known-complete. (The prior code
    // checked `budget_saturated()` first and printed "output complete" here — the defect.)
    let r = response(with_docs(
        with_module_summary(
            json!({}),
            json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
        ),
        1,
    ));
    assert!(
        r.budget_saturated(),
        "the repo is complete (nothing elided anywhere)"
    );
    let full = r.render_human(OrientDepth::Full);
    assert!(
        full.contains("[--full identical to --budget large (nothing further to show)]"),
        "identical notice takes precedence over the completeness line:\n{full}"
    );
    assert!(
        !full.contains("budget not reached — output complete"),
        "the identical notice REPLACES the completeness line when full == large:\n{full}"
    );
}

#[test]
fn full_identical_to_large_renders_the_notice_not_a_false_complete() {
    // ECONOMY-2 (§2.2): when `--full`'s body is BYTE-IDENTICAL to `--budget large`'s (both
    // long tails under `large`'s cap) but something ELSE elided (here the directory-group
    // fallback: 1 shown of 42) — the zvec-grep defect was a silent unmarked repeat of
    // `large`. `--full` now says so explicitly, and does NOT claim completeness.
    let r = response(with_module_summary(
        json!({
            "directory_group_fallback": {
                "groups": [{"name": "a", "fan_in": 1, "fan_out": 0, "file_count": 1}],
                "total": 42
            }
        }),
        json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
    ));
    assert!(!r.budget_saturated(), "the fallback elides → not saturated");
    let full = r.render_human(OrientDepth::Full);
    assert!(
        full.contains("[--full identical to --budget large (nothing further to show)]"),
        "the comparative notice replaces the silent repeat: {full}"
    );
    assert!(
        !full.contains("budget not reached"),
        "the identical notice is NOT a completeness claim: {full}"
    );
    // The elided directory fallback still carries its own honest omission line.
    assert!(
        full.contains("and 41 more group"),
        "elision line present: {full}"
    );
}

#[test]
fn saturated_line_absent_at_full_without_evidence() {
    // review-1 §2: a signal-less response at `--full` must NOT print the completeness
    // line — no evidence, no claim.
    let out = response(json!({})).render_human(OrientDepth::Full);
    assert!(!out.contains("budget not reached"), "{out}");
}

#[test]
fn dir_group_row_missing_numeric_is_not_false_zero() {
    // review-1 §2: a directory row missing `fan_in` must NOT deserialize to `0` (a
    // rendered false-zero). The malformed block fails to parse → the field reads as
    // absent (None), so no fabricated "fan-in 0" ever renders.
    let parsed: Result<OrientResponse, _> = serde_json::from_value(json!({
        "repo": "r", "snapshot": "s", "confidence": "high",
        "focus": {"resolved": true, "resolved_kind": "repo"},
        "directory_group_fallback": {
            "groups": [{"name": "a", "fan_out": 0, "file_count": 1}],
            "total": 1
        }
    }));
    assert!(
        parsed.is_err(),
        "a row missing fan_in must fail parse, not mint a false zero"
    );
}

// ── §2.2 Module identity: name [manifest] on collision / divergence ────────────

use crate::presentation::module_disambiguation::ModuleRow;

/// Build `top_modules`-shaped rows: `(path, name, manifest)`.
fn module_rows<'a>(rows: &'a [(&'a str, Option<&'a str>, Option<&'a str>)]) -> Vec<ModuleRow<'a>> {
    rows.iter()
        .map(|&(path, name, manifest)| ModuleRow {
            path,
            name,
            manifest,
        })
        .collect()
}

fn label(rows: &[ModuleRow<'_>], idx: usize) -> String {
    let names: Vec<&str> = rows.iter().map(|r| r.effective_name()).collect();
    OrientResponse::module_row_label(rows, &names, idx)
}

#[test]
fn module_label_shows_manifest_on_name_path_divergence() {
    // amodx: canonical path `packages/plugins`, declared name `@amodx/plugins`.
    let rows = module_rows(&[(
        "packages/plugins",
        Some("@amodx/plugins"),
        Some("package.json"),
    )]);
    assert_eq!(label(&rows, 0), "@amodx/plugins [package.json]");
}

#[test]
fn module_label_disambiguates_name_collision_by_manifest() {
    // Two modules both named `Django`, one pyproject.toml, one package.json.
    let rows = module_rows(&[
        ("django", Some("Django"), Some("pyproject.toml")),
        ("django-js", Some("Django"), Some("package.json")),
    ]);
    assert_eq!(label(&rows, 0), "Django [pyproject.toml]");
    assert_eq!(label(&rows, 1), "Django [package.json]");
}

#[test]
fn module_label_same_manifest_collision_falls_back_to_path() {
    // django's REAL data: TWO `Django` modules BOTH declared via pyproject.toml. The
    // manifest cannot disambiguate them, so the unique canonical path is the honest
    // tie-break (never two label-identical `Django [pyproject.toml]` rows).
    let rows = module_rows(&[
        (".", Some("Django"), Some("pyproject.toml")),
        ("django/other", Some("Django"), Some("pyproject.toml")),
    ]);
    assert_eq!(label(&rows, 0), "Django [.]");
    assert_eq!(label(&rows, 1), "Django [django/other]");
}

#[test]
fn module_label_path_unchanged_when_no_divergence_or_collision() {
    // Inferred C++ dir (leveldb-shape): no manifest, name equals path → BYTE-IDENTICAL
    // path rendering. Also the no-identity-at-all case.
    let rows = module_rows(&[("db", Some("db"), None), ("table", None, None)]);
    assert_eq!(label(&rows, 0), "db");
    assert_eq!(label(&rows, 1), "table");
}

// ── §2.6 Docs headline cap ─────────────────────────────────────────────────────

#[test]
fn docs_line_caps_at_headline_limit() {
    let files: Vec<Value> = (0..9)
        .map(|i| json!({"path": format!("doc{i}.md"), "kind": "readme", "reason": "x"}))
        .collect();
    let r = response(json!({
        "documentation": {"relevant_files": files, "count": 9}
    }));
    let out = r.render_human(OrientDepth::Small);
    // The Docs headline names the top DOC_HEADLINE_CAP (6), not all nine.
    assert!(out.contains("doc0"), "{out}");
    assert!(out.contains("doc5"), "{out}");
    assert!(!out.contains("doc6"), "cap must drop the 7th doc:\n{out}");
}
