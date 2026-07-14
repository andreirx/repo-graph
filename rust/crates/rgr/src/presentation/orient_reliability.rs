//! Verbose per-axis "Degradation" rendering for the `orient` command.
//!
//! Split out of `orient_sections.rs` to keep each module under the 500-line
//! structural guardrail (ORIENT-DENSITY-1 review-1 #3) — the `orient.rs` /
//! `orient_sections.rs` / `orient_tests.rs` split idiom, extended. This is a
//! THIRD `impl OrientResponse` block (inherent impls may span modules within the
//! crate); it owns the two `--full` reliability blocks: the band-gated
//! `Degradation` block (the machine-reason → human-prose expansion of each
//! degraded reliability axis) and the band-INDEPENDENT `External calls` context
//! block (`render_external_coverage`).
//!
//! The COMPRESSED one-line reliability caveat that leads the dense headline
//! stays in `orient_sections.rs` (`reliability_caveat_line`); this module is the
//! expanded depth shown at `large`/`--full`.
//!
//! RELIABILITY-REFRAME-1: the CALL-GRAPH axis renders in the reader's frame —
//! "your code's calls M% resolved (LOW)" — NOT "Call-graph reliability is LOW",
//! which graded repo-graph's own pipeline. The external coverage map (WHERE the
//! out-of-scope calls go) is reader CONTEXT, not a grade, so it renders in its own
//! `External calls` section independent of the band (review-2 §1), never nested in
//! the band-gated Degradation block. All reader-frame wording + the derivation come
//! from the ONE shared projection [`repo_graph_agent::reliability`]. Import-graph /
//! change-impact stay axis-framed: those describe the reader's own import graph /
//! change-impact surface, not our call resolution.

use repo_graph_agent::reliability;

use super::orient::{OrientResponse, ReliabilityAxis, TrustOverlay};
use super::{bullet_list, heading};

/// Cap on named external receiver targets in orient's compact `--full` coverage
/// map; the remainder summarise as "+N more" (trust's Likely-External Receiver
/// Calls section carries the full list).
const EXTERNAL_MAP_LIMIT: usize = 5;

impl OrientResponse {
    /// The full per-axis "Degradation" block (call-graph / import-graph /
    /// change-impact), shown at `large` / `--full`. Empty when nothing is
    /// degraded. Renders the new `reliability` structure when present, else the
    /// legacy `TrustOverlay` fields (backward compatibility).
    pub(super) fn render_degradation(&self, trust: &TrustOverlay) -> String {
        let mut items: Vec<String> = Vec::new();

        // Render from new reliability structure if present
        if let Some(reliability) = &trust.reliability {
            if let Some(cg) = &reliability.call_graph {
                if cg.level != "HIGH" {
                    items.push(self.format_reliability_axis("Call-graph", cg));
                    // RELIABILITY-REFRAME-1 (review-2 §1): the external COVERAGE MAP used to sit
                    // here, gated on the call-graph band being non-HIGH. That made reader CONTEXT
                    // (where the out-of-scope calls go) conditional on a GRADE — slice §1.3 says the
                    // coverage map is context, not a grade. It now renders band-independently in its
                    // own `External calls` section (`render_external_coverage`), so it stays visible
                    // even when the in-scope band is HIGH. This block keeps ONLY the degradation axis.
                }
            }
            if let Some(ig) = &reliability.import_graph {
                if ig.level != "HIGH" {
                    items.push(self.format_reliability_axis("Import-graph", ig));
                }
            }
            if let Some(ci) = &reliability.change_impact {
                if ci.level != "HIGH" {
                    items.push(self.format_reliability_axis("Change-impact", ci));
                }
            }
        } else {
            // Legacy fallback: use old fields (RELIABILITY-REFRAME-1: reader-frame label,
            // same in-scope `call_resolution_rate` value).
            if let Some(rate) = trust.call_resolution_rate {
                if rate < 0.95 {
                    items.push(reliability::resolved_phrase_pct(rate * 100.0));
                }
            }
            if let Some(level) = &trust.call_graph_reliability {
                if level != "high" {
                    items.push(format!("your code's call resolution is {}", level));
                }
            }
            // Caveats from legacy path
            for caveat in &trust.caveats {
                items.push(caveat.clone());
            }
        }

        if items.is_empty() {
            return String::new();
        }

        let mut out = heading("Degradation");
        out.push_str(&bullet_list(&items));
        out
    }

