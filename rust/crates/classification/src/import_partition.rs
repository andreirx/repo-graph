//! Import partition vocabulary — pure policy (TEST-EDGE-SCOPE-1B, D-TESB-01).
//!
//! The ONE definition of how a stored IMPORTS row is classified on the two
//! axes RG-REQ-004-L12 and RG-REQ-002-L11 partition by:
//!
//! - **Resolution class** — from the stored `edges.resolution`: `static` and
//!   `dynamic` are CERTAIN, `inferred` is INFERRED, and any other value is an
//!   unreadable-resolution error naming the value (D-PSI-R1-VOCAB: an
//!   out-of-vocabulary value is never read as certain and never filtered away).
//! - **Importer test status** — from the importing file's stored `files.is_test`:
//!   `1` is TEST, `0` is PRODUCTION, an absent row is UNKNOWN (never
//!   production-by-default in the counts, never dropped), any other value is an
//!   error. A file not known to be test stays in the production partition
//!   (RG-REQ-001-L07), so UNKNOWN is admitted wherever PRODUCTION is and is only
//!   tallied separately.
//!
//! A view ([`ImportView`]) states which partitions an answer admits; the
//! default admits certain production imports only. Every excluded cell is
//! stated exactly once, under the smallest flag set that shows it
//! ([`RemainderGroup`]).
//!
//! No I/O. Consumers: the module-edge derivation (`module_edges.rs`), storage's
//! partitioned import read and file-level readers, agent, module-queries, trust
//! and the daemon's view layer.

use serde::{Deserialize, Serialize};

/// The stored resolution values this build can read, in vocabulary order.
/// `static` and `dynamic` are certain; `inferred` is inferred.
pub const READABLE_IMPORT_RESOLUTIONS: [&str; 3] = ["static", "dynamic", "inferred"];

/// The stored resolution values that are certain.
pub const CERTAIN_IMPORT_RESOLUTIONS: [&str; 2] = ["static", "dynamic"];

/// Resolution class of one stored IMPORTS row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportClass {
    /// `static` or `dynamic`: the binding is supported by evidence.
    Certain,
    /// `inferred`: a candidate the index cannot determine (RG-REQ-002-L11).
    Inferred,
}

impl ImportClass {
    /// Classify a stored resolution value; any value outside
    /// [`READABLE_IMPORT_RESOLUTIONS`] is an [`ImportPartitionError::UnreadableResolution`].
    pub fn from_resolution(resolution: &str) -> Result<Self, ImportPartitionError> {
        match resolution {
            "static" | "dynamic" => Ok(ImportClass::Certain),
            "inferred" => Ok(ImportClass::Inferred),
            other => Err(ImportPartitionError::UnreadableResolution {
                value: other.to_string(),
            }),
        }
    }

    /// The wire name of the class (`certain` / `inferred`).
    pub fn as_str(self) -> &'static str {
        match self {
            ImportClass::Certain => "certain",
            ImportClass::Inferred => "inferred",
        }
    }
}

/// Test status of the importing file of one IMPORTS row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImporterStatus {
    /// `files.is_test = 0`.
    Production,
    /// `files.is_test = 1`.
    Test,
    /// No `files` row for the importing file: admitted with production, tallied apart.
    Unknown,
}

impl ImporterStatus {
    /// Classify the importing file's stored flag: `Some(1)` Test, `Some(0)` Production,
    /// `None` (no row) Unknown, any other value an
    /// [`ImportPartitionError::UnreadableImporterFlag`].
    pub fn from_is_test(flag: Option<i64>) -> Result<Self, ImportPartitionError> {
        match flag {
            Some(1) => Ok(ImporterStatus::Test),
            Some(0) => Ok(ImporterStatus::Production),
            None => Ok(ImporterStatus::Unknown),
            Some(other) => Err(ImportPartitionError::UnreadableImporterFlag { value: other }),
        }
    }

    /// Whether the importer sits in the production partition (Production or Unknown —
    /// a file not known to be test stays production, RG-REQ-001-L07).
    pub fn is_production_partition(self) -> bool {
        !matches!(self, ImporterStatus::Test)
    }
}

/// The partition of one IMPORTS row: its resolution class and its importer's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImportPartition {
    pub class: ImportClass,
    pub status: ImporterStatus,
}

impl ImportPartition {
    /// Classify one stored row from its resolution and its importer's `is_test` flag.
    pub fn classify(
        resolution: &str,
        importer_is_test: Option<i64>,
    ) -> Result<Self, ImportPartitionError> {
        Ok(ImportPartition {
            class: ImportClass::from_resolution(resolution)?,
            status: ImporterStatus::from_is_test(importer_is_test)?,
        })
    }
}

