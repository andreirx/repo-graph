//! Unit tests for the `cycles` presentation renderer (split from `mod.rs` for the
//! 500-line guardrail; see the module-layout note there).

use super::*;

fn minimal_response() -> CyclesResponse {
    CyclesResponse {
        repo_uid: "repo_01kr12345678".to_string(),
        display_name: Some("test-repo".to_string()),
        snapshot_uid: "snap_01kr12345678".to_string(),
        cycles: vec![],
        count: 0,
        ts_type_only_caveat: false,
        test_composition_note: None,
        module_count: None,
        module_edge_count: None,
        // A current daemon's complete partition payload, nothing excluded (D-TESB-17 fixture
        // rule: only the named partial/absent tests omit keys).
        import_view: Some(serde_json::json!({"include_tests": false, "include_inferred": false})),
        import_remainder: Some(zero_remainder()),
        excluded_cycles: Some(serde_json::json!([])),
        importer_test_status_undetermined: Some(importer_block(0, 0)),
    }
}

fn zero_remainder() -> serde_json::Value {
    serde_json::json!({
        "tests": {"imports": 0, "edges": 0},
        "inferred": {"imports": 0, "edges": 0},
        "tests_and_inferred": {"imports": 0, "edges": 0},
    })
}

/// A MODULE cycle node; `qualified_name` defaults to `None` (exercises the `name` fallback).
fn cnode(node_id: &str, name: &str) -> CycleNode {
    CycleNode {
        node_id: node_id.to_string(),
        name: name.to_string(),
        qualified_name: None,
        file: None,
    }
}

/// A cycle with NO carried edges (the LiveGraph route + older daemon reply) -> unordered render.
/// `test_composition` absent → `NotEvaluated` (the LiveGraph serving path).
fn cyc(nodes: Vec<CycleNode>) -> Cycle {
    Cycle {
        nodes,
        edges: None,
        edges_truncated: None,
        test_composition: None,
        test_composition_unknown_reason: None,
        type_only: None,
    }
}

/// A cycle carrying an explicit test-composition discriminant (the SQLite route).
fn cyc_classified(nodes: Vec<CycleNode>, composition: &str, reason: Option<&str>) -> Cycle {
    Cycle {
        nodes,
        edges: None,
        edges_truncated: None,
        test_composition: Some(composition.to_string()),
        test_composition_unknown_reason: reason.map(str::to_string),
        type_only: None,
    }
}

/// TYPE-ONLY-IMPORTS-1: a cycle carrying an explicit per-cycle type-only verdict (the SQLite route).
fn cyc_type_only(nodes: Vec<CycleNode>, verdict: super::CycleTypeOnly) -> Cycle {
    Cycle {
        nodes,
        edges: None,
        edges_truncated: None,
        test_composition: None,
        test_composition_unknown_reason: None,
        type_only: Some(verdict),
    }
}

/// A cycle explicitly labeled test-only by the daemon (SQLite route).
fn cyc_fixture(nodes: Vec<CycleNode>) -> Cycle {
    cyc_classified(nodes, "test_only", None)
}

#[test]
fn render_demotes_test_only_cycles_below_main() {
    // FIXTURE-POLLUTION-1 §2.2: the daemon-labeled test-only cycle is EXCLUDED from the
    // headline count and DEMOTED to a trailing labeled section — never hidden, and the
    // production cycle leads.
    let mut r = minimal_response();
    r.count = 2;
    r.cycles = vec![
        cyc_classified(
            vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
            "production",
            None,
        ),
        cyc_fixture(vec![
            cnode("t1", "tests/fixtures/mono/pkg-a"),
            cnode("t2", "tests/fixtures/mono/pkg-b"),
        ]),
    ];
    let out = r.render_human();
    // Headline main-only, with the SHARED combined parenthetical (review-4 #3): the exclusion
    // disclosure is inline, phrased identically to `orient`, not a separate line.
    assert!(
        out.contains("1 module-level cycle found (+1 test-only excluded)"),
        "{out}"
    );
    // Demoted section present, labeled, and BELOW the production cycle.
    let prod = out.find("src/a").expect("production cycle shown");
    let section = out
        .find("test-only cycles (1 — excluded from the headline")
        .expect("demoted section present");
    let fixture = out
        .find("tests/fixtures/mono/pkg-a")
        .expect("test-only cycle shown, not hidden");
    assert!(prod < section, "production leads:\n{out}");
    assert!(section < fixture, "test-only under the section:\n{out}");
}

