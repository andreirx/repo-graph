//! Quality policy domain logic: validation and assessment.
//!
//! This crate provides pure domain logic for quality policy declarations
//! and evaluation:
//!
//! # Declaration Validation
//!
//! - [`SupportedMeasurementKind`]: Typed enum of measurement kinds that
//!   quality policies can target. Limited to the **Phase A set** — kinds
//!   that are actually persisted in the measurements table today.
//!
//! - [`validate_quality_policy_payload`]: Validates a `QualityPolicyPayload`
//!   before storage. Returns structured errors for each validation failure.
//!
//! - [`PolicyValidationError`]: Structured error enum for all validation
//!   failures.
//!
//! # Policy Assessment
//!
//! - [`assess::evaluate_policies`]: Batch evaluation of policies against
//!   measurement facts. Pure function, no storage access.
//!
//! - [`assess::MeasurementFact`]: Typed measurement input with scope metadata.
//!
//! - [`assess::PolicyAssessment`]: Evaluation result with verdict and violations.
//!
//! - [`assess::AssessmentVerdict`]: Pass/Fail/NotApplicable/NotComparable.
//!
//! # Design Rationale
//!
//! The storage crate defines `QualityPolicyPayload` with `measurement_kind`
//! as a raw `String`. This is deliberate: storage is schema-stable and must
//! accept whatever JSON was persisted (including kinds that may have been
//! deprecated or renamed). Validation is the domain layer's job.
//!
//! # Validation Boundaries
//!
//! **Write boundary (enforced):** The CLI `declare quality-policy` command
//! calls [`validate_quality_policy_payload`] before inserting. Invalid
//! payloads are rejected with exit code 1.
//!
//! **Read boundary (caller responsibility):** The storage query
//! `get_active_quality_policy_declarations` performs structural
//! deserialization only. It returns payloads that may be semantically
//! invalid (e.g., deprecated measurement kinds, incompatible policy/metric
//! combinations). Callers (e.g., the evaluation use case) must call
//! [`validate_quality_policy_payload`] and handle errors appropriately.
//!
//! This asymmetry exists because storage cannot depend on this crate
//! (would create a dependency cycle). The storage layer is schema-stable;
//! semantic validation belongs to the domain layer.
//!
//! # Scope Matching Semantics
//!
//! - `module:path` — repo-relative path prefix with `/` boundary
//! - `file:pattern` — full repo-relative glob (not basename)
//! - `symbol_kind:KIND` — exact canonical string match (case-insensitive)
//!
//! # Measurement Kinds (Phase A)
//!
//! Only measurement kinds that are **currently persisted** are supported.
//! Declaring a policy against a non-existent kind is unsound — evaluation
//! would always return NOT_APPLICABLE.
//!
//! Phase A set (persisted today):
//! - `cyclomatic_complexity` — AST-extracted, per-function
//! - `cognitive_complexity` — AST-extracted, per-function
//! - `function_length` — AST-extracted, per-function
//! - `parameter_count` — AST-extracted, per-function
//! - `max_nesting_depth` — AST-extracted, per-function
//! - `line_coverage` — imported from Istanbul/c8, per-file
//!
//! Future kinds (module structural, churn, etc.) will be added when their
//! measurement pipelines are implemented. See `docs/architecture/measurement-model.txt`
//! for the full roadmap.

pub mod assess;

use repo_graph_storage::types::{QualityPolicyKind, QualityPolicyPayload};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Supported Measurement Kinds (Phase A) ──────────────────────────────

