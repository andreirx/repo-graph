//! Presentation layer for surfaces commands.
//!
//! # CLI-OUT-4 Group 4
//!
//! Response DTOs and human renderers for:
//! - `surfaces list` — surface catalog
//! - `surfaces show` — surface detail
//!
//! ## Change Axis
//!
//! This file changes when:
//! - Surface catalog format changes
//! - Surface detail format changes
//! - Filter display changes
//! - Degradation messaging changes
//!
//! It does NOT change when:
//! - Module commands change (Groups 1-3)
//! - Boundaries commands change (Group 5)

use serde::Deserialize;

use super::http_boundary::{self, HttpBoundarySurfaceEntry};
use super::module_shared::format_count;

// =============================================================================
// SHARED TYPES
// =============================================================================

/// Degradation info when surfaces are not populated.
#[derive(Debug, Clone, Deserialize)]
pub struct DegradationInfo {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub feature: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub recommendation: String,
}

/// ZEROSTATE-SCOPE-1 §2.1/§2.2: the HTTP surface-detector coverage roster (additive) — the
/// ONE source for the `surfaces list`, `boundaries list`, and `boundaries summary`
/// zero-states (no second roster).
///
/// `http_detector_families` is BUILD-STATIC (the detectors this build ships, from
/// `repo_graph_repo_index::surface_coverage`). `material_gap` is PER-REPO: the daemon
/// (`surface_coverage_read`) names only THIS repo's materially-present languages/frameworks
/// the detectors cannot see — leveldb says its C/C++ truth, django keeps URLconf — so no
/// repo wears another's sentence. The zero-state renders these so the empty answer states
/// the TOOL's coverage instead of blaming the repo, never a totality claim. See
/// [`SurfaceCoverage::coverage_line`] (surfaces) and [`SurfaceCoverage::boundaries_zero_state`]
/// (boundaries).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SurfaceCoverage {
    /// Reader display names of the HTTP surface-detector families this build ships,
    /// sorted (e.g. `["AWS CDK API Gateway v2", "Java Spring …", …]`).
    #[serde(default)]
    pub http_detector_families: Vec<String>,
    /// LEGACY flat field (MODULES-IDENTITY-2 wire), retained for byte-additivity so consumers
    /// that read `surface_coverage.named_uncovered` directly keep parsing (review-1 item 1). A
    /// CURRENT daemon fills it with the PER-REPO known gap names (identical to
    /// `material_gap.Known.named_uncovered`). The renderer consumes it ONLY as the fallback
    /// when `material_gap` is ABSENT (a response predating this slice) — see
    /// [`SurfaceCoverage::resolved_gap`].
    #[serde(default)]
    pub named_uncovered: Vec<String>,
    /// The per-repo gap (this slice): which materially-present languages/frameworks of THIS
    /// repo the detectors have no coverage for, or an unknown-with-reason when the language
    /// read failed (STANDING HONESTY RULE 1 — never a silent empty that would read as full
    /// coverage). `None` ONLY when the field is ABSENT (a response predating this slice —
    /// daemon/rgr version skew); the renderer then falls back to the legacy `named_uncovered`
    /// field rather than assuming "no gap" (review-1 item 2).
    #[serde(default)]
    pub material_gap: Option<SurfaceGap>,
}

/// ZEROSTATE-SCOPE-1: the per-repo gap arm — a positively-known (possibly-empty) set of
/// uncovered names, or an unknown-with-reason when the per-language read failed. Two
/// mutually-exclusive states, so "no gap clause" (Known empty) is never confused with
/// "gap undetermined" (Unknown). Mirrors `resource`'s `MaterialGap` shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum SurfaceGap {
    /// The per-language read succeeded. `named_uncovered` are this repo's materially-present
    /// languages/frameworks with no detector (empty = every material language covered → the
    /// clause is omitted).
    Known {
        #[serde(default)]
        named_uncovered: Vec<String>,
    },
    /// The per-language read failed; the gap is unknown, with the reason preserved.
    Unknown { reason: String },
}

impl SurfaceCoverage {
    /// The effective per-repo gap the renderer reads. When `material_gap` is present (a current
    /// daemon) it is authoritative. When it is ABSENT (a response predating this slice) we do
    /// NOT invent a `Known { empty }` — that would render an affirmative "no gap" from an
    /// unknown arm (the review-1 item-2 defect). Instead we consume the LEGACY flat
    /// `named_uncovered` field exactly as the pre-slice renderer did: it was build-static and
    /// infallible, so an empty legacy list genuinely meant "no gap", and a populated one names
    /// the gap. This never fabricates a NEW claim — it faithfully reproduces old behavior for
    /// old data.
    fn resolved_gap(&self) -> SurfaceGap {
        match &self.material_gap {
            Some(gap) => gap.clone(),
            None => SurfaceGap::Known {
                named_uncovered: self.named_uncovered.clone(),
            },
        }
    }

    /// Whether the build-static family list is present. An EMPTY list means the response
    /// predates the coverage report (version skew) — the callers say so rather than fabricate
    /// a family list or reprint the old repo-blaming hint.
    fn families_available(&self) -> bool {
        !self.http_detector_families.is_empty()
    }