/// A row that cannot be classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportPartitionError {
    /// A stored `edges.resolution` outside [`READABLE_IMPORT_RESOLUTIONS`].
    UnreadableResolution { value: String },
    /// A stored `files.is_test` other than 0 or 1.
    UnreadableImporterFlag { value: i64 },
}

impl std::fmt::Display for ImportPartitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportPartitionError::UnreadableResolution { value } => write!(
                f,
                "unreadable import resolution {value:?} (this build reads static, dynamic and inferred)"
            ),
            ImportPartitionError::UnreadableImporterFlag { value } => write!(
                f,
                "unreadable importer test flag {value} (this build reads 0 and 1)"
            ),
        }
    }
}

impl std::error::Error for ImportPartitionError {}

/// Which partitions an answer admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ImportView {
    /// `--include-tests`: admit imports whose importer is a test file.
    pub include_tests: bool,
    /// `--include-inferred`: admit inferred imports.
    pub include_inferred: bool,
}

/// The daemon/JSON name of the `--include-tests` flag.
pub const FLAG_INCLUDE_TESTS: &str = "include_tests";
/// The daemon/JSON name of the `--include-inferred` flag.
pub const FLAG_INCLUDE_INFERRED: &str = "include_inferred";

impl ImportView {
    /// Certain production imports only — the default of every partitioned surface.
    pub const DEFAULT: ImportView = ImportView {
        include_tests: false,
        include_inferred: false,
    };
    /// Certain imports of every test status — the governance and per-file population.
    pub const CERTAIN_WITH_TESTS: ImportView = ImportView {
        include_tests: true,
        include_inferred: false,
    };
    /// Certain inferred production imports added — `--include-inferred`.
    pub const WITH_INFERRED: ImportView = ImportView {
        include_tests: false,
        include_inferred: true,
    };
    /// All four partitions.
    pub const ALL: ImportView = ImportView {
        include_tests: true,
        include_inferred: true,
    };

    /// Whether this view admits an import of the given partition.
    pub fn admits(&self, partition: ImportPartition) -> bool {
        let status_ok = partition.status.is_production_partition() || self.include_tests;
        let class_ok = partition.class == ImportClass::Certain || self.include_inferred;
        status_ok && class_ok
    }

    /// Whether `self` admits everything `other` admits and more (a strict superset of flags).
    pub fn is_strict_superset_of(&self, other: &ImportView) -> bool {
        self != other
            && (self.include_tests || !other.include_tests)
            && (self.include_inferred || !other.include_inferred)
    }

    /// The views wider than this one, in the order excluded cycles are attributed:
    /// single additions before both (D-TESB-03). Empty for [`ImportView::ALL`].
    pub fn wider_views(&self) -> Vec<ImportView> {
        [Self::CERTAIN_WITH_TESTS, Self::WITH_INFERRED, Self::ALL]
            .into_iter()
            .filter(|v| v.is_strict_superset_of(self))
            .collect()
    }

    /// The complete flag set naming this view, as daemon/JSON flag names
    /// (`include_tests`, `include_inferred`), in that order.
    pub fn flag_names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.include_tests {
            out.push(FLAG_INCLUDE_TESTS);
        }
        if self.include_inferred {
            out.push(FLAG_INCLUDE_INFERRED);
        }
        out
    }

    /// Whether this view admits every partition (nothing can be excluded).
    pub fn admits_everything(&self) -> bool {
        self.include_tests && self.include_inferred
    }
}

/// The smallest flag set, relative to a requested view, that shows an excluded cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemainderGroup {
    /// Shown by adding `--include-tests`.
    Tests,
    /// Shown by adding `--include-inferred`.
    Inferred,
    /// Shown only by adding both flags.
    TestsAndInferred,
}

impl RemainderGroup {
    /// Every group, in rendering order.
    pub const ALL: [RemainderGroup; 3] = [
        RemainderGroup::Tests,
        RemainderGroup::Inferred,
        RemainderGroup::TestsAndInferred,
    ];

    /// The JSON key of the group in `import_remainder`.
    pub fn key(self) -> &'static str {
        match self {
            RemainderGroup::Tests => "tests",
            RemainderGroup::Inferred => "inferred",
            RemainderGroup::TestsAndInferred => "tests_and_inferred",
        }
    }
}

