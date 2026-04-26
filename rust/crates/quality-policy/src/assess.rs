//! Pure assessment engine for quality policy evaluation.
//!
//! This module provides the core evaluation logic for quality policies:
//!
//! - [`evaluate_policies`]: Batch evaluation of policies against measurement facts
//! - [`evaluate_policy`]: Single policy evaluation
//! - [`MeasurementFact`]: Typed measurement input with scope metadata
//! - [`PolicyAssessment`]: Evaluation result with verdict and violations
//!
//! # Design Principles
//!
//! **Pure functions only.** No storage access, no side effects. The reducer
//! operates on normalized DTOs constructed by the caller (use case layer).
//!
//! **Typed inputs.** `MeasurementFact` uses [`SupportedMeasurementKind`] enum,
//! not raw strings. Invalid measurement kinds are rejected at construction.
//!
//! **Explicit baseline.** Comparative policies (`no_new`, `no_worsened`) require
//! baseline facts at batch level. Missing baseline produces `NotComparable`.
//!
//! **Scope matching semantics:**
//! - `module:path` — repo-relative path prefix with `/` boundary
//! - `file:pattern` — full repo-relative glob (not basename)
//! - `symbol_kind:KIND` — exact canonical string match
//!
//! # Assessment UID Format
//!
//! Deterministic, explicit string (not hash):
//! - Absolute policies: `{snapshot_uid}-qpa-{policy_uid}`
//! - Comparative policies: `{snapshot_uid}-qpa-{policy_uid}-vs-{baseline_snapshot_uid}`

use crate::SupportedMeasurementKind;
use glob::{MatchOptions, Pattern};
use repo_graph_storage::types::{QualityPolicyKind, QualityPolicyPayload, ScopeClauseKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Input DTOs ─────────────────────────────────────────────────────────

/// A measurement fact enriched for scope matching.
///
/// Callers (use case layer) construct these from storage rows + node lookups.
/// The `measurement_kind` is typed to prevent invalid-state at evaluation time.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasurementFact {
    /// Stable key of the measurement target (symbol, file, module).
    pub target_stable_key: String,

    /// Typed measurement kind (Phase A set only).
    pub measurement_kind: SupportedMeasurementKind,

    /// Measurement value.
    pub value: f64,

    /// Repo-relative file path for `file:` and `module:` scope matching.
    /// `None` means fact is out of scope for file/module clauses.
    pub file_path: Option<String>,

    /// Symbol kind for `symbol_kind:` scope matching.
    /// Canonical uppercase string (e.g., "FUNCTION", "CLASS").
    /// `None` means fact is out of scope for symbol_kind clauses.
    pub symbol_kind: Option<String>,
}

/// A policy definition with persistence identity.
///
/// Wraps `QualityPolicyPayload` with the storage-assigned UID.
/// The reducer needs this for assessment UID construction.
#[derive(Debug, Clone)]
pub struct PolicyDefinition {
    /// Storage-assigned declaration UID.
    pub policy_uid: String,

    /// Policy payload (identity + rules).
    pub payload: QualityPolicyPayload,
}

/// Batch input for policy evaluation.
///
/// Contains all policies to evaluate and the measurement fact sets.
/// For comparative policies, `baseline_facts` must be `Some`.
#[derive(Debug, Clone)]
pub struct PolicyEvaluationBatch {
    /// Policies to evaluate.
    pub policies: Vec<PolicyDefinition>,

    /// Current measurement facts (from target snapshot).
    pub current_facts: Vec<MeasurementFact>,

    /// Baseline measurement facts (from baseline snapshot).
    /// Required for `no_new` and `no_worsened` policies.
    pub baseline_facts: Option<Vec<MeasurementFact>>,

    /// Target snapshot UID.
    pub snapshot_uid: String,

    /// Baseline snapshot UID (required for comparative policies).
    pub baseline_snapshot_uid: Option<String>,
}

// ── Output DTOs ────────────────────────────────────────────────────────

/// Verdict for a single policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentVerdict {
    /// All scope-matching measurements satisfy the policy constraint.
    Pass,
    /// One or more scope-matching measurements violate the policy constraint.
    Fail,
    /// No measurements match the policy scope (nothing to evaluate).
    NotApplicable,
    /// Comparative policy without baseline facts at batch level.
    NotComparable,
}

impl AssessmentVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotApplicable => "NOT_APPLICABLE",
            Self::NotComparable => "NOT_COMPARABLE",
        }
    }
}

/// An individual measurement that violated a policy constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssessmentViolation {
    /// Stable key of the violating target.
    pub target_stable_key: String,

    /// Measurement value that violated the constraint.
    pub measurement_value: f64,

    /// Policy threshold (for absolute policies and no_new).
    pub threshold: f64,

    /// Baseline value (for no_worsened; `None` for absolute policies).
    pub baseline_value: Option<f64>,
}

