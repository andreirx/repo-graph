//! RESOLUTION-BREAKDOWN-CLI-1 storage reconciliation tests.
//!
//! A controlled fixture proves the load-bearing SQL properties:
//!   1. `Σ by_language == total` and `Σ by_module == total` across BOTH `is_test`
//!      partitions (the parts-reconcile-to-whole invariant, slice §4);
//!   2. only the four CALLS categories count as unresolved calls (an IMPORTS-family
//!      unresolved row is excluded — the same filter the aggregate applies);
//!   3. the `classification` split (external / unknown) is grouped per scope;
//!   4. a source with no attributable language/module folds into the honest
//!      `'(unknown)'` bucket rather than being dropped;
//!   5. (review-0 F2) a language/module present in the symbol inventory but with
//!      ZERO calls is SEEDED as an all-zero scope (→ UNKNOWN), never absent;
//!   6. (review-0 F4) calls are partitioned by `files.is_test`, so a scope's test
//!      and production calls are separable and each reconciles.

use super::*;
use crate::connection::StorageConnection;

const SNAP: &str = "snap-crb-1";

/// A minimal graph exercising all four properties:
///   * `a.ts` (typescript, prod) owns FUNCTION `sa`; `b.java` (java, prod) owns
///     METHOD `sb`; `c.test.ts` (typescript, TEST) owns FUNCTION `sc`; `d.py`
///     (python, prod) owns FUNCTION `sd` with NO calls (F2 seed). SYMBOL `sx` has
///     NO file (→ the `(unknown)` bucket).
///   * module candidates (`module_candidates` ⋈ `module_file_ownership`, the SEMANTIC
///     module population — review-1 #2): `src` owns a.ts/b.java/c.test.ts; `util` owns
///     d.py (so `util` is a module present with symbols but no calls — F2 at module
///     granularity). NO raw `MODULE`/`OWNS` rows exist, so the by-module scopes can only
///     come from the candidate attribution — the test proves candidate-sourcing.
///   * resolved CALLS: sa×2, sb×1, sc×1 (test), sx×1  → total 5.
///   * unresolved CALLS: sa {external, unknown, internal}, sb {unknown, internal},
///     sc {internal} (test)  → total 6 (external 1, unknown 2).
///   * one IMPORTS-family unresolved row on sa — MUST be excluded.
fn fixture() -> StorageConnection {
    let storage = StorageConnection::open_in_memory().unwrap();
    storage
        .connection()
        .execute_batch(&format!(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
               VALUES ('r1', 'test-repo', '/tmp/r1', '2024-01-01T00:00:00Z'); \
             INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at) \
               VALUES ('{SNAP}', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z'); \
             INSERT INTO files (file_uid, repo_uid, path, language, is_test) VALUES \
               ('r1:a.ts', 'r1', 'src/a.ts', 'typescript', 0), \
               ('r1:b.java', 'r1', 'src/b.java', 'java', 0), \
               ('r1:c.test.ts', 'r1', 'src/c.test.ts', 'typescript', 1), \
               ('r1:d.py', 'r1', 'util/d.py', 'python', 0); \
             INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, qualified_name, file_uid) VALUES \
               ('sa',    '{SNAP}', 'r1', 'r1:a.ts#sa',   'SYMBOL', 'FUNCTION', 'sa',  NULL,  'r1:a.ts'), \
               ('sb',    '{SNAP}', 'r1', 'r1:b.java#sb', 'SYMBOL', 'METHOD',   'sb',  NULL,  'r1:b.java'), \
               ('sc',    '{SNAP}', 'r1', 'r1:c#sc',      'SYMBOL', 'FUNCTION', 'sc',  NULL,  'r1:c.test.ts'), \
               ('sd',    '{SNAP}', 'r1', 'r1:d#sd',      'SYMBOL', 'FUNCTION', 'sd',  NULL,  'r1:d.py'), \
               ('sx',    '{SNAP}', 'r1', 'r1:unk#sx',    'SYMBOL', 'FUNCTION', 'sx',  NULL,  NULL); \
             INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor) VALUES \
               ('e_c1', '{SNAP}', 'r1', 'sa', 'sb', 'CALLS', 'static', 't'), \
               ('e_c2', '{SNAP}', 'r1', 'sa', 'sb', 'CALLS', 'static', 't'), \
               ('e_c3', '{SNAP}', 'r1', 'sb', 'sa', 'CALLS', 'static', 't'), \
               ('e_c4', '{SNAP}', 'r1', 'sc', 'sa', 'CALLS', 'static', 't'), \
               ('e_c5', '{SNAP}', 'r1', 'sx', 'sa', 'CALLS', 'static', 't'); \
             INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, canonical_root_path, confidence) VALUES \
               ('mc_src',  '{SNAP}', 'r1', 'src',  'inferred', 'src',  0.7), \
               ('mc_util', '{SNAP}', 'r1', 'util', 'inferred', 'util', 0.7); \
             INSERT INTO module_file_ownership (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) VALUES \
               ('{SNAP}', 'r1', 'r1:a.ts',      'mc_src',  'manifest_prefix', 0.7), \
               ('{SNAP}', 'r1', 'r1:b.java',    'mc_src',  'manifest_prefix', 0.7), \
               ('{SNAP}', 'r1', 'r1:c.test.ts', 'mc_src',  'manifest_prefix', 0.7), \
               ('{SNAP}', 'r1', 'r1:d.py',      'mc_util', 'manifest_prefix', 0.7);"
        ))
        .unwrap();

    // Unresolved CALLS rows (+ one IMPORTS row that must be excluded).
    let ue = |edge: &str, src: &str, class: &str, cat: &str| {
        storage
            .connection()
            .execute(
                "INSERT INTO unresolved_edges \
                 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, \
                  resolution, extractor, category, classification, classifier_version, \
                  basis_code, observed_at) \
                 VALUES (?, ?, 'r1', ?, 'tk', 'CALLS', 'unresolved', 't', ?, ?, 1, 'no_supporting_signal', '2024-01-01T00:00:00Z')",
                rusqlite::params![edge, SNAP, src, cat, class],
            )
            .unwrap();
    };
    ue(
        "u1",
        "sa",
        "external_library_candidate",
        "calls_obj_method_needs_type_info",
    );
    ue("u2", "sa", "unknown", "calls_obj_method_needs_type_info");
    ue(
        "u3",
        "sa",
        "internal_candidate",
        "calls_function_ambiguous_or_missing",
    );
    ue(
        "u4",
        "sb",
        "unknown",
        "calls_this_method_needs_class_context",
    );
    ue(
        "u5",
        "sb",
        "internal_candidate",
        "calls_function_ambiguous_or_missing",
    );
    ue(
        "u6",
        "sc",
        "internal_candidate",
        "calls_function_ambiguous_or_missing",
    );
    // IMPORTS-family: NOT a CALLS category -> excluded from every count.
    ue(
        "u7",
        "sa",
        "external_library_candidate",
        "imports_file_not_found",
    );

    storage
}

