//! MODULE-CYCLES-COMPLETENESS-CERT-1: a module-import-cycle COMPLETENESS CERTIFICATE + a PURE evaluator.
//!
//! The policy boundary the (deferred) `rmap cycles` default migration needs: decide whether the LiveGraph
//! covers the WHOLE module-import-cycle graph for a repo WITHOUT consulting SQLite per query. The evaluator
//! is PURE -- it reads a [`LiveCycleState`] snapshot of the LiveGraph + an optional [`BaselineInput`] (the
//! EXPECTED partition set + non-TS evidence, supplied by prerequisites: enumeration + a one-time audit).
//!
//! SAFE BY CONSTRUCTION: it can never FAKE completeness from loaded state alone. `Complete` requires the
//! baseline (proving the negatives: no missing partition, no non-TS source). No baseline ->
//! [`ModuleCycleCompleteness::UnknownBaselineMissing`]. Conservative on import classes: ANY uncaptured class
//! (package / dynamic / unresolved-after-overlay) -> `IncompleteImportClasses` (the captured graph may miss
//! a cycle edge), even if the cycles happen to be exact -- the cert cannot know that without the compare.
//!
//! This module does NOT change any default; the migration consumes the certificate later.

use std::collections::BTreeSet;

/// The completeness verdict for the MODULE-import-cycle query family (CYCLES-COMPLETENESS-CERT-1 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCycleCompleteness {
    /// The LiveGraph provably covers the whole module-import-cycle graph: baseline present; every EXPECTED
    /// partition is loaded + Fresh; no non-TS cycle source; every import class captured. The default MAY
    /// serve LiveGraph (the gated migration).
    CompleteForModuleImportCycles,
    /// The baseline's EXPECTED partition set has partitions not loaded (or not Fresh).
    IncompleteMissingPartitions,
    /// The repo has a non-TS cycle source (baseline), or a loaded partition is non-TS -> the TS-only
    /// LiveGraph cannot cover it (the repo-graph Rust-cycle case).
    IncompleteUnsupportedLanguage,
    /// An import class is uncaptured (PackageExternal / Dynamic / StaticUnresolved-not-overlay-resolved) ->
    /// the captured graph may be missing a cycle edge.
    IncompleteImportClasses,
    /// NO baseline supplied -> cannot certify (the explicit no-baseline state; never faked from loaded state).
    UnknownBaselineMissing,
    /// Indeterminate (reserved).
    Unknown,
}

impl ModuleCycleCompleteness {
    /// Stable string for diagnostics/JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            ModuleCycleCompleteness::CompleteForModuleImportCycles => {
                "CompleteForModuleImportCycles"
            }
            ModuleCycleCompleteness::IncompleteMissingPartitions => "IncompleteMissingPartitions",
            ModuleCycleCompleteness::IncompleteUnsupportedLanguage => {
                "IncompleteUnsupportedLanguage"
            }
            ModuleCycleCompleteness::IncompleteImportClasses => "IncompleteImportClasses",
            ModuleCycleCompleteness::UnknownBaselineMissing => "UnknownBaselineMissing",
            ModuleCycleCompleteness::Unknown => "Unknown",
        }
    }
    /// D6 trust rule: ONLY `Complete` permits serving the LiveGraph default; every other state REQUIRES the
    /// SQLite fallback (never an Exact no-cycle without a Complete certificate).
    pub fn permits_livegraph_default(self) -> bool {
        matches!(self, ModuleCycleCompleteness::CompleteForModuleImportCycles)
    }
}

/// One loaded partition's certificate-relevant facts (a pure snapshot; no SQLite). The invalidation keys
/// (epoch, source_inputs_hash, producer_fingerprint) ride here so a cached certificate busts on any change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePartition {
    /// Partition id (== the LiveGraph slot key).
    pub id: String,
    /// Partition epoch (bumps on every swap).
    pub epoch: u64,
    /// `status == Fresh`.
    pub fresh: bool,
    /// `language == TypeScriptPrimary`.
    pub ts: bool,
    /// The partition's source inputs hash (invalidation).
    pub source_inputs_hash: String,
    /// The producer fingerprint `indexer@version` (invalidation).
    pub producer_fingerprint: String,
}

