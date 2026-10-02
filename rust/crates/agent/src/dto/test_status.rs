//! TEST-EDGE-SCOPE-1A — the UNDETERMINED test-status DTOs (RG-REQ-001-L07,
//! RG-REQ-002-L11; D-TESA-DERIVED-1, D-TESA-05, D-TESA-13).
//!
//! A tracked file whose path carries a test word, matches no test convention and
//! whose stored `files.is_test` is false has test status UNDETERMINED. It stays in
//! the production partition; every surface that partitions by test status states,
//! against its own universe, how many of its production files are undetermined.
//! Nothing is stored: the status is computed when asked by
//! `repo_graph_classification::test_path::undetermined_test_word`, over exactly the
//! rows a surface already partitions ([`UndeterminedTestFiles::over_partition`]).

use std::collections::BTreeSet;

use serde::Serialize;

/// The universe a surface's undetermined count is stated against (closed set; the
/// renderer maps each to its reader noun). Serialized snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatusUniverse {
    /// Files of the production complexity ranking (orient `HIGH_COMPLEXITY`).
    RankedFiles,
    /// Files `hotspots --exclude-tests` keeps.
    HotspotFiles,
    /// Non-test files whose calls `reliability` counts.
    CallFiles,
    /// Files of the production HTTP surfaces (`surfaces list`, orient's HTTP line).
    SurfaceFiles,
    /// Files of the production boundary rows (`boundaries list` / `summary`).
    BoundaryFiles,
    /// Files of the production inferences (`inferences list`, `dead` causes).
    InferenceFiles,
    /// Files of the production semantic candidates (`find` tier, semantic fallback).
    CandidateFiles,
    /// Owned non-test files (`modules list` / `modules show`).
    OwnedFiles,
    /// Grouped non-test files (the `stats` / orient package groups).
    GroupedFiles,
    /// TEST-EDGE-SCOPE-1B (D-TESB-09): production files whose imports cross module
    /// candidates in the view (`modules list`, `modules deps`, orient's module-edges line).
    CrossModuleImporters,
    /// TEST-EDGE-SCOPE-1B (D-TESB-09): production files whose imports cross directory
    /// modules in the view (`cycles`, orient's cycle line, explain's Import-cycles block).
    CrossDirectoryImporters,
}

/// One surface's undetermined-file block, ONE shape for every surface:
/// `{"count":N,"paths":[…],"universe":u,"universe_count":M,"unknown_count":K}` or,
/// only when a read the block needs failed, `{"unavailable":reason,"universe":u}`.
///
/// - `universe_count` (M) — the distinct paths of every production-partition row the
///   surface keeps, unknown-flag rows included;
/// - `count`/`paths` (N) — the distinct paths whose flag is KNOWN false and for which
///   the shared function holds (sorted, complete — never capped);
/// - `unknown_count` (K) — the distinct paths of the universe whose flag is unknown;
///   never evaluated, never read as false.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum UndeterminedTestFiles {
    Counted {
        count: u64,
        paths: Vec<String>,
        universe: TestStatusUniverse,
        universe_count: u64,
        unknown_count: u64,
    },
    Unavailable {
        unavailable: String,
        universe: TestStatusUniverse,
    },
}

impl UndeterminedTestFiles {
    /// THE one computation (D-TESA-05, D-TESA-13): `rows` are the `(path, flag)`
    /// pairs of the surface's production partition exactly as the surface partitions
    /// them (`None` = the stored flag is unknown). A path seen with differing flags
    /// counts as unknown (never read as false).
    pub fn over_partition<'a, I>(universe: TestStatusUniverse, rows: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, Option<bool>)>,
    {
        // path -> Some(flag) when every row agrees on a known flag; None otherwise.
        let mut flags: std::collections::BTreeMap<&str, Option<bool>> =
            std::collections::BTreeMap::new();
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
        let mut paths = BTreeSet::new();
        let mut unknown = 0u64;
        for (path, flag) in &flags {
            match flag {
                None => unknown += 1,
                Some(is_test) => {
                    if repo_graph_classification::test_path::undetermined_test_word(path, *is_test)
                        .is_some()
                    {
                        paths.insert((*path).to_string());
                    }
                }
            }
        }
        UndeterminedTestFiles::Counted {
            count: paths.len() as u64,
            paths: paths.into_iter().collect(),
            universe,
            universe_count: flags.len() as u64,
            unknown_count: unknown,
        }
    }

    /// The package-group block (`grouped_files`, D-TESA-13 S13): `file_node_rows` are
    /// the snapshot's tracked files that have a FILE node, with their KNOWN stored flag
    /// (every FILE node is OWNS-owned, so they are the grouped files); the count is the
    /// shared function over them; the universe is the groups' non-test files,
    /// `Σ file_count − Σ test_file_count`. Grouped files come through a `files` join
    /// that errors on an unknown flag, so `unknown_count` is 0. The ONE computation
    /// behind both `orient`'s and `stats`' package-group line (RG-REQ-002-L02).
    pub fn over_grouped_files<'a, I>(
        file_node_rows: I,
        group_file_total: u64,
        group_test_total: u64,
    ) -> Self
    where
        I: IntoIterator<Item = (&'a str, bool)>,
    {
        let paths: BTreeSet<String> = file_node_rows
            .into_iter()
            .filter(|(path, is_test)| {
                repo_graph_classification::test_path::undetermined_test_word(path, *is_test)
                    .is_some()
            })
            .map(|(path, _)| path.to_string())
            .collect();
        UndeterminedTestFiles::Counted {
            count: paths.len() as u64,
            paths: paths.into_iter().collect(),
            universe: TestStatusUniverse::GroupedFiles,
            universe_count: group_file_total.saturating_sub(group_test_total),
            unknown_count: 0,
        }
    }

    /// A read the block needs failed: state it as unknown with its reason — never
    /// a zero, never a dropped key (RG-REQ-002-L04).
    pub fn unavailable(universe: TestStatusUniverse, reason: impl Into<String>) -> Self {
        UndeterminedTestFiles::Unavailable {
            unavailable: reason.into(),
            universe,
        }
    }
}