#[test]
fn render_unknown_cycle_stays_in_main_with_marker() {
    // Binding direction rule: an UNKNOWN cycle (a member owns no tracked file) is NEVER
    // demoted — it stays in the main listing carrying an explicit unknown-with-reason
    // marker, and is counted in the headline.
    let mut r = minimal_response();
    r.count = 2;
    r.cycles = vec![
        cyc_classified(
            vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
            "production",
            None,
        ),
        cyc_classified(
            vec![cnode("u1", "vendor/x"), cnode("u2", "vendor/y")],
            "unknown",
            Some("member module `vendor/x` owns no tracked file (is_test unknown)"),
        ),
    ];
    let out = r.render_human();
    // BOTH cycles are in the main headline (unknown is not demoted), and the unknown subset is
    // disclosed inline in the SAME combined form `orient` renders (review-4 #3 assertion).
    assert!(
        out.contains("2 module-level cycles found (test-composition unknown for 1)"),
        "{out}"
    );
    assert!(!out.contains("test-only"), "no demotion:\n{out}");
    // The unknown marker with its reason is present in the main listing.
    assert!(
        out.contains("[test-composition unknown: member module `vendor/x` owns no tracked file"),
        "unknown marker present:\n{out}"
    );
}

#[test]
fn headline_counts_come_from_the_shared_partition() {
    // ORIENT-CYCLES-DISAGREE-1 (review-4 #1): the headline integers are produced by the shared
    // `repo_graph_agent::partition_counts` (via `headline_partition`), and the body GROUPING
    // (`main` / `fixtures`) reads the SAME per-cycle `composition()`. This pins the two to agree —
    // if a future edit skews either, `main.len()`/`fixtures.len()` would diverge from the shared
    // partition and this fails. Set: 1 production + 1 unknown (both in main) + 1 test-only (demoted).
    let mut r = minimal_response();
    r.count = 3;
    r.cycles = vec![
        cyc_classified(
            vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
            "production",
            None,
        ),
        cyc_classified(
            vec![cnode("u1", "vendor/x"), cnode("u2", "vendor/y")],
            "unknown",
            Some("member module `vendor/x` owns no tracked file (is_test unknown)"),
        ),
        cyc_fixture(vec![
            cnode("t1", "tests/fixtures/mono/pkg-a"),
            cnode("t2", "tests/fixtures/mono/pkg-b"),
        ]),
    ];
    let partition = headline_partition(&r.cycles).expect("all classified ⇒ split present");
    let (fixtures, main): (Vec<&Cycle>, Vec<&Cycle>) = r
        .cycles
        .iter()
        .partition(|c| c.composition() == CycleComposition::TestOnly);
    assert_eq!(
        partition.production_count as usize,
        main.len(),
        "shared production_count == body main grouping"
    );
    assert_eq!(
        partition.test_only_count as usize,
        fixtures.len(),
        "shared test_only_count == body demoted grouping"
    );
    assert_eq!(partition.unknown_count, 1, "one unknown in the headline");
    // And the rendered headline reflects those shared integers (production 2, +1 test-only, 1 unknown).
    let out = r.render_human();
    assert!(
        out.contains(
            "2 module-level cycles found (+1 test-only excluded; test-composition unknown for 1)"
        ),
        "{out}"
    );
}

#[test]
fn render_states_livegraph_asymmetry_note() {
    // §2.3: on the LiveGraph serving path no cycle carries test_composition (every cycle
    // is NotEvaluated); the asymmetry is stated honestly instead of pretending uniformity.
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc(vec![cnode("n1", "src/a"), cnode("n2", "src/b")])];
    r.test_composition_note = Some(
        "test-only cycles not evaluated on this serving path (LiveGraph lacks the is_test fact)"
            .to_string(),
    );
    let out = r.render_human();
    assert!(out.contains("1 module-level cycle found"), "{out}");
    assert!(
        out.contains("Note: test-only cycles not evaluated on this serving path"),
        "asymmetry stated:\n{out}"
    );
    // No fabricated test-only section when nothing is labeled.
    assert!(!out.contains("+1 test-only cycle"), "{out}");
}

#[test]
fn render_shows_repo_display_name() {
    let out = minimal_response().render_human();
    assert!(out.contains("Cycles: test-repo"));
}

#[test]
fn render_shows_no_cycles_message() {
    let out = minimal_response().render_human();
    assert!(out.contains("No module-level cycles found"));
}

#[test]
fn zero_state_states_module_and_edge_counts() {
    // IMPORT-RESOLUTION-RUST-1 §2.5 / §4 (cycle-4 ruling C): the zero-state names the graph the
    // acyclicity was computed over by the population term the product already prints in `stats`
    // ("directory groups"), never a bare "modules".
    let mut r = minimal_response();
    r.module_count = Some(59);
    r.module_edge_count = Some(120);
    let out = r.render_human();
    assert!(
        out.contains("over 59 directory groups / 120 resolved import edges"),
        "{out}"
    );
    // Guard against the retired bare-"modules" wording (would collide with `modules list`).
    assert!(!out.contains("over 59 modules"), "{out}");
}

