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

use repo_graph_agent::dto::{IndexDrift, ParseStatus};
use repo_graph_classification::measurement_coverage::MeasurementCoverageBlock;
use repo_graph_coherence::CoherenceEnvelope;
use serde::Deserialize;

use crate::presentation::{bullet, heading, kv_line};

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
    /// surface bounded on a monorepo: `medium` a top-slice, `large`/`--full` a
    /// larger table. Package groups stay BOUNDED at EVERY tier — the complexity
    /// table is the ONLY section `--full` uncaps (the enum contract above: "large
    /// with the complexity table uncapped"). Package groups are deliberately NOT
    /// uncapped at `--full`: on a 160k-file monorepo the group count scales with
    /// directories into the thousands — the exact overrun §13 D7 exists to bound —
    /// so dumping them all would defeat the bounded-human contract on the primary
    /// surface. `--full` therefore renders the SAME capped section as `large`; the
    /// omitted groups ride the honest omission line (→ `stats --json` / `modules`)
    /// and the COMPLETE size-DESC set always rides the JSON, so a cap never
    /// overclaims. `None` at `small` is unused — `small` never renders the section
    /// (`shows_detail()` gates it), the headline being the sole topology surface
    /// there.
    fn package_group_section_cap(self) -> Option<usize> {
        match self {
            Self::Small => None,
            Self::Medium => Some(20),
            Self::Large | Self::Full => Some(50),
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
    /// METRIC-LANG-COVERAGE-1 (part A): per-language complexity measurement coverage,
    /// present when orient renders complexity centers. Its honesty line (`caveat_line`)
    /// renders beside the complexity headline so the ranking never reads as repo-wide
    /// while a whole language is unmeasured (or states that coverage could not be read);
    /// it disappears by itself once every significant language is measured. Deserialized
    /// from the daemon's opaque JSON into the shared `classification` block (the daemon
    /// serialized the same `available`/`unavailable` block). `None` only when orient has
    /// no complexity centers to describe.
    #[serde(default)]
    pub measurement_coverage: Option<MeasurementCoverageBlock>,
    /// RECON-M-R3a (g1u): the daemon's ADDITIVE union-accounting call block (opaque JSON —
    /// rendered through the shared `presentation::witnesses` projection). Present ONLY in
    /// W-BOTH with a current measured ledger; absent on the wire otherwise (R-0).
    #[serde(default)]
    pub witnesses: Option<serde_json::Value>,
    /// INDEX-BASIS-1: the query-time working-tree drift the daemon attached onto
    /// `value` (git basis + how far the tree has moved). Rendered as the honest
    /// "index basis / drift" footer line. Absent on the wire only from an older
    /// daemon; then no drift line is shown.
    #[serde(default)]
    pub index_drift: Option<IndexDrift>,
    /// INDEX-BASIS-1 (review-0 fix #2): the honest parse axis (from
    /// `get_stale_files`), attached by the daemon. Rendered as the footer
    /// `parse: ok|N unparsed|unknown (reason)` — DISTINCT from the coherence
    /// envelope `freshness` meet. Absent on the wire only from an older daemon; then
    /// no parse clause is shown.
    #[serde(default)]
    pub parse_status: Option<ParseStatus>,
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
    // RELIABILITY-REFRAME-1 (RR1_BOUNDARY option A): the reader-frame call-coverage
    // facts the daemon now carries on the trust overlay. Reused verbatim (not mirrored)
    // from the producer so orient builds the SAME
    // `repo_graph_agent::reliability::CallReliabilityView` as trust/check — one shape,
    // no per-surface drift. Absent (`None`) on an overlay that predates the field.
    #[serde(default)]
    pub call_coverage: Option<repo_graph_trust::CallCoverage>,
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
            if !self.limits.is_empty() {
                out.push('\n');
                out.push_str(&self.render_limits());
            }
            if !self.next.is_empty() {
                out.push('\n');
                out.push_str(&self.render_next_steps());
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