    /// The named-gap fragment for the per-repo clause: `Some(reader names)` for a positively-
    /// known non-empty gap, `None` when every material language is covered (Known empty). The
    /// unknown arm is handled separately by [`Self::gap_unknown_reason`]. Operates on
    /// [`Self::resolved_gap`] so the legacy-field fallback is transparent to callers.
    fn gap_names(&self) -> Option<Vec<String>> {
        match self.resolved_gap() {
            SurfaceGap::Known { named_uncovered } if !named_uncovered.is_empty() => {
                Some(named_uncovered)
            }
            _ => None,
        }
    }

    /// The read-failure reason when the per-repo gap could not be determined, else `None`.
    fn gap_unknown_reason(&self) -> Option<String> {
        match self.resolved_gap() {
            SurfaceGap::Unknown { reason } => Some(reason),
            SurfaceGap::Known { .. } => None,
        }
    }

    /// The `surfaces list` zero-state coverage sentence:
    /// `"HTTP surface detectors on this build: <families> — no HTTP detector for <gaps>;
    /// other surface kinds may exist without detectors."`. The gap clause is dropped when the
    /// gap is empty, and renders unknown-with-reason when the per-repo read failed; the
    /// non-totality clause always stays.
    fn coverage_line(&self) -> String {
        if !self.families_available() {
            return "HTTP surface-detector coverage is unavailable (this response predates \
                    the coverage report)."
                .to_string();
        }
        let families = self.http_detector_families.join(", ");
        let gap = if let Some(names) = self.gap_names() {
            // "no HTTP detector for …": the gap clause is scoped to HTTP so it never asserts
            // the language has NO surfaces at all — only that this build has no HTTP detector
            // for them.
            format!(" — no HTTP detector for {}", names.join(", "))
        } else if let Some(reason) = self.gap_unknown_reason() {
            format!(
                " — this repo's uncovered frameworks/languages could not be determined ({})",
                reason
            )
        } else {
            String::new()
        };
        format!(
            "HTTP surface detectors on this build: {}{}; other surface kinds may exist \
             without detectors.",
            families, gap
        )
    }

    /// ZEROSTATE-SCOPE-1 §2.2: the `boundaries list`/`boundaries summary` zero-state, adopting
    /// the coverage form (resource's proven shape) from this SAME roster — never the old
    /// "…in this codebase" blame. Renders (under the caller's headline):
    /// `"Boundary detection on this build covers <families>."` plus the per-repo gap line.
    pub(crate) fn boundaries_zero_state(&self) -> String {
        if !self.families_available() {
            return "Boundary-detector coverage is unavailable (this response predates the \
                    coverage report).\n"
                .to_string();
        }
        let mut out = format!(
            "Boundary detection on this build covers {}.\n",
            self.http_detector_families.join(", ")
        );
        if let Some(names) = self.gap_names() {
            out.push_str(&format!(
                "No detector for {} on this build — their boundaries are not counted.\n",
                names.join(", ")
            ));
        } else if let Some(reason) = self.gap_unknown_reason() {
            out.push_str(&format!(
                "(could not determine this repo's uncovered frameworks/languages: {})\n",
                reason
            ));
        }
        out
    }
}

// =============================================================================
// SURFACES LIST RESPONSE
// =============================================================================

/// A surface entry in the list response.
#[derive(Debug, Clone, Deserialize)]
pub struct SurfaceListEntry {
    #[serde(default)]
    pub project_surface_uid: String,
    #[serde(default)]
    pub surface_kind: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub root_path: Option<String>,
    #[serde(default)]
    pub entrypoint_path: Option<String>,
    #[serde(default)]
    pub runtime_kind: Option<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub evidence_count: u64,
    #[serde(default)]
    pub module_display_name: Option<String>,
    #[serde(default)]
    pub module_root_path: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>,
}

/// Response structure for surfaces list command.
#[derive(Debug, Deserialize)]
pub struct SurfacesListResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub results: Vec<SurfaceListEntry>,
    #[serde(default)]
    pub count: u64,
    /// HTTP-BOUNDARY-1: the REST API map (providers + consumers).
    #[serde(default)]
    pub(crate) http_boundary_surfaces: Vec<HttpBoundarySurfaceEntry>,
    /// HTTP-BOUNDARY-1 (review-4 item 2): reader-framed degradation when the
    /// HTTP-surface read FAILED. Present = UNKNOWN, not "no surfaces": the empty
    /// section and the "no recognized patterns" hint are both suppressed and this
    /// message is shown instead.
    #[serde(default)]
    pub http_boundary_surfaces_degraded: Option<String>,
    #[serde(default)]
    pub filter_kind: Option<String>,
    #[serde(default)]
    pub filter_runtime: Option<String>,
    #[serde(default)]
    pub filter_source: Option<String>,
    #[serde(default)]
    pub filter_module: Option<String>,
    #[serde(default)]
    pub degradation: Option<DegradationInfo>,
    /// MODULES-IDENTITY-2 §2.2: HTTP surface-detector coverage, rendered in the
    /// zero-state so the empty answer states the tool's coverage. Additive; a response
    /// without it deserializes to the empty default (handled honestly by the renderer).
    #[serde(default)]
    pub surface_coverage: SurfaceCoverage,
}

