//! Dependency-inverted read port for the gate crate.
//!
//! The `GateStorageRead` trait is defined here (policy side)
//! and implemented by the storage adapter crate. Gate's
//! `assemble` layer calls these methods to build a
//! `GateInput`; the pure `compute` layer never touches the
//! port.
//!
//! Design rules (mirrors the agent crate port):
//!
//!   1. No storage DTOs leak through the trait. Every return
//!      type is gate-owned (see `types.rs`).
//!
//!   2. Every method is read-only. No writes, no transactions,
//!      no schema mutations.
//!
//!   3. Method names reflect gate's vocabulary, not storage's.
//!      If the storage crate renames a query, the trait method
//!      name stays stable.
//!
//!   4. Errors are mapped to `GateStorageError` at the adapter
//!      boundary. The storage crate never exposes
//!      `StorageError`, `rusqlite::Error`, or SQL diagnostics
//!      across this trait.

use crate::errors::GateStorageError;
use crate::types::{
	GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
	GateRequirement, GateWaiver,
};

/// Narrow read port for the gate policy layer.
///
/// **Defined by the policy layer (gate crate). Implemented by
/// the adapter layer (storage crate).**
pub trait GateStorageRead {
	/// Return all active requirement declarations for a repo,
	/// parsed into gate-owned DTOs. Declarations with empty
	/// verification lists are skipped (mirrors the existing
	/// storage behavior at `get_active_requirement_declarations`).
	fn get_active_requirements(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateRequirement>, GateStorageError>;

	/// Return all active boundary declarations for a repo.
	fn get_boundary_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError>;

	/// Return IMPORTS edges between two file-path prefixes.
	/// Mirrors the storage query used by the existing
	/// `arch_violations` method.
	fn find_boundary_imports(
		&self,
		snapshot_uid: &str,
		source_prefix: &str,
		target_prefix: &str,
	) -> Result<Vec<GateImportEdge>, GateStorageError>;

	/// Return all `line_coverage` measurements for a snapshot.
	/// The assembler filters by target prefix after reading.
	fn get_coverage_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError>;

	/// Return all `cyclomatic_complexity` measurements for a
	/// snapshot. The assembler filters by target prefix after
	/// reading.
	fn get_complexity_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError>;

	/// Return all `hotspot_score` inferences for a snapshot.
	/// The assembler filters by target prefix (or not, when the
	/// obligation has no target) after reading.
	fn get_hotspot_inferences(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateInference>, GateStorageError>;

	/// Return active, non-expired waivers matching a given
	/// `(req_id, req_version, obligation_id)` tuple. `now` is
	/// an ISO 8601 timestamp used for expiry comparison.
	/// First-matching is caller's choice — the assembler keeps
	/// the full list so compute can make the decision.
	fn find_waivers(
		&self,
		repo_uid: &str,
		req_id: &str,
		req_version: i64,
		obligation_id: &str,
		now: &str,
	) -> Result<Vec<GateWaiver>, GateStorageError>;
}
