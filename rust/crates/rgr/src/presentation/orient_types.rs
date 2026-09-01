//! Deserialized response DTOs for the `orient` command.
//!
//! Split out of `orient.rs` to keep each module under the 500-line structural
//! guardrail (review-1 §3). PURE type definitions — the rendering `impl OrientResponse`
//! blocks stay in `orient.rs` / `orient_sections.rs` / `orient_reliability*.rs` /
//! `orient_guidance.rs` / `orient_seg2.rs` (inherent impls may span modules within the
//! crate). `orient.rs` re-exports these (`pub use`) so `presentation::orient::<Type>`
//! paths across the crate stay stable — pure relocation, no behavior changed.

use repo_graph_agent::dto::{IndexDrift, ParseStatus};
use repo_graph_classification::measurement_coverage::MeasurementCoverageBlock;
use repo_graph_coherence::CoherenceEnvelope;
use serde::Deserialize;

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
    /// the envelope ROOT `trust` (rendered separately by [`super::orient::render_orient_envelope`]).
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
    /// ORIENT-SEGMENT-2 §2.1: the directory-group fan-in fallback the daemon injects
    /// ONLY when the package-group topology collapsed to one dominant group (django's
    /// "1 package group: ."). Absent on the wire otherwise — a non-collapsed orient is
    /// byte-identical, so this stays `None` and renders nothing (leveldb's gold
    /// standard). Rendered as the promoted "Directory groups (no manifest topology at
    /// this depth)" section at medium and up.
    // `pub(crate)`: the referenced `orient_seg2` types are crate-internal (operator
    // ruling 2), so this field cannot be more visible than they are.
    #[serde(default)]
    pub(crate) directory_group_fallback: Option<super::orient_seg2::DirectoryGroupFallback>,
    /// ORIENT-SEGMENT-2 §2.5: the HTTP surface architecture the daemon injects from
    /// the HSC-1 unified read. Present only when the repo HAS HTTP surfaces (> 0) or
    /// the union read failed (then `unavailable` is set). Absent on the wire for a
    /// non-HTTP repo — no headline line, byte-identical.
    // `pub(crate)`: see `directory_group_fallback` — crate-internal `orient_seg2` type.
    #[serde(default)]
    pub(crate) http_surfaces: Option<super::orient_seg2::HttpSurfaces>,
    /// MODULE-EDGES-1 §2.3: the top-3 cross-module dependency edges the daemon injects
    /// from the SAME module dependency graph `modules deps`/`modules list` serve.
    /// Present only when the repo HAS cross-module edges (or the graph read failed →
    /// `unavailable`). Absent on the wire for a repo without them — no headline line,
    /// byte-identical.
    // `pub(crate)`: see `directory_group_fallback` — crate-internal `orient_seg2` type.
    #[serde(default)]
    pub(crate) top_module_edges: Option<super::orient_seg2::TopModuleEdges>,
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
    /// EMBED-SEED-IMPL-1 (spec §8.2 Group A): the previously-empty `candidates` list
    /// the semantic fallback tier fills on a `no_match`. Kept as raw `Value` (the
    /// same idiom `find`/Group-B rendering uses) so the shared honesty-preserving
    /// candidate formatter reads the additive `source`/`score`/`module`/`next` fields
    /// directly; a resolved/ambiguous focus carries no `source:"embedding"` candidate,
    /// so this stays inert there (byte-parity).
    #[serde(default)]
    pub candidates: Vec<serde_json::Value>,
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
    /// The daemon-attached per-cause reasons (agent `Limit.reasons`, spec §8.3).
    /// EMBED-SEED-IMPL-1 review-9 #2: the human render MUST surface the specific
    /// cause (e.g. "no local embedding model reachable", "N files changed since
    /// last embed") — collapsing it into the generic `summary` mislabels a dead
    /// endpoint as a generic "hints unavailable". `#[serde(default)]`: an old daemon
    /// (or any limit without reasons) omits the field ⇒ empty, no cause line.
    #[serde(default)]
    pub reasons: Vec<String>,
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