#[test]
fn zero_state_renders_two_crate_fixture_clause_verbatim() {
    // IMPORT-RESOLUTION-RUST-1 §4 (operator ruling cycle-4, `cycles-module-count-semantics` = C):
    // the two-crate Rust fixture drives module_count == 4 (the per-directory MODULE nodes `a`,
    // `a/src`, `b`, `b/src` that `find_cycles` runs its SCC over — dispatch.rs:2558) and exactly
    // one resolved cross-module import edge. The ratified §4 clause is therefore
    // "over 4 directory groups / 1 resolved import edge" — verbatim.
    //
    // The integration test `cross_crate_use_resolves_to_defining_file` proves the fixture actually
    // yields (module_count, module_edge_count) == (4, 1) through the real storage reads the daemon
    // uses; this pins that those numbers RENDER as the ratified clause. It also covers the SINGULAR
    // edge branch (no plural "s"), whose only sibling coverage is the plural (120) and empty (0).
    let mut r = minimal_response();
    r.module_count = Some(4);
    r.module_edge_count = Some(1);
    let out = r.render_human();
    assert!(
        out.contains("over 4 directory groups / 1 resolved import edge"),
        "expected the ratified two-crate-fixture §4 clause, got:\n{out}"
    );
    // Guard the singular: the plural "edges" must NOT appear for e == 1.
    assert!(
        !out.contains("1 resolved import edges"),
        "e == 1 must render singular 'edge', not 'edges':\n{out}"
    );
    // Guard against the retired bare-"modules" wording.
    assert!(
        !out.contains("4 modules"),
        "population is named 'directory groups', never bare 'modules':\n{out}"
    );
    // Not the EMPTY-graph claim (that is the e == 0 branch, a different meaning).
    assert!(
        !out.contains("EMPTY"),
        "1 edge is not an empty graph:\n{out}"
    );
}

#[test]
fn zero_state_names_empty_graph_when_no_resolved_edges() {
    // E == 0 over N>0 directory groups means the module import graph is EMPTY — a distinct
    // claim from "acyclic". Must NOT read as a clean acyclic result.
    let mut r = minimal_response();
    r.module_count = Some(59);
    r.module_edge_count = Some(0);
    let out = r.render_human();
    assert!(
        out.contains("59 directory groups / 0 resolved import edges"),
        "{out}"
    );
    assert!(out.contains("EMPTY"), "{out}");
}

#[test]
fn zero_state_without_counts_keeps_bare_message() {
    // The SQLite-free LiveGraph fastpath omits the counts → bare message, no fabricated size.
    let out = minimal_response().render_human();
    assert!(out.contains("No module-level cycles found"));
    assert!(
        !out.contains("over"),
        "no size clause when counts absent: {out}"
    );
}

#[test]
fn render_shows_cycle_count() {
    let mut r = minimal_response();
    r.count = 3;
    r.cycles = vec![
        cyc(vec![cnode("n1", "src/a"), cnode("n2", "src/b")]),
        cyc(vec![cnode("n3", "src/c"), cnode("n4", "src/d")]),
        cyc(vec![cnode("n5", "src/e"), cnode("n6", "src/f")]),
    ];
    let out = r.render_human();
    assert!(out.contains("3 module-level cycles found"));
}

#[test]
fn render_shows_large_cycle_size() {
    let mut r = minimal_response();
    r.count = 1;
    let nodes: Vec<CycleNode> = (0..10)
        .map(|i| cnode(&format!("n{i}"), &format!("src/mod{i}")))
        .collect();
    r.cycles = vec![cyc(nodes)];
    let out = r.render_human();
    assert!(out.contains("(10 modules)"));
}

#[test]
fn render_falls_back_to_repo_uid_when_no_display_name() {
    let mut r = minimal_response();
    r.display_name = None;
    let out = r.render_human();
    assert!(out.contains("Cycles: repo_01kr12345678"));
}

// ── CYCLE-HONESTY-1 (§2.4): repo-level type-only caveat footer ──

#[test]
fn ts_caveat_footer_present_when_flagged() {
    let mut r = minimal_response();
    r.count = 1;
    r.ts_type_only_caveat = true;
    r.cycles = vec![cyc(vec![cnode("a", "a"), cnode("b", "b")])];
    let out = r.render_human();
    assert!(
        out.contains("this repo contains TypeScript/JavaScript")
            && out.contains("import type")
            && out.contains("vanish at runtime"),
        "repo-scoped type-only caveat footer present: {out}"
    );
}

#[test]
fn ts_caveat_footer_absent_when_not_flagged() {
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc(vec![cnode("a", "a"), cnode("b", "b")])];
    let out = r.render_human();
    assert!(
        !out.contains("import type"),
        "no caveat on a non-TS repo: {out}"
    );
}

// ── CYCLES-FILE-IMPORT-RENDER-1: FILE-import vocabulary (LiveGraph route -> no edges -> unordered) ──

fn two_file_cycle() -> CyclesResponse {
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc(vec![
        cnode("repo:packages/a/src/main.ts:FILE", "packages/a/src/main.ts"),
        cnode("repo:packages/b/src/foo.ts:FILE", "packages/b/src/foo.ts"),
    ])];
    r
}

#[test]
fn file_import_render_empty_says_files_not_modules() {
    let out = minimal_response().render_human_file_import(); // count 0
    assert!(
        out.contains("No FILE import cycles found within the captured scope"),
        "{out}"
    );
    assert!(!out.contains("module"), "empty must not say module: {out}");
}

