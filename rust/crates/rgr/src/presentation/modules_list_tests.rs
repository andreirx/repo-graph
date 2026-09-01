//! Render + wire-parse tests for `modules list` presentation.
//!
//! Split out of `modules_list.rs` (via `#[path]`, the `orient_tests.rs` idiom) so the
//! renderer file stays under the >500-line structural guardrail (review-0 item 2).
//! Still a child module of `modules_list`, so `use super::*` reaches the response
//! structs, `HUMAN_ROW_BUDGET`, and the render entry points exactly as before — the
//! split is pure relocation.
//!
//! Abstraction record (crate-private test module, pre-ratified guardrail carve-out):
//!   - what: the `modules list` presenter's test body, relocated to a sibling file.
//!   - concrete current users: `modules_list::tests` (this crate's `cargo test`).
//!   - axis of variation: `modules_list.rs` grows past 500 lines when the renderer AND
//!     its tests share one file; the guardrail forbids appending to it.
//!   - rejected simpler: leaving the tests inline (keeps the file at ~979 lines, the
//!     exact guardrail violation review-0 flagged).

use super::*;

fn sample_list_response() -> ModulesListResponse {
    ModulesListResponse {
        command: "modules list".to_string(),
        repo: "repo_123".to_string(),
        snapshot: "snap_456".to_string(),
        results: vec![
            ModuleListEntry {
                module_uid: "mod-1".to_string(),
                module_key: "inferred:repo_123:src".to_string(),
                canonical_root_path: "src".to_string(),
                module_kind: "inferred".to_string(),
                display_name: "src".to_string(),
                manifest: None,
                confidence: 0.7,
                owned_file_count: 100,
                owned_test_file_count: 10,
                unref_reduction: None,
                outbound_dependency_count: 0,
                outbound_import_count: 50,
                inbound_dependency_count: 0,
                inbound_import_count: 20,
                violation_count: 0,
                dead_symbol_count: 25,
                dead_test_symbol_count: 5,
            },
            ModuleListEntry {
                module_uid: "mod-2".to_string(),
                module_key: "inferred:repo_123:lib".to_string(),
                canonical_root_path: "lib".to_string(),
                module_kind: "manifest".to_string(),
                display_name: "lib".to_string(),
                manifest: None,
                confidence: 1.0,
                owned_file_count: 20,
                owned_test_file_count: 0,
                unref_reduction: None,
                outbound_dependency_count: 1,
                outbound_import_count: 5,
                inbound_dependency_count: 1,
                inbound_import_count: 10,
                violation_count: 2,
                dead_symbol_count: 3,
                dead_test_symbol_count: 0,
            },
        ],
        http_boundary_link_count: Some(0),
        http_boundary_link_degraded: None,
        // Two cross-module edges (authoritative single-read fact) — plural count,
        // and exercises the reference-count-DESC sort (lib←app 10 before src 5).
        edges: Some(vec![
            ModuleEdgeEntry {
                source: "src".to_string(),
                target: "lib".to_string(),
                import_count: 5,
            },
            ModuleEdgeEntry {
                source: "app".to_string(),
                target: "lib".to_string(),
                import_count: 10,
            },
        ]),
    }
}

fn sample_empty_list_response() -> ModulesListResponse {
    ModulesListResponse {
        command: "modules list".to_string(),
        repo: "repo_123".to_string(),
        snapshot: "snap_456".to_string(),
        results: vec![],
        http_boundary_link_count: Some(0),
        http_boundary_link_degraded: None,
        edges: Some(vec![]),
    }
}

/// review-0 item 2: two modules, zero cross-module IMPORTS, but HTTP links
/// exist → the "boundaries may not be meaningful" hint MUST be suppressed and
/// replaced with an honest pointer to `rmap boundaries links`.
#[test]
fn list_render_http_links_suppress_meaningless_hint() {
    let mut resp = sample_list_response();
    // Zero the import-derived cross-module deps on both rows.
    resp.results[0].outbound_dependency_count = 0;
    resp.results[0].inbound_dependency_count = 0;
    resp.results[1].outbound_dependency_count = 0;
    resp.results[1].inbound_dependency_count = 0;
    resp.edges = Some(vec![]); // authoritative KNOWN-zero cross-module deps
    resp.http_boundary_link_count = Some(3);
    let output = resp.render_human();
    assert!(
        !output.contains("Module boundaries may not be meaningful"),
        "misleading hint must be gone when HTTP links exist:\n{output}"
    );
    // Layer-3 honesty (review-1): a heuristic route-match discovery, NOT a
    // runtime-proven connection. The wording must not overstate.
    assert!(
        output.contains("likely connected via HTTP route match")
            && output.contains("heuristic, 3 links")
            && output.contains("not runtime-proven")
            && output.contains("rmap boundaries links"),
        "honest Layer-3 pointer present:\n{output}"
    );
    assert!(
        !output.contains("at runtime") && !output.contains("ARE meaningful"),
        "must not claim runtime connection or assert meaningfulness:\n{output}"
    );
}

