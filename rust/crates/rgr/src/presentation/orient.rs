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

use repo_graph_coherence::CoherenceEnvelope;
use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line};

/// How much DEPTH the dense `orient` renders (ORIENT-DENSITY-1 §5).
///
/// A budget is a density contract: every tier leads with the same
/// dense, load-bearing HEADLINE (named structure, complexity centers,
/// cycles, docs, and one reliability caveat); the tier only trades how
/// much DEPTH is appended below it. It NEVER strips the headline down
/// to thin meta. Mirrors the CLI `--budget` / `--full` selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientDepth {
    Small,
    Medium,
    Large,
    Full,
}

impl OrientDepth {
    /// Map the CLI budget token (`small|medium|large|full`) to a depth.
    /// Unknown tokens fall back to `Small` (the safe dense default).
    pub fn from_budget(budget: &str) -> Self {
        match budget {
            "medium" => Self::Medium,
            "large" => Self::Large,
            "full" => Self::Full,
            _ => Self::Small,
        }
    }

    /// `true` for the tiers that append the full detail breakdown
    /// (per-axis reliability, module file-counts, the COMPLETE complexity
    /// centers, the remaining signals, the full certainty/provenance block).
    /// `large` and `--full` both render the complete detail. `pub(super)` so the
    /// `orient_sections` renderers can drop the headline's "+N more" pointer when
    /// the full breakdown section follows.
    pub(super) fn shows_full_detail(self) -> bool {
        matches!(self, Self::Large | Self::Full)
    }

    /// `true` for tiers above `small`, where the mid-depth sections
    /// (limits, next steps) are appended below the headline.
    fn shows_detail(self) -> bool {
        !matches!(self, Self::Small)
    }

    /// How many NAMED modules the structure headline lists.
    /// `pub(super)` so the `orient_sections` renderers can read it.
    pub(super) fn module_name_cap(self) -> usize {
        match self {
            Self::Small => 8,
            Self::Medium => 16,
            Self::Large | Self::Full => usize::MAX,
        }
    }

    /// How many complexity-center files the HEADLINE line names. This stays
    /// BOUNDED at every tier — a headline is a dense one-liner, never a dump of
    /// hundreds of symbols. At `large`/`--full` the COMPLETE above-threshold set
    /// rides the dedicated `complexity_breakdown_section` instead (review-1 #2),
    /// so the headline cap does not need to grow to MAX.
    /// `pub(super)` so the `orient_sections` renderers can read it.
    pub(super) fn complexity_center_cap(self) -> usize {
        match self {
            Self::Small => 3,
            Self::Medium | Self::Large | Self::Full => 5,
        }
    }
}

/// HONEST-DEGRADATION-IMPL-2 (D3): a reader-context gloss for the serving `AnswerClass`, faithful to its
/// `repo-graph-trust-model` doc-comments — it describes the answer's required-INPUT basis + serving epoch,
/// NOT a global certainty. Empty for an unrecognized class (the bare scoped `answer basis {class}` then
/// stands on its own). Deliberately NOT a "served from snapshot" gloss (the ratified D3 correction).
fn answer_class_phrase(class: &str) -> &'static str {
    match class {
        "exact" => "required inputs complete for this query",
        "partial" => "some required inputs incomplete",
        "stale" => "served from last-good epoch (refresh in flight)",
        "unavailable" => "not answerable from current state",
        _ => "",
    }
}