#[test]
fn file_import_render_nonempty_says_files_not_modules() {
    let out = two_file_cycle().render_human_file_import();
    assert!(out.contains("1 FILE import cycle found"), "{out}");
    assert!(out.contains("(2 files)"), "{out}");
    // LiveGraph route carries no edges -> unordered listing, NO fabricated arrows.
    assert!(
        out.contains("members (unordered): packages/a/src/main.ts, packages/b/src/foo.ts"),
        "{out}"
    );
    assert!(
        !out.contains(" -> "),
        "no arrows on the edge-less route: {out}"
    );
    assert!(!out.contains("module"), "no module vocab: {out}");
    assert!(
        !out.contains("rmap modules deps"),
        "no module-deps hint: {out}"
    );
}

#[test]
fn sqlite_module_render_keeps_vocabulary() {
    // The SQLite path uses render_human (MODULE) vocabulary + the module-deps hint (unchanged).
    let out = two_file_cycle().render_human();
    assert!(out.contains("1 module-level cycle found"), "{out}");
    assert!(out.contains("(2 modules)"), "{out}");
    assert!(
        out.contains("Run: rmap modules deps <module>"),
        "module-deps hint retained for SQLite: {out}"
    );
}

// ── MODULE-CYCLES-CLI-1: dedicated MODULE-import renderer (module paths; LiveGraph -> unordered) ──

fn two_module_cycle() -> CyclesResponse {
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc(vec![
        cnode("repo:packages/a/src:MODULE", "packages/a/src"),
        cnode("repo:packages/b/src:MODULE", "packages/b/src"),
    ])];
    r
}

#[test]
fn module_import_render_says_modules_with_paths() {
    let out = two_module_cycle().render_human_module_import();
    assert!(out.contains("1 MODULE import cycle found"), "{out}");
    assert!(out.contains("(2 modules)"), "{out}");
    // LiveGraph route -> unordered member PATHS, no fabricated arrows.
    assert!(
        out.contains("members (unordered): packages/a/src, packages/b/src"),
        "members are module PATHS: {out}"
    );
    assert!(
        !out.contains(" -> "),
        "no arrows on the edge-less route: {out}"
    );
    assert!(!out.contains("module-level"), "{out}");
    assert!(!out.contains("FILE import"), "{out}");
    assert!(!out.contains("rmap modules deps"), "{out}");
}

#[test]
fn module_import_render_empty() {
    let out = minimal_response().render_human_module_import();
    assert!(
        out.contains("No MODULE import cycles found within the captured scope"),
        "{out}"
    );
    assert!(!out.contains("module-level"), "{out}");
}

// ── FIXTURE-POLLUTION-1 §2.3: the LiveGraph-route asymmetry note must reach the
//    dedicated FILE-import and MODULE-import renderers too (not only `render_human`),
//    on BOTH the empty and non-empty paths. Without these the LiveGraph cycles read as
//    production-vs-test-classified when they were never evaluated. (review-3 #1)

#[test]
fn file_import_render_states_test_composition_asymmetry_nonempty() {
    let mut r = two_file_cycle();
    r.test_composition_note = Some(
        "test composition not evaluated on this serving path (the LiveGraph IR lacks the \
         is_test fact); FILE-import cycles are not classified test-only vs production"
            .to_string(),
    );
    let out = r.render_human_file_import();
    assert!(out.contains("1 FILE import cycle found"), "{out}");
    assert!(
        out.contains("Note: test composition not evaluated on this serving path"),
        "asymmetry stated on the non-empty FILE route:\n{out}"
    );
}

#[test]
fn file_import_render_states_test_composition_asymmetry_empty() {
    let mut r = minimal_response(); // count 0
    r.test_composition_note = Some(
        "test composition not evaluated on this serving path (the LiveGraph IR lacks the \
         is_test fact); FILE-import cycles are not classified test-only vs production"
            .to_string(),
    );
    let out = r.render_human_file_import();
    assert!(
        out.contains("No FILE import cycles found within the captured scope"),
        "{out}"
    );
    assert!(
        out.contains("Note: test composition not evaluated on this serving path"),
        "asymmetry stated even with zero FILE cycles:\n{out}"
    );
}

#[test]
fn module_import_render_states_test_composition_asymmetry_nonempty() {
    let mut r = two_module_cycle();
    r.test_composition_note = Some(
        "test-only cycles not evaluated on this serving path (LiveGraph lacks the is_test \
         fact); run `rmap cycles --engine sqlite` to classify test-only cycles"
            .to_string(),
    );
    let out = r.render_human_module_import();
    assert!(out.contains("1 MODULE import cycle found"), "{out}");
    assert!(
        out.contains("Note: test-only cycles not evaluated on this serving path"),
        "asymmetry stated on the non-empty MODULE route:\n{out}"
    );
    assert!(
        out.contains("rmap cycles --engine sqlite"),
        "MODULE route points at its classified sqlite equivalent:\n{out}"
    );
}

