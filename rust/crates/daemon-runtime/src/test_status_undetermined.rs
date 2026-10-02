//! TEST-EDGE-SCOPE-1A — the per-surface UNDETERMINED test-status blocks (RG-REQ-001-L07,
//! RG-REQ-002-L11; D-TESA-DERIVED-1, D-TESA-05, D-TESA-11, D-TESA-13).
//!
//! Every surface that partitions by test status attaches ONE same-shaped block
//! (`repo_graph_agent::UndeterminedTestFiles`) naming its universe, computed by the ONE
//! shared function (`repo_graph_classification::test_path::undetermined_test_word`, applied
//! by [`block`]; the agent's `UndeterminedTestFiles::over_partition` applies the same function
//! to the agent-built blocks, pinned equal by test) over EXACTLY the rows the surface
//! already places in its production partition — so a stated count and its denominator can
//! never disagree with the partition:
//!
//! - `universe_count` — every production-partition path the surface keeps, unknown-flag
//!   rows included;
//! - `count`/`paths` — the paths whose flag is KNOWN false and the function marks;
//! - `unknown_count` — the paths whose flag is unknown (never evaluated, never read as
//!   false); each surface keeps its own unknown disclosure unchanged.
//!
//! A read the block needs that failed is `unavailable` with its reason — never a zero.
//! Nothing is stored. The per-surface row extraction lives here (one function per
//! partition shape) so each handler adds one call and the extraction is unit-tested.

use std::collections::{BTreeMap, HashMap};

use repo_graph_agent::{
    AgentDirectoryGroup, DirGroup, PackageGroup, TestStatusUniverse, TrackedFileTestFlag,
    UndeterminedTestFiles,
};

use crate::http_surface_union::UnifiedHttpSurface;
use crate::test_composition::COMPOSITION_KEY;

/// The shared block over a surface's production-partition rows (`(path, flag)`; `None` =
/// the stored flag is unknown).
pub(crate) fn block<'a, I>(universe: TestStatusUniverse, rows: I) -> UndeterminedTestFiles
where
    I: IntoIterator<Item = (&'a str, Option<bool>)>,
{
    // path -> the flag every row agrees on (`None` = unknown, or rows that disagree).
    let mut flags: BTreeMap<&str, Option<bool>> = BTreeMap::new();
    for (path, flag) in rows {
        flags
            .entry(path)
            .and_modify(|seen| {
                if *seen != flag {
                    *seen = None;
                }
            })
            .or_insert(flag);
    }
    let mut paths: Vec<String> = Vec::new();
    let mut unknown_count = 0u64;
    for (path, flag) in &flags {
        match flag {
            // An unknown flag is never evaluated and never read as false.
            None => unknown_count += 1,
            Some(is_test) => {
                if repo_graph_classification::test_path::undetermined_test_word(path, *is_test)
                    .is_some()
                {
                    paths.push((*path).to_string());
                }
            }
        }
    }
    UndeterminedTestFiles::Counted {
        count: paths.len() as u64,
        paths,
        universe,
        universe_count: flags.len() as u64,
        unknown_count,
    }
}

/// For a surface whose production partition is "every row not positively test"
/// (`is_test != Some(true)` — S5–S10): drop the positively-test rows, keep the rest
/// (unknown flags included) as the universe.
pub(crate) fn production_block<'a, I>(
    universe: TestStatusUniverse,
    rows: I,
) -> UndeterminedTestFiles
where
    I: IntoIterator<Item = (&'a str, Option<bool>)>,
{
    block(
        universe,
        rows.into_iter().filter(|(_, flag)| *flag != Some(true)),
    )
}

/// The block's wire JSON — built field by field (no fallible serializer on a render path).
pub(crate) fn to_json(b: &UndeterminedTestFiles) -> serde_json::Value {
    match b {
        UndeterminedTestFiles::Counted {
            count,
            paths,
            universe,
            universe_count,
            unknown_count,
        } => serde_json::json!({
            "count": count,
            "paths": paths,
            "universe": universe_name(*universe),
            "universe_count": universe_count,
            "unknown_count": unknown_count,
        }),
        UndeterminedTestFiles::Unavailable {
            unavailable,
            universe,
        } => serde_json::json!({
            "unavailable": unavailable,
            "universe": universe_name(*universe),
        }),
    }
}

/// The snake_case wire name of a universe (the `TestStatusUniverse` serde names).
fn universe_name(u: TestStatusUniverse) -> &'static str {
    match u {
        TestStatusUniverse::RankedFiles => "ranked_files",
        TestStatusUniverse::HotspotFiles => "hotspot_files",
        TestStatusUniverse::CallFiles => "call_files",
        TestStatusUniverse::SurfaceFiles => "surface_files",
        TestStatusUniverse::BoundaryFiles => "boundary_files",
        TestStatusUniverse::InferenceFiles => "inference_files",
        TestStatusUniverse::CandidateFiles => "candidate_files",
        TestStatusUniverse::OwnedFiles => "owned_files",
        TestStatusUniverse::GroupedFiles => "grouped_files",
        TestStatusUniverse::CrossModuleImporters => "cross_module_importers",
        TestStatusUniverse::CrossDirectoryImporters => "cross_directory_importers",
    }
}

