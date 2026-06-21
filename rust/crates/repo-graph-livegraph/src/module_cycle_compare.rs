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

use repo_graph_import_resolver::{dirname, normalize_join};

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

/// Why a module cycle diverges between the LiveGraph and SQLite (MODULE-AGGREGATION-1 +
/// MODULE-CYCLES-COMPARE-CLASSIFY-1 D3). The MISSING causes are assigned by [`classify_missing_module_cycle`]
/// from LiveGraph EVIDENCE (D2=A; no SQLite-edge read); each is evidence-backed or `UnknownDivergence`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCycleDivergence {
    /// A cycle module imports a PACKAGE name (`PackageExternal`) — out of the captured relative graph; the
    /// SOLE non-captured class across the ring.
    MissingDueToPackageExternal,
    /// A cycle module has a DYNAMIC `import()` (`DynamicUnsupported`) — the SOLE non-captured class.
    MissingDueToDynamicImport,
    /// A relative import (`StaticUnresolved`) whose normalized target lands in ANOTHER cycle module —
    /// confirmed evidence of the bridging import the captured graph missed.
    MissingDueToStaticUnresolved,
    /// A cycle module has NO resident TS files in the LiveGraph (non-resident partition or non-TS) — the
    /// ring is not analyzable.
    MissingDueToUnloadedOrNonTsPartition,
    /// A cycle module has no LiveGraph counterpart but a NEAR-VARIANT (parent/child dir) exists — the
    /// dirname identity diverged from SQLite's on this repo.
    ModuleIdentityMismatch,
    /// The LiveGraph reports a cycle SQLite does NOT — an OVERCLAIM / derivation BUG (the LiveGraph module
    /// cycles must be a subset of SQLite's).
    UnexpectedExtraInLiveGraph,
    /// Evidence does not UNAMBIGUOUSLY explain the divergence (mixed classes, weak evidence) — the honest
    /// default (D4: favor Unknown over a wrong guess).
    UnknownDivergence,
}

impl ModuleCycleDivergence {
    /// Stable string for the compare sidecar / JSON (`divergence` field).
    pub fn as_str(self) -> &'static str {
        match self {
            ModuleCycleDivergence::MissingDueToPackageExternal => "MissingDueToPackageExternal",
            ModuleCycleDivergence::MissingDueToDynamicImport => "MissingDueToDynamicImport",
            ModuleCycleDivergence::MissingDueToStaticUnresolved => "MissingDueToStaticUnresolved",
            ModuleCycleDivergence::MissingDueToUnloadedOrNonTsPartition => {
                "MissingDueToUnloadedOrNonTsPartition"
            }
            ModuleCycleDivergence::ModuleIdentityMismatch => "ModuleIdentityMismatch",
            ModuleCycleDivergence::UnexpectedExtraInLiveGraph => "UnexpectedExtraInLiveGraph",
            ModuleCycleDivergence::UnknownDivergence => "UnknownDivergence",
        }
    }
}

/// The resolution class of an import observation (MODULE-CYCLES-COMPARE-CLASSIFY-1 D5), mirroring
/// `repo_graph_ir::ImportResolution` so the pure classifier needs NO IR types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObsResolution {
    /// Relative, node-resolved to a captured FILE edge (NOT a gap).
    StaticResolved,
    /// Relative, not node-resolved (the bridging-import candidate when its target is another cycle module).
    StaticUnresolved,
    /// A package / bare specifier — no repo path without package resolution.
    PackageExternal,
    /// A dynamic `import()` — no static target.
    DynamicUnsupported,
}

/// A small owned view of an import observation for the pure classifier (D5): the source MODULE is the map
/// key in [`classify_missing_module_cycle`]'s `observations_by_module`, so only the specifier + class are
/// needed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationView {
    /// The raw module specifier as written (e.g. `"../../b/src/foo"`, `"react"`).
    pub raw_specifier: String,
    /// The resolution class.
    pub resolution: ObsResolution,
    /// `export ... from` (a re-export).
    pub is_re_export: bool,
    /// `import type` / `export type`.
    pub is_type_only: bool,
}