#[test]
fn module_import_render_states_test_composition_asymmetry_empty() {
    let mut r = minimal_response(); // count 0
    r.test_composition_note = Some(
        "test-only cycles not evaluated on this serving path (LiveGraph lacks the is_test \
         fact); run `rmap cycles --engine sqlite` to classify test-only cycles"
            .to_string(),
    );
    let out = r.render_human_module_import();
    assert!(
        out.contains("No MODULE import cycles found within the captured scope"),
        "{out}"
    );
    assert!(
        out.contains("Note: test-only cycles not evaluated on this serving path"),
        "asymmetry stated even with zero MODULE cycles:\n{out}"
    );
}

// ── TYPE-ONLY-IMPORTS-1 (slice §4 rendering proof) ────────────────────────────

#[test]
fn type_only_cycle_is_labeled_vanishes_at_runtime() {
    // §4(a): a purely type-only cycle carries the "type-only (vanishes at runtime)" label.
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc_type_only(
        vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
        CycleTypeOnly::TypeOnly,
    )];
    let out = r.render_human();
    assert!(
        out.contains("type-only (vanishes at runtime)"),
        "a pure type-only cycle must be labeled:\n{out}"
    );
    // §4(c): no genuine Unknown ⇒ the blanket/narrowed caveat is ABSENT.
    assert!(
        !out.contains("could not be evaluated"),
        "no Unknown cycles ⇒ no narrowed caveat:\n{out}"
    );
    assert!(
        !out.contains("some cycles may vanish at runtime"),
        "the blanket caveat is retired on the SQLite route:\n{out}"
    );
}

#[test]
fn test_only_cycle_that_is_type_only_is_labeled_in_the_demoted_section() {
    // review-0 item 2: a DEMOTED test-only cycle can ALSO be type-only — a fixture cycle of pure
    // `import type` edges vanishes at runtime just as a production one does. It must carry the label
    // in the trailing test-only section, not go unlabeled there.
    let mut r = minimal_response();
    r.count = 0; // headline production count is 0 (the only cycle is test-only)
    r.cycles = vec![Cycle {
        nodes: vec![cnode("n1", "tests/a"), cnode("n2", "tests/b")],
        edges: None,
        edges_truncated: None,
        test_composition: Some("test_only".to_string()),
        test_composition_unknown_reason: None,
        type_only: Some(CycleTypeOnly::TypeOnly),
    }];
    let out = r.render_human();
    assert!(
        out.contains("test-only cycles ("),
        "the demoted section renders:\n{out}"
    );
    assert!(
        out.contains("type-only (vanishes at runtime)"),
        "a type-only cycle in the demoted test-only section must STILL be labeled:\n{out}"
    );
}

#[test]
fn pure_runtime_cycle_is_not_labeled() {
    // §4(b): a PURE runtime cycle (`type_only == 0`) is a real runtime cycle — NO label, no caveat
    // (byte-stable).
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc_type_only(
        vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
        CycleTypeOnly::HasRuntimeEdges {
            type_only: 0,
            of: 2,
        },
    )];
    let out = r.render_human();
    assert!(
        !out.contains("type-only"),
        "a pure runtime cycle must NOT be labeled type-only:\n{out}"
    );
    assert!(
        !out.contains("could not be evaluated"),
        "a confirmed runtime cycle raises no Unknown caveat:\n{out}"
    );
}

#[test]
fn has_runtime_edges_missing_counts_fails_to_decode_not_defaults_to_zero() {
    // review-1 #2: a `has_runtime_edges` payload MISSING the required counts is producer/mirror
    // schema drift, NOT a pure-runtime cycle. Typed `cycles` decode must FAIL (the error is surfaced
    // by the response-parse `Result`), NEVER silently default to the KNOWN `{0, 0}` pure-runtime
    // state — which would render a false-certain Layer-0 claim and could suppress mixed-SCC detail.
    let missing = serde_json::json!({ "kind": "has_runtime_edges" });
    assert!(
        serde_json::from_value::<CycleTypeOnly>(missing).is_err(),
        "missing type_only/of must be a decode ERROR, not a defaulted {{0,0}}"
    );
    // Guard the legitimate case the fix must NOT break: a COMPLETE pure-runtime payload (the producer
    // always emits both, even `type_only: 0`) still decodes to the exact state.
    let complete = serde_json::json!({ "kind": "has_runtime_edges", "type_only": 0, "of": 2 });
    assert_eq!(
        serde_json::from_value::<CycleTypeOnly>(complete).unwrap(),
        CycleTypeOnly::HasRuntimeEdges {
            type_only: 0,
            of: 2
        },
        "a complete pure-runtime payload must still decode exactly"
    );
}