/// The remainder group an import of `partition` falls in under `view`, or `None`
/// when the view admits it. Each excluded cell is stated exactly once, under the
/// smallest flag set that shows it: test certain → tests; production inferred →
/// inferred; test inferred → inferred when tests are already included, tests when
/// inferred ones are already included, both otherwise.
pub fn remainder_group(view: ImportView, partition: ImportPartition) -> Option<RemainderGroup> {
    if view.admits(partition) {
        return None;
    }
    let needs_tests = !partition.status.is_production_partition() && !view.include_tests;
    let needs_inferred = partition.class == ImportClass::Inferred && !view.include_inferred;
    match (needs_tests, needs_inferred) {
        (true, true) => Some(RemainderGroup::TestsAndInferred),
        (true, false) => Some(RemainderGroup::Tests),
        (false, true) => Some(RemainderGroup::Inferred),
        // `admits` returned false, so at least one axis is missing.
        (false, false) => None,
    }
}

/// Per-partition import counts of one module edge or cycle. `unknown_test_status`
/// is a tally INSIDE the two production cells (importers with no `files` row),
/// never a fifth cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PartitionCounts {
    pub production_certain: u64,
    pub test_certain: u64,
    pub production_inferred: u64,
    pub test_inferred: u64,
    pub unknown_test_status: u64,
}

impl PartitionCounts {
    /// Count one import of `partition`.
    pub fn add(&mut self, partition: ImportPartition) {
        let production = partition.status.is_production_partition();
        match (production, partition.class) {
            (true, ImportClass::Certain) => self.production_certain += 1,
            (false, ImportClass::Certain) => self.test_certain += 1,
            (true, ImportClass::Inferred) => self.production_inferred += 1,
            (false, ImportClass::Inferred) => self.test_inferred += 1,
        }
        if partition.status == ImporterStatus::Unknown {
            self.unknown_test_status += 1;
        }
    }

    /// Imports of the cells `view` admits.
    pub fn admitted(&self, view: ImportView) -> u64 {
        let mut n = self.production_certain;
        if view.include_tests {
            n += self.test_certain;
        }
        if view.include_inferred {
            n += self.production_inferred;
        }
        if view.include_tests && view.include_inferred {
            n += self.test_inferred;
        }
        n
    }

    /// Imports of all four cells.
    pub fn total(&self) -> u64 {
        self.production_certain + self.test_certain + self.production_inferred + self.test_inferred
    }

    /// Imports of the cells `view` excludes that fall in `group`.
    pub fn excluded_in(&self, view: ImportView, group: RemainderGroup) -> u64 {
        let cells = [
            (
                ImporterStatus::Production,
                ImportClass::Certain,
                self.production_certain,
            ),
            (
                ImporterStatus::Test,
                ImportClass::Certain,
                self.test_certain,
            ),
            (
                ImporterStatus::Production,
                ImportClass::Inferred,
                self.production_inferred,
            ),
            (
                ImporterStatus::Test,
                ImportClass::Inferred,
                self.test_inferred,
            ),
        ];
        cells
            .into_iter()
            .filter(|(status, class, _)| {
                remainder_group(
                    view,
                    ImportPartition {
                        class: *class,
                        status: *status,
                    },
                ) == Some(group)
            })
            .map(|(_, _, n)| n)
            .sum()
    }

    /// Add another tally cell by cell.
    pub fn merge(&mut self, other: &PartitionCounts) {
        self.production_certain += other.production_certain;
        self.test_certain += other.test_certain;
        self.production_inferred += other.production_inferred;
        self.test_inferred += other.test_inferred;
        self.unknown_test_status += other.unknown_test_status;
    }
}

/// The imports and relations a view leaves out, for one remainder group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RemainderCount {
    /// Excluded imports in this group.
    pub imports: u64,
    /// Relations (module edges) that exist ONLY through this group's imports under
    /// the requested view (absent from the view, present once this group's flags are added).
    pub edges: u64,
}

/// The excluded remainder of a view, per group (JSON `import_remainder`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImportRemainder {
    pub tests: RemainderCount,
    pub inferred: RemainderCount,
    pub tests_and_inferred: RemainderCount,
}

impl ImportRemainder {
    /// The count of one group.
    pub fn get(&self, group: RemainderGroup) -> RemainderCount {
        match group {
            RemainderGroup::Tests => self.tests,
            RemainderGroup::Inferred => self.inferred,
            RemainderGroup::TestsAndInferred => self.tests_and_inferred,
        }
    }

