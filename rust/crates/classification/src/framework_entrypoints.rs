//! Framework entrypoint detection (node-level).
//!
//! Mirror of `src/core/classification/framework-entrypoints.ts`.
//!
//! Detects symbols invoked by an external framework runtime (not
//! by internal code). These are NODE-LEVEL facts, not edge
//! reclassifications. The important truth is "this function is
//! externally entered," not "calls inside it are framework-
//! boundary."
//!
//! First-slice detector: Lambda/serverless exported handler
//! convention.
//!
//! Pure function. No I/O, no state.

use crate::types::{DetectedEntrypoint, ExportedSymbol, ImportBinding};

// ── Lambda lookup tables ─────────────────────────────────────────

/// Conventional Lambda handler function names.
const LAMBDA_HANDLER_NAMES: &[&str] = &["handler"];

/// Import specifiers that indicate AWS Lambda usage.
const LAMBDA_SPECIFIERS: &[&str] = &[
	"aws-lambda",
	"@types/aws-lambda",
	"@aws-lambda-powertools/commons",
	"@aws-lambda-powertools/logger",
	"@aws-lambda-powertools/tracer",
	"@aws-lambda-powertools/metrics",
	"@aws-lambda-powertools/parameters",
	"@middy/core",
];

/// Symbol subtypes eligible for Lambda handler detection.
const CALLABLE_SUBTYPES: &[&str] = &["FUNCTION", "VARIABLE", "CONSTANT"];

// ── Public interface ─────────────────────────────────────────────

/// Scan a file's exported symbols and import bindings for Lambda
/// handler conventions.
///
/// Returns detected entrypoints (may be empty). Pure function.
///
/// Detection requires TWO signals:
///   (a) file imports from a Lambda-related package
///   (b) file exports a function with a conventional handler name
///
/// Mirror of `detectLambdaEntrypoints` from
/// `framework-entrypoints.ts:87`.
///
/// This function is re-exported as `pub` from the crate root.
pub fn detect_lambda_entrypoints(
	import_bindings: &[ImportBinding],
	exported_symbols: &[ExportedSymbol],
) -> Vec<DetectedEntrypoint> {
	// Signal (a): file has a Lambda-related import.
	let has_lambda_import = import_bindings.iter().any(|b| {
		LAMBDA_SPECIFIERS.contains(&b.specifier.as_str())
			|| b.specifier.starts_with("@aws-lambda-powertools/")
	});
	if !has_lambda_import {
		return Vec::new();
	}

	// Signal (b): file exports a handler-named function.
	let mut results = Vec::new();
	for sym in exported_symbols {
		if sym.visibility.as_deref() == Some("export")
			&& LAMBDA_HANDLER_NAMES.contains(&sym.name.as_str())
			&& sym
				.subtype
				.as_deref()
				.map_or(false, |st| CALLABLE_SUBTYPES.contains(&st))
		{
			results.push(DetectedEntrypoint {
				target_stable_key: sym.stable_key.clone(),
				convention: "lambda_exported_handler".to_string(),
				confidence: 0.9,
				reason: format!(
					"exported function \"{}\" in file importing Lambda types",
					sym.name
				),
			});
		}
	}
	results
}

#[cfg(test)]
mod tests {
	use super::*;

	fn lambda_import() -> ImportBinding {
		ImportBinding {
			identifier: "Handler".into(),
			specifier: "aws-lambda".into(),
			is_relative: false,
			location: None,
			is_type_only: true,
		}
	}

	fn powertools_import() -> ImportBinding {
		ImportBinding {
			identifier: "Logger".into(),
			specifier: "@aws-lambda-powertools/logger".into(),
			is_relative: false,
			location: None,
			is_type_only: false,
		}
	}

	fn middy_import() -> ImportBinding {
		ImportBinding {
			identifier: "middy".into(),
			specifier: "@middy/core".into(),
			is_relative: false,
			location: None,
			is_type_only: false,
		}
	}

	fn handler_function() -> ExportedSymbol {
		ExportedSymbol {
			stable_key: "r1:src/index.ts#handler:SYMBOL:FUNCTION".into(),
			name: "handler".into(),
			visibility: Some("export".into()),
			subtype: Some("FUNCTION".into()),
		}
	}

