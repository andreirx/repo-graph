//! Presentation layer for the `gate` command.
//!
//! # CLI-OUT-7 Group 3
//!
//! Transforms gate evaluation results into human-readable plain text.
//! Shows outcome summary, counts breakdown, and obligation list.
//!
//! # Output Contract
//!
//! - Gate outcome summary (outcome, mode, exit_code)
//! - Counts breakdown (total, pass, fail, waived, missing_evidence, unsupported)
//! - Quality assessment counts (when non-zero)
//! - Obligation list with verdicts
//! - Evidence display (method-specific)
//! - Waiver basis display (when effective != computed)
//! - Domain verdicts preserved exactly (PASS/FAIL/WAIVED/MISSING_EVIDENCE/UNSUPPORTED)
//! - `--json` preserved for machine mode
//!
//! # Domain Verdict Discipline
//!
//! Verdicts are intrinsic domain terms from the gate crate. They must render
//! exactly as returned:
//! - PASS
//! - FAIL
//! - WAIVED
//! - MISSING_EVIDENCE
//! - UNSUPPORTED
//!
//! Do not soften, remap, or translate these terms.

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Counts breakdown for obligations.
#[derive(Debug, Deserialize, Clone)]
pub struct GateCounts {
    pub total: u64,
    pub pass: u64,
    pub fail: u64,
    pub waived: u64,
    pub missing_evidence: u64,
    pub unsupported: u64,
}

/// Counts breakdown for quality assessments.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct QualityCounts {
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub pass: u64,
    #[serde(default)]
    pub fail: u64,
    #[serde(default)]
    pub advisory_fail: u64,
    #[serde(default)]
    pub missing: u64,
    #[serde(default)]
    pub not_comparable: u64,
    #[serde(default)]
    pub not_applicable: u64,
}

/// Gate outcome summary.
#[derive(Debug, Deserialize, Clone)]
pub struct GateOutcome {
    pub outcome: String,
    pub exit_code: i32,
    pub mode: String,
    pub counts: GateCounts,
    #[serde(default)]
    pub quality_counts: QualityCounts,
}

/// Waiver basis audit trail.
#[derive(Debug, Deserialize, Clone)]
pub struct WaiverBasis {
    #[allow(dead_code)]
    pub waiver_uid: String,
    pub reason: String,
    #[allow(dead_code)]
    pub created_at: String,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub rationale_category: Option<String>,
    #[serde(default)]
    pub policy_basis: Option<String>,
}

/// One evaluated obligation result.
#[derive(Debug, Deserialize, Clone)]
pub struct ObligationEvaluation {
    pub req_id: String,
    pub req_version: i64,
    pub obligation_id: String,
    pub obligation: String,
    pub method: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub threshold: Option<f64>,
    #[serde(default)]
    pub operator: Option<String>,
    pub computed_verdict: String,
    pub effective_verdict: String,
    pub evidence: serde_json::Value,
    #[serde(default)]
    pub waiver_basis: Option<WaiverBasis>,
}

/// One quality-policy assessment evaluation.
#[derive(Debug, Deserialize, Clone)]
pub struct QualityAssessmentEvaluation {
    pub policy_id: String,
    pub policy_version: i64,
    pub policy_kind: String,
    pub severity: String,
    pub assessment_state: String,
    #[serde(default)]
    pub computed_verdict: Option<String>,
    pub is_comparative: bool,
    #[serde(default)]
    pub violations_count: Option<u64>,
}

