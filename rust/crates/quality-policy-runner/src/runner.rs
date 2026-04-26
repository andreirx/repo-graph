//! Core runner orchestration.
//!
//! This module contains the main [`QualityPolicyRunner`] that orchestrates
//! the full assessment flow: load policies, load measurements, evaluate,
//! persist.

use std::collections::HashSet;

use repo_graph_quality_policy::{
    assess::{
        evaluate_policies, MeasurementFact, PolicyAssessment, PolicyDefinition,
        PolicyEvaluationBatch,
    },
    parse_measurement_kind, validate_quality_policy_payload,
};
use repo_graph_storage::quality_policy_port::{
    EnrichedMeasurement, LoadedPolicy, QualityPolicyStoragePort,
};
use repo_graph_storage::types::{AssessmentVerdict, QualityAssessmentInput};

use crate::error::RunnerError;

/// Result of running quality policy assessment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessmentResult {
    /// Total number of assessments persisted.
    pub total_assessments: usize,

    /// Count of assessments by verdict.
    pub pass_count: usize,
    pub fail_count: usize,
    pub not_applicable_count: usize,
    pub not_comparable_count: usize,

    /// Number of policies that required baseline.
    pub baseline_required_count: usize,
}

/// Quality policy assessment orchestrator.
///
/// The runner is generic over the storage port, allowing injection
/// of the concrete implementation at composition time.
pub struct QualityPolicyRunner<P: QualityPolicyStoragePort> {
    port: P,
}

impl<P: QualityPolicyStoragePort> QualityPolicyRunner<P> {
    /// Create a new runner with the given storage port.
    pub fn new(port: P) -> Self {
        Self { port }
    }

    /// Assess all active quality policies for a snapshot.
    ///
    /// # Arguments
    ///
    /// * `repo_uid` - Repository UID (for loading policies)
    /// * `snapshot_uid` - Target snapshot to evaluate
    /// * `baseline_snapshot_uid` - Optional baseline for comparative policies
    ///
    /// # Returns
    ///
    /// Assessment result with counts by verdict.
    ///
    /// # Errors
    ///
    /// - [`RunnerError::InvalidPolicy`] if any policy fails semantic validation
    /// - [`RunnerError::BaselineRequired`] if comparative policies exist but no baseline provided
    /// - [`RunnerError::Storage`] on storage failures
    pub fn assess_snapshot(
        &mut self,
        repo_uid: &str,
        snapshot_uid: &str,
        baseline_snapshot_uid: Option<&str>,
    ) -> Result<AssessmentResult, RunnerError> {
        // Step 1: Load active quality policies.
        let loaded_policies = self.port.load_active_quality_policies(repo_uid)?;

        if loaded_policies.is_empty() {
            // No policies — persist empty set and return early.
            self.port.replace_assessments(snapshot_uid, &[])?;
            return Ok(AssessmentResult {
                total_assessments: 0,
                pass_count: 0,
                fail_count: 0,
                not_applicable_count: 0,
                not_comparable_count: 0,
                baseline_required_count: 0,
            });
        }

        // Step 2: Validate all policies semantically.
        let validated_policies = self.validate_policies(&loaded_policies)?;

        // Step 3: Determine which policies require baseline.
        let baseline_required_count = validated_policies
            .iter()
            .filter(|p| p.payload.policy_kind.requires_baseline())
            .count();

        // Step 4: Check baseline requirement.
        if baseline_required_count > 0 && baseline_snapshot_uid.is_none() {
            return Err(RunnerError::BaselineRequired(baseline_required_count));
        }

        // Step 5: Collect distinct measurement kinds needed.
        let required_kinds = self.collect_required_kinds(&validated_policies);

        // Step 6: Load current snapshot measurements.
        let kind_refs: Vec<&str> = required_kinds.iter().map(|s| s.as_str()).collect();
        let current_measurements = self.port.load_enriched_measurements(snapshot_uid, &kind_refs)?;

        // Step 7: Load baseline measurements if needed.
        let baseline_measurements = if baseline_required_count > 0 {
            let baseline_uid = baseline_snapshot_uid.expect("checked above");
            Some(self.port.load_enriched_measurements(baseline_uid, &kind_refs)?)
        } else {
            None
        };

        // Step 8: Convert to pure-engine DTOs.
        let current_facts = self.enrich_to_facts(&current_measurements)?;
        let baseline_facts = baseline_measurements
            .as_ref()
            .map(|m| self.enrich_to_facts(m))
            .transpose()?;

        // Step 9: Build policy definitions for the evaluator.
        let policy_defs: Vec<PolicyDefinition> = validated_policies
            .iter()
            .map(|p| PolicyDefinition {
                policy_uid: p.policy_uid.clone(),
                payload: p.payload.clone(),
            })
            .collect();

        // Step 10: Evaluate.
        let batch = PolicyEvaluationBatch {
            policies: policy_defs,
            current_facts,
            baseline_facts,
            snapshot_uid: snapshot_uid.to_string(),
            baseline_snapshot_uid: baseline_snapshot_uid.map(|s| s.to_string()),
        };

        let assessments = evaluate_policies(&batch);

        // Step 11: Convert to storage input DTOs.
        let now = iso_now();
        let inputs: Vec<QualityAssessmentInput> = assessments
            .iter()
            .map(|a| self.assessment_to_input(a, &now))
            .collect();

        // Step 12: Persist atomically.
        self.port.replace_assessments(snapshot_uid, &inputs)?;

        // Step 13: Build result.
        let result = self.build_result(&assessments, baseline_required_count);

        Ok(result)
    }