#[test]
fn mixed_scc_with_surviving_runtime_cycle_states_the_runtime_truth() {
    // COHERENCE-2 §2.2 (Option A): a MIXED SCC whose runtime subgraph is still cyclic renders as a
    // runtime cycle that REMAINS, with the type-only edge count as detail — NEVER a "breaks at
    // runtime" / "residual one-way coupling" claim the topology does not support (the review-0 fix).
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc_type_only(
        vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
        CycleTypeOnly::HasRuntimeEdges {
            type_only: 2,
            of: 4,
        },
    )];
    let out = r.render_human();
    assert!(
        out.contains("runtime cycle remains: 2 of 4 import edges are `import type`"),
        "a mixed-but-still-cyclic SCC states the runtime cycle remains with the type-only count:\n{out}"
    );
    assert!(
        !out.contains("breaks the cycle at runtime"),
        "a surviving runtime cycle must NOT claim it breaks:\n{out}"
    );
    assert!(
        !out.contains("residual one-way coupling"),
        "the false 'residual one-way coupling' claim is retired:\n{out}"
    );
}

#[test]
fn breaks_at_runtime_cycle_states_no_runtime_cycle_remains() {
    // COHERENCE-2 §2.2 (Option A): a cycle whose runtime subgraph is acyclic BREAKS at runtime — it
    // states the SCC-edge counts and that no runtime cycle remains, NOT "vanishes" and NOT the false
    // "residual one-way coupling" claim.
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc_type_only(
        vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
        CycleTypeOnly::BreaksAtRuntime {
            type_only: 1,
            of: 2,
        },
    )];
    let out = r.render_human();
    assert!(
        out.contains(
            "type-only breaks the cycle at runtime: 1 of 2 import edges are `import type` (no \
             runtime cycle remains)"
        ),
        "a broken cycle states it breaks at runtime with the SCC-edge counts:\n{out}"
    );
    assert!(
        !out.contains("residual one-way coupling"),
        "the false 'residual one-way coupling' claim is retired:\n{out}"
    );
    assert!(
        !out.contains("vanishes at runtime"),
        "a broken-but-not-vanished cycle must NOT claim it vanishes:\n{out}"
    );
    assert!(
        !out.contains("could not be evaluated"),
        "a fully-evaluated cycle raises no Unknown caveat:\n{out}"
    );
}

#[test]
fn unknown_cycles_narrow_the_caveat_and_name_the_count() {
    // The blanket hedge survives ONLY as a narrowed footer naming how many cycles are Unknown.
    let mut r = minimal_response();
    r.count = 2;
    r.cycles = vec![
        cyc_type_only(
            vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
            CycleTypeOnly::TypeOnly,
        ),
        cyc_type_only(
            vec![cnode("n3", "src/c"), cnode("n4", "src/d")],
            CycleTypeOnly::Unknown {
                reason: "indexed before type-only tracking".to_string(),
            },
        ),
    ];
    let out = r.render_human();
    assert!(
        out.contains("type-only (vanishes at runtime)"),
        "the evaluated type-only cycle is still labeled:\n{out}"
    );
    assert!(
        out.contains("1 cycle could not be evaluated for `import type`"),
        "the narrowed footer names the Unknown count:\n{out}"
    );
    // Operator ruling 2b: the CARRIED reason is what renders.
    assert!(
        out.contains("(indexed before type-only tracking)"),
        "the footer renders the reason the verdict carries:\n{out}"
    );
}

#[test]
fn unknown_footer_renders_the_carried_reason_not_a_hardcoded_string() {
    // Operator ruling 2026-09-03 item 2b: the footer must render whatever reason the `Unknown` sum type
    // CARRIES — never a hard-coded "indexed before type-only tracking". Two cycles with DIFFERENT reasons
    // must produce two distinct notes; a cycle whose reason is "cycle import edges unavailable" must NOT
    // be mislabeled as pre-tracking.
    let mut r = minimal_response();
    r.count = 2;
    r.cycles = vec![
        cyc_type_only(
            vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
            CycleTypeOnly::Unknown {
                reason: "cycle import edges unavailable".to_string(),
            },
        ),
        cyc_type_only(
            vec![cnode("n3", "src/c"), cnode("n4", "src/d")],
            CycleTypeOnly::Unknown {
                reason: "type-only fact unreadable".to_string(),
            },
        ),
    ];
    let out = r.render_human();
    assert!(
        out.contains("(cycle import edges unavailable)"),
        "the verdict's OWN reason renders (not a hard-coded pre-tracking string):\n{out}"
    );
    assert!(
        out.contains("(type-only fact unreadable)"),
        "the corrupt-carrier reason renders distinctly:\n{out}"
    );
    assert!(
        !out.contains("indexed before type-only tracking"),
        "no cycle carried that reason, so it must NOT appear (no reason invention):\n{out}"
    );
}

// ── TEST-EDGE-SCOPE-1B: the import partition (RG-REQ-004-L12, D-TESB-07) ──────────────