impl SurfacesListResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // -- Header --
        out.push_str("Surfaces\n\n");

        // -- Count --
        // HTTP-SURFACE-COHERENCE-1 §2.3: this counts the PROJECT-surface catalog
        // only (backend/cli/lib …). When an HTTP section is ALSO present, the noun
        // is made explicit ("N project surfaces") so the top line can never read
        // "0 surfaces" above a populated HTTP section (the audit's glamCRM
        // contradiction). With no HTTP section there is nothing to contradict, so
        // the plain "N surfaces" wording is kept — no-HTTP repos stay byte-stable.
        let http_present = !self.http_boundary_surfaces.is_empty()
            || self.http_boundary_surfaces_degraded.is_some();
        let (sing, plur) = if http_present {
            ("project surface", "project surfaces")
        } else {
            ("surface", "surfaces")
        };
        out.push_str(&format!(
            "{}\n",
            format_count(self.count as usize, sing, plur)
        ));

        // -- Active filters --
        let mut filters = Vec::new();
        if let Some(ref k) = self.filter_kind {
            filters.push(format!("kind={}", k));
        }
        if let Some(ref r) = self.filter_runtime {
            filters.push(format!("runtime={}", r));
        }
        if let Some(ref s) = self.filter_source {
            filters.push(format!("source={}", s));
        }
        if let Some(ref m) = self.filter_module {
            filters.push(format!("module={}", m));
        }
        if !filters.is_empty() {
            out.push_str(&format!("Filtered by: {}\n", filters.join(", ")));
        }

        // -- Degradation warning --
        if let Some(ref deg) = self.degradation {
            out.push_str(&format!("\nwarning: {} — {}\n", deg.feature, deg.message));
            if !deg.recommendation.is_empty() {
                out.push_str(&format!("         {}\n", deg.recommendation));
            }
        }

        // -- Empty case --
        // Only truly empty when BOTH project surfaces AND HTTP boundary surfaces
        // are absent (HTTP-BOUNDARY-1: HTTP surfaces render even with 0 project
        // surfaces). A DEGRADED HTTP read is NOT empty — it is unknown, so the
        // "no recognized patterns" hint must not fire (review-4 item 2).
        if self.results.is_empty()
            && self.http_boundary_surfaces.is_empty()
            && self.http_boundary_surfaces_degraded.is_none()
        {
            // MODULES-IDENTITY-2 §2.2: state the TOOL's coverage, never blame the repo.
            // The old "No recognized patterns found in this codebase." line implied the
            // repo had nothing when in fact this build has no detector for its framework
            // (django URLconf, the audit case). Now the zero-state names the shipped HTTP
            // surface detectors and a known gap, with an explicit non-totality clause.
            out.push_str("\nhint: surfaces are extracted from code patterns (HTTP routes, CLI handlers, etc.).\n");
            out.push_str(&format!(
                "      {}\n",
                self.surface_coverage.coverage_line()
            ));
            return out;
        }

        // -- Project surface rows (deterministic order) --
        if !self.results.is_empty() {
            out.push('\n');
            let mut entries = self.results.clone();
            entries.sort_by(|a, b| {
                (&a.surface_kind, &a.display_name, &a.project_surface_uid).cmp(&(
                    &b.surface_kind,
                    &b.display_name,
                    &b.project_surface_uid,
                ))
            });

            // Full output, no truncation
            for entry in &entries {
                let name = entry
                    .display_name
                    .as_deref()
                    .or(entry.root_path.as_deref())
                    .unwrap_or(&entry.project_surface_uid);

                let runtime = entry.runtime_kind.as_deref().unwrap_or("-");
                let module = entry
                    .module_display_name
                    .as_deref()
                    .or(entry.module_root_path.as_deref())
                    .unwrap_or("-");

                out.push_str(&format!(
                    "  {}  {}  {}  {}\n",
                    entry.surface_kind, name, runtime, module
                ));
            }
        }

        // HTTP-BOUNDARY-1: the HTTP/REST section + degraded messaging live in the
        // crate-private `http_boundary` presenter (kept off this file).
        out.push_str(&http_boundary::render_surfaces(
            &self.http_boundary_surfaces,
        ));
        if let Some(reason) = &self.http_boundary_surfaces_degraded {
            out.push_str(&http_boundary::render_surfaces_degraded(reason));
        }
        out
    }
}

// =============================================================================
// SURFACES SHOW RESPONSE
// =============================================================================

/// Full surface detail object.
#[derive(Debug, Clone, Deserialize)]
pub struct SurfaceDetail {
    #[serde(default)]
    pub project_surface_uid: String,
    #[serde(default)]
    pub surface_kind: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub root_path: Option<String>,
    #[serde(default)]
    pub entrypoint_path: Option<String>,
    #[serde(default)]
    pub build_system: Option<String>,
    #[serde(default)]
    pub runtime_kind: Option<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_specific_id: Option<String>,
    #[serde(default)]
    pub stable_surface_key: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<serde_json::Value>,
}

/// Owning module info in show response.
#[derive(Debug, Clone, Deserialize)]
pub struct SurfaceModule {
    #[serde(default)]
    pub module_candidate_uid: String,
    #[serde(default)]
    pub module_key: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub canonical_root_path: Option<String>,
}

/// Evidence entry in show response.
#[derive(Debug, Clone, Deserialize)]
pub struct SurfaceEvidence {
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub evidence_kind: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// Response structure for surfaces show command.
#[derive(Debug, Deserialize)]
pub struct SurfacesShowResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub surface: Option<SurfaceDetail>,
    #[serde(default)]
    pub module: Option<SurfaceModule>,
    #[serde(default)]
    pub evidence: Vec<SurfaceEvidence>,
}