/// The import-class evidence across the loaded partitions (CYCLES-COMPLETENESS-CERT-1 D3; the package classes
/// refined by IMPORTS-PACKAGE-RESOLUTION-1 D5). Only the BLOCKING flags prevent `Complete`; the benign
/// external flag is reported-but-non-blocking (an external package cannot be in a repo-local module cycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObservationClassSummary {
    /// BENIGN (reported, does NOT block): a `node:`/builtin or DECLARED-dependency external package import
    /// (`ExternalPackageNonLocal`). It cannot participate in a repo-local module cycle.
    pub has_external_nonlocal: bool,
    /// BLOCKS: a WORKSPACE-LOCAL package import this slice cannot yet convert to a module edge
    /// (`WorkspaceLocalUnedgeable`; the edge is IMPORTS-WORKSPACE-PACKAGE-EDGE-1). Detected-local but a hole.
    pub has_workspace_local_unedgeable: bool,
    /// BLOCKS: a non-relative import that is NEITHER workspace-local NOR a declared external NOR a tsconfig
    /// alias (`PackageUnresolved`) -- a genuine unknown bare specifier.
    pub has_unresolved_package: bool,
    /// BLOCKS (IMPORTS-TSCONFIG-PATHS-1): a tsconfig `paths` ALIAS that matched a pattern but did NOT resolve
    /// to an indexed FILE (no file / ambiguous). DISTINCT from has_unresolved_package (an unknown specifier).
    pub has_alias_unresolved: bool,
    /// BLOCKS (IMPORTS-DYNAMIC-CLASSIFICATION-1, model B): a dynamic `import()` that is NON-LITERAL
    /// (`import(expr)`, no static specifier) -- the ONLY genuinely dynamic-unresolvable case. A LITERAL dynamic
    /// is classified by its TARGET CLASS (its static counterpart): resolved-relative -> captured edge;
    /// unresolved-relative -> has_unresolved_after_overlay; bare -> workspace/external/alias/unknown. So a
    /// literal dynamic is NEVER counted here.
    pub has_dynamic_unresolved: bool,
    /// BLOCKS: a `StaticUnresolved` (relative) observation the cross-partition OVERLAY did NOT resolve.
    /// Overlay-resolved ones are captured and do NOT count.
    pub has_unresolved_after_overlay: bool,
}

/// The PURE live snapshot the evaluator reads (produced by `LiveGraph::module_cycle_live_state`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveCycleState {
    /// Every loaded partition's certificate-relevant facts.
    pub partitions: Vec<LivePartition>,
    /// The uncaptured import-class evidence.
    pub observation_classes: ObservationClassSummary,
}

/// The BASELINE (the EXPECTED facts, from prerequisites -- enumeration + a one-time index-time audit; NOT
/// live, NOT per-query SQLite). Absent -> the evaluator returns `UnknownBaselineMissing`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineInput {
    /// The partition ids that SHOULD be loaded for whole-repo coverage (from enumeration).
    pub expected_partition_ids: BTreeSet<String>,
    /// The repo has non-TS sources with import/cycle semantics (from the audit) -> the TS-only LiveGraph
    /// cannot be complete.
    pub has_non_ts_cycle_source: bool,
    /// The repo registration/index epoch (invalidation: a re-index changes the baseline).
    pub repo_index_epoch: u64,
    /// The language-support policy version (invalidation: a language-support change re-opens the question).
    pub language_support_version: u32,
    /// The import-completeness policy version (CYCLES-COMPLETENESS-AUDIT-1 D2): which import classes the
    /// policy treats as cycle-relevant/uncaptured. Invalidation: if the policy that maps
    /// PackageExternal/Dynamic/StaticUnresolved -> `IncompleteImportClasses` changes (e.g. a future
    /// IMPORTS-PACKAGE-RESOLUTION-1 declares resolved package imports non-cycle-relevant), every prior
    /// certificate must be re-evaluated -- so it rides in the fingerprint.
    pub import_completeness_policy_version: u32,
}

