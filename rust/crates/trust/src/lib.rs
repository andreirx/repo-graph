//! repo-graph-trust — Rust port of the trust reporting substrate.
//!
//! This crate is the Rust-side mirror of the TypeScript trust
//! modules at `src/core/trust/`. It is built incrementally as
//! the Rust-4 trust reporting parity slice progresses.
//!
//! Slice substep state (Rust-4):
//!   - R4-A workspace skeleton ........... done
//!   - R4-B trust types .................. done
//!   - R4-C trust rules .................. done
//!   - R4-D supporting storage types ..... done
//!   - R4-E supporting storage (simple) .. done
//!   - R4-F supporting storage (complex) . done
//!   - R4-G trust service ................ done
//!   - R4-H parity harness ............... done
//!   - R4-I script integration ........... done
//!   - R4-J final acceptance gate ........ done
//!
//! ── Architecture (D-4-1 override: trait abstraction) ──────────
//!
//! This crate is POLICY code. It follows the Clean Architecture
//! dependency rule: policy defines the interface, mechanism
//! implements it. Specifically:
//!
//!   - `repo-graph-trust` defines the `TrustStorageRead` trait
//!     (the narrow read port trust needs). This trait lives in
//!     the trust crate, not in the storage crate, because the
//!     policy layer owns the interface definition.
//!
//!   - `repo-graph-storage` (the adapter/mechanism crate) adds
//!     `repo-graph-trust` as a dependency and implements the
//!     trait on `StorageConnection`. The dependency direction is
//!     adapter → policy (outer → inner), which is correct.
//!
//!   - `repo-graph-trust` does NOT depend on `repo-graph-storage`.
//!     The pure trust computation has no knowledge of SQLite,
//!     rusqlite, or `StorageConnection`. It receives its data
//!     through the trait abstraction.
//!
//!   - `repo-graph-trust` DOES depend on
//!     `repo-graph-classification` for the classification types
//!     it consumes (UnresolvedEdgeCategory, BlastRadiusAssessment,
//!     etc.). This is policy → policy, which follows the
//!     dependency rule.
//!
//! ── Two-layer service design ──────────────────────────────────
//!
//! The trust service is split into two layers:
//!
//!   1. **Pure report computation** — `compute_trust_report`
//!      takes a `TrustComputationInput` (a fully-assembled data
//!      bundle with no I/O dependencies) and produces a
//!      `TrustReport`. This layer applies detection rules,
//!      reliability formulas, and diagnostic aggregation. It is
//!      pure: no storage import, no SQL knowledge, no adapter
//!      types.
//!
//!   2. **Storage-backed input assembly** — a thin orchestration
//!      function that takes a `&dyn TrustStorageRead` implementor
//!      and builds the `TrustComputationInput` by pulling raw
//!      data through the trait. This function still lives in the
//!      trust crate (it is policy-layer orchestration, not
//!      adapter code), but it depends on the `TrustStorageRead`
//!      trait to abstract over the concrete storage backend.
//!
//! This split preserves the Clean Architecture boundary while
//! proving real composition of the already-ported storage and
//! classification substrates. The TS `computeTrustReport`
//! function in `service.ts` is effectively this same split,
//! just not made explicit because TS uses the `StoragePort`
//! interface at the function boundary rather than separating
//! the data-assembly phase from the computation phase.
//!
//! ── Public API (locked at Rust-4 lock phase) ──────────────────
//!
//! ```text
//! pub mod types;
//! pub trait TrustStorageRead { ... }
//! pub fn compute_trust_report(input: &TrustComputationInput) -> TrustReport;
//! // + assembly function that takes &dyn TrustStorageRead
//! ```
//!
//! Pure rules/formulas are public only if they are genuine stable
//! domain primitives. Implementation helpers stay `pub(crate)`.
//!
//! No `HashMap` in the public API. Deterministic collections only
//! (`Vec<T>`, sorted DTOs). Same rule as R3 classification DTOs.
//!
//! ── Storage expansion scope (locked, corrected at R4-D) ───────
//!
//! Exactly these 8 read-only trait methods are defined in
//! `TrustStorageRead` and implemented by the storage crate:
//!
//!   - get_snapshot_extraction_diagnostics
//!   - count_edges_by_type
//!   - count_active_declarations  (narrowed from full declarations to count-only)
//!   - count_unresolved_edges_by_classification  (renamed from generic
//!     count_unresolved_edges; now returns typed ClassificationCountRow
//!     instead of stringly-keyed rows)
//!   - query_unresolved_edges
//!   - find_path_prefix_module_cycles
//!   - compute_module_stats
//!   - get_file_paths_by_repo  (narrowed from full TrackedFile to paths-only)
//!
//! The original 7-method lock was refined at R4-D:
//!   - `get_active_declarations` narrowed to `count_active_declarations`
//!     (service only calls `.length`)
//!   - `count_unresolved_edges` specialized to
//!     `count_unresolved_edges_by_classification` (service only groups
//!     by classification; typed return key eliminates raw-string
//!     comparison)
//!   - `get_file_paths_by_repo` added (narrowed from `getFilesByRepo`;
//!     service only extracts `.path`)
//!
//! No write methods. No raw-SQL escape-hatch widening. No broader
//! CRUD expansion. No schema changes. No migrations.
//!
//! ── Explicit deferrals (NOT part of Rust-4) ───────────────────
//!
//!   - Tree-sitter / extractors
//!   - Indexer / daemon orchestration
//!   - CLI formatting (trust produces TrustReport; CLI adapts it)
//!   - Write-side storage methods
//!   - Schema changes / new migrations
//!   - better-sqlite3 debt
//!
//! ── Narrow lock deviation (R4-G) ─────────────────────────────
//!
//!   - `humanLabelForCategory` was listed as "display-time
//!     rendering, stays TS" in the original R4 lock. Ported as
//!     `pub(crate) fn human_label_for_category` in `service.rs`
//!     because the TS `computeTrustReport` calls it inside the
//!     report builder to populate `TrustCategoryRow.label`. The
//!     label is part of the `TrustReport` DTO parity surface.
//!     The function stays crate-private — this is a narrow
//!     DTO-completeness exception, not a general invitation to
//!     port display helpers.

