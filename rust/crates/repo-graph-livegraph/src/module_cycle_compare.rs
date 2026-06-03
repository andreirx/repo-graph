//! MODULE-AGGREGATION-1 (D5): compare LiveGraph-derived MODULE cycles to SQLite `rmap cycles` output,
//! BY MODULE-PATH SETS, and CLASS the divergences.
//!
//! Pure + abstract: it operates on two lists of cycles, each cycle a list of MODULE qualified-names
//! (repo-relative directory paths). It has NO LiveGraph / SQLite dependency — the caller supplies both
//! cycle lists (LiveGraph from [`crate::LiveGraph::module_import_cycles`], SQLite from `rmap cycles`).
//!
//! The equivalence model (D5): on the fixture the two MUST be EXACT; on a real repo the LiveGraph is
//! EXPECTED to be a SUBSET of SQLite (the captured FILE graph is relative + ext/index only, so module
//! rings closed only by package / dynamic / unresolved imports are absent). Every divergence is CLASSED so
//! it is explained, never hand-waved; an `extra` LiveGraph cycle is an OVERCLAIM (a derivation bug).

use std::collections::BTreeSet;

/// A cycle compared as a SET of module paths (order-independent). A `BTreeSet` dedups AND sorts, so the
/// collected `Vec` is already canonical.
fn canonical(cycle: &[String]) -> Vec<String> {
    cycle
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_set(cycles: &[Vec<String>]) -> BTreeSet<Vec<String>> {
    cycles.iter().map(|c| canonical(c)).collect()
}

/// The structural comparison of two module-cycle sets (D5). Cycles are compared as SETS of module paths.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleCycleComparison {
    /// Cycles present in BOTH (canonical module-path sets).
    pub matched: Vec<Vec<String>>,
    /// In SQLite but NOT the LiveGraph — EXPECTED on real repos (the captured-graph completeness gap).
    pub missing_in_livegraph: Vec<Vec<String>>,
    /// In the LiveGraph but NOT SQLite — an OVERCLAIM: the LiveGraph module cycles MUST be a subset.
    pub extra_in_livegraph: Vec<Vec<String>>,
}

impl ModuleCycleComparison {
    /// EXACT equivalence (the fixture gate): no missing AND no extra.
    pub fn is_exact(&self) -> bool {
        self.missing_in_livegraph.is_empty() && self.extra_in_livegraph.is_empty()
    }
    /// The LiveGraph is a SUBSET of SQLite (the real-repo expectation): no `extra` cycles. Missing cycles
    /// are allowed (and must be classed), but an extra cycle is never acceptable.
    pub fn is_livegraph_subset(&self) -> bool {
        self.extra_in_livegraph.is_empty()
    }
}

/// Compare LiveGraph-derived module cycles against SQLite `rmap cycles`, by module-path SETS.
pub fn compare_module_cycles(
    livegraph: &[Vec<String>],
    sqlite: &[Vec<String>],
) -> ModuleCycleComparison {
    let lg = canonical_set(livegraph);
    let sq = canonical_set(sqlite);
    ModuleCycleComparison {
        matched: lg.intersection(&sq).cloned().collect(),
        missing_in_livegraph: sq.difference(&lg).cloned().collect(),
        extra_in_livegraph: lg.difference(&sq).cloned().collect(),
    }
}

/// Why a module cycle diverges between the LiveGraph and SQLite (D5). The vocabulary for explaining every
/// divergence. The cause of a MISSING cycle (the first three) needs import-form context the structural
/// comparison lacks, so the caller supplies it; an EXTRA cycle is always an overclaim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCycleDivergence {
    /// SQLite has the cycle; the LiveGraph lacks it because a PACKAGE-name or DYNAMIC import closes the
    /// ring (not in the captured relative+ext/index graph). The EXPECTED real-repo gap.
    MissingInLiveGraphDueToPackageOrDynamicImport,
    /// SQLite has the cycle; the LiveGraph lacks it because a relative import did NOT resolve
    /// (`StaticUnresolved`, no overlay match) — a resolution gap, not an aggregation bug.
    MissingInLiveGraphDueToUnresolvedImport,
    /// The "same" cycle exists in both but its MODULE members differ — the dirname identity diverged from
    /// SQLite's (would indicate the D1 identity rule is wrong on this repo).
    ModuleIdentityMismatch,
    /// The LiveGraph reports a cycle SQLite does NOT — an OVERCLAIM / derivation BUG (the LiveGraph module
    /// cycles must be a subset of SQLite's).
    UnexpectedExtraInLiveGraph,
    /// A divergence with no determined cause (needs analyst inspection / more context).
    UnknownDivergence,
}

