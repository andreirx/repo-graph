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
use crate::dto::signal::{CycleEvidence, ImportCyclesEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::ordering;
use crate::storage_port::{AgentCancelCheck, AgentCycle, AgentStorageRead};

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
    let mut cycles = storage.find_module_cycles_cancellable(snapshot_uid, cancel)?;

    if cycles.is_empty() {
        return Ok(AggregatorOutput::empty());
    }

    ordering::canonicalize_cycles(&mut cycles);
    let cycle_count = cycles.len() as u64;
    // ORIENT-CYCLES-DISAGREE-1: partition BEFORE the top-3 truncation — the split is over the
    // WHOLE cycle set, not the rendered anchors.
    let split = headline_split(&cycles);
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
        })
        .collect();

    let evidence = ImportCyclesEvidence {
        cycle_count,
        production_count: split.map(|p| p.production_count),
        test_only_count: split.map(|p| p.test_only_count),
        unknown_count: split.map(|p| p.unknown_count),
        cycles: top,
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::import_cycles(evidence)],
        limits: Vec::new(),
    })
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
pub fn aggregate_path_cancellable<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    path_prefix: &str,
    cancel: AgentCancelCheck<'_>,
) -> Result<AggregatorOutput, AgentStorageError> {
    let mut cycles =
        storage.find_cycles_involving_path_cancellable(snapshot_uid, path_prefix, cancel)?;

    if cycles.is_empty() {
        return Ok(AggregatorOutput::empty());
    }

    ordering::canonicalize_cycles(&mut cycles);
    let cycle_count = cycles.len() as u64;
    // ORIENT-CYCLES-DISAGREE-1: path-scoped cycles are read via `find_cycles_involving_path`,
    // which the storage adapter does NOT test-composition-label (this is a focus surface, not
    // the repo headline the slice unifies) — so `test_composition` is `None` here and the split
    // is `None`. The evidence then carries only the raw total, byte-identical to before.
    let split = headline_split(&cycles);
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
        })
        .collect();

    let evidence = ImportCyclesEvidence {
        cycle_count,
        production_count: split.map(|p| p.production_count),
        test_only_count: split.map(|p| p.test_only_count),
        unknown_count: split.map(|p| p.unknown_count),
        cycles: top,
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::import_cycles(evidence)],
        limits: Vec::new(),
    })
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
}
