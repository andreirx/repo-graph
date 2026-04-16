//! Adapter impl: `GateStorageRead` on `StorageConnection`.
//!
//! Added in Rust-43A as part of the `rgr/src/gate.rs`
//! relocation into `repo-graph-gate`. The gate crate defines
//! `GateStorageRead` (policy); this file is the adapter side
//! that lets the gate policy read SQLite through a
//! storage-agnostic port.
//!
//! Responsibilities:
//!
//!   1. Translate storage errors into `GateStorageError`. The
//!      gate crate never sees `rusqlite::Error`, `StorageError`,
//!      table names, or SQL diagnostics.
//!
//!   2. Map storage row DTOs (e.g. `queries::WaiverDeclaration`,
//!      `queries::MeasurementRow`, `queries::InferenceRow`,
//!      `queries::BoundaryDeclaration`, `queries::ImportEdgeResult`,
//!      `queries::RequirementDeclaration`) into gate-owned
//!      DTOs. No storage types leak through the trait.
//!
//! This adapter is additive. It does not remove the existing
//! storage query methods — the pre-relocation `rmap gate`
//! CLI still needed to work during the relocation commit, and
//! the CLI's `run_gate` now calls into `repo_graph_gate`
//! through this adapter.

use repo_graph_gate::{
	GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
	GateObligation, GateRequirement, GateStorageError, GateStorageRead,
	GateWaiver,
};

use crate::connection::StorageConnection;

// ── Error mapping helper ─────────────────────────────────────────

/// Convert any `Display`-able error into a `GateStorageError`
/// tagged with the supplied operation identifier. The message
/// body is the error's `Display` output — storage diagnostics
/// are stringified at this boundary and never parsed by the
/// gate layer.
fn map_err<E: std::fmt::Display>(
	operation: &'static str,
) -> impl FnOnce(E) -> GateStorageError {
	move |e| GateStorageError::new(operation, e.to_string())
}

// ── Impl ─────────────────────────────────────────────────────────

impl GateStorageRead for StorageConnection {
	fn get_active_requirements(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateRequirement>, GateStorageError> {
		let rows = self
			.get_active_requirement_declarations(repo_uid)
			.map_err(map_err("get_active_requirements"))?;
		Ok(rows
			.into_iter()
			.map(|r| GateRequirement {
				req_id: r.req_id,
				version: r.version,
				obligations: r
					.obligations
					.into_iter()
					.map(|o| GateObligation {
						obligation_id: o.obligation_id,
						obligation: o.obligation,
						method: o.method,
						target: o.target,
						threshold: o.threshold,
						operator: o.operator,
					})
					.collect(),
			})
			.collect())
	}

	fn get_boundary_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError> {
		let rows = self
			.get_active_boundary_declarations(repo_uid)
			.map_err(map_err("get_boundary_declarations"))?;
		Ok(rows
			.into_iter()
			.map(|b| GateBoundaryDeclaration {
				boundary_module: b.boundary_module,
				forbids: b.forbids,
				reason: b.reason,
			})
			.collect())
	}

	fn find_boundary_imports(
		&self,
		snapshot_uid: &str,
		source_prefix: &str,
		target_prefix: &str,
	) -> Result<Vec<GateImportEdge>, GateStorageError> {
		let rows = self
			.find_imports_between_paths(snapshot_uid, source_prefix, target_prefix)
			.map_err(map_err("find_boundary_imports"))?;
		Ok(rows
			.into_iter()
			.map(|e| GateImportEdge {
				source_file: e.source_file,
				target_file: e.target_file,
			})
			.collect())
	}

	fn get_coverage_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError> {
		let rows = self
			.query_measurements_by_kind(snapshot_uid, "line_coverage")
			.map_err(map_err("get_coverage_measurements"))?;
		Ok(rows
			.into_iter()
			.map(|m| GateMeasurement {
				target_stable_key: m.target_stable_key,
				value_json: m.value_json,
			})
			.collect())
	}

	fn get_complexity_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError> {
		let rows = self
			.query_measurements_by_kind(snapshot_uid, "cyclomatic_complexity")
			.map_err(map_err("get_complexity_measurements"))?;
		Ok(rows
			.into_iter()
			.map(|m| GateMeasurement {
				target_stable_key: m.target_stable_key,
				value_json: m.value_json,
			})
			.collect())
	}

	fn get_hotspot_inferences(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateInference>, GateStorageError> {
		let rows = self
			.query_inferences_by_kind(snapshot_uid, "hotspot_score")
			.map_err(map_err("get_hotspot_inferences"))?;
		Ok(rows
			.into_iter()
			.map(|i| GateInference {
				target_stable_key: i.target_stable_key,
				value_json: i.value_json,
			})
			.collect())
	}

	fn find_waivers(
		&self,
		repo_uid: &str,
		req_id: &str,
		req_version: i64,
		obligation_id: &str,
		now: &str,
	) -> Result<Vec<GateWaiver>, GateStorageError> {
		let rows = self
			.find_active_waivers(repo_uid, req_id, req_version, obligation_id, now)
			.map_err(map_err("find_waivers"))?;
		Ok(rows
			.into_iter()
			.map(|w| GateWaiver {
				waiver_uid: w.declaration_uid,
				reason: w.reason,
				created_at: w.created_at,
				created_by: w.created_by,
				expires_at: w.expires_at,
				rationale_category: w.rationale_category,
				policy_basis: w.policy_basis,
			})
			.collect())
	}
}
