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
    ///
    /// `full` is the `--full` flag (TRUNCATION-AUDIT-1): when set, the per-section display cap in
    /// [`render_signal_section`](Self::render_signal_section) is lifted so every item renders (the human
    /// analogue of the daemon's `Budget::Full`, for `rmap explain <target> --full | grep <x>`). The default
    /// (`full = false`) path is byte-identical to the pre-flag output. See `explain_sections.rs` for the
    /// two-cap model.
    pub fn render_human(&self, full: bool) -> String {
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
            if let Some(section) = self.render_signal_section(&signal.value, full) {
                out.push_str(&section);
                out.push('\n');
            }
        }

        // ── Truncation warning ──────────────────────���──────────────
        // Only the default (capped) path can truncate; `--full` (full == true) uncaps the daemon AND these
        // renderers, so this notice never fires under `--full`. Guarding on `!full` keeps the message honest
        // (no "truncated" claim when nothing was cut) even if a stale `truncated` flag slipped through.
        if self.truncated && !full {
            // TRUNCATION-AUDIT-1 review-2 #1: plain `--json` does NOT uncap — it is a transport
            // format that preserves the selected budget (the daemon still emits `items_truncated:
            // true`). Only `--full` (⇒ `Budget::Full`) lifts every cap. So the notice points to
            // `--full` for complete output and `--full --json` for complete JSON, never bare `--json`.
            out.push_str(
                "\n[Output truncated. Use --full for complete output, or --full --json for complete JSON.]\n",
            );
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
        let out = r.render_human(false);
        assert!(out.contains("Repo: test-repo"));
    }

    #[test]
    fn render_shows_file_target() {
        let r = minimal_response();
        let out = r.render_human(false);
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
        let out = r.render_human(false);
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
        let out = r.render_human(false);
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
        let out = r.render_human(false);
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
        let out = r.render_human(false);
        assert!(out.contains("Trust"));
        assert!(out.contains("Call resolution: 95%"));
        assert!(out.contains("Call graph reliability: high"));
    }

    #[test]
    fn render_hides_internal_fields() {
        let r = minimal_response();
        let out = r.render_human(false);
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
        let out = env.value.render_human(false);
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

    // ── TRUNCATION-AUDIT-1 review-1 #1/#2: the SECOND (presentation) display cap ──
    //
    // The agent already emits EVERY item under `Budget::Full` (proven at the daemon boundary by
    // `explain_full_budget_uncaps_over_cap_file_listing`). But these renderers cap the HUMAN output AGAIN
    // at a fixed per-section N (EXPLAIN_SYMBOLS: 15). Before this fix `--full` uncapped the JSON yet the
    // human render still truncated, so `rmap explain <target> --full | grep <x>` missed items past the
    // display cap. These two tests pin both paths at the presentation seam (no daemon required).

    /// Build a resolved file-target response carrying an over-cap EXPLAIN_SYMBOLS section of `n` symbols
    /// named `sym00..sym{n-1}`, so a cap of 15 bites at n=20 (sym15..sym19 are the dropped tail).
    fn symbols_response(n: usize) -> ExplainResponse {
        let items: Vec<serde_json::Value> = (0..n)
            .map(|i| serde_json::json!({ "name": format!("sym{:02}", i), "subtype": "function" }))
            .collect();
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_SYMBOLS".to_string(),
            summary: format!("{} symbols.", n),
            evidence: Some(serde_json::json!({ "count": n, "items": items })),
        })];
        r
    }

    #[test]
    fn render_full_uncaps_symbols_past_presentation_cap() {
        // 20 > the EXPLAIN_SYMBOLS presentation cap (15). Under `--full` EVERY symbol must render and the
        // "... (N more)" note must be ABSENT. This FAILS against the pre-fix `.take(15)` renderer, which
        // dropped sym15..sym19 and printed "... (5 more)".
        let out = symbols_response(20).render_human(true);
        assert!(out.contains("sym00"), "first symbol present:\n{out}");
        assert!(
            out.contains("sym19"),
            "--full must render the 20th symbol (past the cap of 15):\n{out}"
        );
        assert!(
            !out.contains("more)"),
            "--full must NOT emit a '... (N more)' truncation note:\n{out}"
        );
    }

    #[test]
    fn render_default_caps_symbols_at_presentation_limit() {
        // The default (full == false) path is unchanged: cap 15, sym15..sym19 dropped, overflow note shown.
        let out = symbols_response(20).render_human(false);
        assert!(out.contains("sym00"));
        assert!(
            out.contains("sym14"),
            "the 15th symbol is the last kept under the default cap:\n{out}"
        );
        assert!(
            !out.contains("sym19"),
            "default cap (15) must drop the 20th symbol:\n{out}"
        );
        assert!(
            out.contains("... (5 more)"),
            "default path reports the cut honestly:\n{out}"
        );
    }

    #[test]
    fn render_truncation_notice_points_to_full_not_bare_json() {
        // TRUNCATION-AUDIT-1 review-2 #1: the `self.truncated` notice (distinct from the per-section
        // "(N more)" overflow note) previously read "Use --json for full results." — false, because
        // plain `--json` preserves the budget (the daemon still reports `items_truncated: true`); only
        // `--full` ⇒ `Budget::Full` uncaps. The corrected notice points to `--full` / `--full --json`.
        // The first two asserts FAIL against the old string; the third pins the old wording gone.
        let mut r = minimal_response();
        r.truncated = true;
        let out = r.render_human(false);
        assert!(
            out.contains("--full for complete output"),
            "notice must point to --full for complete output:\n{out}"
        );
        assert!(
            out.contains("--full --json for complete JSON"),
            "notice must show --full --json (not bare --json) for complete JSON:\n{out}"
        );
        assert!(
            !out.contains("Use --json for full results"),
            "the misleading bare-`--json` wording must be gone:\n{out}"
        );
    }
}
