//! Blast-radius derivation for unresolved edges.
//!
//! Mirror of `src/core/classification/blast-radius.ts`. Pure
//! function, no I/O, no state. Computes a `BlastRadiusAssessment`
//! from three input signals: the edge's category, the classifier's
//! basis code, and the source node's visibility.
//!
//! Scope (first slice):
//!   - defined for CALLS-family unresolved edges only
//!   - other categories return `{ blastRadius: NotApplicable }`
//!   - entrypoint-path detection is deferred (uses visibility only)

use crate::types::{
	BlastRadiusAssessment, BlastRadiusLevel, EnclosingScopeSignificance,
	ReceiverOrigin, UnresolvedEdgeBasisCode, UnresolvedEdgeCategory,
};

/// Derive the blast-radius assessment for a single unresolved edge.
///
/// Mirror of `deriveBlastRadius` from `blast-radius.ts:76`.
///
/// Pure function. All inputs are plain data from the sample row.
/// Returns `NotApplicable` for non-CALLS categories.
///
/// This function is re-exported as `pub` from the crate root
/// because it is one of the 6 locked public entry points.
pub fn derive_blast_radius(
	category: UnresolvedEdgeCategory,
	basis_code: UnresolvedEdgeBasisCode,
	source_node_visibility: Option<&str>,
) -> BlastRadiusAssessment {
	if !category.is_calls_category() {
		return BlastRadiusAssessment {
			receiver_origin: ReceiverOrigin::NotApplicable,
			enclosing_scope_significance: EnclosingScopeSignificance::UnknownScope,
			blast_radius: BlastRadiusLevel::NotApplicable,
		};
	}

	let origin = derive_receiver_origin(basis_code);
	let scope = derive_enclosing_scope_significance(source_node_visibility);
	let radius = compute_radius(origin, scope);

	BlastRadiusAssessment {
		receiver_origin: origin,
		enclosing_scope_significance: scope,
		blast_radius: radius,
	}
}

/// Map basis_code to receiver origin.
///
/// Mirrors `deriveReceiverOrigin` from `blast-radius.ts:106`.
fn derive_receiver_origin(
	basis_code: UnresolvedEdgeBasisCode,
) -> ReceiverOrigin {
	match basis_code {
		UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal => {
			ReceiverOrigin::ThisBound
		}
		UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport
		| UnresolvedEdgeBasisCode::CalleeMatchesExternalImport => {
			ReceiverOrigin::ImportBoundExternal
		}
		UnresolvedEdgeBasisCode::ReceiverMatchesInternalImport
		| UnresolvedEdgeBasisCode::CalleeMatchesInternalImport
		| UnresolvedEdgeBasisCode::SpecifierMatchesProjectAlias => {
			ReceiverOrigin::ImportBoundInternal
		}
		UnresolvedEdgeBasisCode::ReceiverMatchesSameFileSymbol
		| UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol => {
			ReceiverOrigin::SameFileDeclared
		}
		UnresolvedEdgeBasisCode::ReceiverMatchesRuntimeGlobal
		| UnresolvedEdgeBasisCode::CalleeMatchesRuntimeGlobal => {
			ReceiverOrigin::RuntimeGlobal
		}
		UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule => {
			ReceiverOrigin::RuntimeModule
		}
		UnresolvedEdgeBasisCode::NoSupportingSignal => {
			ReceiverOrigin::LocalLike
		}
		// All other basis codes (package dep, relative import,
		// Rust heuristic, Express, etc.) fall to local_like.
		// Mirrors the TS `default: return "local_like"` case.
		_ => ReceiverOrigin::LocalLike,
	}
}

/// Map source node visibility to enclosing-scope significance.
///
/// Mirrors `deriveEnclosingScopeSignificance` from
/// `blast-radius.ts:132`.
fn derive_enclosing_scope_significance(
	visibility: Option<&str>,
) -> EnclosingScopeSignificance {
	match visibility {
		Some("export") | Some("public") => {
			EnclosingScopeSignificance::ExportedPublic
		}
		Some("private") | Some("protected") | Some("internal") | None => {
			EnclosingScopeSignificance::InternalPrivate
		}
		_ => EnclosingScopeSignificance::UnknownScope,
	}
}