/// Result of evaluating a single policy against measurement facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyAssessment {
    /// Deterministic assessment UID.
    /// Format: `{snapshot_uid}-qpa-{policy_uid}` or
    /// `{snapshot_uid}-qpa-{policy_uid}-vs-{baseline_snapshot_uid}`.
    pub assessment_uid: String,

    /// Storage-assigned policy declaration UID.
    pub policy_uid: String,

    /// Human-readable policy ID (display identity).
    pub policy_id: String,

    /// Policy version.
    pub policy_version: i64,

    /// Computed verdict.
    pub computed_verdict: AssessmentVerdict,

    /// Individual violations (empty if verdict is PASS/NOT_APPLICABLE/NOT_COMPARABLE).
    pub violations: Vec<AssessmentViolation>,

    /// Number of facts that matched the policy scope.
    pub scope_match_count: usize,

    /// Number of facts actually evaluated for violation.
    /// May be less than scope_match_count for comparative policies.
    pub measurements_evaluated: usize,

    /// Target snapshot UID.
    pub snapshot_uid: String,

    /// Baseline snapshot UID (populated for comparative policies).
    pub baseline_snapshot_uid: Option<String>,
}

// ── Scope Matching ─────────────────────────────────────────────────────

/// Check if a fact matches all scope clauses of a policy.
///
/// Scope clauses are AND-composed: all must match for the fact to be in scope.
/// If a clause requires metadata the fact lacks, the fact is out of scope.
fn fact_matches_scope(fact: &MeasurementFact, payload: &QualityPolicyPayload) -> bool {
    // Empty scope = all facts match
    if payload.scope_clauses.is_empty() {
        return true;
    }

    // All clauses must match (AND semantics)
    for clause in &payload.scope_clauses {
        if !clause_matches(fact, &clause.clause_kind, &clause.selector) {
            return false;
        }
    }
    true
}

/// Check if a fact matches a single scope clause.
fn clause_matches(fact: &MeasurementFact, kind: &ScopeClauseKind, selector: &str) -> bool {
    match kind {
        ScopeClauseKind::Module => match &fact.file_path {
            Some(path) => module_scope_matches(path, selector),
            None => false, // Missing metadata = out of scope
        },
        ScopeClauseKind::File => match &fact.file_path {
            Some(path) => file_scope_matches(path, selector),
            None => false,
        },
        ScopeClauseKind::SymbolKind => match &fact.symbol_kind {
            Some(sk) => symbol_kind_matches(sk, selector),
            None => false,
        },
    }
}

/// Module scope: repo-relative path prefix with `/` boundary.
///
/// `module:src/core` matches:
/// - `src/core/foo.rs` (prefix + `/`)
/// - `src/core` (exact match, unlikely for files but valid)
///
/// Does NOT match:
/// - `src/core-utils/foo.rs` (no `/` boundary)
fn module_scope_matches(file_path: &str, selector: &str) -> bool {
    if file_path == selector {
        return true;
    }
    // Prefix match with `/` boundary
    let prefix = if selector.ends_with('/') {
        selector.to_string()
    } else {
        format!("{}/", selector)
    };
    file_path.starts_with(&prefix)
}

/// File scope: full repo-relative glob matching.
///
/// `file:src/**/*.test.ts` matches `src/core/foo.test.ts`.
/// `file:*.test.ts` only matches files in repo root (not subdirectories).
///
/// Uses `require_literal_separator: true` so `*` does not match `/`.
/// Use `**` to match across directory boundaries.
fn file_scope_matches(file_path: &str, selector: &str) -> bool {
    // Match options: * does not match /, case sensitive
    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };

    // Try to compile as glob pattern
    match Pattern::new(selector) {
        Ok(pattern) => pattern.matches_with(file_path, options),
        Err(_) => {
            // Invalid glob = exact match fallback
            file_path == selector
        }
    }
}

/// Symbol kind scope: exact canonical string match.
///
/// Requires exact equality. The caller is responsible for providing
/// canonical uppercase values in `MeasurementFact.symbol_kind`.
fn symbol_kind_matches(fact_kind: &str, selector: &str) -> bool {
    fact_kind == selector
}

// ── Assessment UID Construction ────────────────────────────────────────

/// Construct deterministic assessment UID.
///
/// Format (two canonical forms only):
/// - Absolute: `{snapshot_uid}-qpa-{policy_uid}`
/// - Comparative: `{snapshot_uid}-qpa-{policy_uid}-vs-{baseline_snapshot_uid}`
///
/// When a comparative policy is evaluated without baseline, the verdict is
/// `NOT_COMPARABLE` and we use the absolute format. The assessment's
/// `baseline_snapshot_uid` field being `None` already indicates the degraded state.
fn build_assessment_uid(
    snapshot_uid: &str,
    policy_uid: &str,
    _policy_kind: QualityPolicyKind,
    baseline_snapshot_uid: Option<&str>,
) -> String {
    if let Some(baseline) = baseline_snapshot_uid {
        format!("{}-qpa-{}-vs-{}", snapshot_uid, policy_uid, baseline)
    } else {
        format!("{}-qpa-{}", snapshot_uid, policy_uid)
    }
}

// ── Evaluation Logic ───────────────────────────────────────────────────