/// The misleading hint STILL fires when there are no HTTP links (no regression).
#[test]
fn list_render_no_http_links_keeps_meaningless_hint() {
    let mut resp = sample_list_response();
    resp.results[0].outbound_dependency_count = 0;
    resp.results[0].inbound_dependency_count = 0;
    resp.results[1].outbound_dependency_count = 0;
    resp.results[1].inbound_dependency_count = 0;
    resp.edges = Some(vec![]); // authoritative KNOWN-zero cross-module deps
    resp.http_boundary_link_count = Some(0);
    let output = resp.render_human();
    assert!(
        output.contains("Module boundaries may not be meaningful"),
        "hint fires when there are genuinely no boundaries:\n{output}"
    );
}

/// review-4 item 2: a FAILED HTTP-link read is UNKNOWN — the "boundaries may
/// not be meaningful" claim must be SUPPRESSED (a read error must not render
/// as a zero fact), replaced with an honest unknown/degraded note.
#[test]
fn list_render_http_link_read_degraded_suppresses_meaningless_hint() {
    let mut resp = sample_list_response();
    resp.results[0].outbound_dependency_count = 0;
    resp.results[0].inbound_dependency_count = 0;
    resp.results[1].outbound_dependency_count = 0;
    resp.results[1].inbound_dependency_count = 0;
    resp.edges = Some(vec![]); // authoritative KNOWN-zero cross-module deps
    resp.http_boundary_link_count = None;
    resp.http_boundary_link_degraded =
        Some("HTTP boundary link count read failed (degraded): db locked".to_string());
    let output = resp.render_human();
    assert!(
        !output.contains("Module boundaries may not be meaningful"),
        "a degraded read must NOT restore the meaningless claim:\n{output}"
    );
    assert!(
        output.contains("UNKNOWN") && output.contains("degraded"),
        "degraded read shown honestly as unknown:\n{output}"
    );
}

#[test]
fn list_render_shows_header() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.starts_with("Modules\n"));
}

/// Review-1 item 3: the NONZERO g2u aggregate through final human rendering — the
/// reconciled footnote sums the per-module reductions beside the untouched `unref?`
/// column figures.
#[test]
fn list_render_nonzero_reduction_renders_the_reconciled_footnote() {
    let mut resp = sample_list_response();
    resp.results[0].unref_reduction = Some(serde_json::json!({
        "accounting": "union",
        "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
        "fewer_flagged": 2,
        "basis": "compiler-verified references found",
    }));
    let output = resp.render_human();
    // Review-2 item 1: the coverage basis renders beside the reconciled aggregate.
    assert!(
        output.contains(
            "reconciled: 2 fewer flagged across 1 module — compiler-verified \
             references found — combined analyses (coverage: TypeScript (1 partition))."
        ),
        "{output}"
    );
    assert!(
        output.contains("25 unref?"),
        "the pipeline column stays untouched: {output}"
    );
}