/// Measurement kinds that quality policies can target.
///
/// **Phase A set only.** This enum is limited to measurement kinds that
/// are actually persisted in the measurements table today. Declaring a
/// policy against a kind that doesn't exist is unsound — evaluation
/// would always return NOT_APPLICABLE.
///
/// Current Phase A set:
/// - Function/symbol metrics (AST-extracted, per-function)
/// - Coverage metrics (imported, per-file)
///
/// Future phases will add module structural metrics, churn metrics, etc.
/// when their measurement pipelines are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportedMeasurementKind {
	// ── Function/symbol metrics (computed from AST by extractor) ──

	/// Decision point count (McCabe complexity).
	CyclomaticComplexity,
	/// Weighted complexity penalizing nesting.
	CognitiveComplexity,
	/// Line count (line_end - line_start + 1).
	FunctionLength,
	/// Number of formal parameters.
	ParameterCount,
	/// Deepest control flow nesting level.
	MaxNestingDepth,

	// ── Coverage metrics (imported from external tools) ──

	/// Lines covered / total lines. Range [0.0, 1.0].
	LineCoverage,
}

impl SupportedMeasurementKind {
	/// Returns the canonical string representation for storage.
	///
	/// This matches the `kind` column in the measurements table and
	/// the `measurement_kind` field in `QualityPolicyPayload`.
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::CyclomaticComplexity => "cyclomatic_complexity",
			Self::CognitiveComplexity => "cognitive_complexity",
			Self::FunctionLength => "function_length",
			Self::ParameterCount => "parameter_count",
			Self::MaxNestingDepth => "max_nesting_depth",
			Self::LineCoverage => "line_coverage",
		}
	}

	/// Parse a measurement kind string into the enum.
	///
	/// Returns `None` for unknown or unsupported kinds.
	pub fn from_str(s: &str) -> Option<Self> {
		match s {
			"cyclomatic_complexity" => Some(Self::CyclomaticComplexity),
			"cognitive_complexity" => Some(Self::CognitiveComplexity),
			"function_length" => Some(Self::FunctionLength),
			"parameter_count" => Some(Self::ParameterCount),
			"max_nesting_depth" => Some(Self::MaxNestingDepth),
			"line_coverage" => Some(Self::LineCoverage),
			_ => None,
		}
	}

	/// Returns the family this measurement belongs to.
	pub fn family(&self) -> MeasurementFamily {
		match self {
			Self::CyclomaticComplexity
			| Self::CognitiveComplexity
			| Self::FunctionLength
			| Self::ParameterCount
			| Self::MaxNestingDepth => MeasurementFamily::FunctionSymbol,

			Self::LineCoverage => MeasurementFamily::Coverage,
		}
	}

	/// Returns all supported measurement kinds.
	pub fn all() -> &'static [Self] {
		&[
			Self::CyclomaticComplexity,
			Self::CognitiveComplexity,
			Self::FunctionLength,
			Self::ParameterCount,
			Self::MaxNestingDepth,
			Self::LineCoverage,
		]
	}

	/// Returns a human-readable list of supported kinds for error messages.
	pub fn supported_kinds_display() -> &'static str {
		"cyclomatic_complexity, cognitive_complexity, function_length, parameter_count, max_nesting_depth, line_coverage"
	}
}

/// Measurement family for grouping related kinds.
///
/// Phase A supports two families: function/symbol and coverage.
/// Future phases will add module_structural, size, and churn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeasurementFamily {
	/// Function/symbol metrics (computed from AST).
	FunctionSymbol,
	/// Coverage metrics (imported from external tools).
	Coverage,
}

impl MeasurementFamily {
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::FunctionSymbol => "function_symbol",
			Self::Coverage => "coverage",
		}
	}
}

// ── Policy Validation Errors ───────────────────────────────────────────

/// Errors returned by quality policy validation.
#[derive(Debug, Clone, Error)]
pub enum PolicyValidationError {
	/// The `policy_id` field is empty.
	#[error("policy_id must not be empty")]
	EmptyPolicyId,

	/// The `version` field is not positive.
	#[error("version must be > 0, got {version}")]
	InvalidVersion { version: i64 },

	/// The `measurement_kind` is not a recognized measurement kind.
	#[error("unknown measurement kind: '{kind}'")]
	UnknownMeasurementKind { kind: String },

