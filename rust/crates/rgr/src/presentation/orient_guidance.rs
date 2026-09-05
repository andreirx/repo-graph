//! Guidance / limits / remaining-signals section renderers for the `orient` command.
//!
//! Split out of `orient_sections.rs` to keep each module under the 500-line
//! structural guardrail (review-1 §3) — the `orient.rs` / `orient_sections.rs` split
//! idiom, extended. A continuation `impl OrientResponse` block (inherent impls may
//! span modules within the crate). Pure relocation — no behavior changed. It owns the
//! non-headline signal list, the cycle-anchor formatter (shared with the headline
//! `cycles_docs_line`), and the limits / semantic-fallback / next-steps renderers.

use super::orient::{OrientResponse, Signal};
use super::{bullet, heading, DisplaySeverity};

/// Headline signal codes — the load-bearing facts the dense headline synthesizes (the
/// presentation mirror of the agent's `HEADLINE_SIGNAL_CODES`). Kept here so the
/// detail-section renderer can EXCLUDE them (they already surfaced in the headline)
/// when listing the remaining signals at `--full`.
const HEADLINE_CODES: &[&str] = &[
    "MODULE_SUMMARY",
    "HIGH_COMPLEXITY",
    "IMPORT_CYCLES",
    "GATE_FAIL",
    "GATE_INCOMPLETE",
    "BOUNDARY_VIOLATIONS",
];

impl OrientResponse {
    /// The remaining (non-headline) signals, grouped by severity — shown
    /// at `--full` so the complete signal set is preserved (the headline
    /// already covered the load-bearing codes).
    pub(super) fn other_signals_section(&self) -> String {
        let others: Vec<&Signal> = self
            .signals
            .iter()
            .map(|leaf| &leaf.value)
            .filter(|s| !HEADLINE_CODES.contains(&s.code.as_str()))
            .collect();
        if others.is_empty() {
            return String::new();
        }

        let mut out = heading("Other signals");
        for sev in [
            DisplaySeverity::High,
            DisplaySeverity::Medium,
            DisplaySeverity::Low,
        ] {
            for s in others
                .iter()
                .filter(|s| DisplaySeverity::parse(&s.severity) == sev)
            {
                out.push_str(&bullet(&s.summary));
            }
        }
        out
    }

    /// Format a cycle's VERIFIED walk as "A -> B -> C -> ... -> A", or `None` when the carried
    /// `walk` is not a well-formed ordered ring. `pub(super)` so the headline `cycles_docs_line`
    /// (in `orient_sections`) shares it across the module split.
    ///
    /// COHERENCE-3 (§2.1): `walk` is the REAL directed ring the shared `cycle_walk` kernel found
    /// over the cycle's true import edges (carried on the `walk` leaf), in ring order — NOT the
    /// lexically-sorted member set. So the arrows drawn here are real edges; the closing
    /// `-> {first}` is the ring's back-edge. The `first 3 -> … -> last -> first` truncation for a
    /// long ring hides intermediate REAL edges but never invents one.
    ///
    /// COHERENCE-3 review-1 #1 (STANDING HONESTY RULE #1): STRICT validation. The producer
    /// (`agent_cycle_labeling::label_module_cycles` via `find_cycle_walk`) only ever emits a ring of
    /// ≥2 non-empty DISPLAY strings, or `None` (which serializes as an absent/`null` leaf the caller
    /// routes to the unordered form). So ANY non-string element, ANY empty string, or fewer than 2
    /// members reaching here is wire/schema DRIFT, not a walk — return `None` so the caller makes the
    /// unknown VISIBLE with its reason, NEVER a fabricated ring. The prior `filter_map(as_str)`
    /// silently dropped non-strings, turning a two-element `["A", 42]` into the invented self-cycle
    /// `A -> A`; that is exactly the fabrication this rejects.
    pub(super) fn format_cycle_anchor(&self, walk: &[serde_json::Value]) -> Option<String> {
        let mut names: Vec<&str> = Vec::with_capacity(walk.len());
        for m in walk {
            let s = m.as_str()?; // non-string element => drift => None (no silent drop)
            if s.is_empty() {
                return None; // empty display name => drift => None
            }
            names.push(s);
        }
        if names.len() < 2 {
            // A real directed ring closes over ≥2 distinct members (a self-import is not a cycle
            // edge). A one-element walk is the fabricated `A -> A` the reviewer flagged.
            return None;
        }

        let chain = if names.len() <= 4 {
            // Show full chain
            let mut chain = names.join(" -> ");
            chain.push_str(&format!(" -> {}", names[0]));
            chain
        } else {
            // Truncate: first 3 -> ... -> last -> first
            let mut chain = names[..3].join(" -> ");
            chain.push_str(" -> ...");
            chain.push_str(&format!(" -> {} -> {}", names[names.len() - 1], names[0]));
            chain
        };
        Some(chain)
    }