/// A classed divergence: the cycle (canonical module-path set) + its class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedDivergence {
    /// The diverging cycle (canonical module-path set).
    pub cycle: Vec<String>,
    /// Its class.
    pub kind: ModuleCycleDivergence,
}

/// Class every divergence in a comparison (D5): each `extra_in_livegraph` is
/// [`ModuleCycleDivergence::UnexpectedExtraInLiveGraph`] (an overclaim — always a concern); each
/// `missing_in_livegraph` is classed by `classify_missing`, which the CALLER supplies from import-form
/// context (default `|_| UnknownDivergence` when no context is available). Returns an empty list iff the
/// comparison is exact.
pub fn classify_divergences(
    cmp: &ModuleCycleComparison,
    classify_missing: impl Fn(&[String]) -> ModuleCycleDivergence,
) -> Vec<ClassifiedDivergence> {
    let mut out = Vec::new();
    for c in &cmp.extra_in_livegraph {
        out.push(ClassifiedDivergence {
            cycle: c.clone(),
            kind: ModuleCycleDivergence::UnexpectedExtraInLiveGraph,
        });
    }
    for c in &cmp.missing_in_livegraph {
        out.push(ClassifiedDivergence {
            cycle: c.clone(),
            kind: classify_missing(c),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cyc(members: &[&str]) -> Vec<String> {
        members.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn identical_cycle_sets_are_exact() {
        // FIXTURE shape: the one cross-partition module cycle, order-independent.
        let lg = vec![cyc(&["packages/a/src", "packages/b/src"])];
        let sq = vec![cyc(&["packages/b/src", "packages/a/src"])]; // reversed order
        let cmp = compare_module_cycles(&lg, &sq);
        assert!(cmp.is_exact(), "{cmp:?}");
        assert!(cmp.is_livegraph_subset());
        assert_eq!(cmp.matched.len(), 1);
        assert!(
            classify_divergences(&cmp, |_| ModuleCycleDivergence::UnknownDivergence).is_empty()
        );
    }

    #[test]
    fn livegraph_missing_a_sqlite_cycle_is_subset_and_classed() {
        // Real-repo shape: SQLite has an extra ring the LiveGraph can't see (package import).
        let lg = vec![cyc(&["a", "b"])];
        let sq = vec![cyc(&["a", "b"]), cyc(&["c", "d"])];
        let cmp = compare_module_cycles(&lg, &sq);
        assert!(!cmp.is_exact());
        assert!(
            cmp.is_livegraph_subset(),
            "no extra -> LiveGraph is a subset"
        );
        assert_eq!(cmp.missing_in_livegraph, vec![cyc(&["c", "d"])]);
        let classed = classify_divergences(&cmp, |_| {
            ModuleCycleDivergence::MissingInLiveGraphDueToPackageOrDynamicImport
        });
        assert_eq!(classed.len(), 1);
        assert_eq!(
            classed[0].kind,
            ModuleCycleDivergence::MissingInLiveGraphDueToPackageOrDynamicImport
        );
    }

    #[test]
    fn extra_livegraph_cycle_is_not_subset_and_flags_overclaim() {
        // An EXTRA LiveGraph cycle is always a derivation bug (overclaim).
        let lg = vec![cyc(&["a", "b"]), cyc(&["x", "y"])];
        let sq = vec![cyc(&["a", "b"])];
        let cmp = compare_module_cycles(&lg, &sq);
        assert!(!cmp.is_exact());
        assert!(!cmp.is_livegraph_subset(), "an extra cycle breaks subset");
        let classed = classify_divergences(&cmp, |_| ModuleCycleDivergence::UnknownDivergence);
        assert!(classed.iter().any(|d| d.kind
            == ModuleCycleDivergence::UnexpectedExtraInLiveGraph
            && d.cycle == cyc(&["x", "y"])));
    }

    #[test]
    fn classify_missing_defaults_to_unknown_without_context() {
        let cmp = compare_module_cycles(&[], &[cyc(&["a", "b"])]);
        let classed = classify_divergences(&cmp, |_| ModuleCycleDivergence::UnknownDivergence);
        assert_eq!(classed.len(), 1);
        assert_eq!(classed[0].kind, ModuleCycleDivergence::UnknownDivergence);
    }

    #[test]
    fn divergence_vocabulary_is_distinct() {
        // Lock the five classes (the harness/analyst vocabulary).
        use ModuleCycleDivergence::*;
        let all = [
            MissingInLiveGraphDueToPackageOrDynamicImport,
            MissingInLiveGraphDueToUnresolvedImport,
            ModuleIdentityMismatch,
            UnexpectedExtraInLiveGraph,
            UnknownDivergence,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }
    }
}