/// A cycles payload as the partitioned daemon sends it: `view` flags, the three remainder groups
/// (imports, edges), the excluded cycles and the importer block (`None` = key absent).
fn partitioned(
    base: serde_json::Value,
    view: (bool, bool),
    remainder: Option<[(u64, u64); 3]>,
    excluded: Option<serde_json::Value>,
    importers: Option<serde_json::Value>,
) -> CyclesResponse {
    // A current daemon sends every key (D-TESB-17); a `None` argument is its zero value.
    let mut v = base;
    v["import_view"] = serde_json::json!({"include_tests": view.0, "include_inferred": view.1});
    let r = remainder.unwrap_or([(0, 0); 3]);
    v["import_remainder"] = serde_json::json!({
        "tests": {"imports": r[0].0, "edges": r[0].1},
        "inferred": {"imports": r[1].0, "edges": r[1].1},
        "tests_and_inferred": {"imports": r[2].0, "edges": r[2].1},
    });
    v["excluded_cycles"] = excluded.unwrap_or_else(|| serde_json::json!([]));
    v["importer_test_status_undetermined"] = importers.unwrap_or_else(|| importer_block(0, 0));
    serde_json::from_value(v).expect("partitioned cycles payload parses")
}

fn zero_base(groups: u64, edges: u64) -> serde_json::Value {
    serde_json::json!({
        "repo_uid": "repo_leveldb", "display_name": "leveldb", "snapshot_uid": "snap_leveldb",
        "cycles": [], "count": 0, "module_count": groups, "module_edge_count": edges
    })
}

/// One shown production 2-cycle `a <-> b` with its real edges.
fn shown_base() -> serde_json::Value {
    serde_json::json!({
        "repo_uid": "repo_x", "display_name": "x", "snapshot_uid": "snap_x", "count": 1,
        "cycles": [{
            "nodes": [
                {"node_id": "m_a", "name": "a", "qualified_name": "a"},
                {"node_id": "m_b", "name": "b", "qualified_name": "b"}
            ],
            "edges": [
                {"from_node_id": "m_a", "to_node_id": "m_b"},
                {"from_node_id": "m_b", "to_node_id": "m_a"}
            ],
            "test_composition": "production"
        }]
    })
}

fn excluded_cycle(members: &[&str], flags: &[&str], contains: &[&[&str]]) -> serde_json::Value {
    serde_json::json!({
        "members": members,
        "flags": flags,
        "contains_shown": contains,
        "partitions": {"production_certain": 1, "test_certain": 1, "production_inferred": 0,
                       "test_inferred": 0, "unknown_test_status": 0}
    })
}

fn importer_block(count: u64, universe_count: u64) -> serde_json::Value {
    let paths: Vec<String> = (0..count).map(|i| format!("util/f{i}.cc")).collect();
    serde_json::json!({"count": count, "paths": paths, "universe": "cross_directory_importers",
                       "universe_count": universe_count, "unknown_count": 0})
}

#[test]
fn cycles_zero_state_counts_the_view_edges_and_names_the_excluded_cycle() {
    // §2.4 leveldb, verbatim: no cycle remains among production imports; the one closed only by
    // imports from test files is named with the flag that shows it, then the remainder and the
    // importer count.
    let r = partitioned(
        zero_base(10, 15),
        (false, false),
        Some([(105, 6), (0, 0), (0, 0)]),
        Some(serde_json::json!([excluded_cycle(
            &["db", "helpers/memenv", "table", "util"],
            &["include_tests"],
            &[]
        )])),
        Some(importer_block(1, 67)),
    );
    let out = r.render_human();
    let expected = [
        "No module-level cycles found over 10 directory groups / 15 resolved import edges.",
        "+1 cycle only through excluded imports, not shown:",
        "  4 modules: db, helpers/memenv, table, util — --include-tests",
        "+105 imports from test files, not shown (6 directory-group edges only through them) — --include-tests",
        "1 file whose test status can't be determined — open it and look inside (of 67 files importing across directories)",
    ]
    .join("\n");
    assert!(out.ends_with(&expected), "{out}");
}