/// Render orient's coherence-wrapped daemon response, DENSE (ORIENT-DENSITY-1).
///
/// The body (`render_human`) now LEADS with the dense, NAMED, load-bearing
/// orientation and trades DEPTH by budget. This wrapper appends the SERVING /
/// provenance footer from the ROOT axes (`trust` / `freshness` /
/// `provenance.source`) — HONEST-DEGRADATION-IMPL-2 (D3): these are
/// serving/answer-basis facts (the answer's required-input completeness +
/// freshness + sources), NOT global certainty; the relationship reliability
/// rides the separate Degradation section from `value.trust_briefing`. The
/// footer is COMPRESSED to one honest line at small/medium budget and expands
/// to the full provenance block at large/`--full` (§5: budget trades depth, the
/// provenance is never dropped, only condensed).
pub fn render_orient_envelope(
    env: &CoherenceEnvelope<OrientResponse>,
    depth: OrientDepth,
) -> String {
    let mut out = env.value.render_human(depth);

    let class = format!("{:?}", env.trust.class).to_lowercase();
    let freshness = format!("{:?}", env.freshness).to_lowercase();
    let sources: Vec<String> = env
        .provenance
        .source
        .iter()
        .map(|s| format!("{s:?}").to_lowercase())
        .collect();

    // HONEST-DEGRADATION-IMPL-2 (D3): this footer is SERVING/provenance, not global certainty. The
    // answer-class is `AnswerClass` — "every required basis is complete for the query and data is fresh"
    // for `Exact` (`repo-graph-trust-model`), i.e. the answer's INPUT BASIS + freshness, NOT the
    // call/import reliability (which rides the separate Degradation/Reliability sections). Heading +
    // scoped "answer basis {class}" so it never reads as a bare global "exact".
    let phrase = answer_class_phrase(&class);
    if depth.shows_full_detail() {
        // Full serving/provenance block (large / --full).
        out.push_str("\n\n");
        out.push_str(&heading("Serving"));
        if phrase.is_empty() {
            out.push_str(&bullet(&format!(
                "answer basis {class}; freshness {freshness}"
            )));
        } else {
            out.push_str(&bullet(&format!(
                "answer basis {class} ({phrase}); freshness {freshness}"
            )));
        }
        if !sources.is_empty() {
            out.push_str(&bullet(&format!("sources: {}", sources.join(", "))));
        }
        if let Some(reason) = env.provenance.fallback_reason {
            out.push_str(&bullet(&format!("fallback: {}", reason.as_str())));
        }
    } else {
        // Compressed one-line serving posture (small / medium) — honest, not dropped.
        let src = if sources.is_empty() {
            String::new()
        } else {
            format!(" · sources: {}", sources.join(", "))
        };
        out.push_str(&format!(
            "\n\nServing: answer basis {class}, freshness {freshness}{src}"
        ));
    }
    out.trim_end().to_string()
}

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized orient response from daemon.
///
/// This struct captures the subset of daemon DTO fields needed for
/// human rendering. Fields like `schema` and `command` are not included
/// because they are internal envelope scaffolding.
#[derive(Debug, Deserialize)]
pub struct OrientResponse {
    pub repo: String,
    /// Human-readable repo name for CLI display.
    /// Populated by daemon from registry alias or path basename.
    /// When present, prefer this over `repo` (which is internal UID).
    #[serde(default)]
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub snapshot: String,
    pub focus: Focus,
    pub confidence: String,
    #[serde(default)]
    pub documentation: Option<DocumentationSection>,
    /// ORIENT-LIVEGRAPH-IMPL: each signal is now a LEAF `CoherenceEnvelope<Signal>` (contract D7) — the
    /// inner `Signal` is pristine; provenance/trust/freshness ride in the wrapper siblings. The renderer
    /// reads each `.value`.
    #[serde(default)]
    pub signals: Vec<CoherenceEnvelope<Signal>>,
    #[serde(default)]
    pub limits: Vec<Limit>,
    #[serde(default)]
    pub next: Vec<NextAction>,
    #[serde(default)]
    pub truncated: bool,
    /// D-ORIENT-6 = O2: the degraded-state trust briefing overlay, now carried on `value.trust_briefing`
    /// (renamed from the old top-level `trust` key). Present only when degraded. The certainty AXES are
    /// the envelope ROOT `trust` (rendered separately by [`render_orient_envelope`]).
    #[serde(default)]
    pub trust_briefing: Option<TrustOverlay>,
    /// HONEST-DEGRADATION-IMPL-2 (D5): the daemon's toolchain-aware honest next-action line, present only
    /// when relationship reliability is LOW. Rendered beneath the headline reliability caveat. `None`
    /// (absent on the wire) on a resolved repo or when no honest statement applies.
    #[serde(default)]
    pub relationship_next_action: Option<String>,
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
    /// Evidence payload - structure varies by signal code.
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
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
    pub reliability: Option<ReliabilitySection>,
    #[serde(default)]
    pub caveats: Vec<String>,
    // Legacy fields for backward compatibility
    #[serde(default)]
    pub call_graph_reliability: Option<String>,
    #[serde(default)]
    pub call_resolution_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ReliabilitySection {
    #[serde(default)]
    pub call_graph: Option<ReliabilityAxis>,
    #[serde(default)]
    pub import_graph: Option<ReliabilityAxis>,
    #[serde(default)]
    pub change_impact: Option<ReliabilityAxis>,
}

#[derive(Debug, Deserialize)]
pub struct ReliabilityAxis {
    pub level: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl OrientResponse {
    /// Render the DENSE orient response (ORIENT-DENSITY-1).
    ///
    /// Leads with the load-bearing, NAMED orientation (§3): high-severity
    /// alerts, then structure (repo · files · named modules), complexity
    /// centers (named files), cycles + docs, and ONE compressed
    /// reliability caveat. Budget trades DEPTH below that — `small` is the
    /// dense headline alone; `medium` adds limits + next steps; `large` /
    /// `--full` add the per-module breakdown, per-axis reliability, and the
    /// remaining signals. The headline is NEVER stripped to thin meta.
    pub fn render_human(&self, depth: OrientDepth) -> String {
        let mut out = String::new();

        // Non-repo / unresolved focus keeps an explicit line so the agent
        // sees what scope (or failure) it is looking at. Repo focus stays
        // implicit — the structure line names the repo.
        let focus_line = self.render_focus();
        if !focus_line.is_empty() {
            out.push_str(&focus_line);
        }

        // ── HEADLINE (every budget — the dense load-bearing set) ────
        for alert in self.headline_alerts() {
            out.push_str(&alert);
            out.push('\n');
        }
        out.push_str(&self.structure_line(depth));
        out.push('\n');
        if let Some(line) = self.complexity_line(depth) {
            out.push_str(&line);
            out.push('\n');
        }
        if let Some(line) = self.cycles_docs_line() {
            out.push_str(&line);
            out.push('\n');
        }
        if let Some(line) = self.reliability_caveat_line() {
            out.push_str(&line);
            out.push('\n');
        }
        // HONEST-DEGRADATION-IMPL-2 (D5): the toolchain-aware honest next-action, beneath the reliability
        // caveat (rendered at every budget — a next-action is load-bearing at any depth).
        if let Some(line) = &self.relationship_next_action {
            out.push_str(line);
            out.push('\n');
        }

        // ── DEPTH: mid sections (medium and up) ────────────────────
        if depth.shows_detail() {
            if !self.limits.is_empty() {
                out.push('\n');
                out.push_str(&self.render_limits());
            }
            if !self.next.is_empty() {
                out.push('\n');
                out.push_str(&self.render_next_steps());
            }
        }

        // ── DEPTH: full detail (large / --full) ────────────────────
        if depth.shows_full_detail() {
            // Package groups first (the Layer-0/1 directory TOPOLOGY the headline
            // leads with), then the declared/inferred module_candidates breakdown
            // — two separately-labelled notions (MODULE-MODEL-1).
            let pkg_groups = self.package_groups_section();
            if !pkg_groups.is_empty() {
                out.push('\n');
                out.push_str(&pkg_groups);
            }
            let breakdown = self.module_breakdown_section();
            if !breakdown.is_empty() {
                out.push('\n');
                out.push_str(&breakdown);
            }
            // The COMPLETE complexity centers (review-1 #2): the agent now
            // carries every above-threshold center in the evidence at
            // large/--full, so this section is the full set — no "+N more".
            let complexity = self.complexity_breakdown_section();
            if !complexity.is_empty() {
                out.push('\n');
                out.push_str(&complexity);
            }
            if let Some(trust) = &self.trust_briefing {
                let degradation = self.render_degradation(trust);
                if !degradation.is_empty() {
                    out.push('\n');
                    out.push_str(&degradation);
                }
            }
            let others = self.other_signals_section();
            if !others.is_empty() {
                out.push('\n');
                out.push_str(&others);
            }
        } else {
            // Small / medium: the headline is complete; point to the depth.
            out.push_str(
                "\n[--full for the complete breakdown; rmap hotspots / modules / cycles to drill down]\n",
            );
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
                // Repo-level focus — no path. Omit the line entirely as it adds nothing.
                if kind == "repo" {
                    String::new()
                } else {
                    kv_line("Focus", &format!("({})", kind))
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "orient_tests.rs"]
mod tests;
