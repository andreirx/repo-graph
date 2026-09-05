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
//!   - your code's calls 95% resolved (HIGH)
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
//!   - your code's calls 95% resolved (HIGH)
//! ```

use repo_graph_agent::dto::IndexDrift;
use repo_graph_coherence::CoherenceEnvelope;
use serde::Deserialize;

use crate::presentation::{anchor, bullet, heading, kv_line};

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
    /// EMBED-SEED-IMPL-1 (spec §8.2/§8.3): the `Limit` list the daemon attaches — the
    /// semantic fallback's honesty header / degraded line rides here (same `{code,
    /// summary}` shape orient uses). Absent on the wire from an older daemon.
    #[serde(default)]
    pub limits: Vec<crate::presentation::orient::Limit>,
    /// RECON-M-R4 (§5.5): the additive Layer-2 attribution block the daemon attaches on SYMBOL
    /// focus (`layer2_resolution` — likely resolutions + contested signals). `None` on zero-SCIP /
    /// non-symbol / no-hint answers (absent on the wire); rendered by the shared witness projection.
    #[serde(default)]
    pub layer2_resolution: Option<serde_json::Value>,
    /// INDEX-BASIS-1: the query-time working-tree drift the daemon attached onto
    /// `value` (git basis + how far the tree has moved). Rendered as explain's
    /// "index basis / drift" footer line. Absent on the wire only from an older
    /// daemon; then no drift line is shown.
    #[serde(default)]
    pub index_drift: Option<IndexDrift>,
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
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the candidate's start line for the `path:line`
    /// anchor. Absent (older daemon, semantic candidate, or LiveGraph-served candidate
    /// with no same-source line) → the bare path renders, never a fabricated line.
    #[serde(default)]
    pub line: Option<u64>,
    pub kind: String,
    // EMBED-SEED-IMPL-1 (spec §8.2 Group A): additive, semantic-only fields — present
    // ONLY on a semantic fallback candidate (a deterministic *ambiguous* candidate
    // omits them, so its render is byte-identical to before). `module`/`next` stay raw
    // `Value` so the shared honesty-preserving formatters render them.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub module: Option<serde_json::Value>,
    #[serde(default)]
    pub next: Option<serde_json::Value>,
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
            // EMBED-SEED-IMPL-1 (spec §8.2 Group A): explain shares orient's no-match
            // contract — render the labeled semantic candidates (or the honest degraded
            // line) here, so `explain "<concept>"` is not a dead human surface.
            let semantic = self.render_semantic_fallback();
            if !semantic.is_empty() {
                out.push_str(&semantic);
            }
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

        // ── RECON-M-R4 (§5.5): the Layer-2 landing for this focus symbol ──
        // "This call likely resolves to X" hints + contested resolutions. Empty (nothing
        // appended) on zero-SCIP / non-symbol / no-hint answers — byte-identical there.
        let layer2 = crate::presentation::witnesses::render_layer2_resolution_section(
            self.layer2_resolution.as_ref(),
        );
        if !layer2.is_empty() {
            out.push_str(&layer2);
            out.push('\n');
        }

        // ── INDEX-BASIS-1: index basis / working-tree drift footer ──
        // The load-bearing "which commit do these facts describe, and how far has
        // the tree moved" line — the same wording orient/check render (one home,
        // `IndexDrift::describe`). Absent only from an older daemon.
        if let Some(drift) = &self.index_drift {
            out.push_str(&drift.describe());
            out.push('\n');
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
                // ANCHORS-EVERYWHERE-1 (Tier 0): anchor the file at the symbol's line
                // (`path:line`) when the identity evidence carries one — absence renders
                // the bare path (never a fabricated 0/1).
                result.push_str(&format!(
                    "File: {}\n",
                    anchor(path, self.get_identity_line())
                ));
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

    /// ANCHORS-EVERYWHERE-1 (Tier 0): the target symbol's start line from the identity
    /// evidence (`line_start`, already on the wire). `None` when absent — the header then
    /// renders the bare path (STANDING HONESTY RULE: absent line → no line, never 0/1).
    fn get_identity_line(&self) -> Option<u64> {
        for signal in &self.signals {
            if signal.value.code == "EXPLAIN_IDENTITY" {
                if let Some(ref ev) = signal.value.evidence {
                    return ev.get("line_start").and_then(|v| v.as_u64());
                }
            }
        }
        None
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

    /// EMBED-SEED-IMPL-1 (spec §8.2 Group A): render the semantic fallback tier for
    /// explain HUMAN mode — the labeled Layer-3 candidates (or the honest degraded/
    /// known-zero line) a `no_match` explain now carries. Mirrors orient's
    /// `render_semantic_fallback`; reuses the shared honesty-preserving module/next
    /// formatters. Empty when the focus is not a no_match, or when a no-match carries
    /// no seed candidate AND no seed limit (old daemon / seeding not consulted).
    fn render_semantic_fallback(&self) -> String {
        if self.focus.resolved || self.focus.reason.as_deref() != Some("no_match") {
            return String::new();
        }
        let embedding: Vec<&FocusCandidate> = self
            .focus
            .candidates
            .iter()
            .filter(|c| c.source.as_deref() == Some("embedding"))
            .collect();
        let semantic_limit = self
            .limits
            .iter()
            .find(|l| l.code.starts_with("SEMANTIC_FALLBACK"));

        if embedding.is_empty() {
            // Degraded / known-zero: the honest line WITH the specific cause from
            // `reasons` (review-9 #2), not just the generic summary.
            return match semantic_limit {
                Some(limit) => crate::presentation::seed::render_semantic_header(
                    &limit.summary,
                    &limit.reasons,
                ),
                None => String::new(),
            };
        }

        // Fired: honesty header + per-cause reasons (model id + stale-subset detail,
        // review-9 #2), then the labeled candidate list.
        let mut out = match semantic_limit {
            Some(limit) => {
                crate::presentation::seed::render_semantic_header(&limit.summary, &limit.reasons)
            }
            None => "Semantic hints: No exact match — the candidates below are Layer-3 embedding hints, not resolved facts.\n".to_string(),
        };
        for (i, c) in embedding.iter().enumerate() {
            // Every identity field is REQUIRED on a semantic candidate; a genuinely
            // absent one is a malformed response (old daemon / bug), surfaced as such,
            // NEVER fabricated (STANDING HONESTY RULE).
            let (Some(file), Some(score), Some(model)) =
                (c.file.as_deref(), c.score, c.model_id.as_deref())
            else {
                out.push_str(&format!(
                    "  {}. (malformed candidate: missing file/score/model_id)\n",
                    i + 1
                ));
                continue;
            };
            let module = crate::presentation::seed::render_module_hint(c.module.as_ref());
            out.push_str(&format!(
                "  {}. {file}  (score {score:.2}, embedding, model {model}{module})\n",
                i + 1
            ));
            out.push_str(&crate::presentation::seed::render_next(
                c.next.as_ref(),
                &c.stable_key,
            ));
        }
        out
    }

    fn render_candidates(&self) -> String {
        let mut out = heading("Ambiguous target - multiple matches found");
        for c in &self.focus.candidates {
            // ANCHORS-EVERYWHERE-1 (Tier 1): anchor the candidate at `path:line` when a
            // (single-source) line is present; bare path otherwise.
            let file_info = c
                .file
                .as_deref()
                .map(|f| format!(" in {}", anchor(f, c.line)))
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
            limits: vec![],
            layer2_resolution: None,
            index_drift: None,
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
                    line: None,
                    kind: "symbol".to_string(),
                    source: None,
                    model_id: None,
                    score: None,
                    module: None,
                    next: None,
                },
                FocusCandidate {
                    stable_key: "r1:src/user:UserService.validate:SYMBOL".to_string(),
                    file: Some("src/user/service.ts".to_string()),
                    line: None,
                    kind: "symbol".to_string(),
                    source: None,
                    model_id: None,
                    score: None,
                    module: None,
                    next: None,
                },
            ],
        };
        let out = r.render_human(false);
        assert!(out.contains("Ambiguous target"));
        assert!(out.contains("AuthService.validate"));
        assert!(out.contains("UserService.validate"));
    }

    #[test]
    fn semantic_no_match_renders_labeled_candidates_in_human_mode() {
        // EMBED-SEED-IMPL-1 (operator finding 2026-08-25): `explain "<concept>"` in
        // HUMAN mode must render labeled Layer-3 candidates, not a bare no_match line.
        let mut r = minimal_response();
        r.focus = ExplainFocus {
            input: Some("where discounts are applied".to_string()),
            resolved: false,
            resolved_kind: None,
            resolved_path: None,
            reason: Some("no_match".to_string()),
            candidates: vec![FocusCandidate {
                stable_key: "glamCRM:src/price.ts:FILE".to_string(),
                file: Some("src/price.ts".to_string()),
                line: None,
                kind: "file".to_string(),
                source: Some("embedding".to_string()),
                model_id: Some("text-embedding-nomic-embed-text-v1.5".to_string()),
                score: Some(0.71),
                module: Some(serde_json::json!({ "owning": "backend/pricing" })),
                next: Some(serde_json::json!({
                    "cmd": "explain", "args": ["glamCRM:src/price.ts:FILE"], "cwd": "/repo"
                })),
            }],
        };
        r.limits = vec![crate::presentation::orient::Limit {
            code: "SEMANTIC_FALLBACK".to_string(),
            summary: "No exact match. The candidates below are Layer-3 embedding hints, not resolved facts; open one and re-run.".to_string(),
            reasons: Vec::new(),
        }];
        let out = r.render_human(false);
        assert!(out.contains("Semantic hints:"), "honesty header: {out}");
        assert!(out.contains("src/price.ts"), "candidate path: {out}");
        assert!(out.contains("embedding"), "labeled embedding: {out}");
        assert!(
            out.contains("module backend/pricing"),
            "owning module: {out}"
        );
        assert!(
            out.contains("rmap explain glamCRM:src/price.ts:FILE"),
            "next follow-up: {out}"
        );
    }

    /// review-9 #2 + #5: `explain "<concept>"` degraded (model down) must render the
    /// SPECIFIC cause in HUMAN mode, deserialized from the daemon JSON (proving the
    /// rgr `Limit` retains `reasons`).
    #[test]
    fn explain_degraded_renders_specific_cause_from_json() {
        let limit: crate::presentation::orient::Limit = serde_json::from_str(
            r#"{"code":"SEMANTIC_FALLBACK_UNAVAILABLE","summary":"No exact match, and semantic hints are unavailable; deterministic resolution is unaffected.","reasons":["no local embedding model reachable; seeding is optional, resolution is unaffected"]}"#,
        )
        .expect("Limit deserializes with reasons");
        let mut r = minimal_response();
        r.focus = ExplainFocus {
            input: Some("where discounts are applied".to_string()),
            resolved: false,
            resolved_kind: None,
            resolved_path: None,
            reason: Some("no_match".to_string()),
            candidates: Vec::new(),
        };
        r.limits = vec![limit];
        let out = r.render_human(false);
        assert!(out.contains("Semantic hints:"), "honesty header: {out}");
        assert!(
            out.contains("no local embedding model reachable"),
            "the specific cause is rendered, not just the generic summary: {out}"
        );
    }

    #[test]
    fn no_match_without_seed_candidate_is_todays_bare_line() {
        // Byte-parity: a no_match with no seed candidate + no seed limit (old daemon /
        // seeding not consulted) renders exactly today's bare target line.
        let mut r = minimal_response();
        r.focus = ExplainFocus {
            input: Some("nope".to_string()),
            resolved: false,
            resolved_kind: None,
            resolved_path: None,
            reason: Some("no_match".to_string()),
            candidates: vec![],
        };
        let out = r.render_human(false);
        assert!(
            !out.contains("Semantic hints:"),
            "no semantic section: {out}"
        );
        assert!(out.contains("Target: nope (unresolved: no_match)"));
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

    // ── ANCHORS-EVERYWHERE-1 (§4 unit-per-surface: present line renders `path:line`; absent renders nothing) ──

    /// Build a resolved SYMBOL-target response whose EXPLAIN_IDENTITY evidence optionally carries
    /// `line_start`, for the Tier-0 header anchor tests.
    fn symbol_target(line_start: Option<u64>) -> ExplainResponse {
        let mut r = minimal_response();
        r.focus = ExplainFocus {
            input: Some("AuthService.validate".to_string()),
            resolved: true,
            resolved_kind: Some("symbol".to_string()),
            resolved_path: Some("src/core/auth/service.ts".to_string()),
            reason: None,
            candidates: vec![],
        };
        let mut ev = serde_json::json!({ "name": "validate" });
        if let Some(l) = line_start {
            ev["line_start"] = serde_json::json!(l);
        }
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_IDENTITY".to_string(),
            summary: "Identity: symbol target.".to_string(),
            evidence: Some(ev),
        })];
        r
    }

    #[test]
    fn tier0_header_anchors_symbol_file_at_identity_line() {
        // Present line → `File: path:line`.
        let out = symbol_target(Some(42)).render_human(false);
        assert!(
            out.contains("File: src/core/auth/service.ts:42"),
            "header anchors at the symbol's line:\n{out}"
        );
    }

    #[test]
    fn tier0_header_absent_line_renders_bare_path_no_fabrication() {
        // Absent line → bare path, never `:0`/`:1`.
        let out = symbol_target(None).render_human(false);
        assert!(
            out.contains("File: src/core/auth/service.ts\n"),
            "absent line → bare path:\n{out}"
        );
        assert!(
            !out.contains("service.ts:"),
            "no fabricated line suffix when absent:\n{out}"
        );
    }

    #[test]
    fn tier0_symbols_section_anchors_present_line_and_omits_absent() {
        // A file target with a Symbols section: one item has a line (anchored), one does not
        // (rendered bare — byte-identical to the pre-anchor row).
        let mut r = minimal_response();
        r.focus.resolved_path = Some("src/many.ts".to_string());
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_SYMBOLS".to_string(),
            summary: "2 symbols.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 2,
                "items": [
                    {"name": "withLine", "subtype": "function", "line_start": 7},
                    {"name": "noLine", "subtype": "function"}
                ]
            })),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains("withLine (function)  src/many.ts:7"),
            "symbol with a line anchors `path:line`:\n{out}"
        );
        assert!(
            out.contains("noLine (function)") && !out.contains("noLine (function)  src/many.ts"),
            "symbol without a line renders no anchor (bare row):\n{out}"
        );
    }

    /// STANDING HONESTY RULE regression (review-0 blocking defect): a symbol whose stored
    /// `line_start` is the `0` "no span" sentinel must render a BARE row — never `src/x.ts:0`.
    /// Proves the explain rendering path routes through the fixed `anchor()` chokepoint.
    #[test]
    fn tier0_symbols_section_never_emits_zero_line() {
        let mut r = minimal_response();
        r.focus.resolved_path = Some("src/x.ts".to_string());
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_SYMBOLS".to_string(),
            summary: "1 symbol.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 1,
                "items": [
                    {"name": "zeroLine", "subtype": "function", "line_start": 0}
                ]
            })),
        })];
        let out = r.render_human(false);
        assert!(
            !out.contains("src/x.ts:0"),
            "a 0 line_start must NEVER render as `:0`:\n{out}"
        );
        assert!(
            out.contains("zeroLine (function)"),
            "the symbol row still renders (bare, no anchor):\n{out}"
        );
    }

    #[test]
    fn tier1_callers_anchor_file_line_when_both_present() {
        // Caller carrying file+line → `name (module)  file:line`; a caller lacking them stays bare.
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "2 direct callers.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 2,
                "items": [
                    {"name": "handleLogin", "module": "src/ctl", "file": "src/ctl/login.ts", "line": 11},
                    {"name": "legacy", "module": "src/old"}
                ]
            })),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains("handleLogin (src/ctl)  src/ctl/login.ts:11"),
            "caller anchors its own file:line:\n{out}"
        );
        assert!(
            out.contains("legacy (src/old)") && !out.contains("legacy (src/old)  "),
            "caller without file+line renders no anchor:\n{out}"
        );
    }

    #[test]
    fn tier1_candidate_anchor_file_line_in_ambiguous() {
        // An ambiguous candidate carrying a line anchors `in path:line`.
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
                    line: Some(15),
                    kind: "symbol".to_string(),
                    source: None,
                    model_id: None,
                    score: None,
                    module: None,
                    next: None,
                },
                FocusCandidate {
                    stable_key: "r1:src/user:UserService.validate:SYMBOL".to_string(),
                    file: Some("src/user/service.ts".to_string()),
                    line: None,
                    kind: "symbol".to_string(),
                    source: None,
                    model_id: None,
                    score: None,
                    module: None,
                    next: None,
                },
            ],
        };
        let out = r.render_human(false);
        assert!(
            out.contains("in src/auth/service.ts:15"),
            "candidate with a line anchors `in path:line`:\n{out}"
        );
        assert!(
            out.contains("in src/user/service.ts\n") || out.contains("in src/user/service.ts)"),
            "candidate without a line renders the bare path:\n{out}"
        );
    }

    /// Review-1 item 3: the NONZERO g2u-b union-degree second figure through final human
    /// rendering — the daemon-attached `union` object renders as the labeled heading suffix
    /// on BOTH callers and callees, beside the untouched pipeline count.
    #[test]
    fn render_shows_union_degree_suffix_where_it_differs() {
        let mut r = minimal_response();
        r.signals = vec![
            leaf(ExplainSignal {
                code: "EXPLAIN_CALLERS".to_string(),
                summary: "1 direct caller.".to_string(),
                evidence: Some(serde_json::json!({
                    "count": 1,
                    "items": [{"name": "handleLogin", "module": "src/controllers"}],
                    "union": {
                        "count": 2, "pipeline_count": 1, "accounting": "union",
                        "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
                    },
                })),
            }),
            leaf(ExplainSignal {
                code: "EXPLAIN_CALLEES".to_string(),
                summary: "2 callees.".to_string(),
                evidence: Some(serde_json::json!({
                    "count": 2,
                    "items": [{"name": "audit", "module": "src/log"}],
                    "union": {
                        "count": 3, "pipeline_count": 2, "accounting": "union",
                        "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
                    },
                })),
            }),
        ];
        let out = r.render_human(false);
        // Review-2 item 1: the coverage basis renders BESIDE the reconciled value — the
        // §5.3.0 human frame, not a bare labeled count.
        assert!(
            out.contains(
                "Callers (1 · reconciled 2 — combined analyses (coverage: TypeScript (1 partition)))"
            ),
            "{out}"
        );
        assert!(
            out.contains(
                "Callees (2 · reconciled 3 — combined analyses (coverage: TypeScript (1 partition)))"
            ),
            "{out}"
        );
        // No union object (degrees agree / no ledger) → today's exact heading.
        let mut plain = minimal_response();
        plain.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "1 direct caller.".to_string(),
            evidence: Some(serde_json::json!({"count": 1, "items": []})),
        })];
        let plain_out = plain.render_human(false);
        assert!(plain_out.contains("Callers (1)"), "{plain_out}");
        assert!(!plain_out.contains("reconciled"), "{plain_out}");
    }

    /// Review-2 item 1 (negative): a `union` object that fails the §5.3.0 labeling gate —
    /// missing `accounting: "union"`, or missing/malformed coverage — SUPPRESSES the union
    /// degree entirely: the heading is exactly the pipeline heading, never an unlabeled
    /// reconciled figure.
    #[test]
    fn union_degree_without_accounting_or_coverage_never_renders() {
        // Case 1: coverage well-formed, accounting marker ABSENT.
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "1 direct caller.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 1,
                "items": [],
                "union": {
                    "count": 2, "pipeline_count": 1,
                    "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
                },
            })),
        })];
        let out = r.render_human(false);
        assert!(out.contains("Callers (1)"), "{out}");
        assert!(!out.contains("Callers (1 ·"), "{out}");
        assert!(!out.contains("reconciled"), "{out}");

        // Case 2: accounting present, coverage MALFORMED (empty languages).
        let mut r2 = minimal_response();
        r2.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CALLEES".to_string(),
            summary: "2 callees.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 2,
                "items": [],
                "union": {
                    "count": 3, "pipeline_count": 2, "accounting": "union",
                    "coverage": {"languages": [], "partitions": ["p"], "fingerprint": "fp"},
                },
            })),
        })];
        let out2 = r2.render_human(false);
        assert!(out2.contains("Callees (2)"), "{out2}");
        assert!(!out2.contains("Callees (2 ·"), "{out2}");
        assert!(!out2.contains("reconciled"), "{out2}");
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
        // RELIABILITY-REFRAME-1: reader frame from the ONE shared wording, no pipeline grade.
        assert!(out.contains("your code's calls 95% resolved (HIGH)"));
        assert!(!out.contains("Call graph reliability"));
        assert!(!out.contains("Call resolution:"));
    }

    #[test]
    fn render_trust_zero_in_scope_is_unknown_not_fabricated_100() {
        // RELIABILITY-REFRAME-1 (review-1 §1): a 0-of-0 repo must render "no in-scope calls
        // measured", NOT the `call_resolution_rate` 1.0 sentinel's fabricated 100%. The additive
        // in-scope COUNTS on the evidence let the render tell unknown from a genuine 100%.
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_TRUST".to_string(),
            summary: "Trust info.".to_string(),
            evidence: Some(serde_json::json!({
                "call_resolution_rate": 1.0,        // the 0-of-0 rate sentinel
                "call_graph_reliability": "high",   // vacuous band
                "enrichment_state": "ran",
                "resolved_in_scope": 0,
                "in_scope_or_unclassified_total": 0
            })),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains("no in-scope calls measured"),
            "zero in-scope calls is unknown, not 100%:\n{out}"
        );
        assert!(
            !out.contains("100% resolved") && !out.contains("(HIGH)"),
            "no fabricated 100% / vacuous band for an empty call graph:\n{out}"
        );
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
