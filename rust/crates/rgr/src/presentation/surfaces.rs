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
            out.push_str("\nhint: surfaces are extracted from code patterns (HTTP routes, CLI handlers, etc.).\n");
            out.push_str("      No recognized patterns found in this codebase.\n");
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
                let path = ev.source_path.as_deref().unwrap_or("-");
                out.push_str(&format!(
                    "  {}  {}  {:.2}\n",
                    ev.evidence_kind, path, ev.confidence
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
                ..Default::default()
            },
            HttpBoundarySurfaceEntry {
                direction: "provider".to_string(),
                http_method: "POST".to_string(),
                route: Some("/api/b".to_string()),
                source_file: "backend/B.java".to_string(),
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
}
