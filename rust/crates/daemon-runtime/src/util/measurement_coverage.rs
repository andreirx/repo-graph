//! Shared per-language measurement-coverage assembly for the daemon's complexity
//! surfaces (METRIC-LANG-COVERAGE-1 part A).
//!
//! `orient` (via `build_orient_envelope`) and `hotspots` (its handler) each hold a
//! per-operation [`StorageConnection`] and must carry the coverage block. This seam
//! turns that connection into the serialized [`MeasurementCoverageBlock`]: it runs the
//! storage count query and the pure `classification` verdict, then serializes. Keeping
//! it here (not inlined twice) keeps the two surfaces byte-identical.
//! [abstraction: coverage-JSON adapter; users: orient + hotspots handlers; axis: none
//! beyond DRY across the two daemon call sites; rejected: inline the query+verdict+
//! serialize in each handler (drift risk on a honesty surface).]
//!
//! The block is ALWAYS PRESENT (review-6 item 2): a query failure yields the explicit
//! `Unavailable` block, never a dropped one — "coverage is part of the fact" (VISION);
//! a missing block would read as complete coverage.
//!
//! The `metrics` command lives in `rgr` (direct storage) and composes the same
//! `MeasurementCoverageBlock::from_result` + `into_json_value` itself.

use repo_graph_classification::measurement_coverage::MeasurementCoverageBlock;
use repo_graph_storage::StorageConnection;

/// The always-present `measurement_coverage` block for `snapshot_uid`, serialized and
/// ready to drop onto a response value. On a storage-read failure this is the explicit
/// `Unavailable` block (never absent — the review-6 honesty fix).
pub fn measurement_coverage_json(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> serde_json::Value {
    MeasurementCoverageBlock::from_result(storage.query_measurement_coverage(snapshot_uid))
        .into_json_value()
}

/// The explicit `Unavailable` coverage block, for when even OPENING storage failed — a
/// complexity-bearing surface still carries a present, honest block rather than a silent
/// gap. Used by `build_orient_envelope` on a `RepoState::storage()` error.
pub fn measurement_coverage_unavailable_json() -> serde_json::Value {
    MeasurementCoverageBlock::unavailable().into_json_value()
}
