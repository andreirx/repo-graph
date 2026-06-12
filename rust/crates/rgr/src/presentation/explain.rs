//! Presentation layer for the `explain` command.
//!
//! # CLI-OUT-1
//!
//! Transforms daemon explain response (OrientResult envelope) into
//! human-readable plain text focused on the target's context.
//!
//! ## Human Output Structure (symbol target)
//!
//! ```text
//! Repo: billing-service
//! Target: AuthService.validate (symbol)
//! File: src/core/auth/service.ts
//! Confidence: high
//!
//! Callers (12)
//!   - LoginController.handleLogin
//!   - SessionManager.refresh
//!   - AdminController.impersonate
//!   ... (9 more)
//!
//! Callees (5)
//!   - TokenVerifier.verify
//!   - UserRepository.find
//!   ... (3 more)
//!
//! Trust
//!   - Call resolution: 95%
//!   - Call graph reliability: high
//! ```
//!
//! ## Human Output Structure (file target)
//!
//! ```text
//! Repo: billing-service
//! Target: src/core/auth/service.ts (file)
//! Language: typescript
//! Symbols: 8
//! Confidence: high
//!
//! Symbols
//!   - AuthService (class)
//!   - validate (method)
//!   - refresh (method)
//!   ... (5 more)
//!
//! Imports (3)
//!   - src/core/user/repository.ts
//!   - src/shared/crypto.ts
//!   - src/types/auth.ts
//!
//! Trust
//!   - Call resolution: 95%
//!   - Call graph reliability: high
//! ```

use repo_graph_coherence::CoherenceEnvelope;
use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line};

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized explain response from daemon.
///
/// EXPLAIN-LIVEGRAPH-IMPL: the daemon now returns a `CoherenceEnvelope<CoherentOrientResult>` (the wrapper
/// is the top level), so the CLI parses `CoherenceEnvelope<ExplainResponse>` and renders the inner `value`
/// (see `run_explain_cmd`). Each signal is a LEAF `CoherenceEnvelope<ExplainSignal>` (contract D7) — the
/// inner `ExplainSignal` is pristine; provenance/trust/freshness ride in the wrapper siblings. The renderer
/// reads each `.value`, so the section TEXT is byte-identical to the pre-wrapper output (§1e / §5 W4).
/// explain's `value.trust_briefing` (degraded-only) is JSON-only — the human render never read the overlay
/// and does not start (the renderer carries no `trust` field, matching pre-wrapper behaviour).
#[derive(Debug, Deserialize)]
pub struct ExplainResponse {
    pub repo: String,
    /// Human-readable repo name for CLI display.
    /// Populated by daemon from registry alias or path basename.
    /// When present, prefer this over `repo` (which is internal UID).
    /// (Rendering deferred to CLI-OUT-3.)
    #[serde(default)]
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub snapshot: String,
    pub focus: ExplainFocus,
    pub confidence: String,
    /// Each signal is a LEAF `CoherenceEnvelope<ExplainSignal>` (EXPLAIN-LIVEGRAPH-IMPL / contract D7). The
    /// renderer reads each `.value` (the pristine `ExplainSignal`).
    #[serde(default)]
    pub signals: Vec<CoherenceEnvelope<ExplainSignal>>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Deserialize)]
pub struct ExplainFocus {
    #[serde(default)]
    pub input: Option<String>,
    pub resolved: bool,
    #[serde(default)]
    pub resolved_kind: Option<String>,
    #[serde(default)]
    pub resolved_path: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub candidates: Vec<FocusCandidate>,
}

#[derive(Debug, Deserialize)]
pub struct FocusCandidate {
    pub stable_key: String,
    #[serde(default)]
    pub file: Option<String>,
    pub kind: String,
}

#[derive(Debug, Deserialize)]
pub struct ExplainSignal {
    pub code: String,
    pub summary: String,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl ExplainResponse {
    /// Render the explain response as human-readable plain text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        out.push_str(&kv_line("Repo", &self.repo));
        out.push_str(&self.render_target());
        out.push_str(&kv_line("Confidence", &self.confidence));
        out.push('\n');

        // ── Handle ambiguous focus ───────��─────────────────────────
        if !self.focus.resolved
            && self.focus.reason.as_deref() == Some("ambiguous")
            && !self.focus.candidates.is_empty()
        {
            out.push_str(&self.render_candidates());
            return out.trim_end().to_string();
        }

        // ── Handle unresolved focus ──────────────��─────────────────
        if !self.focus.resolved {
            return out.trim_end().to_string();
        }

        // ── Render sections by signal type ─────────────────────────
        // Each signal is a LEAF `CoherenceEnvelope<ExplainSignal>`; read the pristine inner `.value` (the
        // section TEXT is byte-identical to the pre-wrapper output).
        for signal in &self.signals {
            if let Some(section) = self.render_signal_section(&signal.value) {
                out.push_str(&section);
                out.push('\n');
            }
        }

        // ── Truncation warning ──────────────────────���──────────────
        if self.truncated {
            out.push_str("\n[Output truncated. Use --json for full results.]\n");
        }

        out.trim_end().to_string()
    }

