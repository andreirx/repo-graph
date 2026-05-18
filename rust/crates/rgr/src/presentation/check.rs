//! Presentation layer for the `check` command.
//!
//! # CLI-OUT-1
//!
//! Transforms daemon check response (OrientResult envelope) into
//! human-readable plain text focused on the verdict.
//!
//! ## Human Output Structure
//!
//! ```text
//! Repo: billing-service
//! Verdict: FAIL
//!
//! Failing conditions
//!   - GATE_PASS: Gate is failing (1 of 3 obligations).
//!   - NO_STALE_FILES: 2 stale files detected in snapshot.
//!
//! Passing conditions
//!   - SNAPSHOT_EXISTS: Snapshot exists.
//!   - CALL_GRAPH_RELIABLE: Call graph reliability is high.
//! ```

use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line};

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized check response from daemon.
///
/// Check uses the OrientResult envelope but with CHECK_* signals.
#[derive(Debug, Deserialize)]
pub struct CheckResponse {
    pub repo: String,
    /// Human-readable repo name for CLI display.
    /// Populated by daemon from registry alias or path basename.
    /// When present, prefer this over `repo` (which is internal UID).
    #[serde(default)]
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub snapshot: String,
    pub confidence: String,
    #[serde(default)]
    pub signals: Vec<CheckSignal>,
}

#[derive(Debug, Deserialize)]
pub struct CheckSignal {
    pub code: String,
    pub severity: String,
    pub summary: String,
    #[serde(default)]
    pub evidence: Option<CheckEvidence>,
}

#[derive(Debug, Deserialize)]
pub struct CheckEvidence {
    #[serde(default)]
    pub conditions: Vec<ConditionEvidence>,
    #[serde(default)]
    pub fail_conditions: Vec<ConditionEvidence>,
    #[serde(default)]
    pub incomplete_conditions: Vec<ConditionEvidence>,
    #[serde(default)]
    pub passing: Vec<ConditionEvidence>,
}

#[derive(Debug, Deserialize)]
pub struct ConditionEvidence {
    pub code: String,
    pub status: String,
    pub summary: String,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl CheckResponse {
    /// Render the check response as human-readable plain text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        // CLI-OUT-2B: prefer display_name (human-readable) over internal repo UID
        let repo_display = self.display_name.as_deref().unwrap_or(&self.repo);
        out.push_str(&kv_line("Repo", repo_display));

        // Determine verdict from signals
        let verdict = self.determine_verdict();
        out.push_str(&kv_line("Verdict", &verdict));
        out.push('\n');

        // ── Condition details ──────────────────────────────────────
        if let Some(signal) = self.find_check_signal() {
            out.push_str(&self.render_conditions(signal));
        }

        out.trim_end().to_string()
    }

    fn determine_verdict(&self) -> String {
        for signal in &self.signals {
            match signal.code.as_str() {
                "CHECK_PASS" => return "PASS".to_string(),
                "CHECK_FAIL" => return "FAIL".to_string(),
                "CHECK_INCOMPLETE" => return "INCOMPLETE".to_string(),
                _ => {}
            }
        }
        "UNKNOWN".to_string()
    }

    fn find_check_signal(&self) -> Option<&CheckSignal> {
        self.signals.iter().find(|s| {
            matches!(
                s.code.as_str(),
                "CHECK_PASS" | "CHECK_FAIL" | "CHECK_INCOMPLETE"
            )
        })
    }