/// Evaluate all policies in a batch.
///
/// Returns one `PolicyAssessment` per policy in input order.
pub fn evaluate_policies(batch: &PolicyEvaluationBatch) -> Vec<PolicyAssessment> {
    // Build baseline lookup for comparative policies
    let baseline_lookup: Option<HashMap<(&str, SupportedMeasurementKind), f64>> =
        batch.baseline_facts.as_ref().map(|facts| {
            facts
                .iter()
                .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
                .collect()
        });

    batch
        .policies
        .iter()
        .map(|def| {
            evaluate_policy(
                def,
                &batch.current_facts,
                baseline_lookup.as_ref(),
                &batch.snapshot_uid,
                batch.baseline_snapshot_uid.as_deref(),
            )
        })
        .collect()
}

/// Evaluate a single policy against measurement facts.
pub fn evaluate_policy(
    definition: &PolicyDefinition,
    current_facts: &[MeasurementFact],
    baseline_lookup: Option<&HashMap<(&str, SupportedMeasurementKind), f64>>,
    snapshot_uid: &str,
    baseline_snapshot_uid: Option<&str>,
) -> PolicyAssessment {
    let payload = &definition.payload;
    let policy_kind = payload.policy_kind;

    // Parse measurement kind from policy
    let policy_measurement_kind = match SupportedMeasurementKind::from_str(&payload.measurement_kind)
    {
        Some(k) => k,
        None => {
            // Invalid measurement kind in policy — NOT_APPLICABLE
            // (validation should have caught this, but be defensive)
            return PolicyAssessment {
                assessment_uid: build_assessment_uid(
                    snapshot_uid,
                    &definition.policy_uid,
                    policy_kind,
                    baseline_snapshot_uid,
                ),
                policy_uid: definition.policy_uid.clone(),
                policy_id: payload.policy_id.clone(),
                policy_version: payload.version,
                computed_verdict: AssessmentVerdict::NotApplicable,
                violations: vec![],
                scope_match_count: 0,
                measurements_evaluated: 0,
                snapshot_uid: snapshot_uid.to_string(),
                baseline_snapshot_uid: baseline_snapshot_uid.map(|s| s.to_string()),
            };
        }
    };

    // Filter facts: matching measurement kind + matching scope
    let scope_matching_facts: Vec<&MeasurementFact> = current_facts
        .iter()
        .filter(|f| f.measurement_kind == policy_measurement_kind)
        .filter(|f| fact_matches_scope(f, payload))
        .collect();

    let scope_match_count = scope_matching_facts.len();

    // Check for comparative policy without baseline
    if policy_kind.requires_baseline() && baseline_lookup.is_none() {
        return PolicyAssessment {
            assessment_uid: build_assessment_uid(
                snapshot_uid,
                &definition.policy_uid,
                policy_kind,
                baseline_snapshot_uid,
            ),
            policy_uid: definition.policy_uid.clone(),
            policy_id: payload.policy_id.clone(),
            policy_version: payload.version,
            computed_verdict: AssessmentVerdict::NotComparable,
            violations: vec![],
            scope_match_count,
            measurements_evaluated: 0,
            snapshot_uid: snapshot_uid.to_string(),
            baseline_snapshot_uid: None,
        };
    }

    // Evaluate based on policy kind
    match policy_kind {
        QualityPolicyKind::AbsoluteMax => {
            evaluate_absolute_max(&scope_matching_facts, payload, definition, snapshot_uid)
        }
        QualityPolicyKind::AbsoluteMin => {
            evaluate_absolute_min(&scope_matching_facts, payload, definition, snapshot_uid)
        }
        QualityPolicyKind::NoNew => evaluate_no_new(
            &scope_matching_facts,
            payload,
            definition,
            baseline_lookup.unwrap(),
            snapshot_uid,
            baseline_snapshot_uid.unwrap(),
        ),
        QualityPolicyKind::NoWorsened => evaluate_no_worsened(
            &scope_matching_facts,
            payload,
            definition,
            baseline_lookup.unwrap(),
            snapshot_uid,
            baseline_snapshot_uid.unwrap(),
        ),
    }
}

/// absolute_max: value <= threshold
fn evaluate_absolute_max(
    facts: &[&MeasurementFact],
    payload: &QualityPolicyPayload,
    definition: &PolicyDefinition,
    snapshot_uid: &str,
) -> PolicyAssessment {
    if facts.is_empty() {
        return PolicyAssessment {
            assessment_uid: build_assessment_uid(
                snapshot_uid,
                &definition.policy_uid,
                payload.policy_kind,
                None,
            ),
            policy_uid: definition.policy_uid.clone(),
            policy_id: payload.policy_id.clone(),
            policy_version: payload.version,
            computed_verdict: AssessmentVerdict::NotApplicable,
            violations: vec![],
            scope_match_count: 0,
            measurements_evaluated: 0,
            snapshot_uid: snapshot_uid.to_string(),
            baseline_snapshot_uid: None,
        };
    }

    let mut violations = Vec::new();
    for fact in facts {
        if fact.value > payload.threshold {
            violations.push(AssessmentViolation {
                target_stable_key: fact.target_stable_key.clone(),
                measurement_value: fact.value,
                threshold: payload.threshold,
                baseline_value: None,
            });
        }
    }

    let verdict = if violations.is_empty() {
        AssessmentVerdict::Pass
    } else {
        AssessmentVerdict::Fail
    };

    PolicyAssessment {
        assessment_uid: build_assessment_uid(
            snapshot_uid,
            &definition.policy_uid,
            payload.policy_kind,
            None,
        ),
        policy_uid: definition.policy_uid.clone(),
        policy_id: payload.policy_id.clone(),
        policy_version: payload.version,
        computed_verdict: verdict,
        violations,
        scope_match_count: facts.len(),
        measurements_evaluated: facts.len(),
        snapshot_uid: snapshot_uid.to_string(),
        baseline_snapshot_uid: None,
    }
}