/// Evaluate the module-import-cycle completeness certificate (CYCLES-COMPLETENESS-CERT-1 D2/D3). PURE: no
/// SQLite, no IO. Precedence: no-baseline -> missing-partitions -> unsupported-language -> import-classes ->
/// Complete. NEVER `Complete` without a baseline.
pub fn evaluate_module_cycle_completeness(
    live: &LiveCycleState,
    baseline: Option<&BaselineInput>,
) -> ModuleCycleCompleteness {
    let baseline = match baseline {
        Some(b) => b,
        None => return ModuleCycleCompleteness::UnknownBaselineMissing,
    };
    // Structural 1: every EXPECTED partition must be LOADED and FRESH (a stale partition is not current).
    let loaded_fresh: BTreeSet<&str> = live
        .partitions
        .iter()
        .filter(|p| p.fresh)
        .map(|p| p.id.as_str())
        .collect();
    if !baseline
        .expected_partition_ids
        .iter()
        .all(|e| loaded_fresh.contains(e.as_str()))
    {
        return ModuleCycleCompleteness::IncompleteMissingPartitions;
    }
    // Structural 2: no non-TS cycle source (baseline) AND every loaded partition is TS.
    if baseline.has_non_ts_cycle_source || live.partitions.iter().any(|p| !p.ts) {
        return ModuleCycleCompleteness::IncompleteUnsupportedLanguage;
    }
    // Import classes (IMPORTS-PACKAGE-RESOLUTION-1 D5): ANY BLOCKING uncaptured class -> the captured graph
    // may miss a cycle edge. ExternalPackageNonLocal is BENIGN (an external package cannot be in a repo-local
    // module cycle) and does NOT block; only workspace-local-unedgeable / unresolved-package / dynamic /
    // unresolved-relative block.
    let o = &live.observation_classes;
    if o.has_workspace_local_unedgeable
        || o.has_unresolved_package
        || o.has_alias_unresolved
        || o.has_dynamic_unresolved
        || o.has_unresolved_after_overlay
    {
        return ModuleCycleCompleteness::IncompleteImportClasses;
    }
    ModuleCycleCompleteness::CompleteForModuleImportCycles
}