	/// The `threshold` is not a finite number (NaN or infinite).
	#[error("threshold must be finite, got {threshold}")]
	NonFiniteThreshold { threshold: f64 },

	/// A scope clause has an empty selector.
	#[error("scope clause '{clause_kind}' has empty selector")]
	EmptyScopeSelector { clause_kind: String },

	/// The policy_kind is incompatible with the measurement family.
	///
	/// Comparative policies (`no_new`, `no_worsened`) use `<=` semantics,
	/// which is inverted for coverage metrics where higher values are better.
	/// Phase A constraint: comparative policies are only allowed for
	/// function/symbol metrics (complexity, length, etc.).
	#[error(
		"policy_kind '{policy_kind}' is incompatible with measurement '{measurement_kind}': \
		 comparative policies use '<=' semantics which is inverted for coverage metrics"
	)]
	IncompatiblePolicyKind {
		policy_kind: String,
		measurement_kind: String,
	},
}

// ── Validation ─────────────────────────────────────────────────────────

/// Validates a `QualityPolicyPayload` for storage.
///
/// Returns all validation errors found (not just the first).
/// An empty `Vec` indicates the payload is valid.
///
/// Validation rules:
/// 1. `policy_id` must not be empty
/// 2. `version` must be > 0
/// 3. `measurement_kind` must be a known `SupportedMeasurementKind`
/// 4. `threshold` must be finite (not NaN, not infinite)
/// 5. Each scope clause's selector must not be empty
/// 6. Comparative policies (`no_new`, `no_worsened`) not allowed for coverage
///
/// Note: `policy_kind` and `severity` are already typed enums in storage,
/// so they cannot have invalid values after deserialization.
pub fn validate_quality_policy_payload(
	payload: &QualityPolicyPayload,
) -> Vec<PolicyValidationError> {
	let mut errors = Vec::new();

	// 1. policy_id must not be empty
	if payload.policy_id.is_empty() {
		errors.push(PolicyValidationError::EmptyPolicyId);
	}

	// 2. version must be > 0
	if payload.version <= 0 {
		errors.push(PolicyValidationError::InvalidVersion {
			version: payload.version,
		});
	}

	// 3. measurement_kind must be known
	let measurement_kind_parsed = SupportedMeasurementKind::from_str(&payload.measurement_kind);
	if measurement_kind_parsed.is_none() {
		errors.push(PolicyValidationError::UnknownMeasurementKind {
			kind: payload.measurement_kind.clone(),
		});
	}

	// 4. threshold must be finite
	if !payload.threshold.is_finite() {
		errors.push(PolicyValidationError::NonFiniteThreshold {
			threshold: payload.threshold,
		});
	}

	// 5. scope clauses must have non-empty selectors
	for clause in &payload.scope_clauses {
		if clause.selector.is_empty() {
			errors.push(PolicyValidationError::EmptyScopeSelector {
				clause_kind: clause.clause_kind.as_str().to_string(),
			});
		}
	}

	// 6. Comparative policies not allowed for coverage metrics.
	//
	// Comparative policies (no_new, no_worsened) use '<=' semantics:
	// "new value must be <= threshold/baseline". For complexity metrics
	// where lower is better, this is correct. For coverage metrics where
	// higher is better, this is inverted — it would flag coverage
	// INCREASES as violations.
	//
	// Phase A constraint: reject the combination rather than silently
	// produce inverted semantics. Coverage metrics use absolute_min only.
	if let Some(mk) = measurement_kind_parsed {
		if mk.family() == MeasurementFamily::Coverage && payload.policy_kind.requires_baseline() {
			errors.push(PolicyValidationError::IncompatiblePolicyKind {
				policy_kind: payload.policy_kind.as_str().to_string(),
				measurement_kind: payload.measurement_kind.clone(),
			});
		}
	}

	errors
}