	fn handler_variable() -> ExportedSymbol {
		ExportedSymbol {
			stable_key: "r1:src/index.ts#handler:SYMBOL:VARIABLE".into(),
			name: "handler".into(),
			visibility: Some("export".into()),
			subtype: Some("VARIABLE".into()),
		}
	}

	fn handler_class() -> ExportedSymbol {
		ExportedSymbol {
			stable_key: "r1:src/index.ts#handler:SYMBOL:CLASS".into(),
			name: "handler".into(),
			visibility: Some("export".into()),
			subtype: Some("CLASS".into()),
		}
	}

	fn handler_type_alias() -> ExportedSymbol {
		ExportedSymbol {
			stable_key: "r1:src/index.ts#handler:SYMBOL:TYPE_ALIAS".into(),
			name: "handler".into(),
			visibility: Some("export".into()),
			subtype: Some("TYPE_ALIAS".into()),
		}
	}

	fn non_handler_export() -> ExportedSymbol {
		ExportedSymbol {
			stable_key: "r1:src/index.ts#processEvent:SYMBOL:FUNCTION".into(),
			name: "processEvent".into(),
			visibility: Some("export".into()),
			subtype: Some("FUNCTION".into()),
		}
	}

	// ── Positive cases ───────────────────────────────────────

	#[test]
	fn detects_exported_handler_with_aws_lambda_import() {
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[handler_function()],
		);
		assert_eq!(results.len(), 1);
		assert_eq!(results[0].convention, "lambda_exported_handler");
		assert!((results[0].confidence - 0.9).abs() < f64::EPSILON);
	}

	#[test]
	fn detects_exported_const_handler() {
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[handler_variable()],
		);
		assert_eq!(results.len(), 1);
	}

	#[test]
	fn detects_handler_with_powertools_import() {
		let results = detect_lambda_entrypoints(
			&[powertools_import()],
			&[handler_function()],
		);
		assert_eq!(results.len(), 1);
	}

	#[test]
	fn detects_handler_with_middy_import() {
		let results = detect_lambda_entrypoints(
			&[middy_import()],
			&[handler_function()],
		);
		assert_eq!(results.len(), 1);
	}

	#[test]
	fn detects_handler_but_ignores_non_handler_exports() {
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[handler_function(), non_handler_export()],
		);
		assert_eq!(results.len(), 1);
		assert!(results[0].target_stable_key.contains("handler"));
	}

	// ── Negative cases ───────────────────────────────────────

	#[test]
	fn does_not_detect_non_callable_export_named_handler_class() {
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[handler_class()],
		);
		assert!(results.is_empty(), "CLASS is not a callable subtype");
	}

	#[test]
	fn does_not_detect_type_alias_named_handler() {
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[handler_type_alias()],
		);
		assert!(results.is_empty(), "TYPE_ALIAS is not callable");
	}

	#[test]
	fn returns_empty_when_no_lambda_import() {
		let results = detect_lambda_entrypoints(
			&[], // no imports
			&[handler_function()],
		);
		assert!(results.is_empty());
	}

	#[test]
	fn returns_empty_when_handler_is_not_exported() {
		let mut sym = handler_function();
		sym.visibility = Some("private".into());
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[sym],
		);
		assert!(results.is_empty());
	}

	#[test]
	fn returns_empty_when_handler_has_null_visibility() {
		let mut sym = handler_function();
		sym.visibility = None;
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[sym],
		);
		assert!(results.is_empty());
	}

	#[test]
	fn returns_empty_when_no_conventional_handler_name() {
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[non_handler_export()],
		);
		assert!(results.is_empty());
	}

	#[test]
	fn exported_main_does_not_detect_p2_regression() {
		// TS regression pin: "does NOT detect exported main
		// (P2 — too broad, removed)". The name "main" was
		// previously in LAMBDA_HANDLER_NAMES but was removed
		// because it is too common outside Lambda contexts.
		// This test ensures "main" stays non-detecting even if
		// a Lambda import is present.
		let main_fn = ExportedSymbol {
			stable_key: "r1:src/index.ts#main:SYMBOL:FUNCTION".into(),
			name: "main".into(),
			visibility: Some("export".into()),
			subtype: Some("FUNCTION".into()),
		};
		let results = detect_lambda_entrypoints(
			&[lambda_import()],
			&[main_fn],
		);
		assert!(
			results.is_empty(),
			"exported main must NOT be detected as a Lambda handler (P2 regression pin)"
		);
	}
}
