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
    }
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
fn has_runtime_edges_cycle_is_not_labeled() {
    // §4(b): a mixed cycle (≥1 runtime edge) is a real runtime cycle — NO label, no caveat.
    let mut r = minimal_response();
    r.count = 1;
    r.cycles = vec![cyc_type_only(
        vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
        CycleTypeOnly::HasRuntimeEdges,
    )];
    let out = r.render_human();
    assert!(
        !out.contains("type-only"),
        "a runtime cycle must NOT be labeled type-only:\n{out}"
    );
    assert!(
        !out.contains("could not be evaluated"),
        "a confirmed runtime cycle raises no Unknown caveat:\n{out}"
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