/// Validates a `QualityPolicyPayload` and returns `Ok(())` if valid,
/// or `Err` with the first error if invalid.
///
/// Use [`validate_quality_policy_payload`] to get all errors.
pub fn validate_quality_policy_payload_strict(
	payload: &QualityPolicyPayload,
) -> Result<(), PolicyValidationError> {
	let errors = validate_quality_policy_payload(payload);
	if let Some(first) = errors.into_iter().next() {
		Err(first)
	} else {
		Ok(())
	}
}

/// Parses and validates a measurement kind string.
///
/// Returns the typed enum if valid, or an error if unknown.
pub fn parse_measurement_kind(kind: &str) -> Result<SupportedMeasurementKind, PolicyValidationError> {
	SupportedMeasurementKind::from_str(kind).ok_or_else(|| PolicyValidationError::UnknownMeasurementKind {
		kind: kind.to_string(),
	})
}

// ── Policy Kind Semantics ──────────────────────────────────────────────

/// Returns a human-readable description of what a policy kind checks.
///
/// Useful for CLI help and error messages.
pub fn policy_kind_description(kind: QualityPolicyKind) -> &'static str {
	match kind {
		QualityPolicyKind::AbsoluteMax => "every measurement must satisfy value <= threshold",
		QualityPolicyKind::AbsoluteMin => "every measurement must satisfy value >= threshold",
		QualityPolicyKind::NoNew => {
			"no new violations: measurements not in baseline must satisfy value <= threshold"
		}
		QualityPolicyKind::NoWorsened => {
			"no worsened violations: measurements must not exceed their baseline value"
		}
	}
}

