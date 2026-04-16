//! Trust aggregator.
//!
//! Emits up to three signals from one trust summary projection:
//!
//!   - `TRUST_LOW_RESOLUTION` when call resolution rate < 0.20
//!   - `TRUST_STALE_SNAPSHOT` when `get_stale_files` returned
//!     any files (see Sub-Decision B1 for wording discipline)
//!   - `TRUST_NO_ENRICHMENT` when the enrichment phase did NOT
//!     run (enrichment_state == NotRun). Not when the phase ran
//!     and resolved nothing. Not when eligible count was zero.
//!
//! Returns both the aggregator output AND the raw
//! `AgentTrustSummary` + stale flag, because the orient pipeline
//! also needs them for confidence derivation and to gate the
//! dead-code aggregator on trust reliability. Returning them
//! avoids a second round-trip through the port.

use super::AggregatorOutput;
use crate::dto::signal::{
	Signal, TrustLowResolutionEvidence, TrustNoEnrichmentEvidence,
	TrustStaleSnapshotEvidence,
};
use crate::errors::AgentStorageError;
use crate::storage_port::{
	AgentStorageRead, AgentTrustSummary, EnrichmentState,
};

/// Threshold below which call resolution rate is flagged as low.
const LOW_RESOLUTION_THRESHOLD: f64 = 0.20;

pub struct TrustAggregateResult {
	pub output: AggregatorOutput,
	pub summary: AgentTrustSummary,
	pub stale: bool,
}

pub fn aggregate<S: AgentStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
) -> Result<TrustAggregateResult, AgentStorageError> {
	let summary = storage.get_trust_summary(repo_uid, snapshot_uid)?;
	let stale_files = storage.get_stale_files(snapshot_uid)?;
	let stale = !stale_files.is_empty();

	let mut signals: Vec<Signal> = Vec::new();

	// TRUST_LOW_RESOLUTION
	let total_calls = summary.resolved_calls + summary.unresolved_calls;
	if total_calls > 0 && summary.call_resolution_rate < LOW_RESOLUTION_THRESHOLD {
		signals.push(Signal::trust_low_resolution(
			TrustLowResolutionEvidence {
				resolution_rate: summary.call_resolution_rate,
				resolved_count: summary.resolved_calls,
				total_count: total_calls,
			},
		));
	}

	// TRUST_STALE_SNAPSHOT
	if stale {
		signals.push(Signal::trust_stale_snapshot(
			TrustStaleSnapshotEvidence {
				stale_file_count: stale_files.len() as u64,
				snapshot_uid: snapshot_uid.to_string(),
			},
		));
	}

	// TRUST_NO_ENRICHMENT — fires iff the enrichment phase did
	// not run. `Ran` (with any enriched count) and
	// `NotApplicable` (phase executed with nothing to do) are
	// both silent on this axis.
	if summary.enrichment_state == EnrichmentState::NotRun {
		signals.push(Signal::trust_no_enrichment(TrustNoEnrichmentEvidence {
			enrichment_eligible: summary.enrichment_eligible,
			enrichment_enriched: summary.enrichment_enriched,
		}));
	}

	Ok(TrustAggregateResult {
		output: AggregatorOutput { signals, limits: Vec::new() },
		summary,
		stale,
	})
}
