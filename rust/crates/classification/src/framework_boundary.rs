//! Framework-boundary post-classification pass.
//!
//! Mirror of `src/core/classification/framework-boundary.ts`.
//!
//! Runs AFTER the generic classifier and may reclassify selected
//! unresolved edges from their generic bucket (typically `unknown`)
//! to `framework_boundary_candidate` when the edge matches a known
//! runtime-wiring / registration pattern.
//!
//! First-slice detectors: Express route/middleware registration.
//!
//! Pure function. No I/O, no state.

use crate::types::{
	ClassifierVerdict, ImportBinding, UnresolvedEdgeBasisCode,
	UnresolvedEdgeCategory, UnresolvedEdgeClassification,
};

// ── Express lookup tables ────────────────────────────────────────

const EXPRESS_ROUTE_METHODS: &[&str] = &[
	"get", "post", "put", "delete", "patch", "options", "head", "all", "route",
];

const EXPRESS_MIDDLEWARE_METHODS: &[&str] = &["use"];

const EXPRESS_LIFECYCLE_METHODS: &[&str] = &["listen"];

const EXPRESS_SPECIFIERS: &[&str] = &["express", "@types/express"];

const EXPRESS_RECEIVER_NAMES: &[&str] = &["app", "router", "server"];

fn file_has_express_import(import_bindings: &[ImportBinding]) -> bool {
	import_bindings
		.iter()
		.any(|b| EXPRESS_SPECIFIERS.contains(&b.specifier.as_str()))
}

// ── Public interface ─────────────────────────────────────────────

/// Attempt to reclassify a single unresolved edge as a framework
/// boundary observation.
///
/// Returns `Some(ClassifierVerdict)` with classification =
/// `FrameworkBoundaryCandidate` if the edge matches a known
/// pattern, or `None` if no pattern applies. The classification
/// variant is always `FrameworkBoundaryCandidate` when `Some`.
///
/// Mirror of `detectFrameworkBoundary` from
/// `framework-boundary.ts:102`.
///
/// This function is re-exported as `pub` from the crate root.
pub fn detect_framework_boundary(
	target_key: &str,
	category: UnresolvedEdgeCategory,
	import_bindings: &[ImportBinding],
) -> Option<ClassifierVerdict> {
	// Only CALLS_OBJ_METHOD_NEEDS_TYPE_INFO (receiver.method() shape).
	if category != UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo {
		return None;
	}

	// Extract receiver and method from targetKey.
	let dot_idx = target_key.find('.')?;
	if dot_idx == 0 {
		return None;
	}
	let receiver = &target_key[..dot_idx];
	let method_part = &target_key[dot_idx + 1..];
	// For chained calls like app.route('/x').get(...), take the
	// first method segment only.
	let first_method = method_part
		.split('.')
		.next()
		.unwrap_or(method_part);

	// Express detection requires THREE signals:
	//   (a) the file imports from "express" or "@types/express"
	//   (b) the receiver name is a conventional Express app/router name
	//   (c) the method name is a known registration method
	if file_has_express_import(import_bindings)
		&& EXPRESS_RECEIVER_NAMES.contains(&receiver)
	{
		if EXPRESS_MIDDLEWARE_METHODS.contains(&first_method) {
			return Some(ClassifierVerdict {
				classification: UnresolvedEdgeClassification::FrameworkBoundaryCandidate,
				basis_code: UnresolvedEdgeBasisCode::ExpressMiddlewareRegistration,
			});
		}
		if EXPRESS_ROUTE_METHODS.contains(&first_method)
			|| EXPRESS_LIFECYCLE_METHODS.contains(&first_method)
		{
			return Some(ClassifierVerdict {
				classification: UnresolvedEdgeClassification::FrameworkBoundaryCandidate,
				basis_code: UnresolvedEdgeBasisCode::ExpressRouteRegistration,
			});
		}
	}

	None
}

#[cfg(test)]
mod tests {
	use super::*;