/// Returns suggested policy kinds for a measurement kind.
///
/// This is a heuristic for CLI suggestions, not a hard constraint.
/// For example:
/// - Complexity metrics typically use `absolute_max` (lower is better)
/// - Coverage metrics typically use `absolute_min` (higher is better)
/// - Churn metrics might use `no_worsened` (trend constraint)
pub fn suggested_policy_kinds(kind: SupportedMeasurementKind) -> &'static [QualityPolicyKind] {
	use QualityPolicyKind::*;

	match kind.family() {
		MeasurementFamily::FunctionSymbol => {
			// Complexity, length, nesting, params: lower is better
			&[AbsoluteMax, NoNew, NoWorsened]
		}
		MeasurementFamily::Coverage => {
			// Coverage: higher is better. Only absolute_min is allowed.
			// Comparative policies (no_new, no_worsened) use '<=' semantics
			// which is inverted for coverage — rejected by validation rule 6.
			&[AbsoluteMin]
		}
	}
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use repo_graph_storage::types::{QualityPolicySeverity, ScopeClause, ScopeClauseKind};

	#[test]
	fn measurement_kind_roundtrip() {
		for kind in SupportedMeasurementKind::all() {
			let s = kind.as_str();
			let parsed = SupportedMeasurementKind::from_str(s);
			assert_eq!(parsed, Some(*kind), "roundtrip failed for {s}");
		}
	}

	#[test]
	fn measurement_kind_all_has_correct_count() {
		// Phase A: 5 function/symbol + 1 coverage = 6
		assert_eq!(SupportedMeasurementKind::all().len(), 6);
	}

	#[test]
	fn measurement_kind_unknown_returns_none() {
		assert_eq!(SupportedMeasurementKind::from_str("unknown_metric"), None);
		assert_eq!(SupportedMeasurementKind::from_str(""), None);
		assert_eq!(SupportedMeasurementKind::from_str("CYCLOMATIC_COMPLEXITY"), None); // case sensitive
	}

	#[test]
	fn measurement_kind_family_assignment() {
		// Function/symbol metrics
		assert_eq!(
			SupportedMeasurementKind::CyclomaticComplexity.family(),
			MeasurementFamily::FunctionSymbol
		);
		assert_eq!(
			SupportedMeasurementKind::CognitiveComplexity.family(),
			MeasurementFamily::FunctionSymbol
		);
		assert_eq!(
			SupportedMeasurementKind::FunctionLength.family(),
			MeasurementFamily::FunctionSymbol
		);
		assert_eq!(
			SupportedMeasurementKind::ParameterCount.family(),
			MeasurementFamily::FunctionSymbol
		);
		assert_eq!(
			SupportedMeasurementKind::MaxNestingDepth.family(),
			MeasurementFamily::FunctionSymbol
		);
		// Coverage metrics
		assert_eq!(
			SupportedMeasurementKind::LineCoverage.family(),
			MeasurementFamily::Coverage
		);
	}

	fn valid_payload() -> QualityPolicyPayload {
		QualityPolicyPayload {
			policy_id: "QP-001".to_string(),
			version: 1,
			scope_clauses: vec![],
			measurement_kind: "cognitive_complexity".to_string(),
			policy_kind: QualityPolicyKind::AbsoluteMax,
			threshold: 15.0,
			severity: QualityPolicySeverity::Fail,
			description: Some("Max cognitive complexity".to_string()),
		}
	}

	#[test]
	fn validate_valid_payload_returns_empty() {
		let payload = valid_payload();
		let errors = validate_quality_policy_payload(&payload);
		assert!(errors.is_empty(), "expected no errors, got {errors:?}");
	}

	#[test]
	fn validate_empty_policy_id() {
		let mut payload = valid_payload();
		payload.policy_id = "".to_string();
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(errors[0], PolicyValidationError::EmptyPolicyId));
	}

	#[test]
	fn validate_invalid_version() {
		let mut payload = valid_payload();
		payload.version = 0;
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(
			errors[0],
			PolicyValidationError::InvalidVersion { version: 0 }
		));

		payload.version = -5;
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(
			errors[0],
			PolicyValidationError::InvalidVersion { version: -5 }
		));
	}

	#[test]
	fn validate_unknown_measurement_kind() {
		let mut payload = valid_payload();
		payload.measurement_kind = "not_a_real_metric".to_string();
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(
			&errors[0],
			PolicyValidationError::UnknownMeasurementKind { kind } if kind == "not_a_real_metric"
		));
	}

	#[test]
	fn validate_non_finite_threshold() {
		let mut payload = valid_payload();
		payload.threshold = f64::NAN;
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(
			errors[0],
			PolicyValidationError::NonFiniteThreshold { .. }
		));

		payload.threshold = f64::INFINITY;
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(
			errors[0],
			PolicyValidationError::NonFiniteThreshold { .. }
		));

		payload.threshold = f64::NEG_INFINITY;
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(
			errors[0],
			PolicyValidationError::NonFiniteThreshold { .. }
		));
	}

	#[test]
	fn validate_empty_scope_selector() {
		let mut payload = valid_payload();
		payload.scope_clauses = vec![ScopeClause::new(ScopeClauseKind::Module, "")];
		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1);
		assert!(matches!(
			&errors[0],
			PolicyValidationError::EmptyScopeSelector { clause_kind } if clause_kind == "module"
		));
	}

	#[test]
	fn validate_multiple_errors() {
		let mut payload = valid_payload();
		payload.policy_id = "".to_string();
		payload.version = 0;
		payload.measurement_kind = "bad".to_string();
		payload.threshold = f64::NAN;
		payload.scope_clauses = vec![ScopeClause::new(ScopeClauseKind::File, "")];

		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 5, "expected 5 errors, got {errors:?}");
	}

	#[test]
	fn validate_strict_returns_first_error() {
		let mut payload = valid_payload();
		payload.policy_id = "".to_string();
		payload.version = 0;

		let result = validate_quality_policy_payload_strict(&payload);
		assert!(result.is_err());
		// First error should be EmptyPolicyId (order of validation)
		assert!(matches!(result, Err(PolicyValidationError::EmptyPolicyId)));
	}

	#[test]
	fn parse_measurement_kind_valid() {
		let result = parse_measurement_kind("cyclomatic_complexity");
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), SupportedMeasurementKind::CyclomaticComplexity);
	}

	#[test]
	fn parse_measurement_kind_invalid() {
		let result = parse_measurement_kind("unknown");
		assert!(result.is_err());
		assert!(matches!(
			result,
			Err(PolicyValidationError::UnknownMeasurementKind { .. })
		));
	}

	#[test]
	fn policy_kind_description_coverage() {
		// Just verify all variants have descriptions
		for kind in [
			QualityPolicyKind::AbsoluteMax,
			QualityPolicyKind::AbsoluteMin,
			QualityPolicyKind::NoNew,
			QualityPolicyKind::NoWorsened,
		] {
			let desc = policy_kind_description(kind);
			assert!(!desc.is_empty(), "missing description for {kind:?}");
		}
	}

	#[test]
	fn suggested_policy_kinds_non_empty() {
		for kind in SupportedMeasurementKind::all() {
			let suggestions = suggested_policy_kinds(*kind);
			assert!(
				!suggestions.is_empty(),
				"no suggestions for {kind:?}"
			);
		}
	}

	#[test]
	fn suggested_policy_kinds_coverage_excludes_comparative() {
		let suggestions = suggested_policy_kinds(SupportedMeasurementKind::LineCoverage);
		// Coverage only allows absolute_min (comparative policies rejected by rule 6)
		assert_eq!(suggestions, &[QualityPolicyKind::AbsoluteMin]);
	}

	// ── Incompatible policy/measurement validation (rule 6) ────────────────

	#[test]
	fn validate_coverage_with_no_new_rejected() {
		let mut payload = valid_payload();
		payload.measurement_kind = "line_coverage".to_string();
		payload.policy_kind = QualityPolicyKind::NoNew;
		payload.threshold = 0.8;

		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1, "expected 1 error, got {errors:?}");
		assert!(
			matches!(
				&errors[0],
				PolicyValidationError::IncompatiblePolicyKind { policy_kind, measurement_kind }
				if policy_kind == "no_new" && measurement_kind == "line_coverage"
			),
			"expected IncompatiblePolicyKind, got {:?}",
			errors[0]
		);
	}

	#[test]
	fn validate_coverage_with_no_worsened_rejected() {
		let mut payload = valid_payload();
		payload.measurement_kind = "line_coverage".to_string();
		payload.policy_kind = QualityPolicyKind::NoWorsened;
		payload.threshold = 0.8;

		let errors = validate_quality_policy_payload(&payload);
		assert_eq!(errors.len(), 1, "expected 1 error, got {errors:?}");
		assert!(
			matches!(
				&errors[0],
				PolicyValidationError::IncompatiblePolicyKind { policy_kind, measurement_kind }
				if policy_kind == "no_worsened" && measurement_kind == "line_coverage"
			),
			"expected IncompatiblePolicyKind, got {:?}",
			errors[0]
		);
	}

	#[test]
	fn validate_coverage_with_absolute_min_allowed() {
		let mut payload = valid_payload();
		payload.measurement_kind = "line_coverage".to_string();
		payload.policy_kind = QualityPolicyKind::AbsoluteMin;
		payload.threshold = 0.8;

		let errors = validate_quality_policy_payload(&payload);
		assert!(errors.is_empty(), "expected no errors, got {errors:?}");
	}

	#[test]
	fn validate_complexity_with_comparative_allowed() {
		// Complexity metrics allow comparative policies (lower is better matches '<=' semantics)
		let mut payload = valid_payload();
		payload.measurement_kind = "cyclomatic_complexity".to_string();

		payload.policy_kind = QualityPolicyKind::NoNew;
		let errors = validate_quality_policy_payload(&payload);
		assert!(errors.is_empty(), "no_new should be allowed for complexity");

		payload.policy_kind = QualityPolicyKind::NoWorsened;
		let errors = validate_quality_policy_payload(&payload);
		assert!(errors.is_empty(), "no_worsened should be allowed for complexity");
	}
}