/// Find the row for a `(scope, is_test)` cell.
fn get<'a>(rows: &'a [ScopeCountRow], key: &str, is_test: bool) -> &'a CallResolutionCounts {
    &rows
        .iter()
        .find(|r| r.key == key && r.is_test == is_test)
        .unwrap_or_else(|| panic!("no scope row for {key} (is_test={is_test})"))
        .counts
}

fn tuple(c: &CallResolutionCounts) -> (u64, u64, u64, u64) {
    (c.resolved, c.unresolved, c.external, c.unknown)
}

fn sum(rows: &[ScopeCountRow], f: impl Fn(&CallResolutionCounts) -> u64) -> u64 {
    rows.iter().map(|r| f(&r.counts)).sum()
}

/// The four sum axes reconcile parts→whole.
fn assert_reconciles(rows: &[ScopeCountRow], total: &CallResolutionCounts) {
    assert_eq!(sum(rows, |c| c.resolved), total.resolved, "resolved");
    assert_eq!(sum(rows, |c| c.unresolved), total.unresolved, "unresolved");
    assert_eq!(sum(rows, |c| c.external), total.external, "external");
    assert_eq!(sum(rows, |c| c.unknown), total.unknown, "unknown");
}

#[test]
fn total_counts_exclude_non_calls_categories() {
    let s = fixture();
    let total = s.query_call_resolution_total(SNAP).unwrap();
    assert_eq!(total.resolved, 5, "5 CALLS edges");
    assert_eq!(
        total.unresolved, 6,
        "6 CALLS-family unresolved (IMPORTS row excluded)"
    );
    assert_eq!(
        total.external, 1,
        "only the CALLS external row, not the IMPORTS one"
    );
    assert_eq!(total.unknown, 2);
}

