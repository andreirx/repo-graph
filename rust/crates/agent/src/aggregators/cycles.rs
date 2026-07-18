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
use crate::dto::signal::{CycleEvidence, ImportCyclesEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::ordering;
use crate::storage_port::{AgentCancelCheck, AgentStorageRead};

const CYCLE_TOP_N: usize = 3;

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
    let top: Vec<CycleEvidence> = cycles
        .into_iter()
        .take(CYCLE_TOP_N)
        .map(|c| CycleEvidence {
            length: c.length,
            modules: c.modules,
        })
        .collect();

    let evidence = ImportCyclesEvidence {
        cycle_count,
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
    let top: Vec<CycleEvidence> = cycles
        .into_iter()
        .take(CYCLE_TOP_N)
        .map(|c| CycleEvidence {
            length: c.length,
            modules: c.modules,
        })
        .collect();

    let evidence = ImportCyclesEvidence {
        cycle_count,
        cycles: top,
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::import_cycles(evidence)],
        limits: Vec::new(),
    })
}
