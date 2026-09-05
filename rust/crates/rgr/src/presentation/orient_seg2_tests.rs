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
    // fully-shown directory fallback → complete.
    let r = response(with_module_summary(
        json!({
            "directory_group_fallback": {
                "groups": [{"name": "a", "fan_in": 1, "fan_out": 0, "file_count": 1}],
                "total": 1
            }
        }),
        json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
    ));
    assert!(r.budget_saturated());
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
fn budget_saturated_true_with_many_package_groups() {
    // §2.4 (operator ruling 2): `--full` uncaps the package-group section, so a repo
    // with >50 package groups is COMPLETE at full (the old >50 gate is gone). All
    // modules shown, no fallback → complete.
    // Rows carry all three REQUIRED contract fields (name/file_count/test_file_count) —
    // the shape both producers actually emit; a row is well-formed only with all three
    // (review-2: `test_file_count` is required, not optional).
    let groups: Vec<Value> = (0..60)
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
    assert!(r.budget_saturated());
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
    let r = response(with_module_summary(
        json!({}),
        json!({"package_groups": [], "discovered_module_count": 0, "top_modules": []}),
    ));
    // Full + affirmatively-complete → the honest terminal line.
    assert!(r
        .render_human(OrientDepth::Full)
        .contains("budget not reached — output complete"));
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