	fn express_import() -> ImportBinding {
		ImportBinding {
			identifier: "express".into(),
			specifier: "express".into(),
			is_relative: false,
			location: None,
			is_type_only: false,
			imported_name: None,
		}
	}

	fn types_express_import() -> ImportBinding {
		ImportBinding {
			identifier: "Express".into(),
			specifier: "@types/express".into(),
			is_relative: false,
			location: None,
			is_type_only: true,
			imported_name: None,
		}
	}

	fn react_import() -> ImportBinding {
		ImportBinding {
			identifier: "React".into(),
			specifier: "react".into(),
			is_relative: false,
			location: None,
			is_type_only: false,
			imported_name: None,
		}
	}

	// ── Positive cases ───────────────────────────────────────

	#[test]
	fn app_get_is_route_registration() {
		let r = detect_framework_boundary(
			"app.get",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[express_import()],
		);
		assert!(r.is_some());
		assert_eq!(
			r.unwrap().basis_code,
			UnresolvedEdgeBasisCode::ExpressRouteRegistration
		);
	}

	#[test]
	fn router_post_is_route_registration() {
		let r = detect_framework_boundary(
			"router.post",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[express_import()],
		);
		assert!(r.is_some());
		assert_eq!(
			r.unwrap().basis_code,
			UnresolvedEdgeBasisCode::ExpressRouteRegistration
		);
	}

	#[test]
	fn app_use_is_middleware_registration() {
		let r = detect_framework_boundary(
			"app.use",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[express_import()],
		);
		assert!(r.is_some());
		assert_eq!(
			r.unwrap().basis_code,
			UnresolvedEdgeBasisCode::ExpressMiddlewareRegistration
		);
	}

	#[test]
	fn app_listen_is_route_registration_lifecycle() {
		let r = detect_framework_boundary(
			"app.listen",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[express_import()],
		);
		assert!(r.is_some());
		assert_eq!(
			r.unwrap().basis_code,
			UnresolvedEdgeBasisCode::ExpressRouteRegistration
		);
	}

	#[test]
	fn types_express_import_triggers_detection() {
		let r = detect_framework_boundary(
			"app.get",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[types_express_import()],
		);
		assert!(r.is_some());
	}

	#[test]
	fn chained_call_detects_first_method() {
		// app.route("/x").get → first method is "route"
		let r = detect_framework_boundary(
			"app.route.get",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[express_import()],
		);
		assert!(r.is_some());
		assert_eq!(
			r.unwrap().basis_code,
			UnresolvedEdgeBasisCode::ExpressRouteRegistration
		);
	}

	// ── Negative cases ───────────────────────────────────────

	#[test]
	fn returns_none_when_no_express_import() {
		let r = detect_framework_boundary(
			"app.get",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[],
		);
		assert!(r.is_none());
	}

	#[test]
	fn returns_none_for_non_route_method_on_express_file() {
		let r = detect_framework_boundary(
			"app.set",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[express_import()],
		);
		assert!(r.is_none());
	}

	#[test]
	fn returns_none_for_non_express_receiver() {
		// cache.get() in a file that imports express should NOT match.
		let r = detect_framework_boundary(
			"cache.get",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[express_import()],
		);
		assert!(r.is_none());
	}

	#[test]
	fn returns_none_for_calls_function_category() {
		// Only obj.method() shape (CALLS_OBJ_METHOD) triggers.
		let r = detect_framework_boundary(
			"app.get",
			UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
			&[express_import()],
		);
		assert!(r.is_none());
	}

	#[test]
	fn returns_none_for_non_calls_category() {
		let r = detect_framework_boundary(
			"app.get",
			UnresolvedEdgeCategory::ImportsFileNotFound,
			&[express_import()],
		);
		assert!(r.is_none());
	}

	#[test]
	fn returns_none_for_react_import_without_express() {
		let r = detect_framework_boundary(
			"app.get",
			UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			&[react_import()],
		);
		assert!(r.is_none());
	}
}