pub mod overlay;
pub(crate) mod rules;
pub mod service;
pub mod storage_port;
pub mod types;

// ── Public re-exports (locked R4 API surface) ─────────────────
//
// Detection rules and reliability formulas are genuine stable
// domain primitives — public. Diagnostic helpers (sum_unresolved_*,
// count_suspicious_*, group_path_prefix_*) are implementation
// vocabulary consumed by the service — pub(crate).
//
// The TrustStorageRead trait is public because the storage crate
// (adapter layer) needs to import it to provide the impl. The
// trait + its supporting DTOs live in the `storage_port` module.

pub use rules::{
	compute_call_graph_reliability, compute_change_impact_reliability,
	compute_dead_code_reliability, compute_import_graph_reliability,
	detect_alias_resolution_suspicion, detect_framework_heavy_suspicion,
	detect_missing_entrypoint_declarations, detect_registry_pattern_suspicion,
};
pub use service::{
	assemble_trust_report, compute_trust_report, TrustAssemblyError,
	TrustComputationInput,
};
pub use storage_port::TrustStorageRead;

// Trust overlay for query surfaces (inline trust in responses).
//
// Two layers:
// 1. TrustOverlaySummary — repo/snapshot-level trust context
// 2. DeadResultTrust — per-candidate dead-code confidence
//
// EdgeResultTrust remains pub(crate) until callers/callees contracts emit it.
pub use overlay::{
    assess_dead_confidence, DeadResultTrust, ResultConfidence, TrustOverlaySummary,
};