/// Combine receiver origin + enclosing scope into a blast-radius
/// level.
///
/// Mirrors `computeRadius` from `blast-radius.ts:164`.
///
/// Principles:
///   - `local_like + internal_private` = LOW (function-local noise)
///   - `local_like + exported_public` = MEDIUM (local uncertainty
///     on a public path)
///   - `import_bound_*` = MEDIUM (cross-module reference) or LOW
///     (external — well-understood)
///   - `runtime_global / runtime_module` = LOW
///   - `same_file_declared` = LOW
///   - `this_bound + internal` = LOW, `this_bound + exported` = MEDIUM
///
/// Entrypoint-path detection (→ HIGH override) is deferred. No
/// edge currently reaches HIGH.
fn compute_radius(
	origin: ReceiverOrigin,
	scope: EnclosingScopeSignificance,
) -> BlastRadiusLevel {
	match origin {
		ReceiverOrigin::LocalLike | ReceiverOrigin::ThisBound => {
			if scope == EnclosingScopeSignificance::ExportedPublic {
				BlastRadiusLevel::Medium
			} else {
				BlastRadiusLevel::Low
			}
		}
		ReceiverOrigin::ImportBoundInternal => BlastRadiusLevel::Medium,
		ReceiverOrigin::ImportBoundExternal
		| ReceiverOrigin::SameFileDeclared
		| ReceiverOrigin::RuntimeGlobal
		| ReceiverOrigin::RuntimeModule => BlastRadiusLevel::Low,
		ReceiverOrigin::NotApplicable => BlastRadiusLevel::NotApplicable,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// ── Receiver origin mapping ──────────────────────────────

	fn calls_category() -> UnresolvedEdgeCategory {
		UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo
	}

	#[test]
	fn this_receiver_implies_internal_maps_to_this_bound() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::ThisBound);
	}

	#[test]
	fn callee_matches_external_import_maps_to_import_bound_external() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::CalleeMatchesExternalImport,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::ImportBoundExternal);
	}

	#[test]
	fn receiver_matches_internal_import_maps_to_import_bound_internal() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::ReceiverMatchesInternalImport,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::ImportBoundInternal);
	}

	#[test]
	fn specifier_matches_project_alias_maps_to_import_bound_internal() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::SpecifierMatchesProjectAlias,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::ImportBoundInternal);
	}

	#[test]
	fn callee_matches_same_file_symbol_maps_to_same_file_declared() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::SameFileDeclared);
	}

	#[test]
	fn callee_matches_runtime_global_maps_to_runtime_global() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::CalleeMatchesRuntimeGlobal,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::RuntimeGlobal);
	}

	#[test]
	fn specifier_matches_runtime_module_maps_to_runtime_module() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::RuntimeModule);
	}

	#[test]
	fn no_supporting_signal_maps_to_local_like() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			None,
		);
		assert_eq!(r.receiver_origin, ReceiverOrigin::LocalLike);
	}

	// ── Enclosing scope significance ─────────────────────────

	#[test]
	fn export_visibility_is_exported_public() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			Some("export"),
		);
		assert_eq!(
			r.enclosing_scope_significance,
			EnclosingScopeSignificance::ExportedPublic
		);
	}

	#[test]
	fn private_visibility_is_internal_private() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			Some("private"),
		);
		assert_eq!(
			r.enclosing_scope_significance,
			EnclosingScopeSignificance::InternalPrivate
		);
	}

	#[test]
	fn null_visibility_is_internal_private() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			None,
		);
		assert_eq!(
			r.enclosing_scope_significance,
			EnclosingScopeSignificance::InternalPrivate
		);
	}

	// ── Radius computation ───────────────────────────────────

	#[test]
	fn local_like_internal_private_is_low() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			Some("private"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Low);
	}

	#[test]
	fn local_like_exported_public_is_medium() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			Some("export"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Medium);
	}

	#[test]
	fn this_bound_internal_private_is_low() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal,
			Some("private"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Low);
	}

	#[test]
	fn this_bound_exported_public_is_medium() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal,
			Some("export"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Medium);
	}

	#[test]
	fn import_bound_internal_is_medium_regardless_of_scope() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::ReceiverMatchesInternalImport,
			Some("private"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Medium);
	}

	#[test]
	fn import_bound_external_is_low() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::CalleeMatchesExternalImport,
			Some("export"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Low);
	}

	#[test]
	fn same_file_declared_is_low() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol,
			Some("export"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Low);
	}

	#[test]
	fn runtime_global_is_low() {
		let r = derive_blast_radius(
			calls_category(),
			UnresolvedEdgeBasisCode::CalleeMatchesRuntimeGlobal,
			Some("export"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::Low);
	}

	// ── Non-CALLS categories ─────────────────────────────────

	#[test]
	fn imports_category_is_not_applicable() {
		let r = derive_blast_radius(
			UnresolvedEdgeCategory::ImportsFileNotFound,
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			Some("export"),
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::NotApplicable);
		assert_eq!(r.receiver_origin, ReceiverOrigin::NotApplicable);
	}

	#[test]
	fn instantiates_category_is_not_applicable() {
		let r = derive_blast_radius(
			UnresolvedEdgeCategory::InstantiatesClassNotFound,
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			None,
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::NotApplicable);
	}

	#[test]
	fn other_category_is_not_applicable() {
		let r = derive_blast_radius(
			UnresolvedEdgeCategory::Other,
			UnresolvedEdgeBasisCode::NoSupportingSignal,
			None,
		);
		assert_eq!(r.blast_radius, BlastRadiusLevel::NotApplicable);
	}
}