/// A deterministic fingerprint over ALL certificate INPUTS (D4 invalidation): partition {id, epoch, fresh,
/// ts, source_inputs_hash, producer_fingerprint} + baseline {expected set, non-TS flag, index epoch,
/// language-support version}. A cacher recomputes the certificate when this changes; ANY input change ->
/// a different fingerprint -> the cached certificate is invalidated (never trusted stale).
pub fn certificate_inputs_fingerprint(
    live: &LiveCycleState,
    baseline: Option<&BaselineInput>,
) -> String {
    let mut parts: Vec<String> = live
        .partitions
        .iter()
        .map(|p| {
            format!(
                "{}@{}:f{}:ts{}:{}:{}",
                p.id,
                p.epoch,
                p.fresh as u8,
                p.ts as u8,
                p.source_inputs_hash,
                p.producer_fingerprint
            )
        })
        .collect();
    parts.sort();
    let o = &live.observation_classes;
    let mut s = format!(
        "obs[ext{}:wsl{}:unp{}:alu{}:dyu{}:unr{}]|parts[{}]",
        o.has_external_nonlocal as u8,
        o.has_workspace_local_unedgeable as u8,
        o.has_unresolved_package as u8,
        o.has_alias_unresolved as u8,
        o.has_dynamic_unresolved as u8,
        o.has_unresolved_after_overlay as u8,
        parts.join(",")
    );
    match baseline {
        None => s.push_str("|base:NONE"),
        Some(b) => {
            let mut exp: Vec<&str> = b
                .expected_partition_ids
                .iter()
                .map(|x| x.as_str())
                .collect();
            exp.sort();
            s.push_str(&format!(
                "|base[{}:nts{}:ie{}:lv{}:ip{}]",
                exp.join(","),
                b.has_non_ts_cycle_source as u8,
                b.repo_index_epoch,
                b.language_support_version,
                b.import_completeness_policy_version
            ));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(id: &str, fresh: bool, ts: bool) -> LivePartition {
        LivePartition {
            id: id.to_string(),
            epoch: 1,
            fresh,
            ts,
            source_inputs_hash: "h1".to_string(),
            producer_fingerprint: "scip-typescript@0.4.0".to_string(),
        }
    }
    fn baseline(expected: &[&str], non_ts: bool) -> BaselineInput {
        BaselineInput {
            expected_partition_ids: expected.iter().map(|s| s.to_string()).collect(),
            has_non_ts_cycle_source: non_ts,
            repo_index_epoch: 1,
            language_support_version: 1,
            import_completeness_policy_version: 1,
        }
    }
    fn live(parts: Vec<LivePartition>, o: ObservationClassSummary) -> LiveCycleState {
        LiveCycleState {
            partitions: parts,
            observation_classes: o,
        }
    }
    const CLEAN: ObservationClassSummary = ObservationClassSummary {
        has_external_nonlocal: false,
        has_workspace_local_unedgeable: false,
        has_unresolved_package: false,
        has_alias_unresolved: false,
        has_dynamic_unresolved: false,
        has_unresolved_after_overlay: false,
    };

    #[test]
    fn complete_ts_repo_with_baseline() {
        let l = live(vec![part("a", true, true), part("b", true, true)], CLEAN);
        let b = baseline(&["a", "b"], false);
        assert_eq!(
            evaluate_module_cycle_completeness(&l, Some(&b)),
            ModuleCycleCompleteness::CompleteForModuleImportCycles
        );
        assert!(ModuleCycleCompleteness::CompleteForModuleImportCycles.permits_livegraph_default());
    }

    #[test]
    fn missing_expected_partition() {
        let l = live(vec![part("a", true, true)], CLEAN); // b not loaded
        let b = baseline(&["a", "b"], false);
        assert_eq!(
            evaluate_module_cycle_completeness(&l, Some(&b)),
            ModuleCycleCompleteness::IncompleteMissingPartitions
        );
        // a STALE expected partition is also "missing" (not current).
        let l2 = live(vec![part("a", true, true), part("b", false, true)], CLEAN);
        assert_eq!(
            evaluate_module_cycle_completeness(&l2, Some(&b)),
            ModuleCycleCompleteness::IncompleteMissingPartitions
        );
    }

    #[test]
    fn non_ts_language_present() {
        // baseline flags a non-TS cycle source -> unsupported.
        let l = live(vec![part("a", true, true)], CLEAN);
        assert_eq!(
            evaluate_module_cycle_completeness(&l, Some(&baseline(&["a"], true))),
            ModuleCycleCompleteness::IncompleteUnsupportedLanguage
        );
        // a loaded partition is non-TS -> unsupported.
        let l2 = live(vec![part("a", true, false)], CLEAN);
        assert_eq!(
            evaluate_module_cycle_completeness(&l2, Some(&baseline(&["a"], false))),
            ModuleCycleCompleteness::IncompleteUnsupportedLanguage
        );
    }

    #[test]
    fn import_class_split_external_benign_others_block() {
        let b = baseline(&["a"], false);
        let check = |o: ObservationClassSummary| {
            evaluate_module_cycle_completeness(&live(vec![part("a", true, true)], o), Some(&b))
        };
        // IMPORTS-PACKAGE-RESOLUTION-1 D5: ExternalPackageNonLocal ALONE is BENIGN -> still Complete.
        assert_eq!(
            check(ObservationClassSummary {
                has_external_nonlocal: true,
                ..CLEAN
            }),
            ModuleCycleCompleteness::CompleteForModuleImportCycles,
            "external npm/node: imports do NOT block repo-local module cycles"
        );
        // each BLOCKING class -> IncompleteImportClasses.
        for blocking in [
            ObservationClassSummary {
                has_workspace_local_unedgeable: true,
                ..CLEAN
            },
            ObservationClassSummary {
                has_unresolved_package: true,
                ..CLEAN
            },
            ObservationClassSummary {
                has_alias_unresolved: true,
                ..CLEAN
            },
            ObservationClassSummary {
                has_dynamic_unresolved: true,
                ..CLEAN
            },
            ObservationClassSummary {
                has_unresolved_after_overlay: true,
                ..CLEAN
            },
        ] {
            assert_eq!(
                check(blocking),
                ModuleCycleCompleteness::IncompleteImportClasses
            );
        }
        // benign external ALONGSIDE a blocking class still blocks (the blocking one dominates).
        assert_eq!(
            check(ObservationClassSummary {
                has_external_nonlocal: true,
                has_unresolved_package: true,
                ..CLEAN
            }),
            ModuleCycleCompleteness::IncompleteImportClasses
        );
    }

    #[test]
    fn absent_baseline_is_unknown_baseline_missing() {
        let l = live(vec![part("a", true, true)], CLEAN);
        let r = evaluate_module_cycle_completeness(&l, None);
        assert_eq!(r, ModuleCycleCompleteness::UnknownBaselineMissing);
        assert!(
            !r.permits_livegraph_default(),
            "no baseline -> never serve LiveGraph"
        );
    }

    #[test]
    fn structural_precedence_missing_before_language_before_classes() {
        // missing-partition wins over a non-TS + blocking-import-class state.
        let dirty = ObservationClassSummary {
            has_unresolved_package: true,
            ..CLEAN
        };
        let l = live(vec![part("a", true, false)], dirty); // a is non-TS AND dirty, but b is missing
        assert_eq!(
            evaluate_module_cycle_completeness(&l, Some(&baseline(&["a", "b"], true))),
            ModuleCycleCompleteness::IncompleteMissingPartitions
        );
    }

    #[test]
    fn input_change_invalidates_fingerprint() {
        let l = live(vec![part("a", true, true)], CLEAN);
        let b = baseline(&["a"], false);
        let f0 = certificate_inputs_fingerprint(&l, Some(&b));
        // epoch change
        let mut l2 = l.clone();
        l2.partitions[0].epoch = 2;
        assert_ne!(
            f0,
            certificate_inputs_fingerprint(&l2, Some(&b)),
            "epoch bump invalidates"
        );
        // source hash change
        let mut l3 = l.clone();
        l3.partitions[0].source_inputs_hash = "h2".to_string();
        assert_ne!(
            f0,
            certificate_inputs_fingerprint(&l3, Some(&b)),
            "source hash change invalidates"
        );
        // producer fingerprint change
        let mut l4 = l.clone();
        l4.partitions[0].producer_fingerprint = "scip-typescript@0.5.0".to_string();
        assert_ne!(
            f0,
            certificate_inputs_fingerprint(&l4, Some(&b)),
            "producer change invalidates"
        );
        // repo index epoch change
        let mut b2 = b.clone();
        b2.repo_index_epoch = 2;
        assert_ne!(
            f0,
            certificate_inputs_fingerprint(&l, Some(&b2)),
            "index epoch invalidates"
        );
        // language-support version change
        let mut b3 = b.clone();
        b3.language_support_version = 2;
        assert_ne!(
            f0,
            certificate_inputs_fingerprint(&l, Some(&b3)),
            "language version invalidates"
        );
        // import-completeness policy version change (CYCLES-COMPLETENESS-AUDIT-1 D2)
        let mut b4 = b.clone();
        b4.import_completeness_policy_version = 2;
        assert_ne!(
            f0,
            certificate_inputs_fingerprint(&l, Some(&b4)),
            "import-completeness policy version invalidates"
        );
        // baseline present vs absent
        assert_ne!(
            f0,
            certificate_inputs_fingerprint(&l, None),
            "absent baseline invalidates"
        );
        // identical inputs -> identical fingerprint (stable)
        assert_eq!(f0, certificate_inputs_fingerprint(&l, Some(&b)));
    }
}