/// Response DTO for `gate` command.
#[derive(Debug, Deserialize)]
pub struct GateResponse {
    #[allow(dead_code)]
    pub command: String,
    #[allow(dead_code)]
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    #[serde(default)]
    pub toolchain: serde_json::Value,
    pub obligations: Vec<ObligationEvaluation>,
    #[serde(default)]
    pub quality_assessments: Vec<QualityAssessmentEvaluation>,
    pub gate: GateOutcome,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl GateResponse {
    /// Render the gate response as human-readable plain text.
    ///
    /// Sections:
    /// 1. Outcome summary (outcome, mode, exit_code)
    /// 2. Counts breakdown
    /// 3. Quality assessment counts (if any quality assessments)
    /// 4. Obligations list (if any)
    /// 5. Quality assessments list (if any)
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Gate Evaluation\n\n");

        // Outcome summary
        out.push_str(&format!("Outcome: {}\n", self.gate.outcome));
        out.push_str(&format!("Mode: {}\n", self.gate.mode));
        out.push_str(&format!("Exit code: {}\n", self.gate.exit_code));

        // Counts breakdown
        out.push_str("\nObligation counts:\n");
        out.push_str(&format!("  total              {}\n", self.gate.counts.total));
        out.push_str(&format!("  pass               {}\n", self.gate.counts.pass));
        out.push_str(&format!("  fail               {}\n", self.gate.counts.fail));
        out.push_str(&format!("  waived             {}\n", self.gate.counts.waived));
        out.push_str(&format!(
            "  missing evidence   {}\n",
            self.gate.counts.missing_evidence
        ));
        out.push_str(&format!(
            "  unsupported        {}\n",
            self.gate.counts.unsupported
        ));

        // Quality counts (only if there are quality assessments)
        if self.gate.quality_counts.total > 0 {
            out.push_str("\nQuality policy counts:\n");
            out.push_str(&format!(
                "  total              {}\n",
                self.gate.quality_counts.total
            ));
            out.push_str(&format!(
                "  pass               {}\n",
                self.gate.quality_counts.pass
            ));
            out.push_str(&format!(
                "  fail               {}\n",
                self.gate.quality_counts.fail
            ));
            out.push_str(&format!(
                "  advisory fail      {}\n",
                self.gate.quality_counts.advisory_fail
            ));
            out.push_str(&format!(
                "  missing            {}\n",
                self.gate.quality_counts.missing
            ));
            out.push_str(&format!(
                "  not comparable     {}\n",
                self.gate.quality_counts.not_comparable
            ));
            out.push_str(&format!(
                "  not applicable     {}\n",
                self.gate.quality_counts.not_applicable
            ));
        }

        // Obligations list
        if !self.obligations.is_empty() {
            out.push_str("\nObligations:\n");

            // Sort obligations deterministically by (req_id, req_version, obligation_id)
            let mut obligations = self.obligations.clone();
            obligations.sort_by(|a, b| {
                (&a.req_id, a.req_version, &a.obligation_id)
                    .cmp(&(&b.req_id, b.req_version, &b.obligation_id))
            });

            for obl in &obligations {
                out.push_str(&format!(
                    "\n  {} v{} / {}\n",
                    obl.req_id, obl.req_version, obl.obligation_id
                ));
                out.push_str(&format!("    {}\n", obl.obligation));
                out.push_str(&format!("    method: {}\n", obl.method));
                if let Some(ref target) = obl.target {
                    out.push_str(&format!("    target: {}\n", target));
                }
                if let Some(threshold) = obl.threshold {
                    let op = obl.operator.as_deref().unwrap_or(">=");
                    out.push_str(&format!("    threshold: {} {}\n", op, threshold));
                }
                out.push_str(&format!("    computed: {}\n", obl.computed_verdict));
                out.push_str(&format!("    effective: {}\n", obl.effective_verdict));

                // Evidence display (method-specific)
                out.push_str(&format!("    evidence: {}\n", render_evidence(&obl.evidence)));

                // Waiver basis (when effective != computed)
                if let Some(ref waiver) = obl.waiver_basis {
                    out.push_str(&format!("    waiver: {}", waiver.reason));
                    if let Some(ref expires) = waiver.expires_at {
                        out.push_str(&format!(" (expires: {})", expires));
                    }
                    out.push_str("\n");
                }
            }
        }

        // Quality assessments list
        if !self.quality_assessments.is_empty() {
            out.push_str("\nQuality assessments:\n");

            let mut assessments = self.quality_assessments.clone();
            assessments.sort_by(|a, b| {
                (&a.policy_id, a.policy_version).cmp(&(&b.policy_id, b.policy_version))
            });

            for qa in &assessments {
                out.push_str(&format!("\n  {} v{}\n", qa.policy_id, qa.policy_version));
                out.push_str(&format!("    kind: {}\n", qa.policy_kind));
                out.push_str(&format!("    severity: {}\n", qa.severity));
                out.push_str(&format!("    state: {}\n", qa.assessment_state));
                if let Some(ref verdict) = qa.computed_verdict {
                    out.push_str(&format!("    verdict: {}\n", verdict));
                }
                if qa.is_comparative {
                    out.push_str("    comparative: yes\n");
                }
                if let Some(count) = qa.violations_count {
                    out.push_str(&format!("    violations: {}\n", count));
                }
            }
        }

        // Empty obligations case
        if self.obligations.is_empty() && self.quality_assessments.is_empty() {
            out.push_str("\nNo obligations or quality policies configured.\n");
        }

        out
    }
}