/// Review-2 item 1 (negative): a row whose reduction block fails the §5.3.0 labeling
/// gate — missing `accounting: "union"`, or missing/malformed coverage — contributes
/// NOTHING: with no passing row the footnote is entirely absent, never an unlabeled
/// reconciled figure.
#[test]
fn list_render_unlabeled_reduction_rows_render_no_footnote() {
    // Accounting marker absent (coverage well-formed).
    let mut resp = sample_list_response();
    resp.results[0].unref_reduction = Some(serde_json::json!({
        "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
        "fewer_flagged": 2,
    }));
    let output = resp.render_human();
    assert!(!output.contains("reconciled"), "{output}");
    assert!(!output.contains("fewer flagged"), "{output}");

    // Accounting present, coverage malformed (empty languages).
    resp.results[0].unref_reduction = Some(serde_json::json!({
        "accounting": "union",
        "coverage": {"languages": [], "partitions": ["p"], "fingerprint": "fp"},
        "fewer_flagged": 2,
    }));
    let output = resp.render_human();
    assert!(!output.contains("reconciled"), "{output}");
    assert!(!output.contains("fewer flagged"), "{output}");

    // Mixed: one labeled row + one unlabeled row → the gate is PER ROW: only the
    // labeled row's reduction aggregates; the unlabeled row's value is suppressed.
    resp.results[0].unref_reduction = Some(serde_json::json!({
        "accounting": "union",
        "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
        "fewer_flagged": 2,
    }));
    resp.results[1].unref_reduction = Some(serde_json::json!({"fewer_flagged": 5}));
    let output = resp.render_human();
    assert!(
        output.contains(
            "reconciled: 2 fewer flagged across 1 module — compiler-verified \
             references found — combined analyses (coverage: TypeScript (1 partition))."
        ),
        "{output}"
    );
    assert!(!output.contains("7 fewer"), "{output}");
}

#[test]
fn list_render_shows_count() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("2 modules"));
}

#[test]
fn list_render_shows_module_names() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("src"));
    assert!(output.contains("lib"));
}

#[test]
fn list_render_shows_kind_confidence() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("inferred (0.7)"));
    assert!(output.contains("manifest")); // 1.0 hides decimal
}

#[test]
fn list_render_shows_violations() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("2 violations"));
}

#[test]
fn list_render_shows_cross_module_deps() {
    let resp = sample_list_response();
    let output = resp.render_human();
    assert!(output.contains("cross-module dependencies detected"));
}

// ── MODULE-EDGES-1 §2.1: the edge list rendered from the SAME read as its count ──

/// The acid test: the count line and the rendered edge rows come from ONE array
/// (`edges`), so the count EQUALS the number of listed rows — a disagreement is
/// impossible by construction.
#[test]
fn list_render_edge_list_count_equals_rows() {
    let resp = sample_list_response();
    let output = resp.render_human();
    // Count line reflects edges.len() (2), not the old max/2 heuristic.
    assert!(
        output.contains("2 cross-module dependencies detected."),
        "count == edges.len():\n{output}"
    );
    // Both edges are listed, each with its file-level import count.
    assert!(
        output.contains("app \u{2192} lib (10 file-level imports)"),
        "{output}"
    );
    assert!(
        output.contains("src \u{2192} lib (5 file-level imports)"),
        "{output}"
    );
    // The count in the header equals the number of rendered edge rows.
    let rows = output.matches(" \u{2192} ").count();
    assert_eq!(rows, 2, "listed rows match the counted total:\n{output}");
}

/// Sort is deterministic: reference count DESC, then (source, target) ASC. The
/// heavier `app → lib (10)` precedes `src → lib (5)` regardless of input order.
#[test]
fn list_render_edges_sorted_by_refcount_then_name() {
    let resp = sample_list_response();
    let output = resp.render_human();
    let app_pos = output.find("app \u{2192} lib").unwrap();
    let src_pos = output.find("src \u{2192} lib").unwrap();
    assert!(app_pos < src_pos, "heavier edge first:\n{output}");
}

/// Singular grammar when exactly one edge, and the singular import noun.
#[test]
fn list_render_single_edge_is_singular() {
    let mut resp = sample_list_response();
    resp.edges = Some(vec![ModuleEdgeEntry {
        source: "client".to_string(),
        target: "lib".to_string(),
        import_count: 1,
    }]);
    let output = resp.render_human();
    assert!(
        output.contains("1 cross-module dependency detected."),
        "singular count noun:\n{output}"
    );
    assert!(
        output.contains("client \u{2192} lib (1 file-level import)"),
        "singular import noun:\n{output}"
    );
}

