//! Module-cycle aggregator.
//!
//! Calls `AgentStorageRead::find_module_cycles` and emits
//! `IMPORT_CYCLES` when at least one cycle is present. Evidence
//! carries the total cycle count plus the first 3 cycles as
//! summaries. The "top 3" slice is deterministic because the
//! port contract guarantees canonicalized, deduplicated output
//! ordered by internal UID.

use super::AggregatorOutput;
use crate::dto::signal::{CycleEvidence, ImportCyclesEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::AgentStorageRead;

const CYCLE_TOP_N: usize = 3;

pub fn aggregate<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let cycles = storage.find_module_cycles(snapshot_uid)?;

	if cycles.is_empty() {
		return Ok(AggregatorOutput::empty());
	}

	let cycle_count = cycles.len() as u64;
	let top: Vec<CycleEvidence> = cycles
		.into_iter()
		.take(CYCLE_TOP_N)
		.map(|c| CycleEvidence { length: c.length, modules: c.modules })
		.collect();

	let evidence = ImportCyclesEvidence { cycle_count, cycles: top };

	Ok(AggregatorOutput {
		signals: vec![Signal::import_cycles(evidence)],
		limits: Vec::new(),
	})
}