    fn render_conditions(&self, signal: &CheckSignal) -> String {
        let mut out = String::new();

        let evidence = match &signal.evidence {
            Some(e) => e,
            None => return out,
        };

        // Incomplete conditions (if any)
        if !evidence.incomplete_conditions.is_empty() {
            out.push_str(&heading("Incomplete conditions"));
            for c in &evidence.incomplete_conditions {
                out.push_str(&bullet(&format!("{}: {}", c.code, c.summary)));
            }
            out.push('\n');
        }

        // Failing conditions (if any)
        if !evidence.fail_conditions.is_empty() {
            out.push_str(&heading("Failing conditions"));
            for c in &evidence.fail_conditions {
                out.push_str(&bullet(&format!("{}: {}", c.code, c.summary)));
            }
            out.push('\n');
        }

        // Passing conditions
        // For CHECK_PASS, use .conditions; for others, use .passing
        let passing = if signal.code == "CHECK_PASS" {
            &evidence.conditions
        } else {
            &evidence.passing
        };

        if !passing.is_empty() {
            out.push_str(&heading("Passing conditions"));
            for c in passing {
                out.push_str(&bullet(&format!("{}: {}", c.code, c.summary)));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_response() -> CheckResponse {
        CheckResponse {
            repo: "test-repo".to_string(),
            display_name: None,
            snapshot: "snap-123".to_string(),
            confidence: "high".to_string(),
            signals: vec![],
        }
    }

    #[test]
    fn render_shows_repo_name() {
        let mut r = minimal_response();
        r.signals = vec![CheckSignal {
            code: "CHECK_PASS".to_string(),
            severity: "low".to_string(),
            summary: "Check passes.".to_string(),
            evidence: None,
        }];
        let out = r.render_human();
        assert!(out.contains("Repo: test-repo"));
    }

    #[test]
    fn render_shows_pass_verdict() {
        let mut r = minimal_response();
        r.signals = vec![CheckSignal {
            code: "CHECK_PASS".to_string(),
            severity: "low".to_string(),
            summary: "Check passes: all 3 conditions pass.".to_string(),
            evidence: Some(CheckEvidence {
                conditions: vec![ConditionEvidence {
                    code: "SNAPSHOT_EXISTS".to_string(),
                    status: "pass".to_string(),
                    summary: "Snapshot exists.".to_string(),
                }],
                fail_conditions: vec![],
                incomplete_conditions: vec![],
                passing: vec![],
            }),
        }];
        let out = r.render_human();
        assert!(out.contains("Verdict: PASS"));
        assert!(out.contains("Passing conditions"));
        assert!(out.contains("SNAPSHOT_EXISTS: Snapshot exists."));
    }

    #[test]
    fn render_shows_fail_verdict() {
        let mut r = minimal_response();
        r.signals = vec![CheckSignal {
            code: "CHECK_FAIL".to_string(),
            severity: "high".to_string(),
            summary: "Check fails: 1 condition failing.".to_string(),
            evidence: Some(CheckEvidence {
                conditions: vec![],
                fail_conditions: vec![ConditionEvidence {
                    code: "GATE_PASS".to_string(),
                    status: "fail".to_string(),
                    summary: "Gate is failing.".to_string(),
                }],
                incomplete_conditions: vec![],
                passing: vec![ConditionEvidence {
                    code: "SNAPSHOT_EXISTS".to_string(),
                    status: "pass".to_string(),
                    summary: "Snapshot exists.".to_string(),
                }],
            }),
        }];
        let out = r.render_human();
        assert!(out.contains("Verdict: FAIL"));
        assert!(out.contains("Failing conditions"));
        assert!(out.contains("GATE_PASS: Gate is failing."));
        assert!(out.contains("Passing conditions"));
    }

    #[test]
    fn render_shows_incomplete_verdict() {
        let mut r = minimal_response();
        r.signals = vec![CheckSignal {
            code: "CHECK_INCOMPLETE".to_string(),
            severity: "medium".to_string(),
            summary: "Check incomplete: 1 condition missing data.".to_string(),
            evidence: Some(CheckEvidence {
                conditions: vec![],
                fail_conditions: vec![],
                incomplete_conditions: vec![ConditionEvidence {
                    code: "SNAPSHOT_EXISTS".to_string(),
                    status: "incomplete".to_string(),
                    summary: "No snapshot found.".to_string(),
                }],
                passing: vec![],
            }),
        }];
        let out = r.render_human();
        assert!(out.contains("Verdict: INCOMPLETE"));
        assert!(out.contains("Incomplete conditions"));
        assert!(out.contains("SNAPSHOT_EXISTS: No snapshot found."));
    }

    #[test]
    fn render_hides_internal_fields() {
        let mut r = minimal_response();
        r.signals = vec![CheckSignal {
            code: "CHECK_PASS".to_string(),
            severity: "low".to_string(),
            summary: "Check passes.".to_string(),
            evidence: None,
        }];
        let out = r.render_human();
        assert!(!out.contains("snap-123"), "snapshot should be hidden");
    }

    #[test]
    fn deserialize_from_daemon_json() {
        let json = r#"{
            "schema": "rgr.agent.v1",
            "command": "check",
            "repo": "my-app",
            "snapshot": "snap-abc",
            "focus": { "resolved": true, "resolved_kind": "repo" },
            "confidence": "high",
            "signals": [
                {
                    "code": "CHECK_PASS",
                    "severity": "low",
                    "category": "check",
                    "summary": "Check passes: all 3 conditions pass.",
                    "evidence": {
                        "conditions": [
                            { "code": "SNAPSHOT_EXISTS", "status": "pass", "summary": "Snapshot exists." }
                        ]
                    }
                }
            ],
            "limits": [],
            "next": [],
            "truncated": false
        }"#;

        let r: CheckResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.repo, "my-app");
        assert_eq!(r.signals.len(), 1);
        assert_eq!(r.signals[0].code, "CHECK_PASS");
    }
}