/// absolute_min: value >= threshold
fn evaluate_absolute_min(
    facts: &[&MeasurementFact],
    payload: &QualityPolicyPayload,
    definition: &PolicyDefinition,
    snapshot_uid: &str,
) -> PolicyAssessment {
    if facts.is_empty() {
        return PolicyAssessment {
            assessment_uid: build_assessment_uid(
                snapshot_uid,
                &definition.policy_uid,
                payload.policy_kind,
                None,
            ),
            policy_uid: definition.policy_uid.clone(),
            policy_id: payload.policy_id.clone(),
            policy_version: payload.version,
            computed_verdict: AssessmentVerdict::NotApplicable,
            violations: vec![],
            scope_match_count: 0,
            measurements_evaluated: 0,
            snapshot_uid: snapshot_uid.to_string(),
            baseline_snapshot_uid: None,
        };
    }

    let mut violations = Vec::new();
    for fact in facts {
        if fact.value < payload.threshold {
            violations.push(AssessmentViolation {
                target_stable_key: fact.target_stable_key.clone(),
                measurement_value: fact.value,
                threshold: payload.threshold,
                baseline_value: None,
            });
        }
    }

    let verdict = if violations.is_empty() {
        AssessmentVerdict::Pass
    } else {
        AssessmentVerdict::Fail
    };

    PolicyAssessment {
        assessment_uid: build_assessment_uid(
            snapshot_uid,
            &definition.policy_uid,
            payload.policy_kind,
            None,
        ),
        policy_uid: definition.policy_uid.clone(),
        policy_id: payload.policy_id.clone(),
        policy_version: payload.version,
        computed_verdict: verdict,
        violations,
        scope_match_count: facts.len(),
        measurements_evaluated: facts.len(),
        snapshot_uid: snapshot_uid.to_string(),
        baseline_snapshot_uid: None,
    }
}

/// no_new: new targets (not in baseline) must satisfy value <= threshold
fn evaluate_no_new(
    facts: &[&MeasurementFact],
    payload: &QualityPolicyPayload,
    definition: &PolicyDefinition,
    baseline_lookup: &HashMap<(&str, SupportedMeasurementKind), f64>,
    snapshot_uid: &str,
    baseline_snapshot_uid: &str,
) -> PolicyAssessment {
    if facts.is_empty() {
        return PolicyAssessment {
            assessment_uid: build_assessment_uid(
                snapshot_uid,
                &definition.policy_uid,
                payload.policy_kind,
                Some(baseline_snapshot_uid),
            ),
            policy_uid: definition.policy_uid.clone(),
            policy_id: payload.policy_id.clone(),
            policy_version: payload.version,
            computed_verdict: AssessmentVerdict::NotApplicable,
            violations: vec![],
            scope_match_count: 0,
            measurements_evaluated: 0,
            snapshot_uid: snapshot_uid.to_string(),
            baseline_snapshot_uid: Some(baseline_snapshot_uid.to_string()),
        };
    }

    // Find new targets (in current but not in baseline)
    let new_facts: Vec<&&MeasurementFact> = facts
        .iter()
        .filter(|f| {
            !baseline_lookup.contains_key(&(&f.target_stable_key, f.measurement_kind))
        })
        .collect();

    let measurements_evaluated = new_facts.len();

    // Check new targets against threshold
    let mut violations = Vec::new();
    for fact in &new_facts {
        if fact.value > payload.threshold {
            violations.push(AssessmentViolation {
                target_stable_key: fact.target_stable_key.clone(),
                measurement_value: fact.value,
                threshold: payload.threshold,
                baseline_value: None,
            });
        }
    }

    let verdict = if violations.is_empty() {
        AssessmentVerdict::Pass
    } else {
        AssessmentVerdict::Fail
    };

    PolicyAssessment {
        assessment_uid: build_assessment_uid(
            snapshot_uid,
            &definition.policy_uid,
            payload.policy_kind,
            Some(baseline_snapshot_uid),
        ),
        policy_uid: definition.policy_uid.clone(),
        policy_id: payload.policy_id.clone(),
        policy_version: payload.version,
        computed_verdict: verdict,
        violations,
        scope_match_count: facts.len(),
        measurements_evaluated,
        snapshot_uid: snapshot_uid.to_string(),
        baseline_snapshot_uid: Some(baseline_snapshot_uid.to_string()),
    }
}