    pub(super) fn render_limits(&self) -> String {
        // EMBED-SEED-IMPL-1: the SEMANTIC_FALLBACK[_UNAVAILABLE] limit is rendered by
        // the dedicated top-of-output semantic section at EVERY depth
        // (`render_semantic_fallback`); it is NOT rendered here. When it is the ONLY
        // limit, emit NOTHING — never a bare "Limits" heading with no items (that
        // spurious empty section otherwise appears on `no_match --full`, and would
        // break resolved/ambiguous byte-parity if a semantic limit ever rode along).
        let renderable: Vec<_> = self
            .limits
            .iter()
            .filter(|l| !l.code.starts_with("SEMANTIC_FALLBACK"))
            .collect();
        if renderable.is_empty() {
            return String::new();
        }
        let mut out = heading("Limits");
        for limit in renderable {
            out.push_str(&bullet(&limit.summary));
        }
        out
    }

    /// EMBED-SEED-IMPL-1 (spec §8.2 Group A): render the semantic fallback tier for
    /// HUMAN mode — the labeled Layer-3 candidate list (or the honest degraded/
    /// known-zero line) that a `no_match` orient/explain now carries. Rendered at
    /// EVERY depth (the candidates ARE the load-bearing answer on a no-match), right
    /// after the focus line. Returns empty for any resolved/ambiguous focus so those
    /// remain byte-identical (the tier is unreachable there); empty too when a
    /// no-match carries no seed candidates AND no seed limit (an old daemon / seeding
    /// never consulted) — today's output untouched.
    pub(super) fn render_semantic_fallback(&self) -> String {
        // Only the deterministic-zero branch (§8.1) — never resolved/ambiguous.
        if self.focus.resolved || self.focus.reason.as_deref() != Some("no_match") {
            return String::new();
        }
        // Labeled embedding candidates only (a deterministic ambiguity candidate has no
        // `source`); on a no-match the tier is the only candidate producer, but this
        // guard keeps the render honest regardless.
        let embedding: Vec<&serde_json::Value> = self
            .focus
            .candidates
            .iter()
            .filter(|c| c.get("source").and_then(|s| s.as_str()) == Some("embedding"))
            .collect();

        // The honesty header: the SEMANTIC_FALLBACK limit's fixed summary (§8.2). The
        // degraded/known-zero line rides SEMANTIC_FALLBACK_UNAVAILABLE / SEMANTIC_FALLBACK
        // with no candidates. Read it from the limits the daemon attached (never fabricate).
        let semantic_limit = self
            .limits
            .iter()
            .find(|l| l.code.starts_with("SEMANTIC_FALLBACK"));

        if embedding.is_empty() {
            // No candidates: honest degraded / known-zero line (if the tier was
            // consulted), WITH the specific cause from `reasons` (review-9 #2 — a
            // dead endpoint reads "no local embedding model reachable", not the
            // generic summary). Nothing at all ⇒ today's output (old daemon).
            return match semantic_limit {
                Some(limit) => crate::presentation::seed::render_semantic_header(
                    &limit.summary,
                    &limit.reasons,
                ),
                None => String::new(),
            };
        }

        // Fired: the honesty header + the per-cause reasons (which carry the model id
        // and, when present, the stale-subset "N files changed since last embed"
        // detail — review-9 #2), then the labeled candidate list.
        let mut out = match semantic_limit {
            Some(limit) => {
                crate::presentation::seed::render_semantic_header(&limit.summary, &limit.reasons)
            }
            None => "Semantic hints: No exact match — the candidates below are Layer-3 embedding hints, not resolved facts.\n".to_string(),
        };
        for (i, c) in embedding.iter().enumerate() {
            // Group A's `FocusCandidate` serializes the path under `file` — the shared
            // formatter (with Group B) reads that field; honesty rules preserved.
            out.push_str(&format!("  {}. ", i + 1));
            out.push_str(&crate::presentation::seed::render_candidate_body(c, "file"));
        }
        out
    }

    pub(super) fn render_next_steps(&self) -> String {
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
