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

use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line};

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized explain response from daemon.
#[derive(Debug, Deserialize)]
pub struct ExplainResponse {
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    pub focus: ExplainFocus,
    pub confidence: String,
    #[serde(default)]
    pub signals: Vec<ExplainSignal>,
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
        for signal in &self.signals {
            if let Some(section) = self.render_signal_section(signal) {
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
            if signal.code == "EXPLAIN_IDENTITY" {
                if let Some(ref ev) = signal.evidence {
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
            if signal.code == "EXPLAIN_IDENTITY" {
                if let Some(ref ev) = signal.evidence {
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

    fn render_signal_section(&self, signal: &ExplainSignal) -> Option<String> {
        let evidence = signal.evidence.as_ref()?;

        match signal.code.as_str() {
            "EXPLAIN_CALLERS" => Some(self.render_callers(evidence)),
            "EXPLAIN_CALLEES" => Some(self.render_callees(evidence)),
            "EXPLAIN_IMPORTS" => Some(self.render_imports(evidence)),
            "EXPLAIN_SYMBOLS" => Some(self.render_symbols(evidence)),
            "EXPLAIN_FILES" => Some(self.render_files(evidence)),
            "EXPLAIN_CYCLES" => self.render_cycles(evidence),
            "EXPLAIN_BOUNDARY" => self.render_boundary(evidence),
            "EXPLAIN_GATE" => self.render_gate(evidence),
            "EXPLAIN_TRUST" => Some(self.render_trust(evidence)),
            "EXPLAIN_IDENTITY" => None, // Handled in header
            "EXPLAIN_MEASUREMENTS" => self.render_measurements(evidence),
            _ => None,
        }
    }

    fn render_callers(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Callers ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(10) {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let module = item.get("module").and_then(|v| v.as_str());
                if let Some(m) = module {
                    out.push_str(&bullet(&format!("{} ({})", name, m)));
                } else {
                    out.push_str(&bullet(name));
                }
            }
            if items.len() > 10 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 10));
            }
        }

        out
    }

    fn render_callees(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Callees ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(10) {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let module = item.get("module").and_then(|v| v.as_str());
                if let Some(m) = module {
                    out.push_str(&bullet(&format!("{} ({})", name, m)));
                } else {
                    out.push_str(&bullet(name));
                }
            }
            if items.len() > 10 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 10));
            }
        }

        out
    }

    fn render_imports(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Imports ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(15) {
                let target = item
                    .get("target_file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                out.push_str(&bullet(target));
            }
            if items.len() > 15 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 15));
            }
        }

        out
    }

    fn render_symbols(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Symbols ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(15) {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let subtype = item.get("subtype").and_then(|v| v.as_str());
                if let Some(st) = subtype {
                    out.push_str(&bullet(&format!("{} ({})", name, st)));
                } else {
                    out.push_str(&bullet(name));
                }
            }
            if items.len() > 15 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 15));
            }
        }

        out
    }

    fn render_files(&self, evidence: &serde_json::Value) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Files ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(15) {
                let path = item
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let symbol_count = item
                    .get("symbol_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                out.push_str(&bullet(&format!("{} ({} symbols)", path, symbol_count)));
            }
            if items.len() > 15 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 15));
            }
        }

        out
    }

    fn render_cycles(&self, evidence: &serde_json::Value) -> Option<String> {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        if count == 0 {
            return None;
        }

        let mut out = heading(&format!("Import cycles ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for (i, item) in items.iter().take(5).enumerate() {
                if let Some(modules) = item.get("modules").and_then(|v| v.as_array()) {
                    let cycle_str: Vec<&str> = modules.iter().filter_map(|m| m.as_str()).collect();
                    out.push_str(&bullet(&format!(
                        "Cycle {}: {}",
                        i + 1,
                        cycle_str.join(" -> ")
                    )));
                }
            }
            if items.len() > 5 {
                out.push_str(&format!("  ... ({} more)\n", items.len() - 5));
            }
        }

        Some(out)
    }

    fn render_boundary(&self, evidence: &serde_json::Value) -> Option<String> {
        let count = evidence
            .get("violation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if count == 0 {
            return None;
        }

        let mut out = heading(&format!("Boundary violations ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(10) {
                let source = item
                    .get("source_module")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let target = item
                    .get("target_module")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let edges = item.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
                out.push_str(&bullet(&format!(
                    "{} -> {} ({} edges)",
                    source, target, edges
                )));
            }
        }

        Some(out)
    }

    fn render_gate(&self, evidence: &serde_json::Value) -> Option<String> {
        let outcome = evidence
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let count = evidence
            .get("obligation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut out = heading(&format!(
            "Gate ({}: {} obligations)",
            outcome.to_uppercase(),
            count
        ));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            for item in items.iter().take(10) {
                let req_id = item.get("req_id").and_then(|v| v.as_str()).unwrap_or("?");
                let method = item.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                let verdict = item
                    .get("effective_verdict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                out.push_str(&bullet(&format!("{}: {} ({})", req_id, method, verdict)));
            }
        }

        Some(out)
    }

    fn render_trust(&self, evidence: &serde_json::Value) -> String {
        let mut out = heading("Trust");

        if let Some(rate) = evidence
            .get("call_resolution_rate")
            .and_then(|v| v.as_f64())
        {
            out.push_str(&bullet(&format!("Call resolution: {:.0}%", rate * 100.0)));
        }
        if let Some(reliability) = evidence
            .get("call_graph_reliability")
            .and_then(|v| v.as_str())
        {
            out.push_str(&bullet(&format!("Call graph reliability: {}", reliability)));
        }
        if let Some(enrichment) = evidence.get("enrichment_state").and_then(|v| v.as_str()) {
            out.push_str(&bullet(&format!("Enrichment: {}", enrichment)));
        }

        out
    }

    fn render_measurements(&self, evidence: &serde_json::Value) -> Option<String> {
        let items = evidence.get("items").and_then(|v| v.as_array())?;
        if items.is_empty() {
            return None;
        }

        let mut out = heading("Measurements");

        for item in items.iter().take(10) {
            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let value = item.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let aggregation = item
                .get("aggregation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            out.push_str(&bullet(&format!(
                "{} ({}): {:.2}",
                kind, aggregation, value
            )));
        }

        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_response() -> ExplainResponse {
        ExplainResponse {
            repo: "test-repo".to_string(),
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

    #[test]
    fn render_shows_callers() {
        let mut r = minimal_response();
        r.signals = vec![ExplainSignal {
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
        }];
        let out = r.render_human();
        assert!(out.contains("Callers (3)"));
        assert!(out.contains("handleLogin (src/controllers)"));
    }

    #[test]
    fn render_shows_trust() {
        let mut r = minimal_response();
        r.signals = vec![ExplainSignal {
            code: "EXPLAIN_TRUST".to_string(),
            summary: "Trust info.".to_string(),
            evidence: Some(serde_json::json!({
                "call_resolution_rate": 0.95,
                "call_graph_reliability": "high",
                "enrichment_state": "ran"
            })),
        }];
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
}