/// ANCHORS-EVERYWHERE-1 (Tier 2): extract the evidence anchor line from a surface-evidence
/// payload WITHOUT a schema column. Some detectors already store `lineStart` inside
/// `payload_json` (today: the Express `route_registration` evidence written in
/// `repo-index/src/compose.rs`). Returns `None` — the honest "no line" path, rendered as a bare
/// `path` by [`anchor`](super::anchor) — when the payload is absent, has no `lineStart`, or
/// `lineStart` is not a non-negative JSON integer (`as_u64` also rejects negatives and floats).
/// A stored `0` (the DB "no span" sentinel) surfaces as `Some(0)` and is likewise rendered bare
/// by `anchor`. The `?`/`as_u64` chain is optional-field extraction, NOT fallible-read
/// suppression: a genuinely absent field must render no line (STANDING HONESTY RULE #1).
///
/// Abstraction record — free fn `evidence_line`; sole current user: `SurfacesShowResponse::
/// render_human`'s evidence loop; axis: none (single call site) — extracted only for the
/// present/absent/zero/negative unit coverage the reviewer required; rejected simpler: inlining
/// the chain (loses the focused test seam).
fn evidence_line(payload: Option<&serde_json::Value>) -> Option<u64> {
    payload?.get("lineStart")?.as_u64()
}