// ── S3 hotspots --exclude-tests ─────────────────────────────────────────────

/// The kept hotspot rows (after `--exclude-tests` dropped the test files). Every kept row
/// is a churn entry already filtered to tracked paths, so each flag is known; a kept path
/// absent from the tracked-files read is an inconsistency — the block is `unavailable`
/// naming it, never a false flag.
pub(crate) fn hotspot_block(
    kept_paths: &[&str],
    flag_by_path: &HashMap<&str, bool>,
) -> UndeterminedTestFiles {
    let mut rows = Vec::with_capacity(kept_paths.len());
    for p in kept_paths {
        match flag_by_path.get(p) {
            Some(f) => rows.push((*p, Some(*f))),
            None => {
                return UndeterminedTestFiles::unavailable(
                    TestStatusUniverse::HotspotFiles,
                    format!(
                        "kept hotspot {p} has no tracked-file row, so its test status is unknown"
                    ),
                )
            }
        }
    }
    block(TestStatusUniverse::HotspotFiles, rows)
}

// ── S4 reliability ──────────────────────────────────────────────────────────

/// The non-test source files whose calls `reliability` counts (the storage read
/// `query_call_source_file_test_flags`), or the failed read's reason.
pub(crate) fn call_files_block(read: Result<&[(String, bool)], String>) -> UndeterminedTestFiles {
    match read {
        Ok(rows) => block(
            TestStatusUniverse::CallFiles,
            rows.iter()
                .filter(|(_, is_test)| !is_test)
                .map(|(p, t)| (p.as_str(), Some(*t))),
        ),
        Err(reason) => UndeterminedTestFiles::unavailable(
            TestStatusUniverse::CallFiles,
            format!("call-source files could not be read: {reason}"),
        ),
    }
}

// ── S5/S6 HTTP surfaces (`surfaces list` and orient's HTTP line) ────────────

/// The unified HTTP rows' production partition (`is_test != Some(true)`; unknown stays) —
/// the SAME rows `surfaces list` and orient's HTTP line state, so the two blocks agree.
pub(crate) fn http_surface_block(rows: &[UnifiedHttpSurface]) -> UndeterminedTestFiles {
    production_block(
        TestStatusUniverse::SurfaceFiles,
        rows.iter().map(|r| (r.source_file.as_str(), r.is_test)),
    )
}

// ── S7 boundaries list ──────────────────────────────────────────────────────

