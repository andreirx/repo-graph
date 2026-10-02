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
    /// CPP-DECLARATORS-1 §2.3 (2026-09-07): follow-up cursors the daemon attached when a bare
    /// name resolved to its TYPE over its constructor(s) — the constructor's own `explain`
    /// cursor, so a constructor query is NEVER hidden (operator ruling
    /// `cpp_decl_explain_constructor_collision`). Explain populates `next` ONLY in that case;
    /// every other explain answer sends an empty list ⇒ no section, byte-identical output.
    /// `#[serde(default)]`: an older daemon omits the field ⇒ empty ⇒ nothing rendered.
    #[serde(default)]
    pub next: Vec<crate::presentation::orient_types::NextAction>,
    /// CPP-DECLARATORS-1 §2.3 (review-3 #3): constructor cursors the daemon dropped from `next`
    /// because the budget cap was hit. The counted header ("N constructor(s) also match") must
    /// report the TRUE total, so it adds this to `next.len()`. Absent/`None` ⇒ nothing omitted.
    #[serde(default)]
    pub next_omitted_count: Option<usize>,
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

        // ── CPP-DECLARATORS-1 §2.3: constructor(s) that also match the bare name ──
        // When a bare name resolved to its TYPE over its constructor(s), surface the
        // constructor's own `explain` cursor so a constructor query is never hidden. Empty on
        // every other explain answer ⇒ nothing rendered (byte-identical).
        let related = self.render_related_cursors();
        if !related.is_empty() {
            out.push_str(&related);
            out.push('\n');
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

    /// CPP-DECLARATORS-1 §2.3 (review-4 #4): render the follow-up constructor cursor(s) the daemon
    /// attached when a bare name resolved to its TYPE over its constructor(s). The RATIFIED form
    /// (spec §2.3) is a SINGLE counted line — `N constructor(s) also match: explain '<cursor>'` —
    /// grammatically singular/plural, reporting the TRUE total (`next.len()` plus any the budget
    /// omitted), with the runnable `explain '<cursor>'` invocation(s) ON THE SAME LINE (comma-joined
    /// when more than one), so a constructor query is never hidden. Each cursor is single-quoted
    /// (a C++ qualified name / stable key carries no quote) and the verb comes from the action's
    /// own `kind`. Empty `next` ⇒ empty string ⇒ no section (byte-identical to every other answer).
    fn render_related_cursors(&self) -> String {
        if self.next.is_empty() {
            return String::new();
        }
        let total = self.next.len() + self.next_omitted_count.unwrap_or(0);
        let (noun, verb) = if total == 1 {
            ("constructor", "matches")
        } else {
            ("constructors", "match")
        };
        let cursors: Vec<String> = self
            .next
            .iter()
            .map(|action| match &action.target {
                Some(target) => format!("{} '{}'", action.kind, target),
                None => action.reason.clone(),
            })
            .collect();
        heading(&format!(
            "{total} {noun} also {verb}: {}",
            cursors.join(", ")
        ))
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

    /// EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): the focus symbol's `subtype` from the identity
    /// evidence (already on the wire). `None` for a non-symbol focus or when absent.
    fn get_identity_subtype(&self) -> Option<String> {
        for signal in &self.signals {
            if signal.value.code == "EXPLAIN_IDENTITY" {
                if let Some(ref ev) = signal.value.evidence {
                    return ev
                        .get("subtype")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
        }
        None
    }

    /// EXPLAIN-TYPE-SECTIONS-1: is the focus a TYPE (class/struct/enum/interface/trait)? A type is
    /// never called, so its `Callers`/`Callees` zero-lines point the reader at Members / Referenced by.
    pub(super) fn focus_is_type(&self) -> bool {
        matches!(
            self.get_identity_subtype().as_deref(),
            Some("CLASS" | "STRUCT" | "ENUM" | "INTERFACE" | "TRAIT")
        )
    }

    /// EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): the ` — a type is not called; see Members /
    /// Referenced by` suffix appended to a `Callers (0)` / `Callees (0)` heading ONLY on a type focus.
    /// Empty for a function/method focus (byte-identical to before) and for any nonzero count.
    /// PYTHON-RECEIVER-BINDING-1: the focus symbol's qualified name, for the `rmap callers <name>
    /// --include-inferred` next action — read from the identity's stable key
    /// (`<repo>:<path>#<qualified name>:SYMBOL:<SUBTYPE>`), else the identity's name.
    pub(super) fn focus_qualified_name(&self) -> Option<String> {
        let ev = self
            .signals
            .iter()
            .find(|s| s.value.code == "EXPLAIN_IDENTITY")?
            .value
            .evidence
            .as_ref()?;
        let from_key = ev
            .get("stable_key")
            .and_then(|v| v.as_str())
            .and_then(|k| k.split_once('#'))
            .and_then(|(_, rest)| rest.split_once(":SYMBOL:"))
            .map(|(qn, _)| qn.to_string())
            .filter(|qn| !qn.is_empty());
        from_key.or_else(|| ev.get("name").and_then(|v| v.as_str()).map(str::to_string))
    }

    pub(super) fn type_not_called_suffix(&self, count: u64) -> String {
        if count == 0 && self.focus_is_type() {
            " — a type is not called; see Members / Referenced by".to_string()
        } else {
            String::new()
        }
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
                    // TEST-EDGE-SCOPE-1A (RG-REQ-001-L07): the file's UNDETERMINED test status.
                    if let Some(line) = super::test_status::explain_test_status_line(ev) {
                        info.push_str(&format!("{line}\n"));
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
            next: vec![],
            next_omitted_count: None,
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
    fn no_high_confidence_beside_no_match() {
        // SYMBOL-IDENTITY-1 §2.4 / STANDING HONESTY RULE 3: a miss is never rendered
        // "Confidence: high". The agent use case now derives `low` for a no_match (was a static
        // `High` literal, root-caused in 2026-09-06 audit §H-A); this fixture pins the rendered
        // contract — the `no_match` line and the confidence line, with `high` absent.
        let mut r = minimal_response();
        r.confidence = "low".to_string(); // what `build_no_match` now emits
        r.focus = ExplainFocus {
            input: Some("DBImpl::Recover".to_string()),
            resolved: false,
            resolved_kind: None,
            resolved_path: None,
            reason: Some("no_match".to_string()),
            candidates: vec![],
        };
        let out = r.render_human(false);
        assert!(
            out.contains("unresolved: no_match"),
            "the miss is rendered as no_match: {out}"
        );
        assert!(
            !out.contains("Confidence: high"),
            "a miss must NOT print `Confidence: high`: {out}"
        );
        assert!(
            out.contains("Confidence: low"),
            "the honest floor `low` is rendered beside the miss: {out}"
        );
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

    // ── EXPLAIN-CYCLES-HONEST-1 (§2.2 evidence taxonomy: one test per input shape) ──

    /// Build a resolved (file-focus) response carrying ONE `EXPLAIN_CYCLES` signal whose evidence
    /// `items` are the given JSON array — the shape the daemon serializes from `CycleEvidence`.
    fn cycles_response(count: u64, items: serde_json::Value) -> ExplainResponse {
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CYCLES".to_string(),
            summary: format!("{count} import cycle(s)."),
            // A current daemon's complete partition payload, nothing excluded (D-TESB-17 fixture
            // rule).
            evidence: Some(serde_json::json!({
                "count": count, "items": items,
                "import_view": {"include_tests": false, "include_inferred": false},
                "import_remainder": {"tests": {"imports": 0, "edges": 0},
                                     "inferred": {"imports": 0, "edges": 0},
                                     "tests_and_inferred": {"imports": 0, "edges": 0}},
                "excluded_cycles": [],
                "importer_test_status_undetermined": {"count": 0, "paths": [],
                    "universe": "cross_directory_importers", "universe_count": 0,
                    "unknown_count": 0}
            })),
        })];
        r
    }

    // ── TEST-EDGE-SCOPE-1B: the import partition on explain ──

    /// The `excluded_cycles` + importer keys the agent merges into `EXPLAIN_CYCLES` evidence.
    fn partitioned_cycles_evidence(count: u64, items: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "count": count,
            "items": items,
            "import_view": {"include_tests": false, "include_inferred": false},
            "import_remainder": {
                "tests": {"imports": 105, "edges": 6},
                "inferred": {"imports": 0, "edges": 0},
                "tests_and_inferred": {"imports": 0, "edges": 0}
            },
            "excluded_cycles": [{
                "members": ["db", "helpers/memenv", "table", "util"],
                "length": 4,
                "flags": ["include_tests"],
                "contains_shown": [],
                "partitions": {"production_certain": 2, "test_certain": 3, "production_inferred": 0,
                               "test_inferred": 0, "unknown_test_status": 0}
            }],
            "importer_test_status_undetermined": {
                "count": 1, "paths": ["util/testutil.cc"], "universe": "cross_directory_importers",
                "universe_count": 67, "unknown_count": 0
            }
        })
    }

    #[test]
    fn explain_import_cycles_block_renders_excluded_cycles_when_none_remain() {
        // §2.4 leveldb `explain leveldb::DBImpl::Recover`, verbatim: no cycle remains, one exists
        // only through imports from test files — the block states it and the importer count.
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CYCLES".to_string(),
            summary: "0 import cycles.".to_string(),
            evidence: Some(partitioned_cycles_evidence(0, serde_json::json!([]))),
        })];
        let out = r.render_human(false);
        let expected = [
            "Import cycles (0)",
            "  - +1 only through imports from test files — rmap cycles --include-tests",
            "  - 1 file whose test status can't be determined — open it and look inside (of 67 files importing across directories)",
        ]
        .join("\n");
        assert!(out.contains(&expected), "{out}");

        // Beside a remaining cycle the same statements follow its rows.
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CYCLES".to_string(),
            summary: "1 import cycle.".to_string(),
            evidence: Some(partitioned_cycles_evidence(
                1,
                serde_json::json!([{ "length": 2, "modules": ["db", "table"], "walk": ["db", "table"] }]),
            )),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains("Import cycles (1)\n  - Cycle 1 (2 modules): db -> table -> db\n  - +1 only through imports from test files — rmap cycles --include-tests\n"),
            "{out}"
        );

        // No cycle and nothing excluded: no block, as before.
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CYCLES".to_string(),
            summary: "0 import cycles.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 0, "items": [], "excluded_cycles": [],
                "import_view": {"include_tests": false, "include_inferred": false},
                "import_remainder": {"tests": {"imports": 0, "edges": 0},
                                     "inferred": {"imports": 0, "edges": 0},
                                     "tests_and_inferred": {"imports": 0, "edges": 0}},
                "importer_test_status_undetermined": {"count": 0, "paths": [],
                    "universe": "cross_directory_importers", "universe_count": 0,
                    "unknown_count": 0}
            })),
        })];
        assert!(!r.render_human(false).contains("Import cycles"));
    }

    #[test]
    fn explain_imports_section_states_the_inferred_remainder() {
        // RG-REQ-002-L11: the file's certain imports are listed; the inferred ones are counted
        // with the command that lists them for this file.
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_IMPORTS".to_string(),
            summary: "1 import.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 1,
                "items": [{"target_file": "src/core/session.ts"}],
                "import_remainder": {
                    "tests": {"imports": 0, "edges": 0},
                    "inferred": {"imports": 2, "edges": 0},
                    "tests_and_inferred": {"imports": 0, "edges": 0}
                }
            })),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains("Imports (1)\n  - src/core/session.ts\n  - +2 inferred imports, not shown — rmap imports src/core/auth.ts --include-inferred"),
            "{out}"
        );
        // No inferred import: the section is as before.
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_IMPORTS".to_string(),
            summary: "1 import.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 1, "items": [{"target_file": "src/core/session.ts"}]
            })),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains("Imports (1)\n  - src/core/session.ts"),
            "{out}"
        );
        assert!(!out.contains("not shown"), "{out}");
    }

    #[test]
    fn explain_cycles_absent_walk_renders_unordered_no_arrows() {
        // No `walk` (LiveGraph route / older daemon) => the honest unordered listing, ZERO arrows.
        let r = cycles_response(
            1,
            serde_json::json!([{ "length": 3, "modules": ["src/a", "src/b", "src/c"] }]),
        );
        let out = r.render_human(false);
        assert!(out.contains("Import cycles (1)"), "{out}");
        assert!(
            out.contains("Cycle 1 (3 modules): members (unordered): src/a, src/b, src/c"),
            "unordered listing with the full member set:\n{out}"
        );
        assert!(
            !out.contains(" -> "),
            "NO arrows without a verified walk:\n{out}"
        );
    }

    #[test]
    fn explain_cycles_valid_walk_renders_ring_from_walk_not_member_order() {
        // `modules` (the sorted member SET) is [a, b, c]; the REAL walk is a -> c -> b. The ring
        // must follow the WALK, never the sorted member order (which would fabricate a -> b -> c).
        let r = cycles_response(
            1,
            serde_json::json!([{ "length": 3, "modules": ["a", "b", "c"], "walk": ["a", "c", "b"] }]),
        );
        let out = r.render_human(false);
        assert!(
            out.contains("Cycle 1 (3 modules): a -> c -> b -> a"),
            "ring drawn in WALK order, closing on its first member:\n{out}"
        );
        assert!(
            !out.contains("a -> b -> c"),
            "the fabricated member-order ring must NOT appear:\n{out}"
        );
    }

    #[test]
    fn explain_cycles_offwalk_members_reported_as_plus_n_more() {
        // length 4, the walk visits {a, b} => 2 members off the displayed loop.
        let r = cycles_response(
            1,
            serde_json::json!([{ "length": 4, "modules": ["a", "b", "c", "d"], "walk": ["a", "b"] }]),
        );
        let out = r.render_human(false);
        assert!(
            out.contains("Cycle 1 (4 modules): a -> b -> a"),
            "ring:\n{out}"
        );
        assert!(
            out.contains("(+ 2 more members in this cycle)"),
            "off-walk members reported as a count:\n{out}"
        );
    }

    #[test]
    fn explain_cycles_malformed_walk_renders_unreadable_not_ring() {
        // A PRESENT walk with a non-string element is wire/schema drift: the unknown is made
        // VISIBLE, never a fabricated ring.
        let r = cycles_response(
            1,
            serde_json::json!([{ "length": 2, "modules": ["a", "b"], "walk": ["a", 42] }]),
        );
        let out = r.render_human(false);
        assert!(
            out.contains("cycle walk unreadable on this snapshot"),
            "malformed walk => visible unknown:\n{out}"
        );
        assert!(
            !out.contains(" -> "),
            "NO ring drawn from a malformed walk:\n{out}"
        );
    }

    #[test]
    fn explain_cycles_empty_walk_renders_unordered() {
        // An empty `walk` array is legitimate absence (no walk could be formed) => unordered form.
        let r = cycles_response(
            1,
            serde_json::json!([{ "length": 2, "modules": ["a", "b"], "walk": [] }]),
        );
        let out = r.render_human(false);
        assert!(
            out.contains("Cycle 1 (2 modules): members (unordered): a, b"),
            "empty walk => unordered listing:\n{out}"
        );
        assert!(!out.contains(" -> "), "empty walk => no arrows:\n{out}");
    }

    #[test]
    fn explain_cycles_non_string_module_renders_unreadable_not_dropped() {
        // No walk => unordered path; a non-string member must surface as unreadable, NEVER be
        // silently dropped (the prior `filter_map(as_str)` shortened the list on drift).
        let r = cycles_response(
            1,
            serde_json::json!([{ "length": 3, "modules": ["a", 7, "c"] }]),
        );
        let out = r.render_human(false);
        assert!(
            out.contains("cycle members unreadable on this snapshot"),
            "non-string member => visible unknown:\n{out}"
        );
        assert!(
            !out.contains("members (unordered): a, c"),
            "never a silently shortened member list:\n{out}"
        );
    }

    #[test]
    fn explain_cycles_unordered_list_caps_at_eight_with_plus_k_more() {
        // 11 members, no walk => 8 shown + "(+ 3 more)", byte-mirroring `cycles`' render_unordered.
        let mods: Vec<String> = (0..11).map(|i| format!("m{i:02}")).collect();
        let r = cycles_response(1, serde_json::json!([{ "length": 11, "modules": mods }]));
        let out = r.render_human(false);
        assert!(
            out.contains("members (unordered): m00, m01, m02, m03, m04, m05, m06, m07 (+ 3 more)"),
            "cap at 8 with the remainder count:\n{out}"
        );
    }

    #[test]
    fn explain_cycles_non_array_walk_renders_unreadable_not_unordered() {
        // ECH-IR-001: a PRESENT `walk` that is not an array (here a scalar string) is wire/schema
        // drift. It must surface as the visible unreadable state, NEVER take the unordered fallback
        // (which would hide the drift and read like an honest "no walk").
        let r = cycles_response(
            1,
            serde_json::json!([{ "length": 2, "modules": ["a", "b"], "walk": "a -> b" }]),
        );
        let out = r.render_human(false);
        assert!(
            out.contains("cycle walk unreadable on this snapshot"),
            "non-array walk => visible unknown:\n{out}"
        );
        assert!(
            !out.contains("members (unordered)"),
            "a present non-array walk must NOT silently render the unordered fallback:\n{out}"
        );
        assert!(!out.contains(" -> "), "NO ring drawn from drift:\n{out}");
    }

    #[test]
    fn explain_cycles_absent_or_non_numeric_length_renders_size_unreadable_never_module_count() {
        // ECH-IR-001: K comes from the `length` fact ALONE. An absent length must render a NAMED
        // unreadable size, never a count synthesized from `modules.len()` (here 3 members).
        let absent = cycles_response(1, serde_json::json!([{ "modules": ["a", "b", "c"] }]));
        let out = absent.render_human(false);
        assert!(
            out.contains("Cycle 1 (size unreadable):"),
            "absent length => named unreadable size:\n{out}"
        );
        assert!(
            !out.contains("(3 modules)"),
            "the member count must NOT be presented as the cycle size:\n{out}"
        );
        // A non-numeric length (a string) is likewise unreadable, never coerced.
        let non_numeric = cycles_response(
            1,
            serde_json::json!([{ "length": "three", "modules": ["a", "b", "c"] }]),
        );
        let out2 = non_numeric.render_human(false);
        assert!(
            out2.contains("Cycle 1 (size unreadable):"),
            "non-numeric length => named unreadable size:\n{out2}"
        );
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

    fn ctor_action(cursor: &str, type_name: &str) -> crate::presentation::orient_types::NextAction {
        crate::presentation::orient_types::NextAction {
            kind: "explain".to_string(),
            repo: "test-repo".to_string(),
            target: Some(cursor.to_string()),
            reason: format!("constructor of {type_name}"),
        }
    }

    #[test]
    fn render_constructor_also_matches_counted_line_singular() {
        // CPP-DECLARATORS-1 §2.3 (review-4 #4): ONE constructor rides on `next` → the ratified
        // SINGLE-LINE form "1 constructor also matches: explain '<cursor>'" — count and the
        // runnable cursor on ONE line (operator ruling: never hidden).
        let mut r = symbol_target(Some(55));
        r.next = vec![ctor_action(
            "CGHeroInstance::CGHeroInstance",
            "CGHeroInstance",
        )];
        let out = r.render_human(false);
        assert!(
            out.contains("1 constructor also matches: explain 'CGHeroInstance::CGHeroInstance'"),
            "ratified single-line form (count + quoted runnable cursor):\n{out}"
        );
    }

    #[test]
    fn render_constructor_also_matches_counted_line_plural() {
        // CPP-DECLARATORS-1 §2.3 (review-4 #4): TWO constructors → the line pluralizes to
        // "2 constructors also match:" and BOTH cursors render on the same line (never hidden).
        let mut r = symbol_target(Some(55));
        r.next = vec![
            ctor_action("Widget::Widget", "Widget"),
            ctor_action("Widget::Widget", "Widget"),
        ];
        let out = r.render_human(false);
        assert!(
            out.contains(
                "2 constructors also match: explain 'Widget::Widget', explain 'Widget::Widget'"
            ),
            "plural single-line form with both cursors:\n{out}"
        );
    }

    #[test]
    fn render_constructor_counted_line_reports_omitted_total() {
        // review-4 #4: the count is the TRUE total — one rendered cursor plus two the budget
        // dropped → "3 constructors also match:", with the one available cursor on the line.
        let mut r = symbol_target(Some(55));
        r.next = vec![ctor_action("Widget::Widget", "Widget")];
        r.next_omitted_count = Some(2);
        let out = r.render_human(false);
        assert!(
            out.contains("3 constructors also match: explain 'Widget::Widget'"),
            "count = rendered + omitted, with the available cursor shown:\n{out}"
        );
    }

    #[test]
    fn render_no_next_omits_also_matches_section() {
        // Every non-collapse explain answer has an empty `next` ⇒ no section (byte-identical).
        let out = symbol_target(Some(55)).render_human(false);
        assert!(
            !out.contains("Also matches"),
            "empty next ⇒ no section:\n{out}"
        );
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

    // ── EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): Members / Referenced by / type zero-line (ETS-C03) ──

    /// Build a resolved SYMBOL-target response carrying an EXPLAIN_IDENTITY of the given `subtype`
    /// plus the provided section signals — the substrate for the type-focus render tests.
    fn typed_response(subtype: &str, mut sections: Vec<ExplainSignal>) -> ExplainResponse {
        let mut r = minimal_response();
        r.focus = ExplainFocus {
            input: Some("MyType".to_string()),
            resolved: true,
            resolved_kind: Some("symbol".to_string()),
            resolved_path: Some("src/a.h".to_string()),
            reason: None,
            candidates: vec![],
        };
        let mut signals = vec![ExplainSignal {
            code: "EXPLAIN_IDENTITY".to_string(),
            summary: "Identity: symbol target.".to_string(),
            evidence: Some(serde_json::json!({ "name": "MyType", "subtype": subtype })),
        }];
        signals.append(&mut sections);
        r.signals = signals.into_iter().map(leaf).collect();
        r
    }

    fn members_signal(count: u64, items: serde_json::Value) -> ExplainSignal {
        ExplainSignal {
            code: "EXPLAIN_MEMBERS".to_string(),
            summary: format!("{count} member(s)."),
            evidence: Some(serde_json::json!({ "count": count, "items": items })),
        }
    }

    #[test]
    fn explain_members_section_anchors_file_line_and_marks_decl() {
        let r = typed_response(
            "CLASS",
            vec![members_signal(
                2,
                serde_json::json!([
                    {"name": "Recover", "subtype": "METHOD", "file": "db/db_impl.cc", "line": 292, "forward_decl": false},
                    {"name": "Open", "subtype": "METHOD", "file": "db/db_impl.h", "line": 40, "forward_decl": true}
                ]),
            )],
        );
        let out = r.render_human(false);
        assert!(out.contains("Members (2)"), "header:\n{out}");
        assert!(
            out.contains("- Recover (METHOD)  db/db_impl.cc:292"),
            "member row anchors path:line:\n{out}"
        );
        assert!(
            out.contains("- Open (METHOD, decl)  db/db_impl.h:40"),
            "a forward_decl member renders the (decl) marker:\n{out}"
        );
    }

    #[test]
    fn explain_members_section_omits_absent_line_never_zero() {
        let r = typed_response(
            "CLASS",
            vec![members_signal(
                2,
                serde_json::json!([
                    {"name": "noLine", "subtype": "METHOD", "file": "src/a.h", "forward_decl": false},
                    {"name": "zeroLine", "subtype": "METHOD", "file": "src/a.h", "line": 0, "forward_decl": false}
                ]),
            )],
        );
        let out = r.render_human(false);
        assert!(
            out.contains("- noLine (METHOD)  src/a.h"),
            "absent line renders the bare path:\n{out}"
        );
        assert!(
            out.contains("- zeroLine (METHOD)  src/a.h"),
            "a 0 line renders the bare path, never `:0`:\n{out}"
        );
        assert!(!out.contains("src/a.h:0"), "no fabricated `:0`:\n{out}");
    }

    #[test]
    fn explain_members_over_cap_renders_more_line_and_full_uncaps() {
        let items: Vec<serde_json::Value> = (0..16)
            .map(|i| {
                serde_json::json!({"name": format!("m{i:02}"), "subtype": "METHOD", "file": "src/a.h", "line": 10 + i, "forward_decl": false})
            })
            .collect();
        let r = typed_response("CLASS", vec![members_signal(16, serde_json::json!(items))]);
        // Default: 15 rows + "... (1 more)".
        let out = r.render_human(false);
        assert!(out.contains("- m00 (METHOD)"), "first member:\n{out}");
        assert!(out.contains("- m14 (METHOD)"), "15th member kept:\n{out}");
        assert!(
            !out.contains("- m15 (METHOD)"),
            "16th dropped at the cap:\n{out}"
        );
        assert!(
            out.contains("  ... (1 more)"),
            "the remainder is named:\n{out}"
        );
        // --full: every member, no more-line.
        let full = r.render_human(true);
        assert!(
            full.contains("- m15 (METHOD)"),
            "--full renders all:\n{full}"
        );
        assert!(!full.contains("more)"), "--full has no more-line:\n{full}");
    }

    // ── CPP-ATTRIBUTE-MACRO-1A (RG-REQ-002-L11): the undetermined-identity marker on Members ──

    /// A member row carrying the complete marker (the wire shape `ExplainMemberItem` serializes).
    fn marked_member(
        name: &str,
        line: u64,
        forward_decl: bool,
        c1: &str,
        c2: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "name": name, "subtype": "METHOD", "file": "db/db_test.cc", "line": line,
            "forward_decl": forward_decl, "identity": "undetermined",
            "identity_candidates": [c1, c2], "identity_basis": "macro_recovery_ambiguous_identity"
        })
    }

    #[test]
    fn member_row_with_identity_candidates_renders_the_undetermined_marker() {
        let r = typed_response(
            "CLASS",
            vec![members_signal(
                4,
                serde_json::json!([
                    {"name": "IncrementBy", "subtype": "METHOD", "file": "db/db_test.cc", "line": 47, "forward_decl": true},
                    marked_member("LOCKS_EXCLUDED", 47, false, "IncrementBy", "LOCKS_EXCLUDED"),
                    marked_member("GUARDED_FN", 60, true, "Helper", "GUARDED_FN"),
                    {"name": "Bare", "file": "db/db_test.cc", "line": 70, "forward_decl": false,
                     "identity": "undetermined", "identity_candidates": ["Real", "Bare"],
                     "identity_basis": "macro_recovery_ambiguous_identity"}
                ]),
            )],
        );
        let out = r.render_human(false);
        assert!(
            out.contains("  - IncrementBy (METHOD, decl)  db/db_test.cc:47\n"),
            "a determined member renders exactly as before:\n{out}"
        );
        assert!(
            out.contains(
                "  - LOCKS_EXCLUDED (METHOD, identity undetermined — 2 candidates: IncrementBy, LOCKS_EXCLUDED)  db/db_test.cc:47\n"
            ),
            "the marked row names both candidates in carried order:\n{out}"
        );
        assert!(
            out.contains(
                "  - GUARDED_FN (METHOD, decl, identity undetermined — 2 candidates: Helper, GUARDED_FN)  db/db_test.cc:60\n"
            ),
            "decl and marker together:\n{out}"
        );
        assert!(
            out.lines().any(|l| l
                == "  - Bare (identity undetermined — 2 candidates: Real, Bare)  db/db_test.cc:70"),
            "no subtype: the marker alone in parentheses:\n{out}"
        );
        // JSON `null` marker keys are legitimate absence: today's row, byte-identical.
        let nulls = typed_response(
            "CLASS",
            vec![members_signal(
                1,
                serde_json::json!([{"name": "Recover", "subtype": "METHOD", "file": "db/db_impl.cc",
                    "line": 292, "forward_decl": false, "identity": null,
                    "identity_candidates": null, "identity_basis": null}]),
            )],
        );
        assert!(nulls
            .render_human(false)
            .lines()
            .any(|l| l == "  - Recover (METHOD)  db/db_impl.cc:292"));
    }

    /// Render one Members section whose single item is `item`; assert the WHOLE section is the
    /// unreadable line with no row.
    fn assert_members_unreadable(item: serde_json::Value, why: &str) {
        let r = typed_response(
            "CLASS",
            vec![members_signal(
                2,
                serde_json::json!([
                    {"name": "Fine", "subtype": "METHOD", "file": "a.h", "line": 1, "forward_decl": false},
                    item
                ]),
            )],
        );
        let out = r.render_human(false);
        assert!(
            out.contains("Members (2)") && out.contains("members unreadable on this snapshot"),
            "{why}: the whole section is the unreadable line:\n{out}"
        );
        assert!(!out.contains("- Fine"), "{why}: no partial rows:\n{out}");
        assert!(
            !out.contains("identity undetermined"),
            "{why}: never a coerced marker:\n{out}"
        );
    }

    #[test]
    fn members_section_with_a_malformed_identity_marker_renders_the_unreadable_line() {
        let base = marked_member("M", 5, false, "A", "M");
        let with = |key: &str, v: serde_json::Value| {
            let mut item = base.clone();
            item[key] = v;
            item
        };
        assert_members_unreadable(
            with("identity", serde_json::json!("determined")),
            "other identity value",
        );
        assert_members_unreadable(
            with("identity", serde_json::json!(true)),
            "non-string identity",
        );
        assert_members_unreadable(
            with("identity_candidates", serde_json::json!("A, M")),
            "non-array candidates",
        );
        assert_members_unreadable(
            with("identity_candidates", serde_json::json!(["A", 7])),
            "non-string candidate",
        );
        assert_members_unreadable(
            with("identity_basis", serde_json::json!(3)),
            "non-string basis",
        );
        assert_members_unreadable(with("identity_basis", serde_json::json!("")), "empty basis");
    }

    #[test]
    fn members_section_with_a_partial_single_or_duplicate_candidate_marker_renders_the_unreadable_line(
    ) {
        let base = marked_member("M", 5, false, "A", "M");
        let without = |key: &str| {
            let mut item = base.clone();
            item.as_object_mut().unwrap().remove(key);
            item
        };
        let with = |key: &str, v: serde_json::Value| {
            let mut item = base.clone();
            item[key] = v;
            item
        };
        assert_members_unreadable(without("identity"), "partial: no identity");
        assert_members_unreadable(without("identity_candidates"), "partial: no candidates");
        assert_members_unreadable(without("identity_basis"), "partial: no basis");
        assert_members_unreadable(
            with("identity_basis", serde_json::Value::Null),
            "partial: null basis",
        );
        assert_members_unreadable(
            with("identity_candidates", serde_json::json!(["A"])),
            "single candidate",
        );
        assert_members_unreadable(
            with("identity_candidates", serde_json::json!(["A", "M", "B"])),
            "three candidates",
        );
        assert_members_unreadable(
            with("identity_candidates", serde_json::json!(["M", "M"])),
            "duplicate candidate",
        );
        assert_members_unreadable(
            with("identity_candidates", serde_json::json!(["", "M"])),
            "empty candidate",
        );
    }

    #[test]
    fn explain_referenced_by_section_names_count_top_modules_and_files() {
        let r = typed_response(
            "CLASS",
            vec![ExplainSignal {
                code: "EXPLAIN_REFERENCED_BY".to_string(),
                summary: "referenced by 2 files.".to_string(),
                evidence: Some(serde_json::json!({
                    "count": 2,
                    "top_modules": [{"module": "lib", "count": 1}, {"module": "client", "count": 1}],
                    "items": [{"file": "lib/x.cpp", "module": "lib"}, {"file": "client/y.cpp", "module": "client"}]
                })),
            }],
        );
        let out = r.render_human(false);
        assert!(out.contains("Referenced by (2 files)"), "header:\n{out}");
        assert!(
            out.contains("  top modules: lib (1), client (1)"),
            "top modules line:\n{out}"
        );
        assert!(out.contains("- lib/x.cpp"), "file row:\n{out}");
        assert!(out.contains("- client/y.cpp"), "file row:\n{out}");
    }

    // ── PYTHON-SUBMODULE-IMPORT-1 (RG-REQ-002-L11): the inferred remainder of Referenced by ──

    /// The django `explain BaseHandler` evidence shape: one certain importer, two inferred ones.
    fn base_handler_referenced_by() -> serde_json::Value {
        serde_json::json!({
            "count": 1,
            "top_modules": [{"module": "django/test", "count": 1}],
            "items": [{"file": "django/test/client.py", "module": "django/test"}],
            "inferred_count": 2,
            "inferred_items": [
                {"file": "django/core/handlers/asgi.py", "module": "django/core", "basis": "python_submodule",
                 "alternate": "django/core/handlers/__init__.py"},
                {"file": "django/core/handlers/wsgi.py", "module": "django/core", "basis": "python_submodule",
                 "alternate": "django/core/handlers/__init__.py"}
            ]
        })
    }

    #[test]
    fn referenced_by_heading_states_the_inferred_remainder_and_rows_name_the_alternate() {
        let r = typed_response(
            "CLASS",
            vec![referenced_by_evidence(base_handler_referenced_by())],
        );
        let out = r.render_human(false);
        let lines: Vec<&str> = out.lines().collect();
        let head = lines
            .iter()
            .position(|l| l.starts_with("Referenced by"))
            .expect("section heading");
        assert_eq!(
            &lines[head..head + 5],
            &[
                "Referenced by (1 files, +2 inferred — investigate)",
                "  top modules: django/test (1)",
                "  - django/test/client.py",
                "  - django/core/handlers/asgi.py  (inferred: python submodule — alternate django/core/handlers/__init__.py)",
                "  - django/core/handlers/wsgi.py  (inferred: python submodule — alternate django/core/handlers/__init__.py)",
            ],
            "{out}"
        );
        assert!(
            !out.contains("python_submodule"),
            "never the raw basis code:\n{out}"
        );

        // A basis other than python_submodule renders without its code; a budget-cut remainder
        // is stated, never silently dropped.
        let other = typed_response(
            "CLASS",
            vec![referenced_by_evidence(serde_json::json!({
                "count": 0,
                "top_modules": [],
                "items": [],
                "inferred_count": 3,
                "inferred_items": [
                    {"file": "a.c", "basis": "unique_basename", "alternate": "b.h"},
                    {"file": "c.c", "basis": "unique_basename"}
                ],
                "inferred_items_omitted_count": 1
            }))],
        );
        let out = other.render_human(false);
        assert!(
            out.contains("Referenced by (0 files, +3 inferred — investigate)"),
            "{out}"
        );
        assert!(
            out.contains("  - a.c  (inferred — alternate b.h)\n"),
            "{out}"
        );
        assert!(out.contains("  - c.c  (inferred)\n"), "{out}");
        assert!(out.lines().any(|l| l == "  ... (1 more inferred)"), "{out}");
        assert!(!out.contains("unique_basename"), "{out}");

        // One file carried twice (two alternates) is ONE inferred file: the count is the distinct
        // files, and both rows render.
        let one_file_two_rows = typed_response(
            "CLASS",
            vec![referenced_by_evidence(serde_json::json!({
                "count": 0,
                "top_modules": [],
                "items": [],
                "inferred_count": 1,
                "inferred_items": [
                    {"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"},
                    {"file": "a.py", "basis": "python_submodule", "alternate": "q/__init__.py"}
                ]
            }))],
        );
        let out = one_file_two_rows.render_human(false);
        assert!(
            out.contains("Referenced by (0 files, +1 inferred — investigate)"),
            "{out}"
        );
        assert!(
            out.contains("  - a.py  (inferred: python submodule — alternate q/__init__.py)"),
            "{out}"
        );
    }

    #[test]
    fn referenced_by_with_no_inferred_rows_renders_exactly_as_before() {
        let evidence = serde_json::json!({
            "count": 2,
            "top_modules": [{"module": "lib", "count": 1}, {"module": "client", "count": 1}],
            "items": [{"file": "lib/x.cpp", "module": "lib"}, {"file": "client/y.cpp", "module": "client"}]
        });
        let out =
            typed_response("CLASS", vec![referenced_by_evidence(evidence)]).render_human(false);
        let lines: Vec<&str> = out.lines().collect();
        let head = lines
            .iter()
            .position(|l| l.starts_with("Referenced by"))
            .expect("section heading");
        assert_eq!(
            &lines[head..head + 4],
            &[
                "Referenced by (2 files)",
                "  top modules: lib (1), client (1)",
                "  - lib/x.cpp",
                "  - client/y.cpp",
            ],
            "{out}"
        );
        assert!(!out.contains("inferred"), "{out}");
    }

    #[test]
    fn referenced_by_with_malformed_inferred_rows_renders_the_unreadable_line() {
        let malformed = [
            (
                "inferred_count not a number",
                serde_json::json!({"inferred_count": "2"}),
            ),
            (
                "inferred_items not an array",
                serde_json::json!({"inferred_count": 1, "inferred_items": {}}),
            ),
            (
                "row without a file",
                serde_json::json!({"inferred_count": 1, "inferred_items": [{"basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            (
                "row without a basis",
                serde_json::json!({"inferred_count": 1, "inferred_items": [{"file": "a.py", "alternate": "p/__init__.py"}]}),
            ),
            (
                "python submodule without its alternate",
                serde_json::json!({"inferred_count": 1, "inferred_items": [{"file": "a.py", "basis": "python_submodule"}]}),
            ),
            (
                "non-string alternate",
                serde_json::json!({"inferred_count": 1, "inferred_items": [{"file": "a.py", "basis": "python_submodule", "alternate": 3}]}),
            ),
            (
                "count without rows",
                serde_json::json!({"inferred_count": 2, "inferred_items": []}),
            ),
            (
                "rows without a count",
                serde_json::json!({"inferred_items": [{"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            // F-PSI-03: a count the carried rows do not support.
            (
                "count 99 over one uncut file",
                serde_json::json!({"inferred_count": 99, "inferred_items": [{"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            (
                "count below the distinct carried files",
                serde_json::json!({"inferred_count": 1, "inferred_items": [
                    {"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"},
                    {"file": "b.py", "basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            (
                "count above the distinct files plus the cut rows",
                serde_json::json!({"inferred_count": 5, "inferred_items_omitted_count": 1, "inferred_items": [
                    {"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            (
                "cut rows with no carried row",
                serde_json::json!({"inferred_count": 1, "inferred_items_omitted_count": 1, "inferred_items": []}),
            ),
            // F-PSI-03: a PRESENT null is malformed, never a known zero or an absent value.
            (
                "null inferred_count",
                serde_json::json!({"inferred_count": null}),
            ),
            (
                "null inferred_items",
                serde_json::json!({"inferred_count": 1, "inferred_items": null}),
            ),
            (
                "null inferred_items_omitted_count",
                serde_json::json!({"inferred_count": 1, "inferred_items_omitted_count": null, "inferred_items": [
                    {"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            (
                "null alternate",
                serde_json::json!({"inferred_count": 1, "inferred_items": [{"file": "a.py", "basis": "unique_basename", "alternate": null}]}),
            ),
            (
                "null module",
                serde_json::json!({"inferred_count": 1, "inferred_items": [{"file": "a.py", "module": null, "basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            // F-PSI-03 (review 1): a carried total that overflows `u64` is malformed, never a
            // panic or a wrap — the distinct-plus-cut bound ...
            (
                "u64::MAX cut rows over the count bound",
                serde_json::json!({"inferred_count": u64::MAX, "inferred_items_omitted_count": u64::MAX, "inferred_items": [
                    {"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"}]}),
            ),
            // ... and the carried-plus-cut row total the `... (N more inferred)` line sums:
            // seventeen rows of one file (two past the display cap) with `u64::MAX - 1` cut rows,
            // where the distinct-plus-cut bound (1 + u64::MAX - 1) does not overflow but the
            // line's `2 + u64::MAX - 1` would.
            (
                "carried rows plus u64::MAX - 1 cut rows",
                serde_json::json!({"inferred_count": 1, "inferred_items_omitted_count": u64::MAX - 1, "inferred_items":
                    vec![serde_json::json!({"file": "a.py", "basis": "python_submodule", "alternate": "p/__init__.py"}); 17]}),
            ),
        ];
        for (label, extra) in malformed {
            let mut evidence = serde_json::json!({
                "count": 1,
                "top_modules": [{"module": "lib", "count": 1}],
                "items": [{"file": "lib/x.py", "module": "lib"}]
            });
            for (k, v) in extra.as_object().unwrap() {
                evidence[k] = v.clone();
            }
            let out =
                typed_response("CLASS", vec![referenced_by_evidence(evidence)]).render_human(false);
            assert!(
                out.contains(
                    "Referenced by (1 files)\n  - referenced-by unreadable on this snapshot"
                ),
                "{label}:\n{out}"
            );
            assert!(
                !out.contains("- lib/x.py"),
                "{label}: no partial rows:\n{out}"
            );
            assert!(
                !out.contains("inferred"),
                "{label}: no partial remainder:\n{out}"
            );
        }
    }

    #[test]
    fn explain_type_zero_callers_line_says_a_type_is_not_called() {
        let r = typed_response(
            "CLASS",
            vec![
                ExplainSignal {
                    code: "EXPLAIN_CALLERS".to_string(),
                    summary: "0 direct callers.".to_string(),
                    evidence: Some(serde_json::json!({ "count": 0, "items": [] })),
                },
                ExplainSignal {
                    code: "EXPLAIN_CALLEES".to_string(),
                    summary: "0 direct callees.".to_string(),
                    evidence: Some(serde_json::json!({ "count": 0, "items": [] })),
                },
            ],
        );
        let out = r.render_human(false);
        assert!(
            out.contains("Callers (0) — a type is not called; see Members / Referenced by"),
            "type Callers zero-line:\n{out}"
        );
        assert!(
            out.contains("Callees (0) — a type is not called; see Members / Referenced by"),
            "type Callees zero-line:\n{out}"
        );
    }

    #[test]
    fn explain_function_zero_callers_line_is_unchanged() {
        let r = typed_response(
            "FUNCTION",
            vec![ExplainSignal {
                code: "EXPLAIN_CALLERS".to_string(),
                summary: "0 direct callers.".to_string(),
                evidence: Some(serde_json::json!({ "count": 0, "items": [] })),
            }],
        );
        let out = r.render_human(false);
        assert!(out.contains("Callers (0)"), "plain zero-line:\n{out}");
        assert!(
            !out.contains("Callers (0) —"),
            "a function's Callers (0) carries no type suffix:\n{out}"
        );
        assert!(
            !out.contains("a type is not called"),
            "a function is not a type:\n{out}"
        );
    }

    /// A raw EXPLAIN_MEMBERS signal carrying an ARBITRARY evidence object, so a malformed
    /// `count`/`items` (which [`members_signal`] cannot express) can be exercised.
    fn members_evidence(evidence: serde_json::Value) -> ExplainSignal {
        ExplainSignal {
            code: "EXPLAIN_MEMBERS".to_string(),
            summary: "members.".to_string(),
            evidence: Some(evidence),
        }
    }

    /// A raw EXPLAIN_REFERENCED_BY signal carrying an ARBITRARY evidence object.
    fn referenced_by_evidence(evidence: serde_json::Value) -> ExplainSignal {
        ExplainSignal {
            code: "EXPLAIN_REFERENCED_BY".to_string(),
            summary: "referenced by.".to_string(),
            evidence: Some(evidence),
        }
    }

    #[test]
    fn explain_members_rows_render_only_from_carried_evidence() {
        // F-ETS-01 (the class, not the instance): the COMPLETE carried evidence of both type
        // sections is validated before any row renders. ANY malformed field makes the WHOLE section
        // exactly its heading + the single unreadable line — never a fabricated count, a bare
        // heading, a partial row list, or a silently dropped top-module entry (STANDING HONESTY RULE).

        // ── Members ──

        // (1) missing count → unreadable, and NO fabricated `Members (0)` count parenthetical.
        let missing_count = typed_response(
            "CLASS",
            vec![members_evidence(serde_json::json!({
                "items": [{"name": "m", "subtype": "METHOD", "file": "src/a.h", "line": 1}]
            }))],
        );
        let out = missing_count.render_human(false);
        assert!(
            out.contains("members unreadable on this snapshot"),
            "missing count → unreadable:\n{out}"
        );
        assert!(
            !out.contains("Members (0)"),
            "missing count must NOT fabricate `Members (0)`:\n{out}"
        );
        assert!(
            !out.contains("- m (METHOD)"),
            "missing count → no partial rows:\n{out}"
        );

        // (2) non-integer count → unreadable, no rows.
        let bad_count = typed_response(
            "CLASS",
            vec![members_evidence(serde_json::json!({
                "count": "many",
                "items": [{"name": "m", "subtype": "METHOD", "file": "src/a.h", "line": 1}]
            }))],
        );
        let out = bad_count.render_human(false);
        assert!(
            out.contains("members unreadable on this snapshot"),
            "non-integer count → unreadable:\n{out}"
        );
        assert!(
            !out.contains("- m (METHOD)"),
            "non-integer count → no partial rows:\n{out}"
        );

        // (3) non-array items → unreadable (never a bare `Members (N)` heading with no rows/line).
        let non_array_items = typed_response(
            "CLASS",
            vec![members_evidence(
                serde_json::json!({ "count": 3, "items": {} }),
            )],
        );
        let out = non_array_items.render_human(false);
        assert!(
            out.contains("Members (3)") && out.contains("members unreadable on this snapshot"),
            "non-array items → heading with real count + unreadable line:\n{out}"
        );

        // (4) non-string name → unreadable, no partial rows.
        let bad_name = typed_response(
            "CLASS",
            vec![members_signal(
                1,
                serde_json::json!([{"name": 42, "subtype": "METHOD", "file": "src/a.h", "line": 1}]),
            )],
        );
        let out = bad_name.render_human(false);
        assert!(
            out.contains("members unreadable on this snapshot"),
            "non-string name → unreadable:\n{out}"
        );

        // (5) non-integer line on the SECOND item → unreadable, no partial list of the first (valid) row.
        let bad_line = typed_response(
            "CLASS",
            vec![members_signal(
                2,
                serde_json::json!([
                    {"name": "ok", "subtype": "METHOD", "file": "src/a.h", "line": 5},
                    {"name": "bad", "subtype": "METHOD", "file": "src/a.h", "line": "five"}
                ]),
            )],
        );
        let out = bad_line.render_human(false);
        assert!(
            out.contains("members unreadable on this snapshot"),
            "non-integer line → unreadable:\n{out}"
        );
        assert!(
            !out.contains("- ok (METHOD)"),
            "never a partial list when an item is malformed:\n{out}"
        );

        // ── Referenced by ──

        // (6) a malformed top-module entry (missing its count) → unreadable; NEVER a silently dropped
        //     entry and never the authoritative file rows beneath a partial `top modules:` line.
        let bad_top_module = typed_response(
            "CLASS",
            vec![referenced_by_evidence(serde_json::json!({
                "count": 1,
                "top_modules": [{"module": "lib"}],
                "items": [{"file": "lib/x.cpp", "module": "lib"}]
            }))],
        );
        let out = bad_top_module.render_human(false);
        assert!(
            out.contains("referenced-by unreadable on this snapshot"),
            "malformed top-module entry → unreadable:\n{out}"
        );
        assert!(
            !out.contains("top modules:"),
            "malformed top-module entry → no partial `top modules:` line:\n{out}"
        );
        assert!(
            !out.contains("- lib/x.cpp"),
            "malformed top-module entry → no file rows:\n{out}"
        );

        // (7) a malformed referenced-by file row (missing its `file`) → unreadable, no partial rows.
        let bad_refby_row = typed_response(
            "CLASS",
            vec![referenced_by_evidence(serde_json::json!({
                "count": 1,
                "items": [{"module": "lib"}]
            }))],
        );
        let out = bad_refby_row.render_human(false);
        assert!(
            out.contains("referenced-by unreadable on this snapshot"),
            "malformed referenced-by file row → unreadable:\n{out}"
        );
    }

    // ── PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11, RG-REQ-005-L09) ──

    /// A resolved method-focus response (stable key `r:m.py#ListMixin.extend:SYMBOL:METHOD`).
    fn method_response(sections: Vec<ExplainSignal>) -> ExplainResponse {
        let mut r = minimal_response();
        let mut signals = vec![ExplainSignal {
            code: "EXPLAIN_IDENTITY".to_string(),
            summary: "Identity: symbol target.".to_string(),
            evidence: Some(serde_json::json!({
                "name": "extend",
                "subtype": "METHOD",
                "stable_key": "r:django/contrib/gis/geos/mutable_list.py#ListMixin.extend:SYMBOL:METHOD"
            })),
        }];
        signals.extend(sections);
        r.signals = signals.into_iter().map(leaf).collect();
        r
    }

    fn remainder_callers() -> ExplainSignal {
        ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "2 direct callers.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 2,
                "items": [
                    {"name": "__iadd__", "module": "django/contrib/gis/geos", "file": "django/contrib/gis/geos/mutable_list.py", "line": 123},
                    {"name": "__imul__", "module": "django/contrib/gis/geos", "file": "django/contrib/gis/geos/mutable_list.py", "line": 141}
                ],
                "inferred_count": 266,
                "inferred_by_basis": {"receiver_untyped_name_only": 266},
                "inferred_items": [
                    {"stable_key": "k", "name": "_get_dependencies_for_model", "module": "django/db/migrations",
                     "file": "django/db/migrations/autodetector.py", "line": 1737,
                     "basis": "receiver_untyped_name_only", "receiver": "dependencies", "extractor": "python-core:0.2.0"}
                ],
                "unresolved_naming": {"name": "extend", "count": 3, "by_basis": {"self_call_hierarchy_miss": 3}}
            })),
        }
    }

    #[test]
    fn explain_callers_render_the_inferred_and_unresolved_remainder_lines() {
        let out = method_response(vec![remainder_callers()]).render_human(false);
        assert!(out.contains("Callers (2)"), "{out}");
        assert!(
            out.contains("+266 inferred (name-only) — investigate with rmap callers ListMixin.extend --include-inferred"),
            "{out}"
        );
        assert!(
            out.contains("3 unresolved calls name `extend` (couldn't attribute: 3)"),
            "{out}"
        );
        assert!(
            !out.contains("autodetector.py:1737"),
            "the inferred rows are listed only under --full:\n{out}"
        );
    }

    /// Review F-1 (RG-REQ-002-L02): explain's caller rows anchor each caller's declaration, while
    /// `rmap callers` anchors the call site — the differing basis is stated inline with the command
    /// that gives the call sites; rows without a line state nothing.
    #[test]
    fn explain_callers_state_that_their_lines_are_declarations_and_name_the_call_site_command() {
        let out = method_response(vec![remainder_callers()]).render_human(false);
        assert!(
            out.contains("lines above are where each caller is defined — for the call sites run rmap callers ListMixin.extend"),
            "{out}"
        );
        let lineless = ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "1 direct caller.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 1,
                "items": [{"name": "__iadd__", "module": "django/contrib/gis/geos"}]
            })),
        };
        let out = method_response(vec![lineless]).render_human(false);
        assert!(!out.contains("lines above"), "{out}");
    }

    /// Review F-2 (RG-REQ-002-L04): a remainder field that is PRESENT but unreadable renders
    /// unknown with its reason — never as if it were absent; an absent field renders nothing.
    #[test]
    fn explain_remainder_field_present_but_unreadable_renders_unknown_never_absent() {
        let malformed = ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "2 direct callers.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 0,
                "items": [],
                "inferred_count": "266",
                "unresolved_naming": {"name": "extend", "count": "3", "by_basis": {"self_call_hierarchy_miss": 3}}
            })),
        };
        let out = method_response(vec![malformed]).render_human(false);
        assert!(
            out.contains(
                "inferred callers: unknown — this answer's inferred callers field is unreadable"
            ),
            "{out}"
        );
        assert!(
            out.contains(
                "unresolved calls: unknown — this answer's unresolved-call count is unreadable"
            ),
            "{out}"
        );
        let rows_malformed = ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "0 direct callers.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 0,
                "items": [],
                "inferred_count": 1,
                "inferred_items": {"not": "a list"}
            })),
        };
        let out = method_response(vec![rows_malformed]).render_human(true);
        assert!(
            out.contains("inferred callers rows: unknown — this answer's inferred callers rows field is unreadable"),
            "{out}"
        );
        let absent = ExplainSignal {
            code: "EXPLAIN_CALLERS".to_string(),
            summary: "0 direct callers.".to_string(),
            evidence: Some(serde_json::json!({ "count": 0, "items": [] })),
        };
        let out = method_response(vec![absent]).render_human(true);
        assert!(
            !out.contains("unknown —"),
            "an absent field renders nothing:\n{out}"
        );
    }

    #[test]
    fn explain_callees_render_the_inferred_and_unresolved_remainder_lines() {
        let callees = ExplainSignal {
            code: "EXPLAIN_CALLEES".to_string(),
            summary: "1 direct callee.".to_string(),
            evidence: Some(serde_json::json!({
                "count": 1,
                "items": [{"name": "_get_dependencies_for_foreign_key", "module": "django/db/migrations"}],
                "inferred_count": 1,
                "inferred_by_basis": {"receiver_untyped_name_only": 1},
                "unresolved_from": {"count": 1, "by_basis": {"no_supporting_signal": 1}}
            })),
        };
        let out = method_response(vec![callees]).render_human(false);
        assert!(out.contains("Callees (1)"), "{out}");
        assert!(
            out.contains("+1 inferred (name-only) — investigate with rmap callees ListMixin.extend --include-inferred"),
            "{out}"
        );
        assert!(
            out.contains("1 unresolved call from this symbol (couldn't attribute: 1)"),
            "{out}"
        );
    }

    #[test]
    fn explain_full_lists_inferred_callers_marked() {
        let out = method_response(vec![remainder_callers()]).render_human(true);
        assert!(
            out.contains("_get_dependencies_for_model (django/db/migrations)  django/db/migrations/autodetector.py:1737 (inferred: name-only binding — investigate)"),
            "{out}"
        );
    }

    #[test]
    fn render_trust_states_the_inferred_calls_not_counted() {
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_TRUST".to_string(),
            summary: "Trust info.".to_string(),
            evidence: Some(serde_json::json!({
                "call_resolution_rate": 0.25,
                "call_graph_reliability": "low",
                "enrichment_state": "not_applicable",
                "resolved_in_scope": 25,
                "in_scope_or_unclassified_total": 100,
                "inferred_calls": 10
            })),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains(
                "your code's calls 25% resolved (LOW); +10 inferred calls not counted as resolved"
            ),
            "{out}"
        );
    }

    // ── TEST-EDGE-SCOPE-1A (RG-REQ-001-L07): explain <file>'s test-status line ──

    fn file_identity(ev: serde_json::Value) -> ExplainResponse {
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_IDENTITY".to_string(),
            summary: "Identity: file target.".to_string(),
            evidence: Some(ev),
        })];
        r
    }

    #[test]
    fn render_file_target_states_undetermined_test_status() {
        let r = file_identity(serde_json::json!({
            "target_kind": "file", "path": "util/testutil.cc", "language": "cpp",
            "symbol_count": 3, "test_status": "undetermined", "test_status_word": "testutil"
        }));
        let out = r.render_human(false);
        let lines: Vec<&str> = out.lines().map(str::trim).collect();
        let sym = lines.iter().position(|l| *l == "Symbols: 3").expect(&out);
        assert_eq!(
            lines[sym + 1],
            "test status: can't determine — open it and look inside",
            "the line follows `Symbols: N`:\n{out}"
        );
    }

    #[test]
    fn render_file_target_without_test_status_is_unchanged() {
        let ev = serde_json::json!({
            "target_kind": "file", "path": "db/db_impl.cc", "language": "cpp", "symbol_count": 3
        });
        let out = file_identity(ev).render_human(false);
        assert!(!out.contains("test status"), "{out}");
        assert!(
            out.contains("Language: cpp") && out.contains("Symbols: 3"),
            "{out}"
        );
    }

    #[test]
    fn render_tracked_only_file_target_states_undetermined_test_status() {
        // A tracked file with no FILE node: no language, no symbol count.
        let mut r = file_identity(serde_json::json!({
            "target_kind": "file", "path": "integration-tests/pom.xml",
            "test_status": "undetermined", "test_status_word": "tests"
        }));
        r.focus.input = Some("integration-tests/pom.xml".to_string());
        r.focus.resolved_path = Some("integration-tests/pom.xml".to_string());
        let out = r.render_human(false);
        assert!(
            out.contains("Target: integration-tests/pom.xml (file)"),
            "{out}"
        );
        assert!(
            out.lines()
                .any(|l| l.trim() == "test status: can't determine — open it and look inside"),
            "{out}"
        );
        assert!(
            !out.contains("Language:") && !out.contains("Symbols:"),
            "{out}"
        );
    }

    // ── TEST-EDGE-SCOPE-1B: D-TESB-17 (rows U9, W2) on explain's Import-cycles block ──

    #[test]
    fn explain_import_cycles_partitioned_payload_without_excluded_cycles_states_them_unreadable() {
        let mut ev = partitioned_cycles_evidence(0, serde_json::json!([]));
        ev.as_object_mut().unwrap().remove("excluded_cycles");
        let mut r = minimal_response();
        r.signals = vec![leaf(ExplainSignal {
            code: "EXPLAIN_CYCLES".to_string(),
            summary: "0 import cycles.".to_string(),
            evidence: Some(ev),
        })];
        let out = r.render_human(false);
        assert!(
            out.contains(&format!(
                "  - {}",
                crate::presentation::import_partition::PARTITION_UNREADABLE
            )),
            "{out}"
        );
        assert!(!out.contains("only through"), "{out}");
        assert!(!out.contains("can't be determined"), "{out}");
    }

    #[test]
    fn explain_import_cycles_wrong_typed_excluded_cycles_states_them_unreadable() {
        for bad in [
            serde_json::json!("none"),
            serde_json::json!([{"members": [1], "flags": ["include_tests"], "contains_shown": []}]),
        ] {
            let mut ev = partitioned_cycles_evidence(0, serde_json::json!([]));
            ev["excluded_cycles"] = bad.clone();
            let mut r = minimal_response();
            r.signals = vec![leaf(ExplainSignal {
                code: "EXPLAIN_CYCLES".to_string(),
                summary: "0 import cycles.".to_string(),
                evidence: Some(ev),
            })];
            let out = r.render_human(false);
            assert!(
                out.contains(crate::presentation::import_partition::PARTITION_UNREADABLE),
                "{bad}: {out}"
            );
            assert!(!out.contains("only through"), "{out}");
        }
    }
}
