//! PERF-INSTRUMENTATION-1: real per-phase index durations.
//!
//! The orchestrator ([`crate::orchestrator::run_pipeline`]) is the only layer
//! that can see the REAL index phase boundaries — extraction, the storage
//! write calls, edge resolution, and snapshot finalization are all interleaved
//! inside the pipeline and are invisible at the `repo-index` compose seam (the
//! coarse `IndexProgressEvent` stream does not mark the write boundaries). So
//! the orchestrator measures them here and returns them on [`crate::types::IndexResult`];
//! the `repo-index` perf layer formats and emits the `[PERF] index …` summary.
//!
//! This is Layer-0 mechanical timing — wall-clock milliseconds of disjoint
//! sub-windows of one index operation. It is NEVER a trust, freshness, or
//! ownership signal. It is ephemeral diagnostic data: `#[serde(skip)]` on the
//! result field keeps it out of the serialized DTO / parity boundary entirely.
//!
//! # Phase model (honest labels)
//!
//! | field         | measures                                                              |
//! |---------------|-----------------------------------------------------------------------|
//! | `extract_ms`  | the per-file extraction loop only (pure compute; writes come after)    |
//! | `store_ms`    | the SUM of the real storage write calls (files, nodes, edges, modules) |
//! | `resolve_ms`  | edge-resolution compute (resolve window minus the writes inside it)    |
//! | `finalize_ms` | snapshot-metadata finalization (counts/diagnostics/status updates)     |
//!
//! `store_ms` is the storage-write total, NOT the trailing finalization — the
//! RMAPD-PERF-1 / build-0 derivation mislabeled the finalization tail as
//! `store`; this struct fixes that by timing the write calls at their real
//! sites. The four windows are disjoint and each is a sub-window of the
//! pipeline, so `extract_ms + store_ms + resolve_ms + finalize_ms <=
//! IndexResult::duration_ms` (the invariant orchestrator tests assert).

use serde::{Deserialize, Serialize};

/// Real per-phase index durations in milliseconds.
///
/// See the module docs for the honest meaning of each field. Defaults to all
/// zeros (the value used when perf is off and on the deserialize path, since
/// the carrying field is `#[serde(skip)]`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseTimings {
    /// Per-file extraction loop (pure extraction compute).
    pub extract_ms: u64,
    /// Edge-resolution compute (resolve window minus the writes inside it).
    pub resolve_ms: u64,
    /// Sum of the real storage write calls (files, nodes, extraction edges,
    /// resolved edges, unresolved edges, module nodes/edges).
    pub store_ms: u64,
    /// Snapshot-metadata finalization (counts, diagnostics, status updates).
    pub finalize_ms: u64,
}