/// The state an [`ExplainUndeterminedTestStatus`] reports. One variant, serialized
/// `"undetermined"` (the `MemberIdentityState` precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatusState {
    Undetermined,
}

/// `explain <file>`'s test status for an UNDETERMINED file, flattened into the
/// identity evidence as `test_status: "undetermined"` + `test_status_word` (the test
/// word that makes the path look like test code). Absent for a determined file, so
/// its identity JSON is byte-identical to before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainUndeterminedTestStatus {
    pub test_status: TestStatusState,
    pub test_status_word: String,
}

impl ExplainUndeterminedTestStatus {
    /// The status of one tracked file with a KNOWN flag, through the shared function;
    /// `None` when it is determined.
    pub fn of(path: &str, is_test: bool) -> Option<Self> {
        repo_graph_classification::test_path::undetermined_test_word(path, is_test).map(|w| {
            ExplainUndeterminedTestStatus {
                test_status: TestStatusState::Undetermined,
                test_status_word: w.to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undetermined_test_files_serialize_with_count_paths_and_universe_or_unavailable() {
        let b = UndeterminedTestFiles::over_partition(
            TestStatusUniverse::OwnedFiles,
            [
                ("util/testutil.cc", Some(false)),
                ("db/db_impl.cc", Some(false)),
                ("util/testutil.cc", Some(false)),
                ("x/unknown_test.c", None),
                ("db/c_test.c", Some(false)),
            ],
        );
        assert_eq!(
            serde_json::to_value(&b).unwrap(),
            serde_json::json!({
                "count": 2,
                "paths": ["db/c_test.c", "util/testutil.cc"],
                "universe": "owned_files",
                "universe_count": 4,
                "unknown_count": 1
            })
        );
        let u = UndeterminedTestFiles::unavailable(TestStatusUniverse::CallFiles, "read failed");
        assert_eq!(
            serde_json::to_value(&u).unwrap(),
            serde_json::json!({"unavailable": "read failed", "universe": "call_files"})
        );
    }

    fn file_identity(
        status: Option<ExplainUndeterminedTestStatus>,
    ) -> crate::dto::signal::ExplainIdentityEvidence {
        crate::dto::signal::ExplainIdentityEvidence {
            target_kind: "file".to_string(),
            path: Some("util/testutil.cc".to_string()),
            stable_key: Some("r1:util/testutil.cc:FILE".to_string()),
            name: None,
            subtype: None,
            line_start: None,
            language: Some("cpp".to_string()),
            is_test: None,
            module_path: None,
            file_count: None,
            symbol_count: Some(3),
            undetermined_test_status: status,
        }
    }

    #[test]
    fn undetermined_test_status_serializes_flat_as_test_status_and_word() {
        let s = ExplainUndeterminedTestStatus::of("util/testutil.cc", false).unwrap();
        let v = serde_json::to_value(file_identity(Some(s))).unwrap();
        assert_eq!(v["test_status"], "undetermined", "{v}");
        assert_eq!(v["test_status_word"], "testutil", "{v}");
        assert!(
            v.get("undetermined_test_status").is_none(),
            "flattened, not nested: {v}"
        );
        assert_eq!(
            ExplainUndeterminedTestStatus::of("util/testutil.h", true),
            None
        );
        assert_eq!(
            ExplainUndeterminedTestStatus::of("db/db_impl.cc", false),
            None
        );
    }

    #[test]
    fn determined_identity_json_is_unchanged_by_the_test_status_field() {
        let v = serde_json::to_value(file_identity(None)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "target_kind": "file",
                "path": "util/testutil.cc",
                "stable_key": "r1:util/testutil.cc:FILE",
                "language": "cpp",
                "symbol_count": 3
            }),
            "a determined file's identity JSON is byte-identical to before"
        );
    }

    /// TEST-EDGE-SCOPE-1B (D-TESB-09): the two importer universes join 1A's closed set and
    /// serialize snake_case, computed by the same one function.
    #[test]
    fn test_status_universes_include_cross_module_and_cross_directory_importers() {
        for (u, name) in [
            (
                TestStatusUniverse::CrossModuleImporters,
                "cross_module_importers",
            ),
            (
                TestStatusUniverse::CrossDirectoryImporters,
                "cross_directory_importers",
            ),
        ] {
            assert_eq!(serde_json::to_value(u).unwrap(), serde_json::json!(name));
            let b = UndeterminedTestFiles::over_partition(
                u,
                [("db/c_test.c", Some(false)), ("db/db_impl.cc", Some(false))],
            );
            let v = serde_json::to_value(&b).unwrap();
            assert_eq!(v["universe"], name);
            assert_eq!(v["count"], 1);
            assert_eq!(v["universe_count"], 2);
        }
    }
}