/// Is `lg` a NEAR-VARIANT of `m` (one is a strict directory ancestor/descendant of the other)? Signals a
/// dirname identity divergence (e.g. SQLite module `packages/a` vs LiveGraph `packages/a/src`).
fn is_near_variant(lg: &str, m: &str) -> bool {
    lg.starts_with(&format!("{m}/")) || m.starts_with(&format!("{lg}/"))
}

/// Classify ONE missing SQLite module cycle from LiveGraph EVIDENCE ONLY (D2=A; NO SQLite-edge read), via
/// the D4 conservative precedence. `cycle` = the missing cycle's module-path set; `observations_by_module`
/// maps a MODULE path -> its resident files' observation views (key = the source module dir);
/// `livegraph_modules` = the LiveGraph's resident module paths (dirname identities). Returns a cause class
/// or `UnknownDivergence` (favoring Unknown over a wrong guess). IO-free.
pub fn classify_missing_module_cycle(
    cycle: &BTreeSet<String>,
    observations_by_module: &std::collections::BTreeMap<String, Vec<ObservationView>>,
    livegraph_modules: &BTreeSet<String>,
) -> ModuleCycleDivergence {
    // 1+2: residency / identity for any cycle module the LiveGraph does not have as a module.
    for m in cycle {
        if !livegraph_modules.contains(m) {
            return if livegraph_modules.iter().any(|lg| is_near_variant(lg, m)) {
                ModuleCycleDivergence::ModuleIdentityMismatch
            } else {
                ModuleCycleDivergence::MissingDueToUnloadedOrNonTsPartition
            };
        }
    }
    // 3: all cycle modules ARE resident LiveGraph modules -> the gap is an import FORM. Examine the cycle's
    // modules' non-captured observations.
    let mut has_static_bridge = false; // StaticUnresolved whose target normalizes into ANOTHER cycle module
    let mut has_package = false;
    let mut has_dynamic = false;
    for m in cycle {
        let Some(obs) = observations_by_module.get(m) else {
            continue;
        };
        for o in obs {
            match o.resolution {
                ObsResolution::StaticResolved => {} // captured; not a gap
                ObsResolution::StaticUnresolved => {
                    // The source dir IS the module key `m`; normalize the relative target from there.
                    let target = normalize_join(m, &o.raw_specifier);
                    let target_module = dirname(&target);
                    if target_module != m && cycle.contains(target_module) {
                        has_static_bridge = true;
                    }
                }
                ObsResolution::PackageExternal => has_package = true,
                ObsResolution::DynamicUnsupported => has_dynamic = true,
            }
        }
    }
    if has_static_bridge {
        return ModuleCycleDivergence::MissingDueToStaticUnresolved;
    }
    // 4: a SOLE heuristic class (package XOR dynamic); mixed or none -> Unknown (D4 honesty bar).
    match (has_package, has_dynamic) {
        (true, false) => ModuleCycleDivergence::MissingDueToPackageExternal,
        (false, true) => ModuleCycleDivergence::MissingDueToDynamicImport,
        _ => ModuleCycleDivergence::UnknownDivergence,
    }
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
        let classed =
            classify_divergences(&cmp, |_| ModuleCycleDivergence::MissingDueToPackageExternal);
        assert_eq!(classed.len(), 1);
        assert_eq!(
            classed[0].kind,
            ModuleCycleDivergence::MissingDueToPackageExternal
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
    fn divergence_vocabulary_is_distinct_and_stable_strings() {
        // Lock the refined vocabulary (D3) + the stable sidecar strings.
        use ModuleCycleDivergence::*;
        let all = [
            MissingDueToPackageExternal,
            MissingDueToDynamicImport,
            MissingDueToStaticUnresolved,
            MissingDueToUnloadedOrNonTsPartition,
            ModuleIdentityMismatch,
            UnexpectedExtraInLiveGraph,
            UnknownDivergence,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                assert_eq!(i == j, a == b);
                assert_eq!(i == j, a.as_str() == b.as_str(), "as_str must be 1:1");
            }
        }
    }

    // ── classify_missing_module_cycle (MODULE-CYCLES-COMPARE-CLASSIFY-1, D2=A / D4) ──

    fn obs(spec: &str, res: ObsResolution) -> ObservationView {
        ObservationView {
            raw_specifier: spec.to_string(),
            resolution: res,
            is_re_export: false,
            is_type_only: false,
        }
    }
    fn mods(ms: &[&str]) -> BTreeSet<String> {
        ms.iter().map(|s| s.to_string()).collect()
    }
    fn by_mod(
        entries: &[(&str, Vec<ObservationView>)],
    ) -> std::collections::BTreeMap<String, Vec<ObservationView>> {
        entries
            .iter()
            .map(|(m, o)| (m.to_string(), o.clone()))
            .collect()
    }

    #[test]
    fn classify_non_resident_module_is_unloaded_or_non_ts() {
        // A cycle module the LiveGraph has no resident-TS module for, with no near-variant -> unloaded/non-TS.
        let cycle = mods(&["pkg/a", "pkg/b"]);
        let lg = mods(&["pkg/a"]); // pkg/b absent, no near-variant
        let r = classify_missing_module_cycle(&cycle, &by_mod(&[]), &lg);
        assert_eq!(
            r,
            ModuleCycleDivergence::MissingDueToUnloadedOrNonTsPartition
        );
    }

    #[test]
    fn classify_near_variant_is_identity_mismatch() {
        // SQLite cycle module "pkg/a" but the LiveGraph groups under "pkg/a/src" -> identity divergence.
        let cycle = mods(&["pkg/a", "pkg/b"]);
        let lg = mods(&["pkg/a/src", "pkg/b/src"]);
        let r = classify_missing_module_cycle(&cycle, &by_mod(&[]), &lg);
        assert_eq!(r, ModuleCycleDivergence::ModuleIdentityMismatch);
    }

    #[test]
    fn classify_confirmed_static_unresolved_bridge() {
        // a/src has a StaticUnresolved relative import normalizing into b/src (another cycle module).
        let cycle = mods(&["packages/a/src", "packages/b/src"]);
        let lg = cycle.clone();
        let obs_by = by_mod(&[(
            "packages/a/src",
            vec![obs("../../b/src/foo", ObsResolution::StaticUnresolved)],
        )]);
        let r = classify_missing_module_cycle(&cycle, &obs_by, &lg);
        assert_eq!(r, ModuleCycleDivergence::MissingDueToStaticUnresolved);
    }

    #[test]
    fn classify_sole_package_external() {
        let cycle = mods(&["a", "b"]);
        let lg = cycle.clone();
        let obs_by = by_mod(&[("a", vec![obs("react", ObsResolution::PackageExternal)])]);
        assert_eq!(
            classify_missing_module_cycle(&cycle, &obs_by, &lg),
            ModuleCycleDivergence::MissingDueToPackageExternal
        );
    }

    #[test]
    fn classify_sole_dynamic_import() {
        let cycle = mods(&["a", "b"]);
        let lg = cycle.clone();
        let obs_by = by_mod(&[("b", vec![obs("./x", ObsResolution::DynamicUnsupported)])]);
        assert_eq!(
            classify_missing_module_cycle(&cycle, &obs_by, &lg),
            ModuleCycleDivergence::MissingDueToDynamicImport
        );
    }

    #[test]
    fn classify_mixed_package_and_dynamic_is_unknown() {
        // Mixed non-captured classes with no confirmed bridge -> Unknown (D4: no guess).
        let cycle = mods(&["a", "b"]);
        let lg = cycle.clone();
        let obs_by = by_mod(&[
            ("a", vec![obs("react", ObsResolution::PackageExternal)]),
            ("b", vec![obs("./x", ObsResolution::DynamicUnsupported)]),
        ]);
        assert_eq!(
            classify_missing_module_cycle(&cycle, &obs_by, &lg),
            ModuleCycleDivergence::UnknownDivergence
        );
    }

    #[test]
    fn classify_no_evidence_is_unknown() {
        // All resident, only captured (StaticResolved) observations -> no explaining evidence -> Unknown.
        let cycle = mods(&["a", "b"]);
        let lg = cycle.clone();
        let obs_by = by_mod(&[("a", vec![obs("./b", ObsResolution::StaticResolved)])]);
        assert_eq!(
            classify_missing_module_cycle(&cycle, &obs_by, &lg),
            ModuleCycleDivergence::UnknownDivergence
        );
    }
}