    /// The `External calls` context block, shown at `large` / `--full`
    /// (RELIABILITY-REFRAME-1 review-2 §1).
    ///
    /// This is reader CONTEXT — WHERE the reader's out-of-scope calls go — NOT a grade, so it
    /// renders INDEPENDENT of the call-graph band (visible even when the in-scope band is HIGH).
    /// It used to live inside `render_degradation`'s non-HIGH branch, which made context
    /// conditional on the grade (slice §1.3 forbids that). Empty ONLY when there are no calls at
    /// all; when the heuristic identified zero externals but calls exist, the share line reads
    /// "none identified (heuristic)" — a heuristic FINDING, never a measured absence and never a
    /// fabricated "0% external" (review-3 §2). The share line, the named map, and BOTH EY1-A
    /// heuristic bases all come from the ONE shared projection (`call_reliability_view`, band
    /// `None`) — the same `trust`/`check` consume — so orient's named coverage map cannot fork
    /// from trust's.
    pub(super) fn render_external_coverage(&self) -> String {
        let Some(view) = self.call_reliability_view(None) else {
            return String::new();
        };
        let mut items: Vec<String> = Vec::new();
        if let Some(line) = view.external_line() {
            items.push(line);
        }
        if let Some(map) = view.named_coverage_map_line(EXTERNAL_MAP_LIMIT) {
            items.push(map);
            // The named map is a HEURISTIC (receiver type from a language-server hover;
            // externality from a static name-set, not compiler-verified). Both distinct EY1-A
            // bases ride the map — compactly, from the SAME shared vocabulary. Pushed only with
            // the map, so the basis line can never render orphaned.
            items.push(reliability::COMPACT_HEURISTIC_BASES.to_string());
        }
        // review-3 §2: the conservative-rate caveat, from the SAME shared helper the compressed
        // headline uses ([`Self::material_unclassified_caveat`]) — ONE home, so headline and
        // section cannot fork. It belongs here since it is precisely "some unclassified calls
        // could be external."
        if let Some(caveat) = self.material_unclassified_caveat(Some(&view)) {
            items.push(caveat);
        }
        if items.is_empty() {
            return String::new();
        }
        let mut out = heading("External calls");
        out.push_str(&bullet_list(&items));
        out
    }

    /// Format a reliability axis as reader-frame prose.
    ///
    /// The CALL-GRAPH axis speaks the reader's frame ("your code's calls M%
    /// resolved (LOW) — …") via [`Self::call_graph_degradation_line`]. Import-graph
    /// / change-impact keep the axis frame — they describe the reader's own import
    /// graph / change-impact surface, not repo-graph's call resolution.
    fn format_reliability_axis(&self, name: &str, axis: &ReliabilityAxis) -> String {
        if name == "Call-graph" {
            return self.call_graph_degradation_line(axis);
        }

        let level = &axis.level;
        if axis.reasons.is_empty() {
            return format!("{} reliability is {} on this repo. Do not use for safety-critical decisions without verification.", name, level);
        }

        // Convert machine reasons to human prose via the ONE shared humanizer
        // (RELIABILITY-REFRAME-1 — was a near-duplicate of trust's copy).
        let human_reasons: Vec<String> = axis
            .reasons
            .iter()
            .map(|r| reliability::humanize_reason(r))
            .collect();

        format!(
            "{} reliability is {} ({})",
            name,
            level,
            human_reasons.join("; ")
        )
    }

    /// The call-graph axis in the reader's frame: "your code's calls M% resolved
    /// (LOW)", plus any NON-rate downgrade reasons (registry / alias / entrypoint
    /// suspicions) as reader-frame prose. The `call_resolution_rate=` reason is
    /// folded into the phrase, so the rate is never stated twice. Prefers the ONE
    /// shared projection built from the overlay's call-coverage COUNTS; falls back
    /// to the rate parsed from the reason when the overlay predates `call_coverage`.
    fn call_graph_degradation_line(&self, axis: &ReliabilityAxis) -> String {
        let phrase = if let Some(view) = self.call_reliability_view(Some(&axis.level)) {
            view.resolved_with_band()
        } else if let Some(pct) = self.call_resolution_pct(axis) {
            reliability::resolved_phrase_with_band(pct, &axis.level)
        } else {
            format!("your code's call resolution is {}", axis.level)
        };
        let extra: Vec<String> = axis
            .reasons
            .iter()
            .filter(|r| !r.starts_with("call_resolution_rate="))
            .map(|r| reliability::humanize_reason(r))
            .collect();
        if extra.is_empty() {
            phrase
        } else {
            format!("{} — {}", phrase, extra.join("; "))
        }
    }
}