/// Budget: the default render caps the edge list at `HUMAN_ROW_BUDGET` with a
/// truthful "(+N more — --full)" remainder; `--full` uncaps and drops the remainder.
#[test]
fn list_render_edges_budget_and_full() {
    let mut resp = sample_list_response();
    // HUMAN_ROW_BUDGET + 5 distinct edges, descending import counts so sort is stable.
    let n = HUMAN_ROW_BUDGET + 5;
    resp.edges = Some(
        (0..n)
            .map(|i| ModuleEdgeEntry {
                source: format!("m{i:03}"),
                target: "lib".to_string(),
                import_count: (n - i) as u64,
            })
            .collect(),
    );

    let capped = resp.render_human_budgeted(false);
    assert!(
        capped.contains(&format!("{n} cross-module dependencies detected.")),
        "count is the TRUE total even when the list is capped:\n{capped}"
    );
    let shown = capped.matches(" \u{2192} lib (").count();
    assert_eq!(
        shown, HUMAN_ROW_BUDGET,
        "default caps at the row budget:\n{capped}"
    );
    assert!(
        capped.contains(&format!("(+{} more — --full)", n - HUMAN_ROW_BUDGET)),
        "honest remainder line:\n{capped}"
    );

    let full = resp.render_human_budgeted(true);
    let shown_full = full.matches(" \u{2192} lib (").count();
    assert_eq!(shown_full, n, "--full uncaps every edge:\n{full}");
    assert!(
        !full.contains("more — --full"),
        "no remainder at --full:\n{full}"
    );
}

/// review-0 item 1: the pre-slice public signature `render_human(&self)` renders at
/// the DEFAULT budget — identical to `render_human_budgeted(false)`. Pins that the
/// public wrapper did not change behaviour when the `--full` lever moved off it.
#[test]
fn list_render_human_default_equals_budgeted_false() {
    let mut resp = sample_list_response();
    let n = HUMAN_ROW_BUDGET + 5;
    resp.edges = Some(
        (0..n)
            .map(|i| ModuleEdgeEntry {
                source: format!("m{i:03}"),
                target: "lib".to_string(),
                import_count: (n - i) as u64,
            })
            .collect(),
    );
    assert_eq!(resp.render_human(), resp.render_human_budgeted(false));
}

/// KNOWN-zero edges (authoritative empty array) renders the honest zero-state —
/// NOT a fabricated count — and keeps the existing HTTP-boundary note path.
#[test]
fn list_render_empty_edges_is_zero_state() {
    let mut resp = sample_list_response();
    resp.edges = Some(vec![]);
    let output = resp.render_human();
    assert!(
        output.contains("No cross-module dependencies detected."),
        "empty edges → honest zero-state:\n{output}"
    );
    assert!(
        !output.contains(" \u{2192} "),
        "no edge rows on zero-state:\n{output}"
    );
}

/// review-0 item 4 (honesty): an OLDER daemon that sent NO `edges` key (field absent
/// → `None` = UNKNOWN) must NOT render a false zero AND must NOT present the pre-slice
/// rollup count as an authoritative edge-list count. The edge list is labelled
/// unavailable-with-reason; the rough rollup figure, when nonzero, is labelled a rough
/// estimate distinct from the edge-list count.
#[test]
fn list_render_absent_edges_field_is_unavailable_with_reason() {
    let mut resp = sample_list_response();
    resp.edges = None; // wire had no `edges` key (older daemon)
    let output = resp.render_human();
    // The edge list is UNAVAILABLE, and the reason (older daemon) is named.
    assert!(
        output.contains("Cross-module edge list unavailable")
            && output.contains("did not provide it"),
        "absent field labels the edge list unavailable-with-reason:\n{output}"
    );
    // The rough rollup figure (row[1]: outbound 1 / inbound 1 → ~0) is LABELLED a
    // rough estimate, never re-cast as the authoritative edge-list count.
    assert!(
        output.contains("rough rollup estimate, not the authoritative edge-list count"),
        "rough figure is labelled, not authoritative:\n{output}"
    );
    // A false zero is forbidden, and there is no authoritative list to render.
    assert!(
        !output.contains("No cross-module dependencies detected."),
        "absent field must NOT render a false zero:\n{output}"
    );
    assert!(
        !output.contains(" \u{2192} "),
        "no edge rows without a list:\n{output}"
    );
}

/// review-0 item 4, zero-rollup arm: an older daemon whose rollups genuinely report
/// no cross-module deps — the edge list is still unavailable-with-reason, and the
/// rollup zero is stated as a rollup fact (never as the edge-list count).
#[test]
fn list_render_absent_edges_field_zero_rollups_states_rollup_zero() {
    let mut resp = sample_list_response();
    resp.edges = None;
    resp.results[1].outbound_dependency_count = 0;
    resp.results[1].inbound_dependency_count = 0;
    let output = resp.render_human();
    assert!(
        output.contains("Cross-module edge list unavailable"),
        "edge list still unavailable-with-reason:\n{output}"
    );
    assert!(
        output.contains("Module rollups report no cross-module dependencies."),
        "rollup zero stated as a rollup fact:\n{output}"
    );
    assert!(
        !output.contains("No cross-module dependencies detected."),
        "no edge-list zero claim from an absent list:\n{output}"
    );
}