#[test]
fn by_language_splits_and_reconciles_to_total() {
    let s = fixture();
    let total = s.query_call_resolution_total(SNAP).unwrap();
    let langs = s.query_call_resolution_by_language(SNAP).unwrap();

    // typescript production (sa) vs typescript test (sc) — F4 separability.
    assert_eq!(tuple(get(&langs, "typescript", false)), (2, 3, 1, 1));
    assert_eq!(tuple(get(&langs, "typescript", true)), (1, 1, 0, 0));
    assert_eq!(tuple(get(&langs, "java", false)), (1, 2, 0, 1));
    // sx has no file -> its resolved call lands in the honest (unknown) bucket.
    assert_eq!(
        tuple(get(&langs, super::UNATTRIBUTED_SCOPE, false)),
        (1, 0, 0, 0)
    );

    assert_reconciles(&langs, &total);
}

#[test]
fn by_module_splits_and_reconciles_to_total() {
    // review-1 #2: by-module scopes are the SEMANTIC module candidates
    // (`module_candidates` ⋈ `module_file_ownership`), NOT raw MODULE directory nodes —
    // and the fixture has NO MODULE/OWNS rows, so `src`/`util` here can ONLY be the
    // candidate `canonical_root_path`s.
    let s = fixture();
    let total = s.query_call_resolution_total(SNAP).unwrap();
    let mods = s.query_call_resolution_by_module(SNAP).unwrap();

    // candidate `src` production (sa + sb) vs `src` test (sc) — F4 at module granularity.
    assert_eq!(tuple(get(&mods, "src", false)), (3, 5, 1, 2));
    assert_eq!(tuple(get(&mods, "src", true)), (1, 1, 0, 0));
    // sx has no owning candidate -> (unknown) bucket; its one resolved call.
    assert_eq!(
        tuple(get(&mods, super::UNATTRIBUTED_SCOPE, false)),
        (1, 0, 0, 0)
    );

    assert_reconciles(&mods, &total);
}