    fn render_target(&self) -> String {
        if !self.focus.resolved {
            let input = self.focus.input.as_deref().unwrap_or("(unknown)");
            let reason = self.focus.reason.as_deref().unwrap_or("no match");
            return kv_line("Target", &format!("{} (unresolved: {})", input, reason));
        }

        let kind = self.focus.resolved_kind.as_deref().unwrap_or("unknown");

        // Get name from identity signal if available
        let name = self.get_identity_name();

        match (&self.focus.resolved_path, name) {
            (Some(path), Some(name)) if kind == "symbol" => {
                // Symbol target: show name, kind, and file
                let mut result = kv_line("Target", &name);
                result.push_str(&format!("Kind: {}\n", kind));
                result.push_str(&format!("File: {}\n", path));
                result
            }
            (Some(path), _) => {
                // File or path target: show path with kind
                let mut result = kv_line("Target", &format!("{} ({})", path, kind));
                if let Some(info) = self.get_identity_info() {
                    result.push_str(&info);
                }
                result
            }
            (None, Some(name)) => kv_line("Target", &format!("{} ({})", name, kind)),
            (None, None) => kv_line("Target", &format!("({})", kind)),
        }
    }

    fn get_identity_name(&self) -> Option<String> {
        for signal in &self.signals {
            if signal.value.code == "EXPLAIN_IDENTITY" {
                if let Some(ref ev) = signal.value.evidence {
                    return ev
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
        }
        None
    }

    fn get_identity_info(&self) -> Option<String> {
        for signal in &self.signals {
            if signal.value.code == "EXPLAIN_IDENTITY" {
                if let Some(ref ev) = signal.value.evidence {
                    let mut info = String::new();

                    if let Some(lang) = ev.get("language").and_then(|v| v.as_str()) {
                        info.push_str(&format!("Language: {}\n", lang));
                    }
                    if let Some(symbol_count) = ev.get("symbol_count").and_then(|v| v.as_u64()) {
                        info.push_str(&format!("Symbols: {}\n", symbol_count));
                    }
                    if let Some(file_count) = ev.get("file_count").and_then(|v| v.as_u64()) {
                        info.push_str(&format!("Files: {}\n", file_count));
                    }

                    if !info.is_empty() {
                        return Some(info);
                    }
                }
            }
        }
        None
    }

    fn render_candidates(&self) -> String {
        let mut out = heading("Ambiguous target - multiple matches found");
        for c in &self.focus.candidates {
            let file_info = c
                .file
                .as_deref()
                .map(|f| format!(" in {}", f))
                .unwrap_or_default();
            out.push_str(&bullet(&format!(
                "{} ({}){}",
                c.stable_key, c.kind, file_info
            )));
        }
        out.push_str("\nSpecify a more precise target or use the stable_key directly.\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_response() -> ExplainResponse {
        ExplainResponse {
            repo: "test-repo".to_string(),
            display_name: None,
            snapshot: "snap-123".to_string(),
            focus: ExplainFocus {
                input: Some("src/core/auth.ts".to_string()),
                resolved: true,
                resolved_kind: Some("file".to_string()),
                resolved_path: Some("src/core/auth.ts".to_string()),
                reason: None,
                candidates: vec![],
            },
            confidence: "high".to_string(),
            signals: vec![],
            truncated: false,
        }
    }

    #[test]
    fn render_shows_repo_name() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Repo: test-repo"));
    }

    #[test]
    fn render_shows_file_target() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Target: src/core/auth.ts (file)"));
    }

    #[test]
    fn render_shows_unresolved_target() {
        let mut r = minimal_response();
        r.focus = ExplainFocus {
            input: Some("nonexistent".to_string()),
            resolved: false,
            resolved_kind: None,
            resolved_path: None,
            reason: Some("no_match".to_string()),
            candidates: vec![],
        };
        let out = r.render_human();
        assert!(out.contains("Target: nonexistent (unresolved: no_match)"));
    }

    #[test]
    fn render_shows_ambiguous_with_candidates() {
        let mut r = minimal_response();
        r.focus = ExplainFocus {
            input: Some("validate".to_string()),
            resolved: false,
            resolved_kind: None,
            resolved_path: None,
            reason: Some("ambiguous".to_string()),
            candidates: vec![
                FocusCandidate {
                    stable_key: "r1:src/auth:AuthService.validate:SYMBOL".to_string(),
                    file: Some("src/auth/service.ts".to_string()),
                    kind: "symbol".to_string(),
                },
                FocusCandidate {
                    stable_key: "r1:src/user:UserService.validate:SYMBOL".to_string(),
                    file: Some("src/user/service.ts".to_string()),
                    kind: "symbol".to_string(),
                },
            ],
        };
        let out = r.render_human();
        assert!(out.contains("Ambiguous target"));
        assert!(out.contains("AuthService.validate"));
        assert!(out.contains("UserService.validate"));
    }

    /// Wrap a bare `ExplainSignal` as a LEAF `CoherenceEnvelope<ExplainSignal>` (the post-wrapper signal
    /// shape) for the render tests — `sqlite_leaf` is the simplest shared constructor; the render path
    /// reads only the inner `.value`, so the leaf labels are immaterial to the section TEXT.
    fn leaf(signal: ExplainSignal) -> CoherenceEnvelope<ExplainSignal> {
        CoherenceEnvelope::sqlite_leaf(signal, false)
    }

    #[test]
    fn render_shows_callers() {
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "3 direct callers.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 3,
                "items": [
                    {"name": "handleLogin", "module": "src/controllers"},
                    {"name": "refresh", "module": "src/session"},
                    {"name": "impersonate", "module": "src/admin"}
                ]
            })),
        })];
        let out = r.render_human();
        assert!(out.contains("Callers (3)"));
        assert!(out.contains("handleLogin (src/controllers)"));
    }

    #[test]
    fn render_shows_trust() {
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_TRUST".to_string(),
            summary: "Trust info.".to_string(),
            evidence: Some(serde_json::json!({
                "call_resolution_rate": 0.95,
                "call_graph_reliability": "high",
                "enrichment_state": "ran"
            })),
        })];
        let out = r.render_human();
        assert!(out.contains("Trust"));
        assert!(out.contains("Call resolution: 95%"));
        assert!(out.contains("Call graph reliability: high"));
    }

    #[test]
    fn render_hides_internal_fields() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(!out.contains("snap-123"), "snapshot should be hidden");
    }

    #[test]
    fn deserialize_from_daemon_json() {
        let json = r#"{
            "schema": "rgr.agent.v1",
            "command": "explain",
            "repo": "my-app",
            "snapshot": "snap-abc",
            "focus": {
                "input": "src/core/auth.ts",
                "resolved": true,
                "resolved_kind": "file",
                "resolved_path": "src/core/auth.ts"
            },
            "confidence": "high",
            "signals": [],
            "limits": [],
            "next": [],
            "truncated": false
        }"#;

        let r: ExplainResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.repo, "my-app");
        assert!(r.focus.resolved);
        assert_eq!(r.focus.resolved_kind, Some("file".to_string()));
    }

    /// EXPLAIN-LIVEGRAPH-IMPL §5 W1/W4: the CLI parses the FULL `CoherenceEnvelope<ExplainResponse>` wrapper
    /// the daemon now emits — `signals` moved UNDER `value`, each a LEAF with its own `.value` carrying the
    /// `{code, summary, evidence}`. The human render reads `value.signals[*].value`, so the section TEXT is
    /// byte-identical to the pre-wrapper output. This pins the wire→render projection so it cannot silently
    /// drift from the daemon's wrapper (the explain analogue of check's `render_check_envelope` test).
    #[test]
    fn deserialize_and_render_wrapped_envelope() {
        let json = r#"{
            "value": {
                "schema": "rgr.agent.v1",
                "command": "explain",
                "repo": "my-app",
                "snapshot": "snap-abc",
                "focus": {
                    "input": "AuthService.validate",
                    "resolved": true,
                    "resolved_kind": "symbol",
                    "resolved_path": "src/auth.ts"
                },
                "confidence": "high",
                "signals": [
                    {
                        "value": {
                            "code": "EXPLAIN_IDENTITY",
                            "summary": "Identity: symbol target.",
                            "evidence": {"name": "validate"}
                        },
                        "provenance": {"source": ["sqlite"]},
                        "trust": {"class": "Exact", "completeness": "Complete"},
                        "freshness": "Fresh"
                    },
                    {
                        "value": {
                            "code": "EXPLAIN_CALLERS",
                            "summary": "1 direct caller.",
                            "evidence": {"count": 1, "items": [{"name": "handleLogin", "module": "src/ctl"}]}
                        },
                        "provenance": {"source": ["livegraph", "sqlite"]},
                        "trust": {"class": "Exact", "completeness": "Complete"},
                        "freshness": "Fresh"
                    }
                ],
                "limits": [],
                "next": [],
                "truncated": false
            },
            "provenance": {"source": ["livegraph", "sqlite"]},
            "trust": {"class": "Exact", "completeness": "Complete"},
            "freshness": "Fresh"
        }"#;

        let env: CoherenceEnvelope<ExplainResponse> = serde_json::from_str(json).unwrap();
        assert_eq!(env.value.repo, "my-app");
        let out = env.value.render_human();
        // Identity (read from the leaf's inner value) drives the symbol Target header.
        assert!(out.contains("Target: validate"));
        assert!(out.contains("Kind: symbol"));
        assert!(out.contains("File: src/auth.ts"));
        // The caller section is rendered from the leaf's inner value.
        assert!(out.contains("Callers (1)"));
        assert!(out.contains("handleLogin (src/ctl)"));
        // The internal snapshot uid stays hidden.
        assert!(!out.contains("snap-abc"));
    }
}
