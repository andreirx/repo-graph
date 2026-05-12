//! Boundary links summary aggregator.
//!
//! Emits `BOUNDARY_LINKS_SUMMARY` with freshness state derived from
//! the `boundary_interaction_links` table. This is the first signal
//! backed by a freshness-tracked L2 table.
//!
//! Emission rule: always emit when there are any boundary links.
//! The signal carries link count in evidence and freshness state
//! via `Signal.freshness`.
//!
//! Freshness derivation:
//!   - any impacted → `Impacted`
//!   - else any unknown → `Unknown`
//!   - else `Current`

use super::AggregatorOutput;
use crate::dto::signal::{BoundaryLinksSummaryEvidence, FreshnessInfo, FreshnessStateDto, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::AgentStorageRead;

pub fn aggregate<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
    let freshness = storage.get_boundary_links_freshness(snapshot_uid)?;

    // No links → no signal (legitimate zero state, not an error).
    if freshness.total == 0 {
        return Ok(AggregatorOutput::empty());
    }

    let evidence = BoundaryLinksSummaryEvidence {
        link_count: freshness.total,
    };

    // Derive FreshnessInfo from counts.
    // Rule: any impacted → Impacted, else any unknown → Unknown, else Current.
    let state = if freshness.impacted > 0 {
        FreshnessStateDto::Impacted
    } else if freshness.unknown > 0 {
        FreshnessStateDto::Unknown
    } else {
        FreshnessStateDto::Current
    };

    let freshness_info = FreshnessInfo {
        state,
        impacted_since: freshness.earliest_impacted_at,
    };

    let signal = Signal::boundary_links_summary(evidence).with_freshness(freshness_info);

    Ok(AggregatorOutput {
        signals: vec![signal],
        limits: Vec::new(),
    })
}

// Unit tests removed — the mock is too large to maintain here.
// Integration tests in tests/ directory exercise this aggregator
// through the real storage adapter.
//
// The freshness derivation logic is simple (three if-else branches)
// and is covered by the signal.rs freshness DTO tests.
