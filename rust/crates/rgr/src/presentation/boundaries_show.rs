//! Presentation layer for boundaries show command.
//!
//! # CLI-OUT-4 Group 5
//!
//! Response DTO and human renderer for `boundaries show`.
//!
//! ## Change Axis
//!
//! This file changes when:
//! - Boundary detail format changes
//! - Evidence display changes
//! - Source/target semantics change
//!
//! It does NOT change when:
//! - boundaries list changes
//! - boundaries summary changes

use serde::Deserialize;

use super::module_shared::format_count;

// =============================================================================
// BOUNDARIES SHOW RESPONSE
// =============================================================================

/// Full boundary detail object.
#[derive(Debug, Clone, Deserialize)]
pub struct BoundaryDetail {
    #[serde(default)]
    pub boundary_channel_uid: String,
    #[serde(default)]
    pub channel_kind: String,
    #[serde(default)]
    pub boundary_scope: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub protocol_family: Option<String>,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub symbol_key: Option<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub basis: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<serde_json::Value>,
}

/// Surface association in show response.
#[derive(Debug, Clone, Deserialize)]
pub struct BoundarySurface {
    #[serde(default)]
    pub project_surface_uid: String,
    #[serde(default)]
    pub surface_kind: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub root_path: Option<String>,
}

/// Evidence entry in show response.
#[derive(Debug, Clone, Deserialize)]
pub struct BoundaryEvidence {
    #[serde(default)]
    pub evidence_kind: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_line: Option<u32>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// Response structure for boundaries show command.
#[derive(Debug, Deserialize)]
pub struct BoundariesShowResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub boundary: Option<BoundaryDetail>,
    #[serde(default)]
    pub surface: Option<BoundarySurface>,
    #[serde(default)]
    pub evidence: Vec<BoundaryEvidence>,
}

impl BoundariesShowResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        let boundary = match &self.boundary {
            Some(b) => b,
            None => {
                out.push_str("Boundary: (not found)\n");
                return out;
            }
        };

        // -- Header --
        let name = boundary
            .service_name
            .as_deref()
            .or(boundary.file_path.as_deref())
            .unwrap_or(&boundary.boundary_channel_uid);
        out.push_str(&format!("Boundary: {}\n\n", name));

        // -- Classification --
        out.push_str(&format!("Kind: {}\n", boundary.channel_kind));
        out.push_str(&format!("Scope: {}\n", boundary.boundary_scope));
        out.push_str(&format!("Direction: {}\n", boundary.direction));

        if let Some(ref family) = boundary.protocol_family {
            out.push_str(&format!("Protocol: {}\n", family));
        }

        out.push_str(&format!("Confidence: {:.2}\n", boundary.confidence));

        if let Some(ref basis) = boundary.basis {
            out.push_str(&format!("Basis: {}\n", basis));
        }

        // -- Location --
        if boundary.file_path.is_some() || boundary.symbol_key.is_some() {
            out.push_str("\nLocation:\n");
            if let Some(ref path) = boundary.file_path {
                out.push_str(&format!("  File: {}\n", path));
            }
            if let Some(ref symbol) = boundary.symbol_key {
                out.push_str(&format!("  Symbol: {}\n", symbol));
            }
        }

        // -- Surface association --
        if let Some(ref surface) = self.surface {
            out.push_str("\nSurface:\n");
            let surf_name = surface
                .display_name
                .as_deref()
                .or(surface.root_path.as_deref())
                .unwrap_or(&surface.project_surface_uid);
            out.push_str(&format!("  {}", surf_name));
            if let Some(ref kind) = surface.surface_kind {
                out.push_str(&format!(" ({})", kind));
            }
            out.push('\n');
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
                let line_info = ev
                    .source_line
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {}  {}{} ({:.2})\n",
                    ev.evidence_kind, path, line_info, ev.confidence
                ));
            }
        }

        // -- Metadata hint --
        if let Some(ref meta) = boundary.metadata_json {
            if !meta.is_null() {
                out.push_str("\nMetadata: (use --json for full structure)\n");
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_show_response() -> BoundariesShowResponse {
        BoundariesShowResponse {
            command: "boundaries show".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            boundary: Some(BoundaryDetail {
                boundary_channel_uid: "bc-1".to_string(),
                channel_kind: "http_client".to_string(),
                boundary_scope: "external".to_string(),
                direction: "outbound".to_string(),
                protocol_family: Some("REST".to_string()),
                service_name: Some("UserService".to_string()),
                file_path: Some("src/api/client.ts".to_string()),
                symbol_key: Some("fetchUser".to_string()),
                confidence: 0.9,
                basis: Some("pattern".to_string()),
                metadata_json: None,
            }),
            surface: Some(BoundarySurface {
                project_surface_uid: "surf-1".to_string(),
                surface_kind: Some("backend".to_string()),
                display_name: Some("api-server".to_string()),
                root_path: Some("src/api".to_string()),
            }),
            evidence: vec![
                BoundaryEvidence {
                    evidence_kind: "import_pattern".to_string(),
                    source_path: Some("src/api/client.ts".to_string()),
                    source_line: Some(5),
                    confidence: 0.9,
                    payload: None,
                },
                BoundaryEvidence {
                    evidence_kind: "call_site".to_string(),
                    source_path: Some("src/api/client.ts".to_string()),
                    source_line: Some(12),
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
        assert!(output.contains("Boundary: UserService"));
    }

    #[test]
    fn show_render_shows_kind() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Kind: http_client"));
    }

    #[test]
    fn show_render_shows_scope() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Scope: external"));
    }

    #[test]
    fn show_render_shows_direction() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Direction: outbound"));
    }

    #[test]
    fn show_render_shows_protocol() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Protocol: REST"));
    }

    #[test]
    fn show_render_shows_location() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Location:"));
        assert!(output.contains("File: src/api/client.ts"));
        assert!(output.contains("Symbol: fetchUser"));
    }

    #[test]
    fn show_render_shows_surface() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Surface:"));
        assert!(output.contains("api-server (backend)"));
    }

    #[test]
    fn show_render_shows_evidence() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Evidence (2 items)"));
        assert!(output.contains("import_pattern"));
        assert!(output.contains("call_site"));
    }

    #[test]
    fn show_render_evidence_shows_line_numbers() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains(":5"));
        assert!(output.contains(":12"));
    }

    #[test]
    fn show_render_evidence_is_deterministic() {
        let resp = sample_show_response();
        let output = resp.render_human();
        // call_site comes before import_pattern alphabetically
        let call_pos = output.find("call_site").unwrap();
        let import_pos = output.find("import_pattern").unwrap();
        assert!(
            call_pos < import_pos,
            "Evidence should be sorted by (kind, path)"
        );
    }

    #[test]
    fn show_render_not_found() {
        let resp = BoundariesShowResponse {
            command: "boundaries show".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            boundary: None,
            surface: None,
            evidence: vec![],
        };
        let output = resp.render_human();
        assert!(output.contains("(not found)"));
    }
}