impl SurfacesShowResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        let surface = match &self.surface {
            Some(s) => s,
            None => {
                out.push_str("Surface: (not found)\n");
                return out;
            }
        };

        // -- Header --
        let name = surface
            .display_name
            .as_deref()
            .or(surface.root_path.as_deref())
            .unwrap_or(&surface.project_surface_uid);
        out.push_str(&format!("Surface: {}\n\n", name));

        // -- Identity --
        out.push_str(&format!("Kind: {}\n", surface.surface_kind));

        if let Some(ref runtime) = surface.runtime_kind {
            out.push_str(&format!("Runtime: {}\n", runtime));
        }

        if let Some(ref build) = surface.build_system {
            out.push_str(&format!("Build system: {}\n", build));
        }

        out.push_str(&format!("Confidence: {:.2}\n", surface.confidence));

        // -- Paths --
        if surface.root_path.is_some() || surface.entrypoint_path.is_some() {
            out.push_str("\nPaths:\n");
            if let Some(ref root) = surface.root_path {
                out.push_str(&format!("  Root: {}\n", root));
            }
            if let Some(ref entry) = surface.entrypoint_path {
                out.push_str(&format!("  Entrypoint: {}\n", entry));
            }
        }

        // -- Module --
        if let Some(ref module) = self.module {
            out.push_str("\nModule:\n");
            let mod_name = module
                .display_name
                .as_deref()
                .or(module.canonical_root_path.as_deref())
                .unwrap_or(&module.module_candidate_uid);
            out.push_str(&format!("  {}\n", mod_name));
        }

        // -- Source --
        if surface.source_type.is_some() || surface.source_specific_id.is_some() {
            out.push_str("\nSource:\n");
            if let Some(ref st) = surface.source_type {
                out.push_str(&format!("  Type: {}\n", st));
            }
            if let Some(ref sid) = surface.source_specific_id {
                out.push_str(&format!("  ID: {}\n", sid));
            }
        }

        // -- Evidence (full list, deterministic order) --
        if !self.evidence.is_empty() {
            out.push_str(&format!(
                "\nEvidence ({}):\n",
                format_count(self.evidence.len(), "item", "items")
            ));

            let mut evidence = self.evidence.clone();
            evidence.sort_by(|a, b| {
                (&a.evidence_kind, &a.source_path).cmp(&(&b.evidence_kind, &b.source_path))
            });

            for ev in &evidence {
                // ANCHORS-EVERYWHERE-1 (Tier 2): if the evidence's OWN payload already carries a
                // line (`payload_json.lineStart`, written today by the Express route detector in
                // repo-index/src/compose.rs), anchor the source as `path:line` — same shape as
                // `find`, no schema column. The line and `source_path` come from the SAME evidence
                // record (single-source). Absent path → "-" and never anchored; absent/malformed/
                // non-positive line → bare path (enforced by `evidence_line` + `anchor`).
                let rendered_path = match ev.source_path.as_deref() {
                    Some(sp) => super::anchor(sp, evidence_line(ev.payload.as_ref())),
                    None => "-".to_string(),
                };
                out.push_str(&format!(
                    "  {}  {}  {:.2}\n",
                    ev.evidence_kind, rendered_path, ev.confidence
                ));
            }
        }

        // -- Metadata (if present and parsed) --
        if let Some(ref meta) = surface.metadata_json {
            if let Some(parsed) = meta.get("parsed") {
                if !parsed.is_null() {
                    out.push_str("\nMetadata: (use --json for full structure)\n");
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- SurfacesListResponse tests --

    fn sample_list_response() -> SurfacesListResponse {
        SurfacesListResponse {
            command: "surfaces list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![
                SurfaceListEntry {
                    project_surface_uid: "surf-1".to_string(),
                    surface_kind: "backend".to_string(),
                    display_name: Some("api-server".to_string()),
                    root_path: Some("src/api".to_string()),
                    entrypoint_path: Some("src/api/main.ts".to_string()),
                    runtime_kind: Some("node".to_string()),
                    confidence: 0.9,
                    evidence_count: 3,
                    module_display_name: Some("api".to_string()),
                    module_root_path: Some("src/api".to_string()),
                    source_type: Some("package_json".to_string()),
                },
                SurfaceListEntry {
                    project_surface_uid: "surf-2".to_string(),
                    surface_kind: "cli".to_string(),
                    display_name: Some("rmap".to_string()),
                    root_path: Some("cli".to_string()),
                    entrypoint_path: None,
                    runtime_kind: Some("rust".to_string()),
                    confidence: 0.8,
                    evidence_count: 1,
                    module_display_name: None,
                    module_root_path: Some("cli".to_string()),
                    source_type: Some("cargo_toml".to_string()),
                },
            ],
            count: 2,
            http_boundary_surfaces: vec![],
            http_boundary_surfaces_degraded: None,
            filter_kind: None,
            filter_runtime: None,
            filter_source: None,
            filter_module: None,
            degradation: None,
            surface_coverage: sample_coverage(),
        }
    }

    /// A representative build-static coverage payload (the shape the daemon emits from
    /// `repo_graph_repo_index::surface_coverage`), for the zero-state renderer tests.
    fn sample_coverage() -> SurfaceCoverage {
        SurfaceCoverage {
            http_detector_families: vec![
                "AWS CDK API Gateway v2".to_string(),
                "Java Spring (@RestController/@Controller)".to_string(),
                "Next.js App Router".to_string(),
            ],
            named_uncovered: vec!["Django URLconf routes".to_string()],
            material_gap: Some(SurfaceGap::Known {
                named_uncovered: vec!["Django URLconf routes".to_string()],
            }),
        }
    }

    fn sample_empty_list_response() -> SurfacesListResponse {
        SurfacesListResponse {
            command: "surfaces list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![],
            count: 0,
            http_boundary_surfaces: vec![],
            http_boundary_surfaces_degraded: None,
            filter_kind: None,
            filter_runtime: None,
            filter_source: None,
            filter_module: None,
            degradation: Some(DegradationInfo {
                status: "unsupported".to_string(),
                feature: "ProjectSurfaces".to_string(),
                message: "project_surfaces not populated".to_string(),
                recommendation: "use TypeScript indexer".to_string(),
            }),
            surface_coverage: sample_coverage(),
        }
    }

    #[test]
    fn list_render_shows_header() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("Surfaces"));
    }

    #[test]
    fn list_render_shows_count() {
        // §2.3: with NO HTTP section present, the plain "N surfaces" wording is
        // kept (nothing to contradict) — no-HTTP outputs stay byte-stable.
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("2 surfaces"), "{output}");
        assert!(!output.contains("project surfaces"), "{output}");
    }

    #[test]
    fn list_render_shows_surfaces() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("backend"));
        assert!(output.contains("api-server"));
        assert!(output.contains("cli"));
        assert!(output.contains("rmap"));
    }

    #[test]
    fn list_render_shows_runtime() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("node"));
        assert!(output.contains("rust"));
    }

    // The pure HTTP-surface section rendering (providers/consumers/dynamic) is
    // unit-tested in `presentation::http_boundary`. The two tests below exercise
    // surfaces.rs's OWN concern: the empty/degraded interaction with the section.

    /// HTTP surfaces render even when there are ZERO project surfaces (the empty
    /// hint must not swallow them).
    #[test]
    fn list_render_http_surfaces_with_no_project_surfaces() {
        let mut resp = sample_empty_list_response();
        resp.degradation = None;
        resp.http_boundary_surfaces = vec![HttpBoundarySurfaceEntry {
            direction: "provider".to_string(),
            http_method: "POST".to_string(),
            route: Some("/api/v2/etape".to_string()),
            source_file: "serverless/api.ts".to_string(),
            line: None,
            ..Default::default()
        }];
        let output = resp.render_human();
        assert!(output.contains("HTTP/REST API surfaces"), "{output}");
        assert!(output.contains("POST"), "{output}");
        assert!(
            !output.contains("No recognized patterns"),
            "empty-hint must not fire when HTTP surfaces exist:\n{output}"
        );
    }

    /// §2.3: the glamCRM contradiction — ZERO project surfaces with a populated
    /// HTTP section must NOT headline "0 surfaces" above the HTTP rows. The top
    /// count is explicitly scoped ("0 project surfaces") and the HTTP section
    /// carries its own coherent provider count, so an agent is never misled.
    #[test]
    fn list_render_top_count_scoped_not_contradicting_http_section() {
        let mut resp = sample_empty_list_response();
        resp.degradation = None;
        resp.results = vec![];
        resp.count = 0;
        resp.http_boundary_surfaces = vec![
            HttpBoundarySurfaceEntry {
                direction: "provider".to_string(),
                http_method: "GET".to_string(),
                route: Some("/api/a".to_string()),
                source_file: "backend/A.java".to_string(),
                line: None,
                ..Default::default()
            },
            HttpBoundarySurfaceEntry {
                direction: "provider".to_string(),
                http_method: "POST".to_string(),
                route: Some("/api/b".to_string()),
                source_file: "backend/B.java".to_string(),
                line: None,
                ..Default::default()
            },
        ];
        let output = resp.render_human();
        // Top line is scoped to PROJECT surfaces, not a bare "0 surfaces".
        assert!(output.contains("0 project surfaces"), "{output}");
        // The HTTP section reports the real provider count below.
        assert!(
            output.contains("HTTP/REST API surfaces: 2 providers"),
            "{output}"
        );
    }

    /// review-4 item 2: a FAILED HTTP-surface read renders as UNKNOWN — the
    /// "no recognized patterns" empty hint must NOT fire, and a degradation must
    /// be shown (never an empty REST map presented as fact).
    #[test]
    fn list_render_http_read_degraded_is_unknown_not_empty() {
        let mut resp = sample_empty_list_response();
        resp.degradation = None;
        resp.http_boundary_surfaces = vec![];
        resp.http_boundary_surfaces_degraded =
            Some("HTTP boundary surfaces read failed (degraded): db locked".to_string());
        let output = resp.render_human();
        assert!(
            output.contains("HTTP/REST API surfaces: unknown"),
            "degraded read shown as unknown:\n{output}"
        );
        assert!(
            !output.contains("No recognized patterns"),
            "empty-hint must not fire on a degraded read:\n{output}"
        );
    }

    #[test]
    fn list_render_empty_shows_hint() {
        let resp = sample_empty_list_response();
        let output = resp.render_human();
        assert!(output.contains("hint:"));
        assert!(output.contains("surfaces are extracted from code patterns"));
    }

    /// §2.2: the zero-state states the TOOL's coverage — names the shipped HTTP
    /// surface detectors and a known gap (django URLconf) with an explicit non-totality
    /// clause — and NO LONGER blames the repo ("No recognized patterns …").
    #[test]
    fn list_render_empty_states_detector_coverage_not_repo_blame() {
        let resp = sample_empty_list_response();
        let output = resp.render_human();
        assert!(
            output.contains("HTTP surface detectors on this build:"),
            "zero-state must state coverage:\n{output}"
        );
        // Shipped families are named.
        assert!(
            output.contains("Java Spring (@RestController/@Controller)"),
            "{output}"
        );
        assert!(output.contains("Next.js App Router"), "{output}");
        // The motivating gap is named, HTTP-scoped.
        assert!(
            output.contains("no HTTP detector for Django URLconf routes"),
            "{output}"
        );
        // Never a totality claim.
        assert!(
            output.contains("other surface kinds may exist without detectors"),
            "{output}"
        );
        // The old repo-blaming line is gone.
        assert!(
            !output.contains("No recognized patterns"),
            "zero-state must not blame the repo:\n{output}"
        );
    }

    /// §2.2 honesty: a response WITHOUT coverage (version skew — empty families) states
    /// coverage is unavailable rather than fabricating a family list or reprinting the
    /// old repo-blaming hint.
    #[test]
    fn list_render_empty_with_absent_coverage_says_unavailable_not_blame() {
        let mut resp = sample_empty_list_response();
        resp.surface_coverage = SurfaceCoverage::default();
        let output = resp.render_human();
        assert!(
            output.contains("HTTP surface-detector coverage is unavailable"),
            "{output}"
        );
        assert!(!output.contains("No recognized patterns"), "{output}");
        // No fabricated family list.
        assert!(
            !output.contains("HTTP surface detectors on this build:"),
            "{output}"
        );
    }

    /// §2.2: with families present but NO named gap, the sentence drops the gap clause
    /// yet keeps the non-totality clause (never implies full coverage).
    #[test]
    fn list_render_empty_coverage_without_named_gap_keeps_non_totality_clause() {
        let mut resp = sample_empty_list_response();
        resp.surface_coverage.material_gap = Some(SurfaceGap::Known {
            named_uncovered: vec![],
        });
        let output = resp.render_human();
        assert!(
            output.contains("HTTP surface detectors on this build:"),
            "{output}"
        );
        assert!(!output.contains("no HTTP detector for"), "{output}");
        assert!(
            output.contains("other surface kinds may exist without detectors"),
            "{output}"
        );
    }

    /// §2.1: leveldb's shape — a materially C/C++ repo names its OWN C/C++ truth in the gap,
    /// NOT django's URLconf sentence. No repo wears another's.
    #[test]
    fn list_render_empty_gap_is_this_repos_languages_not_djangos() {
        let mut resp = sample_empty_list_response();
        resp.surface_coverage.material_gap = Some(SurfaceGap::Known {
            named_uncovered: vec!["C".to_string(), "C++".to_string()],
        });
        let output = resp.render_human();
        assert!(
            output.contains("no HTTP detector for C, C++"),
            "leveldb must say its C/C++ truth:\n{output}"
        );
        assert!(
            !output.contains("Django"),
            "leveldb must NOT wear django's sentence:\n{output}"
        );
    }

    /// §2.1 + STANDING HONESTY RULE 1: a FAILED per-repo language read renders
    /// unknown-with-reason, never a silent omission that would read as full coverage.
    #[test]
    fn list_render_empty_gap_unknown_renders_reason() {
        let mut resp = sample_empty_list_response();
        resp.surface_coverage.material_gap = Some(SurfaceGap::Unknown {
            reason: "db locked".to_string(),
        });
        let output = resp.render_human();
        assert!(
            output.contains("could not be determined (db locked)"),
            "{output}"
        );
        // The families line still renders; only the gap arm is unknown.
        assert!(
            output.contains("HTTP surface detectors on this build:"),
            "{output}"
        );
    }

    // -- review-1 raw-JSON wire-compatibility tests --
    //
    // The daemon builds the `surface_coverage` object as raw JSON (never via this struct), and
    // cross-version consumers may too, so these parse RAW JSON (not struct literals) to pin the
    // additive contract: the legacy flat `named_uncovered` field is still accepted, `material_gap`
    // is authoritative when present, and an ABSENT `material_gap` never renders an affirmative
    // "no gap" (STANDING HONESTY RULE 1 / review-1 item 2).

    /// A current daemon emits BOTH the flat `named_uncovered` and the tagged `material_gap`.
    /// `material_gap` is authoritative; the flat field is ignored when the tag is present (here
    /// they agree, as a real daemon guarantees).
    #[test]
    fn coverage_wire_material_gap_present_is_authoritative() {
        let cov: SurfaceCoverage = serde_json::from_str(
            r#"{
                "http_detector_families": ["Java Spring (@RestController/@Controller)"],
                "named_uncovered": ["C", "C++"],
                "material_gap": {"status": "known", "named_uncovered": ["C", "C++"]}
            }"#,
        )
        .expect("parse");
        assert_eq!(
            cov.gap_names(),
            Some(vec!["C".to_string(), "C++".to_string()])
        );
        assert_eq!(cov.gap_unknown_reason(), None);
    }

    /// review-1 item 2: a response predating this slice carries the flat `named_uncovered` but
    /// NO `material_gap`. The renderer must FALL BACK to the legacy field — never treat the
    /// absent arm as an affirmative "no gap".
    #[test]
    fn coverage_wire_absent_material_gap_falls_back_to_legacy_named_uncovered() {
        let cov: SurfaceCoverage = serde_json::from_str(
            r#"{
                "http_detector_families": ["Next.js App Router"],
                "named_uncovered": ["Django URLconf routes"]
            }"#,
        )
        .expect("parse");
        // The legacy field is consumed, so the gap still renders — not silently dropped.
        assert_eq!(
            cov.gap_names(),
            Some(vec!["Django URLconf routes".to_string()])
        );
        assert_eq!(cov.gap_unknown_reason(), None);
    }

    /// A pre-slice response with families present but an EMPTY legacy `named_uncovered` and no
    /// `material_gap` renders no gap clause — byte-identical to the pre-slice renderer (the old
    /// field was build-static/infallible, so empty genuinely meant "no gap"). This is NOT a new
    /// false claim; it faithfully reproduces old behavior for old data.
    #[test]
    fn coverage_wire_absent_material_gap_empty_legacy_is_no_gap() {
        let cov: SurfaceCoverage = serde_json::from_str(
            r#"{
                "http_detector_families": ["Next.js App Router"],
                "named_uncovered": []
            }"#,
        )
        .expect("parse");
        assert_eq!(cov.gap_names(), None);
        assert_eq!(cov.gap_unknown_reason(), None);
    }

    /// A current daemon's failed per-repo read: `material_gap` is `unknown`-with-reason while the
    /// legacy flat field is empty. The tagged arm wins → unknown is preserved (never masked as
    /// "no gap" by the empty legacy field).
    #[test]
    fn coverage_wire_unknown_material_gap_wins_over_empty_legacy() {
        let cov: SurfaceCoverage = serde_json::from_str(
            r#"{
                "http_detector_families": ["Next.js App Router"],
                "named_uncovered": [],
                "material_gap": {"status": "unknown", "reason": "db locked"}
            }"#,
        )
        .expect("parse");
        assert_eq!(cov.gap_names(), None);
        assert_eq!(cov.gap_unknown_reason(), Some("db locked".to_string()));
    }

    #[test]
    fn list_render_shows_degradation_warning() {
        let resp = sample_empty_list_response();
        let output = resp.render_human();
        assert!(output.contains("warning:"));
        assert!(output.contains("ProjectSurfaces"));
    }

    #[test]
    fn list_render_is_deterministic() {
        let resp = sample_list_response();
        let output = resp.render_human();
        // backend comes before cli alphabetically
        let backend_pos = output.find("backend").unwrap();
        let cli_pos = output.find("cli").unwrap();
        assert!(
            backend_pos < cli_pos,
            "Surfaces should be sorted by (kind, name)"
        );
    }

    // -- SurfacesShowResponse tests --

    fn sample_show_response() -> SurfacesShowResponse {
        SurfacesShowResponse {
            command: "surfaces show".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            surface: Some(SurfaceDetail {
                project_surface_uid: "surf-1".to_string(),
                surface_kind: "backend".to_string(),
                display_name: Some("api-server".to_string()),
                root_path: Some("src/api".to_string()),
                entrypoint_path: Some("src/api/main.ts".to_string()),
                build_system: Some("npm".to_string()),
                runtime_kind: Some("node".to_string()),
                confidence: 0.9,
                source_type: Some("package_json".to_string()),
                source_specific_id: Some("@myorg/api".to_string()),
                stable_surface_key: Some("backend:api-server".to_string()),
                metadata_json: None,
            }),
            module: Some(SurfaceModule {
                module_candidate_uid: "mod-1".to_string(),
                module_key: Some("inferred:repo:src/api".to_string()),
                display_name: Some("api".to_string()),
                canonical_root_path: Some("src/api".to_string()),
            }),
            evidence: vec![
                SurfaceEvidence {
                    source_type: "file".to_string(),
                    source_path: Some("src/api/package.json".to_string()),
                    evidence_kind: "package_json_main".to_string(),
                    confidence: 0.9,
                    payload: None,
                },
                SurfaceEvidence {
                    source_type: "file".to_string(),
                    source_path: Some("src/api/server.ts".to_string()),
                    evidence_kind: "express_app".to_string(),
                    confidence: 0.8,
                    payload: None,
                },
            ],
        }
    }

    #[test]
    fn show_render_shows_header() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Surface: api-server"));
    }

    #[test]
    fn show_render_shows_kind() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Kind: backend"));
    }

    #[test]
    fn show_render_shows_runtime() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Runtime: node"));
    }

    #[test]
    fn show_render_shows_paths() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Root: src/api"));
        assert!(output.contains("Entrypoint: src/api/main.ts"));
    }

    #[test]
    fn show_render_shows_module() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Module:"));
        assert!(output.contains("api"));
    }

    #[test]
    fn show_render_shows_evidence() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Evidence (2 items)"));
        assert!(output.contains("package_json_main"));
        assert!(output.contains("express_app"));
    }

    #[test]
    fn show_render_evidence_is_deterministic() {
        let resp = sample_show_response();
        let output = resp.render_human();
        // express_app comes before package_json_main alphabetically by evidence_kind
        let express_pos = output.find("express_app").unwrap();
        let pkg_pos = output.find("package_json_main").unwrap();
        assert!(
            express_pos < pkg_pos,
            "Evidence should be sorted by (kind, path)"
        );
    }

    // -- ANCHORS-EVERYWHERE-1 (Tier 2): evidence payload line rendering --

    fn show_with_evidence(payload: Option<serde_json::Value>) -> SurfacesShowResponse {
        let mut resp = sample_show_response();
        resp.evidence = vec![SurfaceEvidence {
            source_type: "code_detection".to_string(),
            source_path: Some("src/routes.ts".to_string()),
            evidence_kind: "route_registration".to_string(),
            confidence: 0.9,
            payload,
        }];
        resp
    }

    #[test]
    fn evidence_line_extracts_positive_line_start() {
        // Present positive `lineStart` in the payload → the extracted anchor line.
        let payload = serde_json::json!({ "method": "GET", "lineStart": 42 });
        assert_eq!(evidence_line(Some(&payload)), Some(42));
    }

    #[test]
    fn evidence_line_is_none_when_absent_missing_negative_or_noninteger() {
        // Absent payload, missing field, negative, and non-integer all → None, so the renderer
        // emits a bare path (STANDING HONESTY RULE #1 — no invented line). NOTE: a stored `0`
        // is NOT in this set — it returns `Some(0)` here and is suppressed downstream by
        // `anchor()` (see `evidence_line_zero_returns_some_zero` and the render-level test).
        assert_eq!(evidence_line(None), None);
        assert_eq!(
            evidence_line(Some(&serde_json::json!({ "path": "/x" }))),
            None
        );
        // `as_u64` rejects negatives and non-integers.
        assert_eq!(
            evidence_line(Some(&serde_json::json!({ "lineStart": -3 }))),
            None
        );
        assert_eq!(
            evidence_line(Some(&serde_json::json!({ "lineStart": "12" }))),
            None
        );
    }

    #[test]
    fn evidence_line_zero_returns_some_zero() {
        // A stored `0` is a real (if degenerate) `as_u64` value: `evidence_line` returns
        // `Some(0)` — it does NOT filter the zero sentinel. Suppression of `:0` is the
        // single responsibility of `anchor()` (asserted at render level in
        // `show_render_evidence_zero_line_is_bare_path`). This test pins that division of
        // labor so the misnaming corrected in review-2 iteration 3 cannot silently return.
        assert_eq!(
            evidence_line(Some(&serde_json::json!({ "lineStart": 0 }))),
            Some(0)
        );
    }

    #[test]
    fn show_render_evidence_anchors_present_line() {
        let resp = show_with_evidence(Some(serde_json::json!({ "lineStart": 42 })));
        let output = resp.render_human();
        assert!(
            output.contains("src/routes.ts:42"),
            "evidence with a payload lineStart anchors path:line:\n{output}"
        );
    }

    #[test]
    fn show_render_evidence_absent_line_is_bare_path() {
        let resp = show_with_evidence(None);
        let output = resp.render_human();
        assert!(
            output.contains("src/routes.ts") && !output.contains("src/routes.ts:"),
            "absent line renders a bare path, never `:N`:\n{output}"
        );
    }

    #[test]
    fn show_render_evidence_zero_line_is_bare_path() {
        // The "no span" sentinel (0) must NOT render as `:0`.
        let resp = show_with_evidence(Some(serde_json::json!({ "lineStart": 0 })));
        let output = resp.render_human();
        assert!(
            !output.contains("src/routes.ts:0"),
            "zero lineStart must never render `:0`:\n{output}"
        );
        assert!(
            output.contains("src/routes.ts"),
            "path still rendered:\n{output}"
        );
    }
}