// ── review-0 item 3: no fabricated edge values — required scalars fail the parse ──

/// A well-formed edges array deserializes fine (the same-version daemon always emits
/// all three scalars for every edge).
#[test]
fn edge_wellformed_json_parses() {
    let json = serde_json::json!({
        "results": [],
        "edges": [{"source": "client", "target": "lib", "import_count": 14}],
    });
    let resp: ModulesListResponse = serde_json::from_value(json).expect("well-formed edges parse");
    let edges = resp.edges.expect("edges present");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].source, "client");
    assert_eq!(edges[0].import_count, 14);
}

/// A malformed edge missing an endpoint FAILS the parse with the concrete serde
/// reason (naming the field) — never a fabricated blank endpoint (honesty rule #1).
#[test]
fn edge_missing_source_fails_parse_with_reason() {
    let json = serde_json::json!({
        "results": [],
        "edges": [{"target": "lib", "import_count": 14}],
    });
    let err = serde_json::from_value::<ModulesListResponse>(json)
        .expect_err("missing `source` must fail the parse, not default to empty");
    assert!(
        err.to_string().contains("source"),
        "the parse error names the missing field: {err}"
    );
}

/// A malformed edge missing the count FAILS the parse — never a fabricated `0`
/// file-level imports.
#[test]
fn edge_missing_import_count_fails_parse_with_reason() {
    let json = serde_json::json!({
        "results": [],
        "edges": [{"source": "client", "target": "lib"}],
    });
    let err = serde_json::from_value::<ModulesListResponse>(json)
        .expect_err("missing `import_count` must fail the parse, not default to 0");
    assert!(
        err.to_string().contains("import_count"),
        "the parse error names the missing field: {err}"
    );
}

#[test]
fn list_render_empty_shows_hint() {
    let resp = sample_empty_list_response();
    let output = resp.render_human();
    assert!(output.contains("No modules detected"));
    assert!(output.contains("hint:"));
}

#[test]
fn list_render_relabels_dead_as_unref_with_caveat() {
    // OUTPUT-DOC-TRUTH-AUDIT-1: dead_symbol_count is a low-reliability Layer-2
    // graph-orphan inference, not a Layer-0 fact. The column must read `unref?`
    // (never the overclaiming bare `dead`) and carry a caveat that points at the
    // reliability surface and never claims the count is safe-to-delete.
    let resp = sample_list_response();
    let output = resp.render_human();

    // Honest relabel present on the rows (25 -> "25 unref?", 3 -> "3 unref?").
    assert!(
        output.contains("unref?"),
        "honest column label present:\n{output}"
    );

    // The overclaiming bare label is GONE from the whole surface.
    assert!(
        !output.contains("dead"),
        "the overclaiming `dead` label must be absent:\n{output}"
    );

    // Caveat footnote present, scoped honestly, and routes to `rmap trust`.
    assert!(
        output.contains("note: unref? = symbols with no inbound reference"),
        "caveat footnote present:\n{output}"
    );
    assert!(
        output.contains("run `rmap trust` for reliability."),
        "caveat routes to the reliability surface:\n{output}"
    );
}

// ── MODULES-IDENTITY-2 §2.1: twin-name disambiguation ──────────────────────

/// Build a minimal entry carrying only the identity fields disambiguation
/// reads (display name / canonical path / owning manifest); everything else is
/// a benign default.
fn identity_entry(display_name: &str, path: &str, manifest: Option<&str>) -> ModuleListEntry {
    ModuleListEntry {
        module_uid: format!("uid-{path}"),
        module_key: format!("k:repo:{path}"),
        canonical_root_path: path.to_string(),
        module_kind: "manifest".to_string(),
        display_name: display_name.to_string(),
        manifest: manifest.map(str::to_string),
        confidence: 1.0,
        owned_file_count: 1,
        owned_test_file_count: 0,
        unref_reduction: None,
        outbound_dependency_count: 0,
        outbound_import_count: 0,
        inbound_dependency_count: 0,
        inbound_import_count: 0,
        violation_count: 0,
        dead_symbol_count: 0,
        dead_test_symbol_count: 0,
    }
}

