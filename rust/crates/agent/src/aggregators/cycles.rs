//! Module-cycle aggregator.
//!
//! Calls `AgentStorageRead::find_module_cycles` and emits
//! `IMPORT_CYCLES` when at least one cycle is present. Evidence
//! carries the total cycle count plus the top 3 cycles as
//! summaries. TRUNCATION-AUDIT-1: the "top 3" slice is taken AFTER
//! ranking the full set by `ordering::canonicalize_cycles` (length DESC,
//! then ring members) — so the surviving cycles are the biggest,
//! and the order is deterministic and source-independent rather
//! than relying on the storage UID order.

use super::AggregatorOutput;
use crate::cycle_composition::{partition_counts, CyclePartition, CycleTestComposition};
use crate::dto::signal::{
    CycleEvidence, EvidenceAdditions, ExcludedCycleEvidence, ImportCyclesEvidence, Signal,
};
use crate::dto::test_status::{TestStatusUniverse, UndeterminedTestFiles};
use crate::errors::AgentStorageError;
use crate::ordering;
use crate::storage_port::{
    AgentCancelCheck, AgentCycle, AgentExcludedCycle, AgentImportCyclePartition, AgentStorageRead,
};
use repo_graph_classification::import_partition::ImportView;

const CYCLE_TOP_N: usize = 3;

/// ORIENT-CYCLES-DISAGREE-1: derive the exclusion-aware headline split
/// `(production_count, test_only_count)` for the emitted evidence — the SAME two integers
/// `cycles` renders. Returns `Some` ONLY when EVERY cycle carries a
/// [`CycleTestComposition`] (the SQLite-served path, where the stored `is_test` fact is
/// reachable). If ANY cycle lacks one (the LiveGraph module-cycle serve — FIXTURE-POLLUTION-1
/// §2.3 asymmetry — or a focus/path-scoped read the adapter does not classify) the split is
/// UNKNOWN → `None`, and the renderer falls back to the raw total, exactly as `cycles` does on
/// those same paths. NEVER a partial/0 split from a mix (that would mislabel absence as zero).
fn headline_split(cycles: &[AgentCycle]) -> Option<CyclePartition> {
    let comps: Vec<&CycleTestComposition> = cycles
        .iter()
        .filter_map(|c| c.test_composition.as_ref())
        .collect();
    if comps.len() == cycles.len() {
        Some(partition_counts(comps))
    } else {
        None
    }
}

/// HEADLINE-TRUTH-1 (COH-2, review-3 #3): the `type_only` verdict of the FIRST strictly-production
/// cycle in canonical order — found over the WHOLE cycle set, called BEFORE the `CYCLE_TOP_N`
/// truncation. When the first N canonical cycles are all test-only, the production cycle whose
/// verdict `orient` renders ranks beyond the truncation; carrying it here (not recovering it from
/// the truncated `cycles[]` leaf) is what lets the verdict survive. Matches the orient renderer's
/// old search semantics: strictly [`CycleTestComposition::Production`] (NOT `Unknown`, which is
/// counted in the headline but is not a proven-production example). `None` when no cycle is
/// labeled production (LiveGraph/focus paths, or an all-test-only/unknown set), or when the first
/// production cycle carries no TS/JS verdict — honest absence, never a fabricated verdict.
fn first_production_type_only(
    cycles: &[AgentCycle],
) -> Option<crate::cycle_type_only::CycleTypeOnly> {
    cycles
        .iter()
        .find(|c| matches!(c.test_composition, Some(CycleTestComposition::Production)))
        .and_then(|c| c.type_only.clone())
}

pub fn aggregate<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
    aggregate_cancellable(storage, snapshot_uid, &mut || {
        std::ops::ControlFlow::Continue(())
    })
}