    /// Mutable count of one group.
    pub fn get_mut(&mut self, group: RemainderGroup) -> &mut RemainderCount {
        match group {
            RemainderGroup::Tests => &mut self.tests,
            RemainderGroup::Inferred => &mut self.inferred,
            RemainderGroup::TestsAndInferred => &mut self.tests_and_inferred,
        }
    }

    /// Whether the view excluded nothing.
    pub fn is_empty(&self) -> bool {
        RemainderGroup::ALL
            .iter()
            .all(|g| self.get(*g) == RemainderCount::default())
    }

    /// Build the remainder of a set of relations, each with its partition counts:
    /// imports per group summed over every relation, and `edges` per group counting
    /// the relations the view does NOT admit (no admitted import) that the group's
    /// flags would add (they carry an import of that group).
    ///
    /// A relation absent from the view is attributed to the smallest group that
    /// shows it (tests, inferred, then both), so each relation is counted once.
    pub fn from_relations<'a, I>(view: ImportView, relations: I) -> ImportRemainder
    where
        I: IntoIterator<Item = &'a PartitionCounts>,
    {
        let mut out = ImportRemainder::default();
        for counts in relations {
            for group in RemainderGroup::ALL {
                out.get_mut(group).imports += counts.excluded_in(view, group);
            }
            if counts.admitted(view) == 0 {
                if let Some(group) = RemainderGroup::ALL
                    .into_iter()
                    .find(|g| counts.excluded_in(view, *g) > 0)
                {
                    out.get_mut(group).edges += 1;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PC: ImportPartition = ImportPartition {
        class: ImportClass::Certain,
        status: ImporterStatus::Production,
    };
    const TC: ImportPartition = ImportPartition {
        class: ImportClass::Certain,
        status: ImporterStatus::Test,
    };
    const PI: ImportPartition = ImportPartition {
        class: ImportClass::Inferred,
        status: ImporterStatus::Production,
    };
    const TI: ImportPartition = ImportPartition {
        class: ImportClass::Inferred,
        status: ImporterStatus::Test,
    };
    const UC: ImportPartition = ImportPartition {
        class: ImportClass::Certain,
        status: ImporterStatus::Unknown,
    };
    const UI: ImportPartition = ImportPartition {
        class: ImportClass::Inferred,
        status: ImporterStatus::Unknown,
    };

    #[test]
    fn static_and_dynamic_are_certain_and_inferred_is_inferred() {
        assert_eq!(
            ImportClass::from_resolution("static"),
            Ok(ImportClass::Certain)
        );
        assert_eq!(
            ImportClass::from_resolution("dynamic"),
            Ok(ImportClass::Certain)
        );
        assert_eq!(
            ImportClass::from_resolution("inferred"),
            Ok(ImportClass::Inferred)
        );
        for r in CERTAIN_IMPORT_RESOLUTIONS {
            assert_eq!(ImportClass::from_resolution(r), Ok(ImportClass::Certain));
        }
        assert_eq!(READABLE_IMPORT_RESOLUTIONS.len(), 3);
    }

    #[test]
    fn a_resolution_outside_the_vocabulary_is_unreadable_never_certain() {
        for bad in ["resolved", "unresolved", "", "STATIC", "Static"] {
            let err = ImportClass::from_resolution(bad).expect_err("out of vocabulary");
            assert_eq!(
                err,
                ImportPartitionError::UnreadableResolution {
                    value: bad.to_string()
                }
            );
            assert!(
                err.to_string().contains(&format!("{bad:?}")),
                "names the value"
            );
            assert!(ImportPartition::classify(bad, Some(0)).is_err());
        }
    }

    #[test]
    fn importer_flag_one_is_test_zero_is_production_absent_is_unknown() {
        assert_eq!(
            ImporterStatus::from_is_test(Some(1)),
            Ok(ImporterStatus::Test)
        );
        assert_eq!(
            ImporterStatus::from_is_test(Some(0)),
            Ok(ImporterStatus::Production)
        );
        assert_eq!(
            ImporterStatus::from_is_test(None),
            Ok(ImporterStatus::Unknown)
        );
        assert!(ImporterStatus::Unknown.is_production_partition());
        assert!(ImporterStatus::Production.is_production_partition());
        assert!(!ImporterStatus::Test.is_production_partition());
    }

    #[test]
    fn an_importer_flag_outside_zero_and_one_is_unreadable() {
        for bad in [2, -1, 42] {
            assert_eq!(
                ImporterStatus::from_is_test(Some(bad)),
                Err(ImportPartitionError::UnreadableImporterFlag { value: bad })
            );
            assert!(ImportPartition::classify("static", Some(bad)).is_err());
        }
    }

    #[test]
    fn default_view_admits_production_certain_and_unknown_status_certain_only() {
        let v = ImportView::DEFAULT;
        assert!(v.admits(PC));
        assert!(
            v.admits(UC),
            "an importer with no files row stays production"
        );
        assert!(!v.admits(TC));
        assert!(!v.admits(PI));
        assert!(!v.admits(TI));
        assert!(!v.admits(UI));
        assert_eq!(ImportView::default(), ImportView::DEFAULT);
    }

    #[test]
    fn include_tests_adds_test_certain() {
        let v = ImportView::CERTAIN_WITH_TESTS;
        assert!(v.admits(PC) && v.admits(TC) && v.admits(UC));
        assert!(!v.admits(PI) && !v.admits(TI));
    }

    #[test]
    fn include_inferred_adds_production_inferred() {
        let v = ImportView::WITH_INFERRED;
        assert!(v.admits(PC) && v.admits(PI) && v.admits(UI));
        assert!(!v.admits(TC) && !v.admits(TI));
    }

    #[test]
    fn both_flags_admit_all_four_partitions() {
        let v = ImportView::ALL;
        for part in [PC, TC, PI, TI, UC, UI] {
            assert!(v.admits(part));
            assert_eq!(remainder_group(v, part), None);
        }
        assert!(v.admits_everything());
        assert!(v.wider_views().is_empty());
    }

    #[test]
    fn each_excluded_partition_is_stated_once_under_the_smallest_flag_set_that_shows_it() {
        use RemainderGroup::*;
        // Default: test certain → tests; production inferred → inferred; test inferred → both.
        let d = ImportView::DEFAULT;
        assert_eq!(remainder_group(d, PC), None);
        assert_eq!(remainder_group(d, TC), Some(Tests));
        assert_eq!(remainder_group(d, PI), Some(Inferred));
        assert_eq!(remainder_group(d, TI), Some(TestsAndInferred));
        assert_eq!(remainder_group(d, UI), Some(Inferred));
        // Tests already included: test inferred joins inferred.
        let t = ImportView::CERTAIN_WITH_TESTS;
        assert_eq!(remainder_group(t, TC), None);
        assert_eq!(remainder_group(t, PI), Some(Inferred));
        assert_eq!(remainder_group(t, TI), Some(Inferred));
        // Inferred already included: test inferred joins tests.
        let i = ImportView::WITH_INFERRED;
        assert_eq!(remainder_group(i, TC), Some(Tests));
        assert_eq!(remainder_group(i, TI), Some(Tests));
        assert_eq!(remainder_group(i, PI), None);
        // Every cell counted once: the groups partition the excluded total.
        let mut c = PartitionCounts::default();
        for part in [PC, TC, TC, PI, TI, TI, TI] {
            c.add(part);
        }
        for view in [d, t, i, ImportView::ALL] {
            let excluded: u64 = RemainderGroup::ALL
                .iter()
                .map(|g| c.excluded_in(view, *g))
                .sum();
            assert_eq!(excluded + c.admitted(view), c.total(), "{view:?}");
        }
        assert_eq!(c.excluded_in(d, Tests), 2);
        assert_eq!(c.excluded_in(d, Inferred), 1);
        assert_eq!(c.excluded_in(d, TestsAndInferred), 3);
        // Wider views are strict supersets only, singles before both.
        assert_eq!(
            d.wider_views(),
            vec![
                ImportView::CERTAIN_WITH_TESTS,
                ImportView::WITH_INFERRED,
                ImportView::ALL
            ]
        );
        assert_eq!(t.wider_views(), vec![ImportView::ALL]);
        assert_eq!(i.wider_views(), vec![ImportView::ALL]);
        assert_eq!(
            ImportView::ALL.flag_names(),
            vec!["include_tests", "include_inferred"]
        );
    }

    #[test]
    fn partition_counts_tally_unknown_inside_the_production_cells() {
        let mut c = PartitionCounts::default();
        for part in [PC, UC, TC, PI, UI, TI] {
            c.add(part);
        }
        assert_eq!(
            c,
            PartitionCounts {
                production_certain: 2,
                test_certain: 1,
                production_inferred: 2,
                test_inferred: 1,
                unknown_test_status: 2,
            }
        );
        assert_eq!(c.total(), 6, "the unknown tally is not a fifth cell");
        assert_eq!(c.admitted(ImportView::DEFAULT), 2);
        assert_eq!(c.admitted(ImportView::ALL), 6);
    }
}