    /// Validate all loaded policies.
    ///
    /// Returns validated policies (same as input if all valid).
    /// Fails on first invalid policy with [`RunnerError::InvalidPolicy`].
    fn validate_policies(
        &self,
        policies: &[LoadedPolicy],
    ) -> Result<Vec<LoadedPolicy>, RunnerError> {
        for policy in policies {
            let errors = validate_quality_policy_payload(&policy.payload);
            if let Some(first_error) = errors.into_iter().next() {
                return Err(RunnerError::InvalidPolicy {
                    policy_uid: policy.policy_uid.clone(),
                    source: first_error,
                });
            }
        }
        Ok(policies.to_vec())
    }

    /// Collect distinct measurement kinds required by all policies.
    fn collect_required_kinds(&self, policies: &[LoadedPolicy]) -> HashSet<String> {
        policies
            .iter()
            .map(|p| p.payload.measurement_kind.clone())
            .collect()
    }

    /// Convert enriched measurements to pure-engine facts.
    fn enrich_to_facts(
        &self,
        measurements: &[EnrichedMeasurement],
    ) -> Result<Vec<MeasurementFact>, RunnerError> {
        let mut facts = Vec::with_capacity(measurements.len());

        for m in measurements {
            // Parse the measurement kind to the typed enum.
            // If the kind is unknown, skip this measurement (it won't match any policy).
            let kind = match parse_measurement_kind(&m.measurement_kind) {
                Ok(k) => k,
                Err(_) => continue, // Unknown kind — skip silently (not a policy target)
            };

            facts.push(MeasurementFact {
                target_stable_key: m.target_stable_key.clone(),
                measurement_kind: kind,
                value: m.value,
                file_path: m.file_path.clone(),
                symbol_kind: m.symbol_kind.clone(),
            });
        }

        Ok(facts)
    }

    /// Convert a policy assessment to storage input DTO.
    fn assessment_to_input(
        &self,
        assessment: &PolicyAssessment,
        created_at: &str,
    ) -> QualityAssessmentInput {
        let verdict = match assessment.computed_verdict {
            repo_graph_quality_policy::assess::AssessmentVerdict::Pass => AssessmentVerdict::Pass,
            repo_graph_quality_policy::assess::AssessmentVerdict::Fail => AssessmentVerdict::Fail,
            repo_graph_quality_policy::assess::AssessmentVerdict::NotApplicable => {
                AssessmentVerdict::NotApplicable
            }
            repo_graph_quality_policy::assess::AssessmentVerdict::NotComparable => {
                AssessmentVerdict::NotComparable
            }
        };

        // Convert violations to storage format.
        let violations: Vec<repo_graph_storage::types::AssessmentViolation> = assessment
            .violations
            .iter()
            .map(|v| repo_graph_storage::types::AssessmentViolation {
                target_stable_key: v.target_stable_key.clone(),
                measurement_value: v.measurement_value,
                threshold: v.threshold,
                baseline_value: v.baseline_value,
            })
            .collect();

        // Compute new_violations and worsened_violations from violations.
        // new_violations: violations with no baseline_value (symbol didn't exist in baseline)
        // worsened_violations: violations with baseline_value (symbol existed but got worse)
        // These are only meaningful for comparative policies (has baseline).
        let (new_violations, worsened_violations) = if assessment.baseline_snapshot_uid.is_some() {
            let new_count = assessment
                .violations
                .iter()
                .filter(|v| v.baseline_value.is_none())
                .count() as i64;
            let worsened_count = assessment
                .violations
                .iter()
                .filter(|v| v.baseline_value.is_some())
                .count() as i64;
            (Some(new_count), Some(worsened_count))
        } else {
            (None, None)
        };

        QualityAssessmentInput {
            assessment_uid: assessment.assessment_uid.clone(),
            snapshot_uid: assessment.snapshot_uid.clone(),
            policy_uid: assessment.policy_uid.clone(),
            baseline_snapshot_uid: assessment.baseline_snapshot_uid.clone(),
            computed_verdict: verdict,
            measurements_evaluated: assessment.measurements_evaluated as i64,
            violations,
            new_violations,
            worsened_violations,
            created_at: created_at.to_string(),
        }
    }