fn identity_response(results: Vec<ModuleListEntry>) -> ModulesListResponse {
    ModulesListResponse {
        command: "modules list".to_string(),
        repo: "repo_123".to_string(),
        snapshot: "snap_456".to_string(),
        results,
        http_boundary_link_count: Some(0),
        http_boundary_link_degraded: None,
        edges: Some(vec![]),
    }
}

/// The measured django defect: two modules both named `Django` (one
/// `pyproject.toml`, one `package.json`), both rendered as a bare `Django` with
/// nothing to tell them apart. Each must now carry its owning manifest suffix —
/// from the SAME derivation `orient` uses (proven by `shared_derivation_*`).
#[test]
fn list_render_disambiguates_twin_names_by_manifest() {
    let resp = identity_response(vec![
        identity_entry("Django", ".", Some("pyproject.toml")),
        identity_entry("Django", "packages/js", Some("package.json")),
    ]);
    let output = resp.render_human();
    assert!(
        output.contains("Django [pyproject.toml]"),
        "python twin disambiguated by manifest:\n{output}"
    );
    assert!(
        output.contains("Django [package.json]"),
        "js twin disambiguated by manifest:\n{output}"
    );
    // The bare, indistinguishable `Django  ` row (name followed by column
    // padding, no suffix) must be gone.
    assert!(
        !output.contains("Django  "),
        "no bare indistinguishable row remains:\n{output}"
    );
}

/// django's REAL shape: two `Django` modules BOTH declared via `pyproject.toml`,
/// both rooted at `.` in the module_candidates surface — the manifest cannot
/// disambiguate them, so the unique canonical path is the honest tie-break
/// (never two label-identical `Django [pyproject.toml]` rows).
#[test]
fn list_render_twin_same_manifest_falls_back_to_path() {
    let resp = identity_response(vec![
        identity_entry("Django", ".", Some("pyproject.toml")),
        identity_entry("Django", "django/other", Some("pyproject.toml")),
    ]);
    let output = resp.render_human();
    assert!(output.contains("Django [.]"), "{output}");
    assert!(output.contains("Django [django/other]"), "{output}");
}

/// Unique display names get NO suffix — the glamCRM spot-check case (its module
/// names are unique). No suffix noise; byte-stable versus pre-slice output.
#[test]
fn list_render_unique_names_carry_no_suffix() {
    let resp = identity_response(vec![
        identity_entry("api", "src/api", Some("package.json")),
        identity_entry("core", "src/core", Some("Cargo.toml")),
    ]);
    let output = resp.render_human();
    assert!(
        !output.contains('['),
        "no disambiguation suffix on unique names:\n{output}"
    );
    assert!(output.contains("api"), "{output}");
    assert!(output.contains("core"), "{output}");
}

/// The disambiguation is the SAME implementation `orient` uses: modules-list and
/// orient both call `module_disambiguation::collision_disambiguator`, so the SAME
/// twin rows resolve to the SAME manifest/path tokens (one implementation, never
/// a second copy — the identity-divergence this slice kills cannot recur).
#[test]
fn shared_derivation_matches_orient() {
    use crate::presentation::module_disambiguation::{collision_disambiguator, ModuleRow};

    // Same twin shape orient's `orient_seg2` tests assert
    // (`module_label_disambiguates_name_collision_by_manifest`).
    let rows = vec![
        ModuleRow {
            path: "django",
            name: Some("Django"),
            manifest: Some("pyproject.toml"),
        },
        ModuleRow {
            path: "django-js",
            name: Some("Django"),
            manifest: Some("package.json"),
        },
    ];
    let names: Vec<&str> = rows.iter().map(|r| r.effective_name()).collect();
    assert_eq!(
        collision_disambiguator(&rows, &names, 0),
        Some("pyproject.toml")
    );
    assert_eq!(
        collision_disambiguator(&rows, &names, 1),
        Some("package.json")
    );

    // And that exact helper is what modules-list renders through.
    let resp = identity_response(vec![
        identity_entry("Django", "django", Some("pyproject.toml")),
        identity_entry("Django", "django-js", Some("package.json")),
    ]);
    let output = resp.render_human();
    assert!(output.contains("Django [pyproject.toml]"), "{output}");
    assert!(output.contains("Django [package.json]"), "{output}");
}
