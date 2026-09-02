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
//!   - your code's calls 78% resolved (below 85% target).
//!
//! Next steps
//!   - rmap check
//!   - rmap explain src/core/auth/session.ts
//! ```

use repo_graph_coherence::CoherenceEnvelope;

use crate::presentation::{bullet, heading, kv_line};

// ── Response Types ───────────────────────────────────────────────────────────
// The deserialized orient response DTOs live in `orient_types` (guardrail split,
// review-1 §3); re-exported here so `presentation::orient::<Type>` paths across the
// crate stay stable. The rendering `impl OrientResponse` blocks remain in this file
// and its section siblings.
pub use super::orient_types::{
    DocumentationSection, Focus, Limit, NextAction, OrientResponse, RelevantDoc, ReliabilityAxis,
    ReliabilitySection, Signal, TrustOverlay,
};

/// How much DEPTH the dense `orient` renders — the progressive-disclosure
/// ladder (ORIENT-DENSITY-1). Every tier leads with the same load-bearing
/// HEADLINE (named structure, complexity centers, cycles, docs, one reliability
/// caveat, the relationship next-action) and NEVER strips it to thin meta; the
/// tier only trades DEPTH below, each a genuine superset of the previous:
/// - `Small`  — the headline alone ("where to look first").
/// - `Medium` — + the scannable KEY STRUCTURE: package-group topology, the
///   declared/inferred module list, a complexity top-slice, limits/next-steps.
/// - `Large`  — + the DETAILED TABLES: a larger (capped) complexity table, the
///   per-axis reliability breakdown, the remaining signals.
/// - `Full`   — `large` with the complexity table uncapped (the only uncapped tier).
///
/// Mirrors the CLI `--budget` / `--full` selection.
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

    /// `true` for the tiers (`large` / `--full`) that append the DETAILED TABLES
    /// on top of medium's key structure — the per-axis reliability breakdown
    /// (`render_degradation`), the remaining non-headline signals, and the full
    /// serving footer. (The complexity cap is owned by `complexity_breakdown_cap`.)
    /// `pub(super)` so `render_orient_envelope` can gate the footer expansion.
    pub(super) fn shows_full_detail(self) -> bool {
        matches!(self, Self::Large | Self::Full)
    }

    /// `true` for `medium` and up, where the KEY-STRUCTURE sections append below
    /// the headline: limits/next-steps, the package-group topology, the module
    /// list, and the complexity table (capped per tier). `pub(super)` so
    /// `complexity_line` can drop its headline "+N more" once the dedicated
    /// complexity section follows (no double pointer).
    pub(super) fn shows_detail(self) -> bool {
        !matches!(self, Self::Small)
    }

    /// How many NAMED package groups the structure HEADLINE lists (§13 D7). A
    /// headline is a one-liner, so this is BOUNDED at every tier (even `--full`) —
    /// the overflow rides the "+N more" pointer at `small` and the dedicated
    /// package-group section at `medium`+. (Renamed from `module_name_cap`: the
    /// headline names package GROUPS, not the declared/inferred `module_candidates`
    /// notion — the very distinction MODULE-MODEL-1 draws.) `pub(super)` so the
    /// `orient_sections` renderers can read it.
    pub(super) fn package_group_name_cap(self) -> usize {
        match self {
            Self::Small => 8,
            Self::Medium => 16,
            Self::Large | Self::Full => 24,
        }
    }

    /// The per-tier render cap for the package-group SECTION (§13 D7), distinct
    /// from the headline cap above — the knob that keeps the primary orientation
    /// surface bounded on a monorepo: `medium` a top-slice, `large` a larger table.
    /// ORIENT-SEGMENT-2 §2.4 (operator ruling 2, 2026-08-28): `--full` means the
    /// COMPLETE breakdown the small-budget footer advertises, so `Full` UNCAPS the
    /// package-group section (`None`) — a >50-group repo's `--full` renders every
    /// group, no elision. `Large` keeps the bounded `Some(50)` table (the omitted
    /// groups ride the honest omission line → `stats --json` / `modules`; the
    /// COMPLETE size-DESC set always rides the JSON, so the cap never overclaims).
    /// `None` at `small` is unused — `small` never renders the section
    /// (`shows_detail()` gates it), the headline being the sole topology surface
    /// there.
    fn package_group_section_cap(self) -> Option<usize> {
        match self {
            Self::Small | Self::Full => None,
            Self::Medium => Some(20),
            Self::Large => Some(50),
        }
    }

    /// How many complexity-center files the HEADLINE line names — BOUNDED at
    /// every tier (a headline is a one-liner, never a dump). The fuller set rides
    /// the dedicated `complexity_breakdown_section` instead, so this cap need not
    /// grow to MAX. `pub(super)` so the `orient_sections` renderers can read it.
    pub(super) fn complexity_center_cap(self) -> usize {
        match self {
            Self::Small => 3,
            Self::Medium | Self::Large | Self::Full => 5,
        }
    }

    /// The per-tier render cap for the complexity-centers SECTION (distinct from
    /// the headline cap above) — the knob that makes the ladder progressive below
    /// the headline: `medium` a top-slice, `large` a larger table, `--full`
    /// uncapped. `None` = uncapped; `small` never renders the section (unused).
    /// Re-composition only: a PREFIX of the SAME `top_complex` evidence + a tail.
    fn complexity_breakdown_cap(self) -> Option<usize> {
        match self {
            Self::Small => None,
            Self::Medium => Some(10),
            Self::Large => Some(50),
            Self::Full => None,
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
    // INDEX-BASIS-1 (review-0 fix #2): the envelope `freshness` axis is the coherence
    // serving MEET (it can be `precisionpending` mid-refresh) — it KEEPS its own name.
    // The `parse` axis is a SEPARATE honest value (`value.parse_status`, from
    // `get_stale_files`), rendered only when the daemon attached it. The word
    // "fresh"/drift belongs ONLY to the basis/drift line below (from `IndexDrift`).
    let parse_clause = env.value.parse_status.as_ref().map(|p| p.footer_clause());
    let phrase = answer_class_phrase(&class);
    if depth.shows_full_detail() {
        // Full serving/provenance block (large / --full).
        out.push_str("\n\n");
        out.push_str(&heading("Serving"));
        let mut posture = if phrase.is_empty() {
            format!("answer basis {class}; freshness {freshness}")
        } else {
            format!("answer basis {class} ({phrase}); freshness {freshness}")
        };
        if let Some(parse) = &parse_clause {
            posture.push_str(&format!("; parse: {parse}"));
        }
        out.push_str(&bullet(&posture));
        if let Some(drift) = &env.value.index_drift {
            out.push_str(&bullet(&drift.describe()));
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
        // `None` here = the daemon attached NO `parse_status` (old daemon) → no
        // parse clause. A parse READ FAILURE is never None: it is
        // `ParseStatus::Unknown` and renders `parse: unknown (reason)`.
        let parse_seg = match &parse_clause {
            Some(p) => format!(", parse: {p}"),
            None => String::new(),
        };
        out.push_str(&format!(
            "\n\nServing: answer basis {class}, freshness {freshness}{parse_seg}{src}"
        ));
        // The basis/drift line always rides its OWN line — it is the load-bearing
        // "which commit do these facts describe, and how far have you moved" fact.
        if let Some(drift) = &env.value.index_drift {
            out.push('\n');
            out.push_str(&drift.describe());
        }
    }
    out.trim_end().to_string()
}

impl OrientResponse {
    /// Render the DENSE orient response as the progressive four-tier ladder — see
    /// [`OrientDepth`] for the per-tier contract. Every tier leads with the
    /// load-bearing, NAMED HEADLINE (§3: alerts, structure, complexity centers,
    /// cycles + docs, one reliability caveat, the relationship next-action) and
    /// trades only DEPTH below it. The headline is NEVER stripped to thin meta,
    /// and the honesty posture (reliability caveat, next-action, and — via the
    /// envelope wrapper — the serving footer) rides EVERY tier.
    pub fn render_human(&self, depth: OrientDepth) -> String {
        let mut out = String::new();

        // Non-repo / unresolved focus keeps an explicit line so the agent
        // sees what scope (or failure) it is looking at. Repo focus stays
        // implicit — the structure line names the repo.
        let focus_line = self.render_focus();
        if !focus_line.is_empty() {
            out.push_str(&focus_line);
        }

        // EMBED-SEED-IMPL-1 (spec §8.2 Group A): the semantic fallback tier's labeled
        // candidates (or the honest degraded line) on a `no_match`, at EVERY depth —
        // they are the load-bearing answer when deterministic resolution found nothing.
        // Empty (byte-identical to today) for any resolved/ambiguous focus.
        let semantic = self.render_semantic_fallback();
        if !semantic.is_empty() {
            out.push_str(&semantic);
        }

        // ── HEADLINE (every budget — the dense load-bearing set) ────
        for alert in self.headline_alerts() {
            out.push_str(&alert);
            out.push('\n');
        }
        out.push_str(&self.structure_line(depth));
        out.push('\n');
        // ORIENT-SEGMENT-2 §2.5: the HTTP architecture joins the headline where > 0.
        if let Some(line) = self.http_surfaces_line(depth) {
            out.push_str(&line);
            out.push('\n');
        }
        // MODULE-EDGES-1 §2.3: the top cross-module edges join the headline (the VISION
        // primary use case — "how modules relate / where the boundaries are" — on the
        // first-60-seconds surface). Present only where the repo HAS cross-module edges.
        if let Some(line) = self.top_module_edges_line(depth) {
            out.push_str(&line);
            out.push('\n');
        }
        if let Some(line) = self.complexity_line(depth) {
            out.push_str(&line);
            out.push('\n');
        }
        // METRIC-LANG-COVERAGE-1 (part A): the measurement-coverage caveat rides the
        // headline beside the complexity centers, at EVERY tier — an honesty caveat is
        // load-bearing at any depth, and it names the languages the ranking omits.
        if let Some(line) = self.measurement_coverage_caveat_line() {
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
        // CHECK-LANG-SPLIT-1 (§2 + ruling A): the per-language breakdown rides directly UNDER the blended
        // reliability caveat, so a mixed repo's reader sees which language carries the unresolved mass. The
        // daemon sets it whenever that call-graph caveat is present — a non-HIGH call-graph figure (LOW OR
        // MEDIUM) — and the repo is mixed; absent otherwise (HIGH / single-language output byte-identical).
        if let Some(line) = &self.reliability_by_language {
            out.push_str(line);
            out.push('\n');
        }
        // RECON-M-R3a (g1u, §5.3.2): the ADDITIVE reconciled union-call line beside the
        // pipeline reliability figures — rendered ONLY when the daemon attached the
        // coverage-labeled block (W-BOTH with a current measured ledger); absent otherwise
        // (zero-SCIP output byte-identical, R-0). Never replaces a pipeline figure.
        if let Some(line) = self
            .witnesses
            .as_ref()
            .and_then(crate::presentation::witnesses::g1u_line)
        {
            out.push_str(&line);
            out.push('\n');
        }
        // HONEST-DEGRADATION-IMPL-2 (D5): the toolchain-aware honest next-action, beneath the reliability
        // caveat (rendered at every budget — a next-action is load-bearing at any depth).
        if let Some(line) = &self.relationship_next_action {
            out.push_str(line);
            out.push('\n');
        }

        // ── DEPTH: KEY STRUCTURE (medium and up) ───────────────────
        // The scannable middle tier: limits/next-steps, package-group topology,
        // the module list, and a complexity top-slice — re-composed from the SAME
        // sections `large` uses, only capped tighter (see `complexity_breakdown_cap`).
        if depth.shows_detail() {
            // `render_limits` returns "" when the only limits are semantic (rendered
            // by the top section) — guard on the rendered content, not the raw list,
            // so a semantic-only limit set adds no empty "Limits" heading / blank line.
            let limits = self.render_limits();
            if !limits.is_empty() {
                out.push('\n');
                out.push_str(&limits);
            }
            if !self.next.is_empty() {
                out.push('\n');
                out.push_str(&self.render_next_steps());
            }
            // ORIENT-SEGMENT-2 §2.1: on package-group collapse the daemon injected the
            // directory-group fan-in view — PROMOTE it above the (degenerate) package
            // groups so the agent orients by real subsystems (django db/test/core/…).
            // Absent (`None`) on a non-collapsed repo, so nothing renders there.
            let dir_groups = self.directory_groups_section();
            if !dir_groups.is_empty() {
                out.push('\n');
                out.push_str(&dir_groups);
            }
            // Package groups (Layer-0/1 directory TOPOLOGY) then the declared/
            // inferred module_candidates breakdown — two labelled notions (MODULE-MODEL-1).
            let pkg_groups = self.package_groups_section(depth.package_group_section_cap());
            if !pkg_groups.is_empty() {
                out.push('\n');
                out.push_str(&pkg_groups);
            }
            let breakdown = self.module_breakdown_section();
            if !breakdown.is_empty() {
                out.push('\n');
                out.push_str(&breakdown);
            }
            // Complexity centers, capped per tier (top-slice / larger / uncapped);
            // the cap carries an honest "+N more — rmap hotspots" tail.
            let complexity = self.complexity_breakdown_section(depth.complexity_breakdown_cap());
            if !complexity.is_empty() {
                out.push('\n');
                out.push_str(&complexity);
            }
        }

        // ── DEPTH: DETAILED TABLES (large / --full) ────────────────
        // On top of medium's key structure: the per-axis reliability breakdown
        // and the remaining non-headline signals (complexity already expanded above).
        if depth.shows_full_detail() {
            if let Some(trust) = &self.trust_briefing {
                let degradation = self.render_degradation(trust);
                if !degradation.is_empty() {
                    out.push('\n');
                    out.push_str(&degradation);
                }
                // RELIABILITY-REFRAME-1 (review-2 §1): the external coverage map is reader CONTEXT,
                // not a grade — it renders in its OWN band-independent section, so it stays visible
                // even when the call-graph band is HIGH (unlike Degradation, which is band-gated).
                let external = self.render_external_coverage();
                if !external.is_empty() {
                    out.push('\n');
                    out.push_str(&external);
                }
            }
            let others = self.other_signals_section();
            if !others.is_empty() {
                out.push('\n');
                out.push_str(&others);
            }
        }

        // Small / medium have not yet reached the detailed tables — point there.
        if !depth.shows_full_detail() {
            out.push_str(
                "\n[--full for the complete breakdown; rmap hotspots / modules / cycles to drill down]\n",
            );
        } else if matches!(depth, OrientDepth::Full) && self.budget_saturated() {
            // ORIENT-SEGMENT-2 §2.4: a saturated ladder (repo smaller than the budget —
            // every section rendered complete, no elision) states so, instead of `--full`
            // being a silent byte-copy of `large` under a footer that promised "the
            // complete breakdown". `--full` now MEANS something on such repos.
            out.push_str("\n[budget not reached — output complete]\n");
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

// The progressive budget-ladder tests live in a sibling file (via `#[path]`) so
// neither test module grows past the >500-line structural guardrail (review-1).
#[cfg(test)]
#[path = "orient_density_tests.rs"]
mod density_tests;