/// Render evidence object as a compact string.
///
/// Method-specific evidence fields are rendered as key=value pairs.
fn render_evidence(evidence: &serde_json::Value) -> String {
    if evidence.is_null() {
        return "none".to_string();
    }

    if let Some(obj) = evidence.as_object() {
        if obj.is_empty() {
            return "none".to_string();
        }

        // Render as key=value pairs
        let mut parts: Vec<String> = obj
            .iter()
            .map(|(k, v)| {
                let val_str = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => "null".to_string(),
                    _ => v.to_string(),
                };
                format!("{}={}", k, val_str)
            })
            .collect();

        // Sort for determinism
        parts.sort();
        parts.join(", ")
    } else {
        evidence.to_string()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_counts(
        total: u64,
        pass: u64,
        fail: u64,
        waived: u64,
        missing: u64,
        unsupported: u64,
    ) -> GateCounts {
        GateCounts {
            total,
            pass,
            fail,
            waived,
            missing_evidence: missing,
            unsupported,
        }
    }

    fn make_outcome(outcome: &str, mode: &str, exit_code: i32, counts: GateCounts) -> GateOutcome {
        GateOutcome {
            outcome: outcome.to_string(),
            exit_code,
            mode: mode.to_string(),
            counts,
            quality_counts: QualityCounts::default(),
        }
    }

    fn make_obligation(
        req_id: &str,
        version: i64,
        obl_id: &str,
        obl: &str,
        method: &str,
        computed: &str,
        effective: &str,
    ) -> ObligationEvaluation {
        ObligationEvaluation {
            req_id: req_id.to_string(),
            req_version: version,
            obligation_id: obl_id.to_string(),
            obligation: obl.to_string(),
            method: method.to_string(),
            target: None,
            threshold: None,
            operator: None,
            computed_verdict: computed.to_string(),
            effective_verdict: effective.to_string(),
            evidence: serde_json::json!({}),
            waiver_basis: None,
        }
    }

    fn make_response(outcome: GateOutcome, obligations: Vec<ObligationEvaluation>) -> GateResponse {
        GateResponse {
            command: "gate".to_string(),
            repo: "test-repo".to_string(),
            snapshot: "snap_abc".to_string(),
            toolchain: None,
            obligations,
            quality_assessments: vec![],
            gate: outcome,
        }
    }

    #[test]
    fn render_empty_gate() {
        let resp = make_response(
            make_outcome("pass", "default", 0, make_counts(0, 0, 0, 0, 0, 0)),
            vec![],
        );
        let out = resp.render_human();

        assert!(out.contains("Gate Evaluation"));
        assert!(out.contains("Outcome: pass"));
        assert!(out.contains("Mode: default"));
        assert!(out.contains("Exit code: 0"));
        assert!(out.contains("total              0"));
        assert!(out.contains("No obligations or quality policies configured"));
    }

    #[test]
    fn render_passing_gate() {
        let resp = make_response(
            make_outcome("pass", "strict", 0, make_counts(2, 2, 0, 0, 0, 0)),
            vec![
                make_obligation("REQ-001", 1, "obl-1", "core clean", "arch_violations", "PASS", "PASS"),
                make_obligation("REQ-001", 1, "obl-2", "no hotspots", "hotspot_threshold", "PASS", "PASS"),
            ],
        );
        let out = resp.render_human();

        assert!(out.contains("Outcome: pass"));
        assert!(out.contains("Mode: strict"));
        assert!(out.contains("total              2"));
        assert!(out.contains("pass               2"));
        assert!(out.contains("REQ-001 v1 / obl-1"));
        assert!(out.contains("core clean"));
        assert!(out.contains("method: arch_violations"));
        assert!(out.contains("computed: PASS"));
        assert!(out.contains("effective: PASS"));
    }

    #[test]
    fn render_failing_gate() {
        let resp = make_response(
            make_outcome("fail", "default", 1, make_counts(1, 0, 1, 0, 0, 0)),
            vec![make_obligation(
                "REQ-001",
                1,
                "obl-1",
                "core clean",
                "arch_violations",
                "FAIL",
                "FAIL",
            )],
        );
        let out = resp.render_human();

        assert!(out.contains("Outcome: fail"));
        assert!(out.contains("Exit code: 1"));
        assert!(out.contains("fail               1"));
        assert!(out.contains("computed: FAIL"));
        assert!(out.contains("effective: FAIL"));
    }

    #[test]
    fn render_waived_obligation() {
        let mut obl = make_obligation(
            "REQ-001",
            1,
            "obl-1",
            "known issue",
            "arch_violations",
            "FAIL",
            "WAIVED",
        );
        obl.waiver_basis = Some(WaiverBasis {
            waiver_uid: "w1".to_string(),
            reason: "tracked for removal".to_string(),
            created_at: "2024-01-01".to_string(),
            created_by: None,
            expires_at: Some("2024-06-01".to_string()),
            rationale_category: None,
            policy_basis: None,
        });

        let resp = make_response(
            make_outcome("pass", "default", 0, make_counts(1, 0, 0, 1, 0, 0)),
            vec![obl],
        );
        let out = resp.render_human();

        assert!(out.contains("Outcome: pass"));
        assert!(out.contains("waived             1"));
        assert!(out.contains("computed: FAIL"));
        assert!(out.contains("effective: WAIVED"));
        assert!(out.contains("waiver: tracked for removal (expires: 2024-06-01)"));
    }

    #[test]
    fn render_missing_evidence() {
        let resp = make_response(
            make_outcome("incomplete", "default", 2, make_counts(1, 0, 0, 0, 1, 0)),
            vec![make_obligation(
                "REQ-001",
                1,
                "obl-1",
                "coverage check",
                "coverage_threshold",
                "MISSING_EVIDENCE",
                "MISSING_EVIDENCE",
            )],
        );
        let out = resp.render_human();

        assert!(out.contains("Outcome: incomplete"));
        assert!(out.contains("Exit code: 2"));
        assert!(out.contains("missing evidence   1"));
        assert!(out.contains("computed: MISSING_EVIDENCE"));
        assert!(out.contains("effective: MISSING_EVIDENCE"));
    }

    #[test]
    fn render_unsupported_method() {
        let resp = make_response(
            make_outcome("incomplete", "default", 2, make_counts(1, 0, 0, 0, 0, 1)),
            vec![make_obligation(
                "REQ-001",
                1,
                "obl-1",
                "unknown check",
                "future_method",
                "UNSUPPORTED",
                "UNSUPPORTED",
            )],
        );
        let out = resp.render_human();

        assert!(out.contains("unsupported        1"));
        assert!(out.contains("computed: UNSUPPORTED"));
        assert!(out.contains("effective: UNSUPPORTED"));
    }

    #[test]
    fn render_with_target_and_threshold() {
        let mut obl = make_obligation(
            "REQ-001",
            1,
            "obl-1",
            "coverage check",
            "coverage_threshold",
            "PASS",
            "PASS",
        );
        obl.target = Some("src/core".to_string());
        obl.threshold = Some(0.80);
        obl.operator = Some(">=".to_string());
        obl.evidence = serde_json::json!({
            "files_measured": 10,
            "average_coverage": 0.85
        });

        let resp = make_response(
            make_outcome("pass", "default", 0, make_counts(1, 1, 0, 0, 0, 0)),
            vec![obl],
        );
        let out = resp.render_human();

        assert!(out.contains("target: src/core"));
        assert!(out.contains("threshold: >= 0.8"));
        assert!(out.contains("average_coverage=0.85"));
        assert!(out.contains("files_measured=10"));
    }

    #[test]
    fn render_deterministic_ordering() {
        let resp = make_response(
            make_outcome("pass", "default", 0, make_counts(2, 2, 0, 0, 0, 0)),
            vec![
                make_obligation("REQ-002", 1, "obl-1", "second", "arch_violations", "PASS", "PASS"),
                make_obligation("REQ-001", 1, "obl-1", "first", "arch_violations", "PASS", "PASS"),
            ],
        );
        let out = resp.render_human();

        let req1_pos = out.find("REQ-001 v1").unwrap();
        let req2_pos = out.find("REQ-002 v1").unwrap();
        assert!(req1_pos < req2_pos, "REQ-001 should come before REQ-002");
    }

    #[test]
    fn domain_verdicts_preserved() {
        // Verify verdicts are rendered exactly as returned, not softened
        let resp = make_response(
            make_outcome("incomplete", "default", 2, make_counts(4, 1, 1, 1, 1, 0)),
            vec![
                make_obligation("R", 1, "1", "a", "m", "PASS", "PASS"),
                make_obligation("R", 1, "2", "b", "m", "FAIL", "FAIL"),
                make_obligation("R", 1, "3", "c", "m", "FAIL", "WAIVED"),
                make_obligation("R", 1, "4", "d", "m", "MISSING_EVIDENCE", "MISSING_EVIDENCE"),
            ],
        );
        let out = resp.render_human();

        // Verdicts must appear exactly as domain terms
        assert!(out.contains("computed: PASS"), "PASS not found");
        assert!(out.contains("computed: FAIL"), "FAIL not found");
        assert!(out.contains("effective: WAIVED"), "WAIVED not found");
        assert!(
            out.contains("computed: MISSING_EVIDENCE"),
            "MISSING_EVIDENCE not found"
        );

        // Must NOT contain softened versions
        assert!(!out.contains("passed"), "should not soften PASS");
        assert!(!out.contains("failed"), "should not soften FAIL");
    }

    #[test]
    fn render_evidence_empty() {
        let evidence = serde_json::json!({});
        assert_eq!(render_evidence(&evidence), "none");
    }

    #[test]
    fn render_evidence_null() {
        let evidence = serde_json::Value::Null;
        assert_eq!(render_evidence(&evidence), "none");
    }

    #[test]
    fn render_evidence_object() {
        let evidence = serde_json::json!({
            "violation_count": 0,
            "boundary_count": 2
        });
        let rendered = render_evidence(&evidence);
        // Keys should be sorted
        assert!(rendered.contains("boundary_count=2"));
        assert!(rendered.contains("violation_count=0"));
    }

    #[test]
    fn render_quality_assessments() {
        let mut resp = make_response(
            make_outcome("pass", "default", 0, make_counts(0, 0, 0, 0, 0, 0)),
            vec![],
        );
        resp.gate.quality_counts = QualityCounts {
            total: 1,
            pass: 1,
            fail: 0,
            advisory_fail: 0,
            missing: 0,
            not_comparable: 0,
            not_applicable: 0,
        };
        resp.quality_assessments = vec![QualityAssessmentEvaluation {
            policy_id: "QP-001".to_string(),
            policy_version: 1,
            policy_kind: "absolute_max".to_string(),
            severity: "fail".to_string(),
            assessment_state: "present".to_string(),
            computed_verdict: Some("PASS".to_string()),
            is_comparative: false,
            violations_count: Some(0),
        }];

        let out = resp.render_human();

        assert!(out.contains("Quality policy counts:"));
        assert!(out.contains("Quality assessments:"));
        assert!(out.contains("QP-001 v1"));
        assert!(out.contains("kind: absolute_max"));
        assert!(out.contains("severity: fail"));
        assert!(out.contains("verdict: PASS"));
    }
}