/// The `boundaries list` result rows in its MAIN partition (every row whose
/// `test_composition` is not `test_only`); `production` → known false, `unknown` → unknown.
pub(crate) fn boundary_list_block(results: &[serde_json::Value]) -> UndeterminedTestFiles {
    let rows = results.iter().filter_map(|r| {
        let path = r.get("sourceFile").and_then(|v| v.as_str())?;
        match r.get(COMPOSITION_KEY).and_then(|v| v.as_str()) {
            Some("test_only") => None,
            Some("production") => Some((path, Some(false))),
            // `unknown` — or a row carrying no composition — is never read as production.
            _ => Some((path, None)),
        }
    });
    block(TestStatusUniverse::BoundaryFiles, rows)
}

// ── S8 boundaries summary ───────────────────────────────────────────────────

/// The reconciled summary rows (non-HTTP boundary rows + unified HTTP rows) with their
/// stored flags; the headline keeps every row not positively test-only.
pub(crate) fn boundary_summary_block<'a, I>(rows: I) -> UndeterminedTestFiles
where
    I: IntoIterator<Item = (&'a str, Option<bool>)>,
{
    production_block(TestStatusUniverse::BoundaryFiles, rows)
}

// ── S9/S10 inferences list, `dead` causes ───────────────────────────────────

/// Inference rows as `(path from the target stable key, stored flag)`; the production
/// partition is `is_test != Some(true)`; a key yielding no path is outside the file
/// universe (it stays listed).
pub(crate) fn inference_block<'a, I>(rows: I) -> UndeterminedTestFiles
where
    I: IntoIterator<Item = (Option<&'a str>, Option<bool>)>,
{
    production_block(
        TestStatusUniverse::InferenceFiles,
        rows.into_iter().filter_map(|(p, f)| p.map(|p| (p, f))),
    )
}

// ── S11 semantic seeds (`find` tier, callers/callees/path fallback) ─────────

/// The semantic candidates' production partition (`is_test == false`; the daemon reads
/// `seed_vectors.is_test` as a non-null integer, so every flag is known).
pub(crate) fn candidate_block<'a, I>(candidates: I) -> UndeterminedTestFiles
where
    I: IntoIterator<Item = (&'a str, bool)>,
{
    block(
        TestStatusUniverse::CandidateFiles,
        candidates
            .into_iter()
            .filter(|(_, is_test)| !is_test)
            .map(|(p, t)| (p, Some(t))),
    )
}

// ── S12 modules list / show ─────────────────────────────────────────────────

/// Owned files (INNER JOIN to `files`, strict flag) whose `is_test` is false.
pub(crate) fn owned_files_block<'a, I>(owned: I) -> UndeterminedTestFiles
where
    I: IntoIterator<Item = (&'a str, bool)>,
{
    block(
        TestStatusUniverse::OwnedFiles,
        owned
            .into_iter()
            .filter(|(_, is_test)| !is_test)
            .map(|(p, t)| (p, Some(t))),
    )
}

// ── S13 stats package groups (D-TESA-11) ────────────────────────────────────

/// Join the `stats` directory rows (the fold input: `module` path + `file_count`) to the
/// STORED directory groups BY PATH. Both reads keep exactly the MODULE nodes that own at
/// least one file, so two successful reads must agree: the same paths and, per path, the
/// same file count. Each directory's test count is its matching stored row's
/// `test_file_count` — a zero only where that row says zero. Any disagreement (a stats row
/// with no stored row, a stored row with no stats row, differing file counts) is an error
/// naming the first differing path — never a zero, never a guessed count.
pub(crate) fn stats_dir_groups(
    stats_rows: &[(String, u64)],
    stored: &[AgentDirectoryGroup],
) -> Result<Vec<DirGroup>, String> {
    let by_path: BTreeMap<&str, &AgentDirectoryGroup> =
        stored.iter().map(|g| (g.path.as_str(), g)).collect();
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(stats_rows.len());
    for (path, file_count) in stats_rows {
        let Some(g) = by_path.get(path.as_str()) else {
            return Err(format!(
                "package-group test counts: directory {path} ({file_count} files in stats) has no \
                 stored directory group"
            ));
        };
        if g.file_count != *file_count {
            return Err(format!(
                "package-group test counts: directory {path} has {file_count} files in stats but \
                 {} in the stored directory groups",
                g.file_count
            ));
        }
        seen.insert(path.as_str());
        out.push(DirGroup {
            path: path.clone(),
            file_count: *file_count,
            test_file_count: g.test_file_count,
        });
    }
    if let Some(g) = stored.iter().find(|g| !seen.contains(g.path.as_str())) {
        return Err(format!(
            "package-group test counts: stored directory group {} ({} files) has no stats row",
            g.path, g.file_count
        ));
    }
    Ok(out)
}

/// The `grouped_files` block for folded package groups: the shared function over the
/// tracked files that have a FILE node (every FILE node is OWNS-owned), against
/// `Σ file_count − Σ test_file_count` — the SAME `UndeterminedTestFiles::over_grouped_files`
/// orient's module summary calls. A failed tracked-flags read is `unavailable`.
pub(crate) fn grouped_block(
    groups: &[PackageGroup],
    flags: Result<&[TrackedFileTestFlag], &str>,
) -> UndeterminedTestFiles {
    match flags {
        Ok(flags) => UndeterminedTestFiles::over_grouped_files(
            flags
                .iter()
                .filter(|f| f.has_file_node)
                .map(|f| (f.path.as_str(), f.is_test)),
            groups.iter().map(|g| g.file_count).sum(),
            groups.iter().map(|g| g.test_file_count).sum(),
        ),
        Err(reason) => UndeterminedTestFiles::unavailable(
            TestStatusUniverse::GroupedFiles,
            format!("tracked-file test flags could not be read: {reason}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_surface_union::UnifiedHttpSurface;
    use crate::test_composition::TestComposition;
    use serde_json::json;

    fn counted(b: &UndeterminedTestFiles) -> (u64, Vec<String>, u64, u64) {
        match b {
            UndeterminedTestFiles::Counted {
                count,
                paths,
                universe_count,
                unknown_count,
                ..
            } => (*count, paths.clone(), *universe_count, *unknown_count),
            other => panic!("expected a counted block, got {other:?}"),
        }
    }

    /// The shared function over (path, known-false) rows — the independent oracle.
    fn marked<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut v: Vec<String> = paths
            .into_iter()
            .filter(|p| {
                repo_graph_classification::test_path::undetermined_test_word(p, false).is_some()
            })
            .map(str::to_string)
            .collect();
        v.sort();
        v.dedup();
        v
    }

    fn http(file: &str, is_test: Option<bool>) -> UnifiedHttpSurface {
        UnifiedHttpSurface {
            direction: "provider".into(),
            http_method: "GET".into(),
            route: Some(format!("/{file}")),
            source_file: file.into(),
            line: None,
            is_test,
            framework: None,
            route_unknown_reason: None,
            module: None,
            provenance: vec!["boundary"],
            conflict: None,
        }
    }

    // ── the block ────────────────────────────────────────────────────────────

    #[test]
    fn block_counts_distinct_production_paths_the_shared_function_marks() {
        let b = block(
            TestStatusUniverse::OwnedFiles,
            [
                ("util/testutil.cc", Some(false)),
                ("util/testutil.cc", Some(false)),
                ("db/c_test.c", Some(false)),
                ("db/db_impl.cc", Some(false)),
            ],
        );
        assert_eq!(
            counted(&b),
            (
                2,
                vec!["db/c_test.c".into(), "util/testutil.cc".into()],
                3,
                0
            )
        );
        // The agent-side computation (complexity, module summary) agrees row for row.
        let rows = [
            ("util/testutil.cc", Some(false)),
            ("db/c_test.c", Some(false)),
            ("db/db_impl.cc", Some(false)),
            ("x/unknown_test.c", None),
            ("x/unknown_test.c", Some(false)),
        ];
        assert_eq!(
            block(TestStatusUniverse::OwnedFiles, rows),
            UndeterminedTestFiles::over_partition(TestStatusUniverse::OwnedFiles, rows)
        );
    }

    #[test]
    fn block_is_zero_with_empty_paths_when_the_function_marks_none() {
        let b = block(
            TestStatusUniverse::OwnedFiles,
            [
                ("db/db_impl.cc", Some(false)),
                ("src/contest.c", Some(false)),
            ],
        );
        assert_eq!(counted(&b), (0, Vec::new(), 2, 0));
        assert_eq!(to_json(&b)["count"], 0, "attached at zero, never dropped");
        assert_eq!(to_json(&b)["paths"], json!([]));
    }

    #[test]
    fn block_names_a_failed_read_as_unavailable_with_its_universe() {
        let b = call_files_block(Err("db locked".into()));
        assert_eq!(
            to_json(&b),
            json!({
                "unavailable": "call-source files could not be read: db locked",
                "universe": "call_files"
            })
        );
        assert!(to_json(&b).get("count").is_none(), "never a zero");
    }

    #[test]
    fn block_carries_its_universe_and_universe_count() {
        for (u, name) in [
            (TestStatusUniverse::RankedFiles, "ranked_files"),
            (TestStatusUniverse::HotspotFiles, "hotspot_files"),
            (TestStatusUniverse::CallFiles, "call_files"),
            (TestStatusUniverse::SurfaceFiles, "surface_files"),
            (TestStatusUniverse::BoundaryFiles, "boundary_files"),
            (TestStatusUniverse::InferenceFiles, "inference_files"),
            (TestStatusUniverse::CandidateFiles, "candidate_files"),
            (TestStatusUniverse::OwnedFiles, "owned_files"),
            (TestStatusUniverse::GroupedFiles, "grouped_files"),
        ] {
            let b = block(u, [("a.c", Some(false)), ("b/test.c", Some(false))]);
            let j = to_json(&b);
            assert_eq!(j["universe"], name);
            assert_eq!(j["universe_count"], 2);
            // `to_json` and the DTO's serde agree field for field.
            assert_eq!(j, serde_json::to_value(&b).unwrap());
        }
    }

    #[test]
    fn block_universe_keeps_unknown_status_rows_and_tallies_them_separately() {
        let b = block(
            TestStatusUniverse::SurfaceFiles,
            [
                ("util/testutil.cc", Some(false)),
                ("db/db_impl.cc", Some(false)),
                ("src/unknown_test.c", None),
            ],
        );
        let (count, paths, universe, unknown) = counted(&b);
        assert_eq!((count, universe, unknown), (1, 3, 1));
        assert_eq!(paths, vec!["util/testutil.cc".to_string()]);
        assert!(
            !paths.contains(&"src/unknown_test.c".to_string()),
            "an unknown flag is never evaluated as false"
        );
    }

    // ── S3 hotspots ──────────────────────────────────────────────────────────

    #[test]
    fn hotspots_exclude_tests_block_counts_kept_rows() {
        // The kept rows after `--exclude-tests` (util/testutil.h, a test file, was dropped).
        let flags: HashMap<&str, bool> = [
            ("util/testutil.cc", false),
            ("util/testutil.h", true),
            ("db/db_impl.cc", false),
        ]
        .into_iter()
        .collect();
        let kept = ["db/db_impl.cc", "util/testutil.cc"];
        let b = hotspot_block(&kept, &flags);
        assert_eq!(counted(&b), (1, marked(kept), 2, 0));
        assert_eq!(to_json(&b)["universe"], "hotspot_files");
    }

    #[test]
    fn hotspots_block_is_unavailable_when_a_kept_path_has_no_tracked_row() {
        let flags: HashMap<&str, bool> = [("db/db_impl.cc", false)].into_iter().collect();
        let b = hotspot_block(&["db/db_impl.cc", "gone/test_x.c"], &flags);
        let j = to_json(&b);
        assert_eq!(j["universe"], "hotspot_files");
        assert!(
            j["unavailable"].as_str().unwrap().contains("gone/test_x.c"),
            "{j}"
        );
    }

    // ── S4 reliability ───────────────────────────────────────────────────────

    #[test]
    fn reliability_block_counts_undetermined_call_files() {
        // leveldb-shaped: the non-test call-source files are the universe.
        let rows = vec![
            ("db/c_test.c".to_string(), false),
            ("util/testutil.cc".to_string(), false),
            ("db/db_impl.cc".to_string(), false),
            ("db/db_test.cc".to_string(), true),
        ];
        let b = call_files_block(Ok(&rows));
        let prod: Vec<&str> = rows
            .iter()
            .filter(|(_, t)| !t)
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(counted(&b), (2, marked(prod.clone()), prod.len() as u64, 0));
    }

    // ── S5/S6 HTTP ───────────────────────────────────────────────────────────

    #[test]
    fn surfaces_block_counts_production_rows_only() {
        let rows = vec![
            http("api/test_routes.py", Some(false)),
            http("api/routes.py", Some(false)),
            http("tests/test_api.py", Some(true)),
            http("api/testing_views.py", None),
        ];
        let b = http_surface_block(&rows);
        // Production = not positively test: 3 files; the unknown one is tallied, not counted.
        assert_eq!(
            counted(&b),
            (1, vec!["api/test_routes.py".to_string()], 3, 1)
        );
    }

    #[test]
    fn orient_http_block_equals_the_surfaces_list_block() {
        let rows = vec![
            http("api/test_routes.py", Some(false)),
            http("api/routes.py", Some(false)),
            http("tests/test_api.py", Some(true)),
            http("api/x.py", None),
        ];
        let orient = crate::orient_additive_fields::http_surfaces_headline_json(&rows);
        assert_eq!(
            orient["test_status_undetermined"],
            to_json(&http_surface_block(&rows)),
            "orient's HTTP line and `surfaces list` state the same block (RG-REQ-002-L02)"
        );
        // The existing production counts are unchanged beside it.
        assert_eq!(orient["total"], 3);
        assert_eq!(orient["test_fixture_excluded"], 1);
        assert_eq!(orient["test_status_unknown"], 1);
    }

    // ── S7/S8 boundaries ─────────────────────────────────────────────────────

    fn boundary_row(file: &str, fact: Option<bool>) -> serde_json::Value {
        let mut v = json!({"sourceFile": file, "channelKind": "db"});
        TestComposition::from_is_test_fact(fact, file).write_json(&mut v);
        v
    }

    #[test]
    fn boundaries_list_block_counts_main_partition_rows() {
        let results = vec![
            boundary_row("src/test_db.c", Some(false)),
            boundary_row("src/db.c", Some(false)),
            boundary_row("tests/db_fixture.c", Some(true)),
            boundary_row("src/tester_db.c", None),
        ];
        let b = boundary_list_block(&results);
        assert_eq!(
            counted(&b),
            (1, vec!["src/test_db.c".to_string()], 3, 1),
            "test-only rows are demoted out; unknown rows stay in the universe, tallied"
        );
        assert_eq!(to_json(&b)["universe"], "boundary_files");
    }

    #[test]
    fn boundaries_summary_block_counts_headline_surfaces() {
        // The reconciled summary rows: a non-HTTP boundary row + unified HTTP rows.
        let rows: Vec<(&str, Option<bool>)> = vec![
            ("src/test_db.c", Some(false)),
            ("tests/db_fixture.c", Some(true)),
            ("api/testing_views.py", Some(false)),
            ("api/y.py", None),
        ];
        let b = boundary_summary_block(rows.clone());
        let headline: Vec<&str> = rows
            .iter()
            .filter(|(_, f)| *f == Some(false))
            .map(|(p, _)| *p)
            .collect();
        assert_eq!(counted(&b), (2, marked(headline), 3, 1));
    }

    // ── S11 semantic seeds ───────────────────────────────────────────────────

    fn cand(path: &str, is_test: bool) -> crate::seed::SemanticCandidate {
        crate::seed::SemanticCandidate {
            stable_key: format!("r:{path}#f:SYMBOL:FUNCTION"),
            path: path.into(),
            line: Some(1),
            qualified_name: Some("f".into()),
            is_test,
            is_decl: false,
            is_field: false,
            module: repo_graph_agent::dto::envelope::ModuleHint::Unavailable("none".into()),
            score: 0.5,
            model_id: "m".into(),
        }
    }

    fn fired() -> crate::seed::SemanticResult {
        crate::seed::SemanticResult::Fired {
            candidates: vec![
                cand("util/testutil.cc", false),
                cand("db/db_impl.cc", false),
                cand("util/testutil.h", true),
            ],
            stale_count: 0,
            total: 3,
        }
    }

    #[test]
    fn find_seed_block_counts_production_candidates() {
        let resp = crate::seed::build_find_response("r", "r", "s", "q", &[], Some(fired()), None);
        let j = serde_json::to_value(&resp).unwrap();
        assert_eq!(
            j["test_status_undetermined"],
            json!({"count": 1, "paths": ["util/testutil.cc"], "universe": "candidate_files",
                   "universe_count": 2, "unknown_count": 0})
        );
        // `--exact` (tier not consulted): no candidate partition, no block.
        let exact = crate::seed::build_find_response("r", "r", "s", "q", &[], None, None);
        assert!(serde_json::to_value(&exact)
            .unwrap()
            .get("test_status_undetermined")
            .is_none());
    }

    #[test]
    fn seed_fallback_block_counts_production_candidates() {
        let data = crate::seed::build_group_b_data("callers", fired(), None);
        assert_eq!(
            data["test_status_undetermined"],
            json!({"count": 1, "paths": ["util/testutil.cc"], "universe": "candidate_files",
                   "universe_count": 2, "unknown_count": 0})
        );
    }

    // ── S12 modules ──────────────────────────────────────────────────────────

    #[test]
    fn modules_list_block_counts_owned_production_files() {
        let owned = [
            ("util/testutil.cc", false),
            ("util/testutil.h", true),
            ("db/c_test.c", false),
            ("db/db_impl.cc", false),
        ];
        let b = owned_files_block(owned);
        assert_eq!(
            counted(&b),
            (
                2,
                vec!["db/c_test.c".into(), "util/testutil.cc".into()],
                3,
                0
            )
        );
        assert_eq!(to_json(&b)["universe"], "owned_files");
    }

    #[test]
    fn modules_show_block_counts_the_modules_owned_production_files() {
        // (path, module, is_test) — `modules show util` counts only util's owned files.
        let owned = [
            ("util/testutil.cc", "util", false),
            ("util/env.cc", "util", false),
            ("util/testutil.h", "util", true),
            ("db/c_test.c", "db", false),
        ];
        let b = owned_files_block(
            owned
                .iter()
                .filter(|(_, m, _)| *m == "util")
                .map(|(p, _, t)| (*p, *t)),
        );
        assert_eq!(counted(&b), (1, vec!["util/testutil.cc".to_string()], 2, 0));
    }

    // ── S13 stats package groups (D-TESA-11) ─────────────────────────────────

    fn stored(path: &str, files: u64, tests: u64) -> AgentDirectoryGroup {
        AgentDirectoryGroup {
            path: path.into(),
            file_count: files,
            test_file_count: tests,
        }
    }

    #[test]
    fn stats_package_group_test_counts_read_the_stored_directory_groups() {
        let rows = vec![
            ("Foundation/testsuite/src".to_string(), 12),
            ("Foundation/src".to_string(), 30),
        ];
        let st = vec![
            stored("Foundation/src", 30, 0),
            stored("Foundation/testsuite/src", 12, 12),
        ];
        let dirs = stats_dir_groups(&rows, &st).unwrap();
        assert_eq!(
            dirs,
            vec![
                DirGroup {
                    path: "Foundation/testsuite/src".into(),
                    file_count: 12,
                    test_file_count: 12
                },
                DirGroup {
                    path: "Foundation/src".into(),
                    file_count: 30,
                    test_file_count: 0
                },
            ],
            "names and file counts from the stats rows; test counts from the stored rows"
        );
    }

    #[test]
    fn stats_errors_when_stored_directory_groups_disagree_with_its_rows() {
        let st = vec![stored("db", 40, 9), stored("util", 20, 3)];
        // (a) a stats row with no stored row.
        let e = stats_dir_groups(
            &[("db".into(), 40), ("util".into(), 20), ("port".into(), 5)],
            &st,
        )
        .unwrap_err();
        assert!(
            e.contains("port") && e.contains("no stored directory group"),
            "{e}"
        );
        // (b) a stored row with no stats row.
        let e = stats_dir_groups(&[("db".into(), 40)], &st).unwrap_err();
        assert!(e.contains("util") && e.contains("has no stats row"), "{e}");
        // (c) a file-count mismatch on one path.
        let e = stats_dir_groups(&[("db".into(), 41), ("util".into(), 20)], &st).unwrap_err();
        assert!(
            e.contains("db") && e.contains("41") && e.contains("40"),
            "{e}"
        );
    }

    #[test]
    fn stats_zero_test_count_comes_only_from_a_matching_stored_zero_row() {
        let dirs = stats_dir_groups(&[("port".into(), 6)], &[stored("port", 6, 0)]).unwrap();
        assert_eq!(dirs[0].test_file_count, 0, "a stored row that says zero");
        // The same directory with no stored row is the mismatch error — never a zero.
        assert!(stats_dir_groups(&[("port".into(), 6)], &[]).is_err());
    }

    #[test]
    fn stats_block_counts_undetermined_grouped_files() {
        let groups = vec![
            PackageGroup {
                name: "db".into(),
                file_count: 3,
                test_file_count: 1,
            },
            PackageGroup {
                name: "util".into(),
                file_count: 3,
                test_file_count: 1,
            },
        ];
        let flag = |p: &str, t: bool, n: bool| TrackedFileTestFlag {
            path: p.into(),
            is_test: t,
            has_file_node: n,
        };
        let flags = vec![
            flag("db/c_test.c", false, true),
            flag("db/db_impl.cc", false, true),
            flag("db/db_test.cc", true, true),
            flag("util/testutil.cc", false, true),
            flag("util/testutil.h", true, true),
            flag("util/env.cc", false, true),
            flag("integration-tests/pom.xml", false, false),
        ];
        let b = grouped_block(&groups, Ok(&flags));
        assert_eq!(
            to_json(&b),
            json!({"count": 2, "paths": ["db/c_test.c", "util/testutil.cc"],
                   "universe": "grouped_files", "universe_count": 4, "unknown_count": 0})
        );
        // The SAME computation orient's module summary uses.
        assert_eq!(
            b,
            UndeterminedTestFiles::over_grouped_files(
                flags
                    .iter()
                    .filter(|f| f.has_file_node)
                    .map(|f| (f.path.as_str(), f.is_test)),
                6,
                2
            )
        );
        let u = grouped_block(&groups, Err("disk I/O error"));
        assert_eq!(to_json(&u)["universe"], "grouped_files");
        assert!(to_json(&u)["unavailable"]
            .as_str()
            .unwrap()
            .contains("disk I/O error"));
    }
}