#[test]
fn by_module_attributes_each_file_to_its_most_specific_candidate() {
    // review-1 #2 robustness: `module_file_ownership`'s UNIQUE is on the (snapshot,
    // file, candidate) TRIPLE, so a file MAY carry >1 ownership row (a parent and a
    // nested candidate). The by-module read must attribute each call to EXACTLY ONE
    // candidate — the most specific (longest `canonical_root_path`), mirroring the
    // write-time longest-prefix winner — so the parts still reconcile to the whole
    // (no double-count). This fixture gives `f.ts` two owners (`app` and the nested
    // `app/api`); its calls MUST land under `app/api` only.
    let s = StorageConnection::open_in_memory().unwrap();
    s.connection()
        .execute_batch(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
               VALUES ('r1', 'r', '/tmp/r1', '2024-01-01T00:00:00Z'); \
             INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at) \
               VALUES ('nest', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z'); \
             INSERT INTO files (file_uid, repo_uid, path, language, is_test) VALUES \
               ('r1:f', 'r1', 'app/api/f.ts', 'typescript', 0); \
             INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, qualified_name, file_uid) VALUES \
               ('sf', 'nest', 'r1', 'r1:f#sf', 'SYMBOL', 'FUNCTION', 'sf', NULL, 'r1:f'); \
             INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor) VALUES \
               ('e1', 'nest', 'r1', 'sf', 'sf', 'CALLS', 'static', 't'), \
               ('e2', 'nest', 'r1', 'sf', 'sf', 'CALLS', 'static', 't'); \
             INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, canonical_root_path, confidence) VALUES \
               ('mc_app',   'nest', 'r1', 'app',     'inferred', 'app',     0.7), \
               ('mc_appapi','nest', 'r1', 'app/api', 'inferred', 'app/api', 0.7); \
             INSERT INTO module_file_ownership (snapshot_uid, repo_uid, file_uid, module_candidate_uid, assignment_kind, confidence) VALUES \
               ('nest', 'r1', 'r1:f', 'mc_app',    'manifest_prefix', 0.7), \
               ('nest', 'r1', 'r1:f', 'mc_appapi', 'manifest_prefix', 0.7);",
        )
        .unwrap();

    let total = s.query_call_resolution_total("nest").unwrap();
    let mods = s.query_call_resolution_by_module("nest").unwrap();

    // The 2 resolved calls attribute to the MOST SPECIFIC candidate only.
    assert_eq!(tuple(get(&mods, "app/api", false)), (2, 0, 0, 0));
    // The parent `app` must NOT also receive them (no double-count).
    assert!(
        !mods.iter().any(|r| r.key == "app"),
        "calls must not double-count under the parent candidate: {mods:?}"
    );
    // Strict partition holds: the single-owner pick keeps Σ == total.
    assert_reconciles(&mods, &total);
    assert_eq!(total.resolved, 2);
}

#[test]
fn present_but_callless_scope_is_seeded_as_zero_not_dropped() {
    // review-0 F2: python (`d.py`) has a function symbol but makes NO calls; the
    // module `util` owns only that callless file. Both MUST appear as all-zero rows
    // (the projection renders them UNKNOWN), never absent.
    let s = fixture();
    let langs = s.query_call_resolution_by_language(SNAP).unwrap();
    assert_eq!(
        tuple(get(&langs, "python", false)),
        (0, 0, 0, 0),
        "python present in the inventory but callless -> seeded zero, not dropped"
    );

    let mods = s.query_call_resolution_by_module(SNAP).unwrap();
    assert_eq!(
        tuple(get(&mods, "util", false)),
        (0, 0, 0, 0),
        "module `util` present but callless -> seeded zero, not dropped"
    );
}

#[test]
fn test_partition_reconciles_production_plus_test_to_total() {
    // review-0 F4: production rows + test rows sum to the grand total, on every axis.
    let s = fixture();
    let total = s.query_call_resolution_total(SNAP).unwrap();
    let langs = s.query_call_resolution_by_language(SNAP).unwrap();

    let prod: Vec<ScopeCountRow> = langs.iter().filter(|r| !r.is_test).cloned().collect();
    let test: Vec<ScopeCountRow> = langs.iter().filter(|r| r.is_test).cloned().collect();
    assert!(!test.is_empty(), "the fixture has a test-file scope");
    assert_eq!(
        sum(&prod, |c| c.resolved) + sum(&test, |c| c.resolved),
        total.resolved
    );
    assert_eq!(
        sum(&prod, |c| c.unresolved) + sum(&test, |c| c.unresolved),
        total.unresolved
    );
}

#[test]
fn empty_snapshot_yields_zero_total_and_no_scopes() {
    let s = StorageConnection::open_in_memory().unwrap();
    s.connection()
        .execute_batch(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
               VALUES ('r1', 'r', '/tmp/r1', '2024-01-01T00:00:00Z'); \
             INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at) \
               VALUES ('empty', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z');",
        )
        .unwrap();
    let total = s.query_call_resolution_total("empty").unwrap();
    assert_eq!(tuple(&total), (0, 0, 0, 0));
    // No symbols -> nothing to seed -> no scopes (an empty snapshot, honestly empty).
    assert!(s
        .query_call_resolution_by_language("empty")
        .unwrap()
        .is_empty());
    assert!(s
        .query_call_resolution_by_module("empty")
        .unwrap()
        .is_empty());
}