/// DAEMON-CANCEL-3: cancellable variant of [`aggregate`]. Threads the cooperative
/// `cancel` checkpoint into the module-cycle Tarjan via
/// `find_module_cycles_cancellable`; everything else is identical. The daemon's
/// orient handler passes a real checkpoint here; `aggregate` passes a no-op.
pub fn aggregate_cancellable<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    cancel: AgentCancelCheck<'_>,
) -> Result<AggregatorOutput, AgentStorageError> {
    let cycles = storage.find_module_cycles_cancellable(snapshot_uid, &mut *cancel)?;
    // TEST-EDGE-SCOPE-1B (RG-REQ-004-L12): the cycles above answer the DEFAULT view; what that
    // view leaves out (the excluded cycles, the remainder, the importer UNDETERMINED block) rides
    // the same signal.
    let partition = storage.import_cycle_partition(snapshot_uid, ImportView::DEFAULT, cancel)?;
    let excluded: Vec<&AgentExcludedCycle> = partition.excluded_cycles.iter().collect();
    Ok(build_import_cycles_output(cycles, &partition, &excluded))
}

/// TEST-EDGE-SCOPE-1B: the `IMPORT_CYCLES` signal over the view's `cycles`, with the additive
/// partition keys. Emitted when the view has a cycle OR a cycle exists only through excluded
/// imports (so "0 cycles, +1 only through test imports" is stated, never silent).
pub(crate) fn build_import_cycles_output(
    mut cycles: Vec<AgentCycle>,
    partition: &AgentImportCyclePartition,
    excluded: &[&AgentExcludedCycle],
) -> AggregatorOutput {
    if cycles.is_empty() && excluded.is_empty() {
        return AggregatorOutput::empty();
    }

    ordering::canonicalize_cycles(&mut cycles);
    let cycle_count = cycles.len() as u64;
    // ORIENT-CYCLES-DISAGREE-1: partition BEFORE the top-3 truncation — the split is over the
    // WHOLE cycle set, not the rendered anchors.
    let split = headline_split(&cycles);
    // HEADLINE-TRUTH-1 (COH-2, review-3 #3): the first production cycle's verdict, found over
    // the WHOLE set BEFORE truncation — so it survives even when the top-N are all test-only.
    let production_type_only = first_production_type_only(&cycles);
    let top: Vec<CycleEvidence> = cycles
        .into_iter()
        .take(CYCLE_TOP_N)
        .map(|c| CycleEvidence {
            length: c.length,
            modules: c.modules,
            // TYPE-ONLY-IMPORTS-1: carry the per-cycle verdict into `orient`'s leaf. `Some` on the
            // SQLite path (the storage adapter computed it via the shared kernel); `None` on the
            // LiveGraph/focus paths and non-TS cycles (§5) — omitted from JSON there.
            type_only: c.type_only,
            // COHERENCE-3: carry the precomputed REAL walk (shared `cycle_walk` kernel) into the
            // leaf. `Some` on the SQLite path; `None` on LiveGraph/focus/truncated — orient then
            // renders the unordered form, matching `cycles`.
            walk: c.walk,
        })
        .collect();
    let additions = cycle_partition_additions(
        partition,
        excluded,
        top.iter().map(|c| c.modules.as_slice()),
    );

    let evidence = ImportCyclesEvidence {
        cycle_count,
        production_count: split.map(|p| p.production_count),
        test_only_count: split.map(|p| p.test_only_count),
        unknown_count: split.map(|p| p.unknown_count),
        production_type_only,
        cycles: top,
    };

    AggregatorOutput {
        signals: vec![Signal::import_cycles(evidence).with_evidence_additions(additions)],
        limits: Vec::new(),
    }
}

/// TEST-EDGE-SCOPE-1B (D-TESB-08, D-TESB-09): the additive keys the cycle signals carry —
/// `import_view`, `import_remainder`, `excluded_cycles` (the ones given — repo-wide or involving
/// the focus), `importer_test_status_undetermined` over the view's cross-directory production
/// importers (1A's one function), and each rendered cycle's `partitions` (by its member names).
pub(crate) fn cycle_partition_additions<'a, I>(
    partition: &AgentImportCyclePartition,
    excluded: &[&AgentExcludedCycle],
    rendered_members: I,
) -> EvidenceAdditions
where
    I: IntoIterator<Item = &'a [String]>,
{
    EvidenceAdditions {
        import_view: Some(partition.view),
        import_remainder: Some(partition.remainder),
        excluded_cycles: Some(
            excluded
                .iter()
                .map(|e| ExcludedCycleEvidence {
                    members: e.members.clone(),
                    length: e.members.len(),
                    flags: e.flags.clone(),
                    contains_shown: e.contains_shown.clone(),
                    partitions: e.partitions,
                })
                .collect(),
        ),
        importer_test_status_undetermined: Some(UndeterminedTestFiles::over_partition(
            TestStatusUniverse::CrossDirectoryImporters,
            partition
                .importers
                .iter()
                .map(|i| (i.path.as_str(), i.is_test)),
        )),
        inferred_imports_not_judged: None,
        item_partitions: rendered_members
            .into_iter()
            .map(|members| partition.partitions_of(members))
            .collect(),
    }
}