    /// Build the result summary from assessments.
    fn build_result(
        &self,
        assessments: &[PolicyAssessment],
        baseline_required_count: usize,
    ) -> AssessmentResult {
        use repo_graph_quality_policy::assess::AssessmentVerdict as AV;

        let mut pass = 0;
        let mut fail = 0;
        let mut not_applicable = 0;
        let mut not_comparable = 0;

        for a in assessments {
            match a.computed_verdict {
                AV::Pass => pass += 1,
                AV::Fail => fail += 1,
                AV::NotApplicable => not_applicable += 1,
                AV::NotComparable => not_comparable += 1,
            }
        }

        AssessmentResult {
            total_assessments: assessments.len(),
            pass_count: pass,
            fail_count: fail,
            not_applicable_count: not_applicable,
            not_comparable_count: not_comparable,
            baseline_required_count,
        }
    }
}

/// Get current time in ISO-8601 format.
///
/// Returns UTC timestamp in the format `YYYY-MM-DDTHH:MM:SSZ`.
/// Uses standard library only (no chrono dependency).
fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Convert Unix timestamp to UTC datetime components.
    // This is a simplified calculation that works for dates after 1970.
    const SECS_PER_MIN: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let days_since_epoch = secs / SECS_PER_DAY;
    let time_of_day = secs % SECS_PER_DAY;

    let hour = time_of_day / SECS_PER_HOUR;
    let minute = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;
    let second = time_of_day % SECS_PER_MIN;

    // Calculate year, month, day from days since epoch (1970-01-01).
    // Using a simple iterative approach for correctness.
    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Days in each month (non-leap year).
    const DAYS_IN_MONTH: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    fn is_leap_year(year: u64) -> bool {
        (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
    }

    fn days_in_year(year: u64) -> u64 {
        if is_leap_year(year) { 366 } else { 365 }
    }

    let mut remaining = days;
    let mut year = 1970u64;

    // Find year.
    loop {
        let days_this_year = days_in_year(year);
        if remaining < days_this_year {
            break;
        }
        remaining -= days_this_year;
        year += 1;
    }

    // Find month.
    let mut month = 1u64;
    for (i, &days_in_m) in DAYS_IN_MONTH.iter().enumerate() {
        let days_this_month = if i == 1 && is_leap_year(year) {
            29
        } else {
            days_in_m
        };
        if remaining < days_this_month {
            break;
        }
        remaining -= days_this_month;
        month += 1;
    }

    let day = remaining + 1; // Days are 1-indexed.

    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_storage::error::StorageError;
    use repo_graph_storage::quality_policy_port::{
        EnrichedMeasurement, LoadedPolicy, QualityPolicyStoragePort,
    };
    use repo_graph_storage::types::{
        QualityAssessmentInput, QualityPolicyKind, QualityPolicyPayload, QualityPolicySeverity,
    };
    use std::cell::RefCell;

    /// Mock port for testing.
    struct MockPort {
        policies: Vec<LoadedPolicy>,
        current_measurements: Vec<EnrichedMeasurement>,
        baseline_measurements: Option<Vec<EnrichedMeasurement>>,
        persisted: RefCell<Vec<QualityAssessmentInput>>,
    }

    impl MockPort {
        fn new() -> Self {
            Self {
                policies: vec![],
                current_measurements: vec![],
                baseline_measurements: None,
                persisted: RefCell::new(vec![]),
            }
        }

        fn with_policies(mut self, policies: Vec<LoadedPolicy>) -> Self {
            self.policies = policies;
            self
        }

        fn with_current_measurements(mut self, measurements: Vec<EnrichedMeasurement>) -> Self {
            self.current_measurements = measurements;
            self
        }

        fn with_baseline_measurements(mut self, measurements: Vec<EnrichedMeasurement>) -> Self {
            self.baseline_measurements = Some(measurements);
            self
        }
    }

    impl QualityPolicyStoragePort for MockPort {
        fn load_active_quality_policies(
            &self,
            _repo_uid: &str,
        ) -> Result<Vec<LoadedPolicy>, StorageError> {
            Ok(self.policies.clone())
        }

        fn load_enriched_measurements(
            &self,
            snapshot_uid: &str,
            _kinds: &[&str],
        ) -> Result<Vec<EnrichedMeasurement>, StorageError> {
            // Simple logic: return current or baseline based on snapshot_uid pattern.
            if snapshot_uid.contains("baseline") {
                Ok(self.baseline_measurements.clone().unwrap_or_default())
            } else {
                Ok(self.current_measurements.clone())
            }
        }

        fn replace_assessments(
            &mut self,
            _snapshot_uid: &str,
            assessments: &[QualityAssessmentInput],
        ) -> Result<usize, StorageError> {
            *self.persisted.borrow_mut() = assessments.to_vec();
            Ok(assessments.len())
        }
    }

    fn make_policy(
        uid: &str,
        policy_kind: QualityPolicyKind,
        measurement_kind: &str,
        threshold: f64,
    ) -> LoadedPolicy {
        LoadedPolicy {
            policy_uid: uid.to_string(),
            payload: QualityPolicyPayload {
                policy_id: "test".to_string(),
                version: 1,
                scope_clauses: vec![],
                measurement_kind: measurement_kind.to_string(),
                policy_kind,
                threshold,
                severity: QualityPolicySeverity::Fail,
                description: None,
            },
        }
    }

    fn make_measurement(
        stable_key: &str,
        kind: &str,
        value: f64,
        file_path: Option<&str>,
        symbol_kind: Option<&str>,
    ) -> EnrichedMeasurement {
        EnrichedMeasurement {
            target_stable_key: stable_key.to_string(),
            measurement_kind: kind.to_string(),
            value,
            file_path: file_path.map(|s| s.to_string()),
            symbol_kind: symbol_kind.map(|s| s.to_string()),
        }
    }

    #[test]
    fn assess_snapshot_no_policies() {
        let port = MockPort::new();
        let mut runner = QualityPolicyRunner::new(port);

        let result = runner
            .assess_snapshot("r1", "snap1", None)
            .expect("should succeed");

        assert_eq!(result.total_assessments, 0);
    }

    #[test]
    fn assess_snapshot_absolute_max_pass() {
        let policy = make_policy(
            "p1",
            QualityPolicyKind::AbsoluteMax,
            "cyclomatic_complexity",
            15.0,
        );
        let measurement = make_measurement(
            "r1:src/a.ts#foo:SYMBOL:FUNCTION",
            "cyclomatic_complexity",
            10.0,
            Some("src/a.ts"),
            Some("FUNCTION"),
        );

        let port = MockPort::new()
            .with_policies(vec![policy])
            .with_current_measurements(vec![measurement]);

        let mut runner = QualityPolicyRunner::new(port);

        let result = runner
            .assess_snapshot("r1", "snap1", None)
            .expect("should succeed");

        assert_eq!(result.total_assessments, 1);
        assert_eq!(result.pass_count, 1);
        assert_eq!(result.fail_count, 0);
    }

    #[test]
    fn assess_snapshot_absolute_max_fail() {
        let policy = make_policy(
            "p1",
            QualityPolicyKind::AbsoluteMax,
            "cyclomatic_complexity",
            10.0,
        );
        let measurement = make_measurement(
            "r1:src/a.ts#foo:SYMBOL:FUNCTION",
            "cyclomatic_complexity",
            15.0, // exceeds threshold
            Some("src/a.ts"),
            Some("FUNCTION"),
        );

        let port = MockPort::new()
            .with_policies(vec![policy])
            .with_current_measurements(vec![measurement]);

        let mut runner = QualityPolicyRunner::new(port);

        let result = runner
            .assess_snapshot("r1", "snap1", None)
            .expect("should succeed");

        assert_eq!(result.total_assessments, 1);
        assert_eq!(result.pass_count, 0);
        assert_eq!(result.fail_count, 1);
    }

    #[test]
    fn assess_snapshot_comparative_requires_baseline() {
        let policy = make_policy(
            "p1",
            QualityPolicyKind::NoNew,
            "cyclomatic_complexity",
            10.0,
        );

        let port = MockPort::new().with_policies(vec![policy]);

        let mut runner = QualityPolicyRunner::new(port);

        let err = runner
            .assess_snapshot("r1", "snap1", None)
            .expect_err("should fail");

        match err {
            RunnerError::BaselineRequired(count) => assert_eq!(count, 1),
            _ => panic!("expected BaselineRequired error"),
        }
    }

    #[test]
    fn assess_snapshot_comparative_with_baseline() {
        let policy = make_policy(
            "p1",
            QualityPolicyKind::NoNew,
            "cyclomatic_complexity",
            10.0,
        );

        // Baseline has one violator.
        let baseline_measurement = make_measurement(
            "r1:src/a.ts#foo:SYMBOL:FUNCTION",
            "cyclomatic_complexity",
            15.0,
            Some("src/a.ts"),
            Some("FUNCTION"),
        );

        // Current has same violator (no new).
        let current_measurement = make_measurement(
            "r1:src/a.ts#foo:SYMBOL:FUNCTION",
            "cyclomatic_complexity",
            15.0,
            Some("src/a.ts"),
            Some("FUNCTION"),
        );

        let port = MockPort::new()
            .with_policies(vec![policy])
            .with_current_measurements(vec![current_measurement])
            .with_baseline_measurements(vec![baseline_measurement]);

        let mut runner = QualityPolicyRunner::new(port);

        let result = runner
            .assess_snapshot("r1", "snap1", Some("snap-baseline"))
            .expect("should succeed");

        assert_eq!(result.total_assessments, 1);
        assert_eq!(result.baseline_required_count, 1);
        // No new violators → PASS
        assert_eq!(result.pass_count, 1);
    }

    #[test]
    fn assess_snapshot_invalid_policy_fails_loudly() {
        // Invalid policy: empty policy_id
        let invalid_policy = LoadedPolicy {
            policy_uid: "p1".to_string(),
            payload: QualityPolicyPayload {
                policy_id: "".to_string(), // Invalid
                version: 1,
                scope_clauses: vec![],
                measurement_kind: "cyclomatic_complexity".to_string(),
                policy_kind: QualityPolicyKind::AbsoluteMax,
                threshold: 10.0,
                severity: QualityPolicySeverity::Fail,
                description: None,
            },
        };

        let port = MockPort::new().with_policies(vec![invalid_policy]);

        let mut runner = QualityPolicyRunner::new(port);

        let err = runner
            .assess_snapshot("r1", "snap1", None)
            .expect_err("should fail");

        match err {
            RunnerError::InvalidPolicy { policy_uid, .. } => {
                assert_eq!(policy_uid, "p1");
            }
            _ => panic!("expected InvalidPolicy error"),
        }
    }

    #[test]
    fn assess_snapshot_persists_assessments() {
        let policy = make_policy(
            "p1",
            QualityPolicyKind::AbsoluteMax,
            "cyclomatic_complexity",
            15.0,
        );
        let measurement = make_measurement(
            "r1:src/a.ts#foo:SYMBOL:FUNCTION",
            "cyclomatic_complexity",
            10.0,
            Some("src/a.ts"),
            Some("FUNCTION"),
        );

        let port = MockPort::new()
            .with_policies(vec![policy])
            .with_current_measurements(vec![measurement]);

        let mut runner = QualityPolicyRunner::new(port);

        runner
            .assess_snapshot("r1", "snap1", None)
            .expect("should succeed");

        // Check that assessments were persisted.
        let persisted = runner.port.persisted.borrow();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].policy_uid, "p1");
        assert_eq!(persisted[0].snapshot_uid, "snap1");
    }

    // ── Timestamp tests ────────────────────────────────────────────────────

    #[test]
    fn days_to_ymd_epoch() {
        // Day 0 is 1970-01-01.
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2026-04-26 is 20569 days after 1970-01-01.
        // Let's verify a simpler date: 2000-01-01 is day 10957.
        assert_eq!(days_to_ymd(10957), (2000, 1, 1));
    }

    #[test]
    fn days_to_ymd_leap_year() {
        // 2000-02-29 (leap year) is day 10957 + 31 + 28 = 11016.
        // Actually 2000 is a leap year, so Feb has 29 days.
        // 2000-02-29 is day 10957 + 31 + 28 = 11016.
        assert_eq!(days_to_ymd(11016), (2000, 2, 29));
    }

    #[test]
    fn iso_now_format() {
        let ts = iso_now();
        // Should match YYYY-MM-DDTHH:MM:SSZ pattern.
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }
}
