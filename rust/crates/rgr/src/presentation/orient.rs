//! Presentation layer for the `orient` command.
//!
//! # CLI-OUT-1
//!
//! Transforms daemon OrientResult DTO into human-readable plain text.
//!
//! ## Daemon Response Fields
//!
//! The daemon returns:
//! - schema, command — internal, hidden from human output
//! - repo, snapshot — repo identity
//! - focus — what was resolved
//! - confidence — overall confidence level
//! - documentation — relevant doc files (primary orientation evidence)
//! - signals — actionable findings
//! - limits — processing limitations
//! - next — suggested follow-up actions
//! - truncated — whether output was truncated
//! - trust (optional) — degradation overlay when trust is not high
//!
//! ## Human Output Structure
//!
//! ```text
//! Repo: billing-service
//! Focus: src/core/auth (module)
//! Confidence: high
//!
//! Documentation
//!   - README.md (repo root)
//!   - src/core/auth/README.md (module path)
//!
//! Signals
//!   High
//!     - Gate fails: 2 of 5 obligations failing.
//!   Medium
//!     - 3 import cycles detected at the module level.
//!   Low
//!     - 150 files, 1200 symbols indexed.
//!
//! Degradation
//!   - Call resolution rate is 78% (120 of 154 calls resolved).
//!
//! Next steps
//!   - rmap check
//!   - rmap explain src/core/auth/session.ts
//! ```

use serde::Deserialize;

use crate::presentation::{bullet, bullet_list, heading, kv_line, DisplaySeverity};

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized orient response from daemon.
///
/// This struct captures the subset of daemon DTO fields needed for
/// human rendering. Fields like `schema` and `command` are not included
/// because they are internal envelope scaffolding.
#[derive(Debug, Deserialize)]
pub struct OrientResponse {
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    pub focus: Focus,
    pub confidence: String,
    #[serde(default)]
    pub documentation: Option<DocumentationSection>,
    #[serde(default)]
    pub signals: Vec<Signal>,
    #[serde(default)]
    pub limits: Vec<Limit>,
    #[serde(default)]
    pub next: Vec<NextAction>,
    #[serde(default)]
    pub truncated: bool,
    /// Trust overlay — present when there is degradation.
    #[serde(default)]
    pub trust: Option<TrustOverlay>,
}