/// Whether an excluded cycle has a member at or under `prefix` (the path-focus rule the cycle
/// reads apply to shown cycles).
pub(crate) fn involves_path(e: &AgentExcludedCycle, prefix: &str) -> bool {
    e.members
        .iter()
        .any(|m| m == prefix || m.starts_with(&format!("{prefix}/")))
}

/// Path-scoped cycle aggregator.
///
/// Reads cycles involving modules under the given path prefix
/// via `find_cycles_involving_path`. Same evidence construction
/// as the repo-level `aggregate`.
pub fn aggregate_path<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    path_prefix: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
    aggregate_path_cancellable(storage, snapshot_uid, path_prefix, &mut || {
        std::ops::ControlFlow::Continue(())
    })
}

/// DAEMON-CANCEL-3: cancellable variant of [`aggregate_path`]. Threads `cancel` into
/// the path-scoped cycle Tarjan + filter via `find_cycles_involving_path_cancellable`.
///
/// Path-scoped cycles are read via `find_cycles_involving_path`, which the storage adapter does
/// NOT test-composition-label (a focus surface) — so the split is `None` and the evidence carries
/// the raw total. TEST-EDGE-SCOPE-1B: the excluded cycles named are those involving the prefix.
pub fn aggregate_path_cancellable<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    path_prefix: &str,
    cancel: AgentCancelCheck<'_>,
) -> Result<AggregatorOutput, AgentStorageError> {
    let cycles =
        storage.find_cycles_involving_path_cancellable(snapshot_uid, path_prefix, &mut *cancel)?;
    let partition = storage.import_cycle_partition(snapshot_uid, ImportView::DEFAULT, cancel)?;
    let excluded: Vec<&AgentExcludedCycle> = partition
        .excluded_cycles
        .iter()
        .filter(|e| involves_path(e, path_prefix))
        .collect();
    Ok(build_import_cycles_output(cycles, &partition, &excluded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cyc(comp: Option<CycleTestComposition>) -> AgentCycle {
        AgentCycle {
            length: 2,
            modules: vec!["a".into(), "b".into()],
            test_composition: comp,
            type_only: None,
            walk: None,
        }
    }

    #[test]
    fn split_present_only_when_every_cycle_is_labeled() {
        // ORIENT-CYCLES-DISAGREE-1: the split is derived ONLY when the serving computation
        // labeled EVERY cycle (SQLite path). production = non-test-only, test_only = test-only.
        let all_labeled = vec![
            cyc(Some(CycleTestComposition::Production)),
            cyc(Some(CycleTestComposition::TestOnly)),
            cyc(Some(CycleTestComposition::Unknown("x".into()))),
        ];
        assert_eq!(
            headline_split(&all_labeled),
            Some(CyclePartition {
                production_count: 2,
                test_only_count: 1,
                unknown_count: 1,
            })
        );
    }

    #[test]
    fn split_is_none_when_any_cycle_is_unlabeled_never_partial() {
        // A mix of labeled + unlabeled cannot yield a trustworthy split — it is UNKNOWN (None),
        // never a partial/zero count (STANDING HONESTY RULE #1: absence is not zero).
        let mixed = vec![cyc(Some(CycleTestComposition::Production)), cyc(None)];
        assert_eq!(headline_split(&mixed), None);
        // Fully-unlabeled (LiveGraph/focus path) is also None.
        assert_eq!(headline_split(&[cyc(None)]), None);
    }

    /// A test cycle carrying BOTH a composition and a type-only verdict.
    fn cyc_v(
        comp: Option<CycleTestComposition>,
        verdict: Option<crate::cycle_type_only::CycleTypeOnly>,
    ) -> AgentCycle {
        AgentCycle {
            length: 2,
            modules: vec!["a".into(), "b".into()],
            test_composition: comp,
            type_only: verdict,
            walk: None,
        }
    }

    #[test]
    fn first_production_verdict_found_past_the_top_n_truncation() {
        // HEADLINE-TRUTH-1 (COH-2, review-3 #3): the FIRST production cycle ranks at index 3 —
        // BEYOND the `CYCLE_TOP_N = 3` slice that becomes the rendered `cycles[]` leaf. Because
        // this helper runs over the WHOLE set before truncation, the production verdict is still
        // recovered (before the fix the renderer searched only the truncated top-3 and lost it).
        use crate::cycle_type_only::CycleTypeOnly;
        let cycles = vec![
            cyc_v(Some(CycleTestComposition::TestOnly), None),
            cyc_v(Some(CycleTestComposition::TestOnly), None),
            cyc_v(Some(CycleTestComposition::TestOnly), None),
            // The 4th (first PRODUCTION) cycle carries the type-only verdict.
            cyc_v(
                Some(CycleTestComposition::Production),
                Some(CycleTypeOnly::TypeOnly),
            ),
        ];
        assert!(cycles.len() > CYCLE_TOP_N, "fixture must exceed the top-N");
        assert_eq!(
            first_production_type_only(&cycles),
            Some(CycleTypeOnly::TypeOnly),
            "the production verdict must be found over the WHOLE set, not the truncated top-N"
        );
    }

    #[test]
    fn first_production_verdict_is_none_when_no_production_cycle() {
        // All test-only / unlabeled → no production example → None (never fabricated).
        assert_eq!(
            first_production_type_only(&[cyc(Some(CycleTestComposition::TestOnly)), cyc(None),]),
            None
        );
    }

    // ── TEST-EDGE-SCOPE-1B: D-TESB-17 row U7 ───────────────────────

    fn two_member_cycle(a: &str, b: &str) -> AgentCycle {
        AgentCycle {
            length: 2,
            modules: vec![a.into(), b.into()],
            test_composition: Some(CycleTestComposition::Production),
            type_only: None,
            walk: None,
        }
    }

    fn counts(n: u64) -> repo_graph_classification::import_partition::PartitionCounts {
        repo_graph_classification::import_partition::PartitionCounts {
            production_certain: n,
            ..Default::default()
        }
    }

    fn cycle_partition(
        members: &[&str],
        parts: Option<u64>,
    ) -> crate::storage_port::AgentCyclePartitions {
        crate::storage_port::AgentCyclePartitions {
            members: members.iter().map(|m| m.to_string()).collect(),
            short_members: members.iter().map(|m| m.to_string()).collect(),
            partitions: parts.map(counts),
        }
    }

    fn evidence_of(output: &AggregatorOutput) -> serde_json::Value {
        serde_json::to_value(&output.signals[0]).unwrap()["evidence"].clone()
    }

    /// An item whose members match no shown cycle, or more than one, or whose storage counts
    /// are absent, and any item beyond the partitions supplied, carries `partitions: null`
    /// beside a reason — never the key omitted, never zeros.
    #[test]
    fn cycle_items_without_matching_partition_evidence_carry_null_with_a_reason() {
        let partition = AgentImportCyclePartition {
            view: ImportView::DEFAULT,
            remainder: Default::default(),
            excluded_cycles: vec![],
            cycle_partitions: vec![
                cycle_partition(&["a", "b"], Some(4)),
                cycle_partition(&["e", "f"], Some(1)),
                cycle_partition(&["e", "f"], Some(2)),
                cycle_partition(&["g", "h"], None),
            ],
            importers: vec![],
        };
        let out = build_import_cycles_output(
            vec![
                two_member_cycle("a", "b"),
                two_member_cycle("c", "d"),
                two_member_cycle("e", "f"),
            ],
            &partition,
            &[],
        );
        let ev = evidence_of(&out);
        let cycles = ev["cycles"].as_array().unwrap();
        assert_eq!(cycles.len(), 3);
        let by = |m: &str| {
            cycles
                .iter()
                .find(|c| c["modules"][0] == m)
                .unwrap()
                .clone()
        };
        assert_eq!(by("a")["partitions"]["production_certain"], 4);
        assert!(by("a").get("partitions_unavailable").is_none());
        for m in ["c", "e"] {
            let c = by(m);
            assert!(
                c.get("partitions").is_some_and(serde_json::Value::is_null),
                "{m}: the key is present and null, never omitted: {c}"
            );
            assert!(
                c["partitions_unavailable"]
                    .as_str()
                    .is_some_and(|r| !r.is_empty()),
                "{m}: a reason names why: {c}"
            );
        }
        // Absent storage counts on a matched cycle: null with a reason too.
        let out = build_import_cycles_output(vec![two_member_cycle("g", "h")], &partition, &[]);
        let ev = evidence_of(&out);
        assert!(ev["cycles"][0]["partitions"].is_null());
        assert!(ev["cycles"][0]["partitions_unavailable"].is_string());
        // An item beyond the partitions supplied: null with a reason, never truncated away.
        let evidence = ImportCyclesEvidence {
            cycle_count: 2,
            production_count: None,
            test_only_count: None,
            unknown_count: None,
            production_type_only: None,
            cycles: vec![
                CycleEvidence {
                    length: 2,
                    modules: vec!["a".into(), "b".into()],
                    type_only: None,
                    walk: None,
                },
                CycleEvidence {
                    length: 2,
                    modules: vec!["x".into(), "y".into()],
                    type_only: None,
                    walk: None,
                },
            ],
        };
        let mut additions = cycle_partition_additions(&partition, &[], std::iter::empty());
        additions.item_partitions = vec![Some(counts(4))];
        let ev = serde_json::to_value(
            Signal::import_cycles(evidence).with_evidence_additions(additions),
        )
        .unwrap()["evidence"]
            .clone();
        assert_eq!(ev["cycles"][0]["partitions"]["production_certain"], 4);
        assert!(ev["cycles"][1]["partitions"].is_null());
        assert!(ev["cycles"][1]["partitions_unavailable"].is_string());
    }

    /// Whenever the evidence carries `import_view`, it carries `import_remainder`,
    /// `excluded_cycles` and `importer_test_status_undetermined` too — zeros included, and a
    /// missing one is written `null` (unreadable downstream), never omitted.
    #[test]
    fn cycle_evidence_carries_every_partition_key_whenever_it_carries_the_view() {
        let partition = AgentImportCyclePartition {
            view: ImportView::DEFAULT,
            remainder: Default::default(),
            excluded_cycles: vec![],
            cycle_partitions: vec![cycle_partition(&["a", "b"], Some(1))],
            importers: vec![],
        };
        let out = build_import_cycles_output(vec![two_member_cycle("a", "b")], &partition, &[]);
        let ev = evidence_of(&out);
        assert_eq!(
            ev["import_view"],
            serde_json::json!({"include_tests": false, "include_inferred": false})
        );
        assert_eq!(ev["import_remainder"]["tests"]["imports"], 0);
        assert_eq!(ev["excluded_cycles"], serde_json::json!([]));
        assert_eq!(ev["importer_test_status_undetermined"]["count"], 0);
        // A producer that set the view but not its siblings: the keys are present, null.
        let additions = EvidenceAdditions {
            import_view: Some(ImportView::DEFAULT),
            ..Default::default()
        };
        let evidence = ImportCyclesEvidence {
            cycle_count: 0,
            production_count: None,
            test_only_count: None,
            unknown_count: None,
            production_type_only: None,
            cycles: vec![],
        };
        let ev = serde_json::to_value(
            Signal::import_cycles(evidence).with_evidence_additions(additions),
        )
        .unwrap()["evidence"]
            .clone();
        for key in [
            "import_remainder",
            "excluded_cycles",
            "importer_test_status_undetermined",
        ] {
            assert!(
                ev.get(key).is_some_and(serde_json::Value::is_null),
                "{key} present and null: {ev}"
            );
        }
    }
}