#[test]
fn cycles_names_excluded_cycles_with_their_flags_capped_with_the_elision_line() {
    // D-TESB-07 budget: at most 5 rows, at most 8 members per row, then the elision line naming
    // the command that lists all of them.
    let big: Vec<String> = (0..10).map(|i| format!("m{i:02}")).collect();
    let big: Vec<&str> = big.iter().map(String::as_str).collect();
    let mut cycles = vec![excluded_cycle(&big, &["include_tests"], &[])];
    for i in 0..6 {
        let a = format!("p{i}");
        let b = format!("q{i}");
        cycles.push(excluded_cycle(&[&a, &b], &["include_inferred"], &[]));
    }
    let r = partitioned(
        zero_base(30, 40),
        (false, false),
        Some([(3, 1), (7, 6), (0, 0)]),
        Some(serde_json::Value::Array(cycles)),
        None,
    );
    let out = r.render_human();
    assert!(
        out.contains("+7 cycles only through excluded imports, not shown:\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "  10 modules: m00, m01, m02, m03, m04, m05, m06, m07, + 2 more — --include-tests\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("  2 modules: p0, q0 — --include-inferred\n"),
        "{out}"
    );
    assert!(
        out.contains("  2 modules: p3, q3 — --include-inferred\n"),
        "{out}"
    );
    assert!(!out.contains("p4, q4"), "only five rows are listed:\n{out}");
    assert!(
        out.contains(
            "  … and 2 more cycles — rmap cycles --include-tests --include-inferred --json\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("+7 inferred imports, not shown (6 directory-group edges only through them) — --include-inferred"),
        "{out}"
    );
}

#[test]
fn cycles_render_with_zero_partitions_equals_the_render_without_them() {
    // Additive-key neutrality (P-TESB-04): zero-valued remainder, no excluded cycle and a zero
    // importer count add no text, in the zero state and beside a shown cycle. "Without" is the
    // same payload with no partition key at all (a daemon that predates the partition), whose
    // text is the same plus exactly the one partition-unavailable line (D-TESB-17: absence is
    // stated, never read as zero).
    use crate::presentation::import_partition::PARTITION_UNAVAILABLE;
    for base in [zero_base(4, 2), shown_base()] {
        let without: CyclesResponse = serde_json::from_value(base.clone()).unwrap();
        let without = without.render_human();
        let with = partitioned(
            base,
            (false, false),
            Some([(0, 0), (0, 0), (0, 0)]),
            Some(serde_json::json!([])),
            Some(importer_block(0, 12)),
        )
        .render_human();
        assert!(!with.contains(PARTITION_UNAVAILABLE), "{with}");
        assert_eq!(
            without.matches(PARTITION_UNAVAILABLE).count(),
            1,
            "{without}"
        );
        let stripped: String = without
            .lines()
            .filter(|l| *l != PARTITION_UNAVAILABLE)
            .map(|l| format!("{l}\n"))
            .collect();
        let stripped = stripped.replace("\n\n\n", "\n\n");
        assert_eq!(with.trim_end(), stripped.trim_end());
    }
}

#[test]
fn cycles_names_a_larger_excluded_cycle_with_the_shown_cycles_it_contains() {
    // D-TESB-07: an excluded cycle that grows a shown one is named with the shown cycles it
    // contains — the reader sees the larger SCC is not a separate problem.
    let r = partitioned(
        shown_base(),
        (false, false),
        Some([(2, 1), (0, 0), (0, 0)]),
        Some(serde_json::json!([excluded_cycle(
            &["a", "b", "c"],
            &["include_tests"],
            &[&["a", "b"]]
        )])),
        None,
    );
    let out = r.render_human();
    assert!(out.contains("1 module-level cycle found"), "{out}");
    assert!(
        out.contains("\n\n+1 cycle only through excluded imports, not shown:\n  3 modules: a, b, c — --include-tests (contains 1 shown cycle)\n"),
        "{out}"
    );
}

#[test]
fn cycles_excluded_row_names_the_full_flag_set_of_the_wider_view() {
    // Under `--include-tests` a cycle that also needs inferred imports is shown only by the view
    // with both flags — its row names both, the command that shows it.
    let r = partitioned(
        zero_base(5, 6),
        (true, false),
        Some([(0, 0), (1, 1), (0, 0)]),
        Some(serde_json::json!([excluded_cycle(
            &["a", "b"],
            &["include_tests", "include_inferred"],
            &[]
        )])),
        None,
    );
    let out = r.render_human();
    assert!(
        out.contains("  2 modules: a, b — --include-tests --include-inferred\n"),
        "{out}"
    );
    assert!(
        out.contains("+1 inferred import, not shown (1 directory-group edge only through them) — --include-inferred"),
        "{out}"
    );
}

// ── TEST-EDGE-SCOPE-1B: D-TESB-17 (rows U9, W2) on `cycles` ──

#[test]
fn cycles_partitioned_payload_without_excluded_cycles_states_them_unreadable() {
    use crate::presentation::import_partition::PARTITION_UNREADABLE;
    let mut r = partitioned(zero_base(10, 15), (false, false), None, None, None);
    r.excluded_cycles = None;
    let out = r.render_human();
    assert_eq!(out.matches(PARTITION_UNREADABLE).count(), 1, "{out}");
    assert!(!out.contains("only through excluded imports"), "{out}");
}

#[test]
fn cycles_wrong_typed_excluded_cycles_renders_unreadable() {
    use crate::presentation::import_partition::PARTITION_UNREADABLE;
    for bad in [
        serde_json::json!({"members": ["db", "table"]}),
        serde_json::json!([{"members": ["db", 7], "flags": ["include_tests"], "contains_shown": []}]),
        serde_json::json!([{"members": ["db", "table"], "flags": "include_tests", "contains_shown": []}]),
    ] {
        let r = partitioned(
            zero_base(10, 15),
            (false, false),
            None,
            Some(bad.clone()),
            None,
        );
        let out = r.render_human();
        assert!(out.contains(PARTITION_UNREADABLE), "{bad}: {out}");
        assert!(!out.contains("only through excluded imports"), "{out}");
    }
}