#[derive(Debug, Deserialize)]
pub struct Focus {
    #[serde(default)]
    pub input: Option<String>,
    pub resolved: bool,
    #[serde(default)]
    pub resolved_kind: Option<String>,
    #[serde(default)]
    pub resolved_path: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DocumentationSection {
    #[serde(default)]
    pub relevant_files: Vec<RelevantDoc>,
    #[serde(default)]
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct RelevantDoc {
    pub path: String,
    pub kind: String,
    #[serde(default)]
    pub generated: bool,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct Signal {
    pub code: String,
    pub severity: String,
    pub category: String,
    pub summary: String,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Limit {
    pub code: String,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct NextAction {
    pub kind: String,
    pub repo: String,
    #[serde(default)]
    pub target: Option<String>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct TrustOverlay {
    #[serde(default)]
    pub call_graph_reliability: Option<String>,
    #[serde(default)]
    pub call_resolution_rate: Option<f64>,
    #[serde(default)]
    pub caveats: Vec<String>,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl OrientResponse {
    /// Render the orient response as human-readable plain text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        out.push_str(&kv_line("Repo", &self.repo));
        out.push_str(&self.render_focus());
        out.push_str(&kv_line("Confidence", &self.confidence));
        out.push('\n');

        // ── Documentation ──────────────────────────────────────────
        if let Some(docs) = &self.documentation {
            if !docs.relevant_files.is_empty() {
                out.push_str(&self.render_documentation(docs));
                out.push('\n');
            }
        }

        // ── Signals ────────────────────────────────────────────────
        if !self.signals.is_empty() {
            out.push_str(&self.render_signals());
            out.push('\n');
        }

        // ── Degradation (from trust overlay) ───────────────────────
        if let Some(trust) = &self.trust {
            let degradation = self.render_degradation(trust);
            if !degradation.is_empty() {
                out.push_str(&degradation);
                out.push('\n');
            }
        }

        // ── Limits ─────────────────────────────────────────────────
        if !self.limits.is_empty() {
            out.push_str(&self.render_limits());
            out.push('\n');
        }

        // ── Next steps ─────────────────────────────────────────────
        if !self.next.is_empty() {
            out.push_str(&self.render_next_steps());
        }

        // ── Truncation warning ─────────────────────────────────────
        if self.truncated {
            out.push_str("\n[Output truncated. Use --json for full results.]\n");
        }

        out.trim_end().to_string()
    }

    fn render_focus(&self) -> String {
        if !self.focus.resolved {
            let input = self.focus.input.as_deref().unwrap_or("(unknown)");
            let reason = self.focus.reason.as_deref().unwrap_or("no match");
            return kv_line("Focus", &format!("{} (unresolved: {})", input, reason));
        }

        let kind = self.focus.resolved_kind.as_deref().unwrap_or("unknown");
        match &self.focus.resolved_path {
            Some(path) => kv_line("Focus", &format!("{} ({})", path, kind)),
            None => {
                // Repo-level focus — no path
                if kind == "repo" {
                    kv_line("Focus", "(repo)")
                } else {
                    kv_line("Focus", &format!("({})", kind))
                }
            }
        }
    }

    fn render_documentation(&self, docs: &DocumentationSection) -> String {
        let mut out = heading("Documentation");
        for doc in &docs.relevant_files {
            let marker = if doc.generated { " (generated)" } else { "" };
            out.push_str(&bullet(&format!("{}{}", doc.path, marker)));
        }
        out
    }

    fn render_signals(&self) -> String {
        let mut out = heading("Signals");

        // Group signals by severity
        let mut high: Vec<&Signal> = Vec::new();
        let mut medium: Vec<&Signal> = Vec::new();
        let mut low: Vec<&Signal> = Vec::new();

        for signal in &self.signals {
            match DisplaySeverity::parse(&signal.severity) {
                DisplaySeverity::High => high.push(signal),
                DisplaySeverity::Medium => medium.push(signal),
                DisplaySeverity::Low => low.push(signal),
            }
        }

        // Render each severity group
        if !high.is_empty() {
            out.push_str("  High\n");
            for s in high {
                out.push_str(&format!("    - {}\n", s.summary));
            }
        }
        if !medium.is_empty() {
            out.push_str("  Medium\n");
            for s in medium {
                out.push_str(&format!("    - {}\n", s.summary));
            }
        }
        if !low.is_empty() {
            out.push_str("  Low\n");
            for s in low {
                out.push_str(&format!("    - {}\n", s.summary));
            }
        }

        out
    }

    fn render_degradation(&self, trust: &TrustOverlay) -> String {
        let mut items: Vec<String> = Vec::new();

        // Call resolution rate
        if let Some(rate) = trust.call_resolution_rate {
            if rate < 0.95 {
                items.push(format!("Call resolution rate: {:.0}%", rate * 100.0));
            }
        }

        // Call graph reliability
        if let Some(reliability) = &trust.call_graph_reliability {
            if reliability != "high" {
                items.push(format!("Call graph reliability: {}", reliability));
            }
        }

        // Caveats
        for caveat in &trust.caveats {
            items.push(caveat.clone());
        }

        if items.is_empty() {
            return String::new();
        }

        let mut out = heading("Degradation");
        out.push_str(&bullet_list(&items));
        out
    }

    fn render_limits(&self) -> String {
        let mut out = heading("Limits");
        for limit in &self.limits {
            out.push_str(&bullet(&limit.summary));
        }
        out
    }

    fn render_next_steps(&self) -> String {
        let mut out = heading("Next steps");
        for action in &self.next {
            let cmd = match &action.target {
                Some(target) => format!("rmap {} {}", action.kind, target),
                None => format!("rmap {}", action.kind),
            };
            out.push_str(&bullet(&cmd));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_response() -> OrientResponse {
        OrientResponse {
            repo: "test-repo".to_string(),
            snapshot: "snap-123".to_string(),
            focus: Focus {
                input: None,
                resolved: true,
                resolved_kind: Some("repo".to_string()),
                resolved_path: None,
                reason: None,
            },
            confidence: "high".to_string(),
            documentation: None,
            signals: vec![],
            limits: vec![],
            next: vec![],
            truncated: false,
            trust: None,
        }
    }

    #[test]
    fn render_shows_repo_name() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Repo: test-repo"));
    }

    #[test]
    fn render_shows_confidence() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Confidence: high"));
    }

    #[test]
    fn render_hides_internal_fields() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(!out.contains("snap-123"), "snapshot should be hidden");
        assert!(!out.contains("schema"), "schema should be hidden");
        assert!(!out.contains("command\":"), "command should be hidden");
    }

    #[test]
    fn render_shows_focus_repo_level() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Focus: (repo)"));
    }

    #[test]
    fn render_shows_focus_with_path() {
        let mut r = minimal_response();
        r.focus = Focus {
            input: Some("src/core".to_string()),
            resolved: true,
            resolved_kind: Some("module".to_string()),
            resolved_path: Some("src/core".to_string()),
            reason: None,
        };
        let out = r.render_human();
        assert!(out.contains("Focus: src/core (module)"));
    }

    #[test]
    fn render_shows_unresolved_focus() {
        let mut r = minimal_response();
        r.focus = Focus {
            input: Some("nonexistent/path".to_string()),
            resolved: false,
            resolved_kind: None,
            resolved_path: None,
            reason: Some("no_match".to_string()),
        };
        let out = r.render_human();
        assert!(out.contains("Focus: nonexistent/path (unresolved: no_match)"));
    }

    #[test]
    fn render_shows_documentation() {
        let mut r = minimal_response();
        r.documentation = Some(DocumentationSection {
            relevant_files: vec![
                RelevantDoc {
                    path: "README.md".to_string(),
                    kind: "readme".to_string(),
                    generated: false,
                    reason: "repo_root_doc".to_string(),
                },
                RelevantDoc {
                    path: "docs/MAP.md".to_string(),
                    kind: "map".to_string(),
                    generated: true,
                    reason: "generated_map_for_target".to_string(),
                },
            ],
            count: 2,
        });
        let out = r.render_human();
        assert!(out.contains("Documentation"));
        assert!(out.contains("README.md"));
        assert!(out.contains("docs/MAP.md (generated)"));
    }

    #[test]
    fn render_shows_signals_grouped_by_severity() {
        let mut r = minimal_response();
        r.signals = vec![
            Signal {
                code: "GATE_FAIL".to_string(),
                severity: "high".to_string(),
                category: "gate".to_string(),
                summary: "Gate fails: 2 of 5 obligations failing.".to_string(),
                scope: None,
            },
            Signal {
                code: "IMPORT_CYCLES".to_string(),
                severity: "medium".to_string(),
                category: "structure".to_string(),
                summary: "3 import cycles detected.".to_string(),
                scope: None,
            },
            Signal {
                code: "MODULE_SUMMARY".to_string(),
                severity: "low".to_string(),
                category: "informational".to_string(),
                summary: "150 files, 1200 symbols indexed.".to_string(),
                scope: None,
            },
        ];
        let out = r.render_human();
        assert!(out.contains("Signals"));
        assert!(out.contains("High"));
        assert!(out.contains("Gate fails:"));
        assert!(out.contains("Medium"));
        assert!(out.contains("3 import cycles"));
        assert!(out.contains("Low"));
        assert!(out.contains("150 files"));
    }

    #[test]
    fn render_shows_degradation_from_trust() {
        let mut r = minimal_response();
        r.trust = Some(TrustOverlay {
            call_graph_reliability: Some("medium".to_string()),
            call_resolution_rate: Some(0.78),
            caveats: vec!["Enrichment phase did not run.".to_string()],
        });
        let out = r.render_human();
        assert!(out.contains("Degradation"));
        assert!(out.contains("Call resolution rate: 78%"));
        assert!(out.contains("Call graph reliability: medium"));
        assert!(out.contains("Enrichment phase did not run."));
    }

    #[test]
    fn render_hides_degradation_when_trust_is_high() {
        let mut r = minimal_response();
        r.trust = Some(TrustOverlay {
            call_graph_reliability: Some("high".to_string()),
            call_resolution_rate: Some(0.98),
            caveats: vec![],
        });
        let out = r.render_human();
        assert!(!out.contains("Degradation"));
    }

    #[test]
    fn render_shows_limits() {
        let mut r = minimal_response();
        r.limits = vec![Limit {
            code: "MODULE_DATA_UNAVAILABLE".to_string(),
            summary: "Module discovery data unavailable.".to_string(),
        }];
        let out = r.render_human();
        assert!(out.contains("Limits"));
        assert!(out.contains("Module discovery data unavailable."));
    }

    #[test]
    fn render_shows_next_steps() {
        let mut r = minimal_response();
        r.next = vec![
            NextAction {
                kind: "check".to_string(),
                repo: "test-repo".to_string(),
                target: None,
                reason: "Verify current state.".to_string(),
            },
            NextAction {
                kind: "explain".to_string(),
                repo: "test-repo".to_string(),
                target: Some("src/core/auth.ts".to_string()),
                reason: "Deep dive on auth module.".to_string(),
            },
        ];
        let out = r.render_human();
        assert!(out.contains("Next steps"));
        assert!(out.contains("rmap check"));
        assert!(out.contains("rmap explain src/core/auth.ts"));
    }

    #[test]
    fn render_shows_truncation_warning() {
        let mut r = minimal_response();
        r.truncated = true;
        let out = r.render_human();
        assert!(out.contains("[Output truncated. Use --json for full results.]"));
    }

    #[test]
    fn deserialize_from_daemon_json() {
        let json = r#"{
            "schema": "rgr.agent.v1",
            "command": "orient",
            "repo": "my-app",
            "snapshot": "snap-abc",
            "focus": {
                "resolved": true,
                "resolved_kind": "repo"
            },
            "confidence": "high",
            "signals": [],
            "limits": [],
            "next": [],
            "truncated": false
        }"#;

        let r: OrientResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.repo, "my-app");
        assert_eq!(r.confidence, "high");
        assert!(r.focus.resolved);
    }
}