/// no_worsened: baseline violators must not exceed their baseline value.
///
/// Semantics:
/// 1. Consider only targets present in both snapshots
/// 2. From those, filter to baseline violators (baseline_value > threshold)
/// 3. For each baseline violator, check if current > baseline (worsened)
///
/// Targets within threshold in baseline are ignored — they weren't violations.
/// Targets that improve (current < baseline) pass.
/// Targets that stay the same pass.
fn evaluate_no_worsened(
    facts: &[&MeasurementFact],
    payload: &QualityPolicyPayload,
    definition: &PolicyDefinition,
    baseline_lookup: &HashMap<(&str, SupportedMeasurementKind), f64>,
    snapshot_uid: &str,
    baseline_snapshot_uid: &str,
) -> PolicyAssessment {
    if facts.is_empty() {
        return PolicyAssessment {
            assessment_uid: build_assessment_uid(
                snapshot_uid,
                &definition.policy_uid,
                payload.policy_kind,
                Some(baseline_snapshot_uid),
            ),
            policy_uid: definition.policy_uid.clone(),
            policy_id: payload.policy_id.clone(),
            policy_version: payload.version,
            computed_verdict: AssessmentVerdict::NotApplicable,
            violations: vec![],
            scope_match_count: 0,
            measurements_evaluated: 0,
            snapshot_uid: snapshot_uid.to_string(),
            baseline_snapshot_uid: Some(baseline_snapshot_uid.to_string()),
        };
    }

    // Find baseline violators that worsened
    let mut violations = Vec::new();
    let mut measurements_evaluated = 0;

    for fact in facts {
        if let Some(&baseline_value) =
            baseline_lookup.get(&(&fact.target_stable_key, fact.measurement_kind))
        {
            // Only evaluate baseline violators (baseline > threshold)
            if baseline_value > payload.threshold {
                measurements_evaluated += 1;
                // Violation: current value exceeds baseline (got worse)
                if fact.value > baseline_value {
                    violations.push(AssessmentViolation {
                        target_stable_key: fact.target_stable_key.clone(),
                        measurement_value: fact.value,
                        threshold: payload.threshold,
                        baseline_value: Some(baseline_value),
                    });
                }
            }
            // Targets within threshold in baseline are not evaluated
        }
        // New targets (not in baseline) are ignored by no_worsened
    }

    let verdict = if violations.is_empty() {
        AssessmentVerdict::Pass
    } else {
        AssessmentVerdict::Fail
    };

    PolicyAssessment {
        assessment_uid: build_assessment_uid(
            snapshot_uid,
            &definition.policy_uid,
            payload.policy_kind,
            Some(baseline_snapshot_uid),
        ),
        policy_uid: definition.policy_uid.clone(),
        policy_id: payload.policy_id.clone(),
        policy_version: payload.version,
        computed_verdict: verdict,
        violations,
        scope_match_count: facts.len(),
        measurements_evaluated,
        snapshot_uid: snapshot_uid.to_string(),
        baseline_snapshot_uid: Some(baseline_snapshot_uid.to_string()),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_storage::types::{QualityPolicySeverity, ScopeClause};

    fn make_fact(key: &str, kind: SupportedMeasurementKind, value: f64) -> MeasurementFact {
        MeasurementFact {
            target_stable_key: key.to_string(),
            measurement_kind: kind,
            value,
            file_path: None,
            symbol_kind: None,
        }
    }

    fn make_fact_with_path(
        key: &str,
        kind: SupportedMeasurementKind,
        value: f64,
        path: &str,
    ) -> MeasurementFact {
        MeasurementFact {
            target_stable_key: key.to_string(),
            measurement_kind: kind,
            value,
            file_path: Some(path.to_string()),
            symbol_kind: None,
        }
    }

    fn make_fact_with_symbol_kind(
        key: &str,
        kind: SupportedMeasurementKind,
        value: f64,
        symbol_kind: &str,
    ) -> MeasurementFact {
        MeasurementFact {
            target_stable_key: key.to_string(),
            measurement_kind: kind,
            value,
            file_path: None,
            symbol_kind: Some(symbol_kind.to_string()),
        }
    }

    fn make_policy(
        uid: &str,
        id: &str,
        measurement: &str,
        policy_kind: QualityPolicyKind,
        threshold: f64,
    ) -> PolicyDefinition {
        PolicyDefinition {
            policy_uid: uid.to_string(),
            payload: QualityPolicyPayload {
                policy_id: id.to_string(),
                version: 1,
                scope_clauses: vec![],
                measurement_kind: measurement.to_string(),
                policy_kind,
                threshold,
                severity: QualityPolicySeverity::Fail,
                description: None,
            },
        }
    }

    fn make_policy_with_scope(
        uid: &str,
        id: &str,
        measurement: &str,
        policy_kind: QualityPolicyKind,
        threshold: f64,
        scope: Vec<ScopeClause>,
    ) -> PolicyDefinition {
        PolicyDefinition {
            policy_uid: uid.to_string(),
            payload: QualityPolicyPayload {
                policy_id: id.to_string(),
                version: 1,
                scope_clauses: scope,
                measurement_kind: measurement.to_string(),
                policy_kind,
                threshold,
                severity: QualityPolicySeverity::Fail,
                description: None,
            },
        }
    }

    // ── Scope Matching Tests ───────────────────────────────────────────

    #[test]
    fn module_scope_exact_match() {
        assert!(module_scope_matches("src/core", "src/core"));
    }

    #[test]
    fn module_scope_prefix_with_slash() {
        assert!(module_scope_matches("src/core/foo.rs", "src/core"));
        assert!(module_scope_matches("src/core/nested/bar.rs", "src/core"));
    }

    #[test]
    fn module_scope_rejects_partial_name() {
        // src/core-utils should NOT match module:src/core
        assert!(!module_scope_matches("src/core-utils/foo.rs", "src/core"));
    }

    #[test]
    fn module_scope_trailing_slash_selector() {
        assert!(module_scope_matches("src/core/foo.rs", "src/core/"));
    }

    #[test]
    fn file_scope_exact_match() {
        assert!(file_scope_matches("src/core/foo.rs", "src/core/foo.rs"));
    }

    #[test]
    fn file_scope_glob_star() {
        assert!(file_scope_matches("src/core/foo.test.ts", "src/core/*.test.ts"));
        assert!(!file_scope_matches("src/other/foo.test.ts", "src/core/*.test.ts"));
    }

    #[test]
    fn file_scope_glob_double_star() {
        assert!(file_scope_matches("src/core/nested/foo.test.ts", "src/**/*.test.ts"));
        assert!(file_scope_matches("src/foo.test.ts", "src/**/*.test.ts"));
    }

    #[test]
    fn file_scope_root_only_glob() {
        // *.test.ts should only match files in root, not subdirectories
        assert!(file_scope_matches("foo.test.ts", "*.test.ts"));
        assert!(!file_scope_matches("src/foo.test.ts", "*.test.ts"));
    }

    #[test]
    fn symbol_kind_exact_match() {
        assert!(symbol_kind_matches("FUNCTION", "FUNCTION"));
        assert!(symbol_kind_matches("CLASS", "CLASS"));
    }

    #[test]
    fn symbol_kind_rejects_case_mismatch() {
        // Exact canonical match — case matters
        assert!(!symbol_kind_matches("FUNCTION", "function"));
        assert!(!symbol_kind_matches("function", "FUNCTION"));
        assert!(!symbol_kind_matches("Function", "FUNCTION"));
    }

    // ── absolute_max Tests ─────────────────────────────────────────────

    #[test]
    fn absolute_max_pass_all_below_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
        );
        let facts = vec![
            make_fact("sym1", SupportedMeasurementKind::CognitiveComplexity, 10.0),
            make_fact("sym2", SupportedMeasurementKind::CognitiveComplexity, 14.0),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
        assert!(result.violations.is_empty());
        assert_eq!(result.scope_match_count, 2);
        assert_eq!(result.measurements_evaluated, 2);
    }

    #[test]
    fn absolute_max_fail_one_above_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
        );
        let facts = vec![
            make_fact("sym1", SupportedMeasurementKind::CognitiveComplexity, 10.0),
            make_fact("sym2", SupportedMeasurementKind::CognitiveComplexity, 20.0),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::Fail);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].target_stable_key, "sym2");
        assert_eq!(result.violations[0].measurement_value, 20.0);
    }

    #[test]
    fn absolute_max_pass_at_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
        );
        let facts = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            15.0,
        )];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
    }

    #[test]
    fn absolute_max_not_applicable_no_matching_facts() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
        );
        let facts = vec![
            // Different measurement kind
            make_fact("sym1", SupportedMeasurementKind::FunctionLength, 100.0),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::NotApplicable);
        assert_eq!(result.scope_match_count, 0);
    }

    // ── absolute_min Tests ─────────────────────────────────────────────

    #[test]
    fn absolute_min_pass_all_above_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "line_coverage",
            QualityPolicyKind::AbsoluteMin,
            0.8,
        );
        let facts = vec![
            make_fact("file1", SupportedMeasurementKind::LineCoverage, 0.85),
            make_fact("file2", SupportedMeasurementKind::LineCoverage, 0.90),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn absolute_min_fail_one_below_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "line_coverage",
            QualityPolicyKind::AbsoluteMin,
            0.8,
        );
        let facts = vec![
            make_fact("file1", SupportedMeasurementKind::LineCoverage, 0.85),
            make_fact("file2", SupportedMeasurementKind::LineCoverage, 0.70),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::Fail);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].target_stable_key, "file2");
    }

    // ── no_new Tests ───────────────────────────────────────────────────

    #[test]
    fn no_new_not_comparable_without_baseline() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoNew,
            15.0,
        );
        let facts = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0,
        )];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::NotComparable);
    }

    #[test]
    fn no_new_pass_existing_symbol_above_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoNew,
            15.0,
        );
        let current = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0,
        )];
        let baseline = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            18.0,
        )];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        // sym1 exists in baseline, so no_new doesn't evaluate it
        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
        assert_eq!(result.measurements_evaluated, 0);
    }

    #[test]
    fn no_new_fail_new_symbol_above_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoNew,
            15.0,
        );
        let current = vec![
            make_fact("sym1", SupportedMeasurementKind::CognitiveComplexity, 10.0),
            make_fact("sym2", SupportedMeasurementKind::CognitiveComplexity, 20.0), // new, above threshold
        ];
        let baseline = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            10.0,
        )];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        assert_eq!(result.computed_verdict, AssessmentVerdict::Fail);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].target_stable_key, "sym2");
        assert_eq!(result.measurements_evaluated, 1); // only new symbol evaluated
    }

    #[test]
    fn no_new_pass_new_symbol_below_threshold() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoNew,
            15.0,
        );
        let current = vec![make_fact(
            "sym2",
            SupportedMeasurementKind::CognitiveComplexity,
            12.0,
        )];
        let baseline: Vec<MeasurementFact> = vec![];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
    }

    // ── no_worsened Tests ──────────────────────────────────────────────
    //
    // no_worsened semantics:
    // 1. Only evaluate targets present in both snapshots
    // 2. From those, only evaluate BASELINE VIOLATORS (baseline > threshold)
    // 3. For baseline violators, check if current > baseline (got worse)

    #[test]
    fn no_worsened_not_comparable_without_baseline() {
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoWorsened,
            15.0,
        );
        let facts = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0,
        )];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::NotComparable);
    }

    #[test]
    fn no_worsened_pass_baseline_violator_unchanged() {
        // Baseline violator (20 > 15) stays at 20 -> PASS
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoWorsened,
            15.0,
        );
        let current = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0,
        )];
        let baseline = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0, // baseline violator: 20 > 15
        )];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
        assert_eq!(result.measurements_evaluated, 1);
    }

    #[test]
    fn no_worsened_pass_baseline_violator_improved() {
        // Baseline violator (20 > 15) improved to 18 -> PASS
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoWorsened,
            15.0,
        );
        let current = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            18.0, // improved from 20
        )];
        let baseline = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0, // baseline violator: 20 > 15
        )];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
    }

    #[test]
    fn no_worsened_fail_baseline_violator_worsened() {
        // Baseline violator (20 > 15) worsened to 25 -> FAIL
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoWorsened,
            15.0,
        );
        let current = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            25.0, // worsened from 20
        )];
        let baseline = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0, // baseline violator: 20 > 15
        )];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        assert_eq!(result.computed_verdict, AssessmentVerdict::Fail);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].baseline_value, Some(20.0));
        assert_eq!(result.violations[0].measurement_value, 25.0);
    }

    #[test]
    fn no_worsened_pass_non_violator_increased() {
        // Non-violator in baseline (10 <= 15) increased to 12 -> PASS (not evaluated)
        // This is the key semantic: we don't care about increases within threshold
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoWorsened,
            15.0,
        );
        let current = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            12.0, // increased from 10
        )];
        let baseline = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            10.0, // NOT a baseline violator: 10 <= 15
        )];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        // Key assertion: PASS even though value increased, because baseline wasn't a violator
        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
        assert_eq!(result.measurements_evaluated, 0); // not evaluated because baseline wasn't violator
    }

    #[test]
    fn no_worsened_ignores_new_symbols() {
        // New symbol with high value -> ignored by no_worsened (that's no_new's job)
        let policy = make_policy(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::NoWorsened,
            15.0,
        );
        let current = vec![
            make_fact("sym1", SupportedMeasurementKind::CognitiveComplexity, 20.0), // existing violator
            make_fact("sym2", SupportedMeasurementKind::CognitiveComplexity, 50.0), // new, high
        ];
        let baseline = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0, // baseline violator: 20 > 15
        )];

        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap2",
            Some("snap1"),
        );

        // sym2 is new, ignored by no_worsened
        assert_eq!(result.computed_verdict, AssessmentVerdict::Pass);
        assert_eq!(result.measurements_evaluated, 1); // only sym1
    }

    // ── Scope Clause Filtering Tests ───────────────────────────────────

    #[test]
    fn scope_filters_by_module() {
        let policy = make_policy_with_scope(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
            vec![ScopeClause::new(ScopeClauseKind::Module, "src/core")],
        );
        let facts = vec![
            make_fact_with_path(
                "sym1",
                SupportedMeasurementKind::CognitiveComplexity,
                20.0,
                "src/core/foo.rs",
            ),
            make_fact_with_path(
                "sym2",
                SupportedMeasurementKind::CognitiveComplexity,
                20.0,
                "src/other/bar.rs",
            ),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.scope_match_count, 1);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].target_stable_key, "sym1");
    }

    #[test]
    fn scope_filters_by_file_glob() {
        let policy = make_policy_with_scope(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
            vec![ScopeClause::new(ScopeClauseKind::File, "**/*.test.ts")],
        );
        let facts = vec![
            make_fact_with_path(
                "sym1",
                SupportedMeasurementKind::CognitiveComplexity,
                20.0,
                "src/core/foo.test.ts",
            ),
            make_fact_with_path(
                "sym2",
                SupportedMeasurementKind::CognitiveComplexity,
                20.0,
                "src/core/foo.ts",
            ),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.scope_match_count, 1);
        assert_eq!(result.violations[0].target_stable_key, "sym1");
    }

    #[test]
    fn scope_filters_by_symbol_kind() {
        let policy = make_policy_with_scope(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
            vec![ScopeClause::new(ScopeClauseKind::SymbolKind, "FUNCTION")],
        );
        let facts = vec![
            make_fact_with_symbol_kind(
                "sym1",
                SupportedMeasurementKind::CognitiveComplexity,
                20.0,
                "FUNCTION",
            ),
            make_fact_with_symbol_kind(
                "sym2",
                SupportedMeasurementKind::CognitiveComplexity,
                20.0,
                "METHOD",
            ),
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.scope_match_count, 1);
        assert_eq!(result.violations[0].target_stable_key, "sym1");
    }

    #[test]
    fn scope_missing_metadata_excludes_fact() {
        let policy = make_policy_with_scope(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
            vec![ScopeClause::new(ScopeClauseKind::File, "**/*.ts")],
        );
        // Fact without file_path
        let facts = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            20.0,
        )];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.computed_verdict, AssessmentVerdict::NotApplicable);
        assert_eq!(result.scope_match_count, 0);
    }

    #[test]
    fn scope_and_composition() {
        let policy = make_policy_with_scope(
            "p1",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
            vec![
                ScopeClause::new(ScopeClauseKind::Module, "src/core"),
                ScopeClause::new(ScopeClauseKind::SymbolKind, "FUNCTION"),
            ],
        );
        let facts = vec![
            // Matches both: module + symbol_kind
            MeasurementFact {
                target_stable_key: "sym1".to_string(),
                measurement_kind: SupportedMeasurementKind::CognitiveComplexity,
                value: 20.0,
                file_path: Some("src/core/foo.rs".to_string()),
                symbol_kind: Some("FUNCTION".to_string()),
            },
            // Matches module but not symbol_kind
            MeasurementFact {
                target_stable_key: "sym2".to_string(),
                measurement_kind: SupportedMeasurementKind::CognitiveComplexity,
                value: 20.0,
                file_path: Some("src/core/foo.rs".to_string()),
                symbol_kind: Some("METHOD".to_string()),
            },
            // Matches symbol_kind but not module
            MeasurementFact {
                target_stable_key: "sym3".to_string(),
                measurement_kind: SupportedMeasurementKind::CognitiveComplexity,
                value: 20.0,
                file_path: Some("src/other/bar.rs".to_string()),
                symbol_kind: Some("FUNCTION".to_string()),
            },
        ];

        let result = evaluate_policy(&policy, &facts, None, "snap1", None);

        assert_eq!(result.scope_match_count, 1);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].target_stable_key, "sym1");
    }

    // ── Assessment UID Tests ───────────────────────────────────────────

    #[test]
    fn assessment_uid_absolute_policy() {
        let policy = make_policy(
            "decl-123",
            "QP-001",
            "cognitive_complexity",
            QualityPolicyKind::AbsoluteMax,
            15.0,
        );
        let facts = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            10.0,
        )];

        let result = evaluate_policy(&policy, &facts, None, "snap-abc", None);

        assert_eq!(result.assessment_uid, "snap-abc-qpa-decl-123");
    }

    #[test]
    fn assessment_uid_comparative_policy() {
        let policy = make_policy(
            "decl-456",
            "QP-002",
            "cognitive_complexity",
            QualityPolicyKind::NoNew,
            15.0,
        );
        let current = vec![make_fact(
            "sym1",
            SupportedMeasurementKind::CognitiveComplexity,
            10.0,
        )];
        let baseline: Vec<MeasurementFact> = vec![];
        let baseline_lookup: HashMap<(&str, SupportedMeasurementKind), f64> = baseline
            .iter()
            .map(|f| ((&*f.target_stable_key, f.measurement_kind), f.value))
            .collect();

        let result = evaluate_policy(
            &policy,
            &current,
            Some(&baseline_lookup),
            "snap-def",
            Some("snap-baseline"),
        );

        assert_eq!(
            result.assessment_uid,
            "snap-def-qpa-decl-456-vs-snap-baseline"
        );
    }

    // ── Batch Evaluation Tests ─────────────────────────────────────────

    #[test]
    fn batch_evaluates_all_policies() {
        let policies = vec![
            make_policy(
                "p1",
                "QP-001",
                "cognitive_complexity",
                QualityPolicyKind::AbsoluteMax,
                15.0,
            ),
            make_policy(
                "p2",
                "QP-002",
                "function_length",
                QualityPolicyKind::AbsoluteMax,
                100.0,
            ),
        ];
        let facts = vec![
            make_fact("sym1", SupportedMeasurementKind::CognitiveComplexity, 10.0),
            make_fact("sym1", SupportedMeasurementKind::FunctionLength, 50.0),
        ];

        let batch = PolicyEvaluationBatch {
            policies,
            current_facts: facts,
            baseline_facts: None,
            snapshot_uid: "snap1".to_string(),
            baseline_snapshot_uid: None,
        };

        let results = evaluate_policies(&batch);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].policy_id, "QP-001");
        assert_eq!(results[0].computed_verdict, AssessmentVerdict::Pass);
        assert_eq!(results[1].policy_id, "QP-002");
        assert_eq!(results[1].computed_verdict, AssessmentVerdict::Pass);
    }
}
