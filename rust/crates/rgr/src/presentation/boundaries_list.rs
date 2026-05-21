//! Presentation layer for boundaries list command.
//!
//! # CLI-OUT-4 Group 5
//!
//! Response DTO and human renderer for `boundaries list`.
//!
//! ## Change Axis
//!
//! This file changes when:
//! - Boundary catalog format changes
//! - Filter display changes
//! - List row formatting changes
//!
//! It does NOT change when:
//! - boundaries show changes
//! - boundaries summary changes

use serde::Deserialize;

use super::module_shared::format_count;

// =============================================================================
// BOUNDARIES LIST RESPONSE
// =============================================================================

/// A boundary entry in the list response.
#[derive(Debug, Clone, Deserialize)]
pub struct BoundaryListEntry {
    #[serde(default, rename = "surfaceUid")]
    pub boundary_channel_uid: String,
    #[serde(default, rename = "channelKind")]
    pub channel_kind: String,
    #[serde(default, rename = "boundaryScope")]
    pub boundary_scope: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default, rename = "protocolFamily")]
    pub protocol_family: Option<String>,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default, rename = "sourceFile")]
    pub file_path: Option<String>,
    #[serde(default, rename = "symbolStableKey")]
    pub symbol_key: Option<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub basis: Option<String>,
    #[serde(default)]
    pub surface_uid: Option<String>,
    #[serde(default)]
    pub surface_display_name: Option<String>,
}

/// Response structure for boundaries list command.
#[derive(Debug, Deserialize)]
pub struct BoundariesListResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub results: Vec<BoundaryListEntry>,
    #[serde(default)]
    pub count: u64,
    // Filter echo fields (if present in response)
    #[serde(default)]
    pub filter_kind: Option<String>,
    #[serde(default)]
    pub filter_scope: Option<String>,
    #[serde(default)]
    pub filter_direction: Option<String>,
    #[serde(default)]
    pub filter_family: Option<String>,
    #[serde(default)]
    pub filter_file: Option<String>,
    #[serde(default)]
    pub filter_file_prefix: Option<String>,
    #[serde(default)]
    pub filter_symbol: Option<String>,
}

impl BoundariesListResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // -- Header --
        out.push_str("Boundaries\n\n");

        // -- Count --
        out.push_str(&format!(
            "{}\n",
            format_count(self.count as usize, "boundary", "boundaries")
        ));

        // -- Active filters --
        let mut filters = Vec::new();
        if let Some(ref k) = self.filter_kind {
            filters.push(format!("kind={}", k));
        }
        if let Some(ref s) = self.filter_scope {
            filters.push(format!("scope={}", s));
        }
        if let Some(ref d) = self.filter_direction {
            filters.push(format!("direction={}", d));
        }
        if let Some(ref f) = self.filter_family {
            filters.push(format!("family={}", f));
        }
        if let Some(ref f) = self.filter_file {
            filters.push(format!("file={}", f));
        }
        if let Some(ref p) = self.filter_file_prefix {
            filters.push(format!("file-prefix={}", p));
        }
        if let Some(ref s) = self.filter_symbol {
            filters.push(format!("symbol={}", s));
        }
        if !filters.is_empty() {
            out.push_str(&format!("Filtered by: {}\n", filters.join(", ")));
        }

        // -- Empty case --
        if self.results.is_empty() {
            out.push_str(
                "\nhint: boundaries are interactions between code and external systems.\n",
            );
            out.push_str("      No recognized boundary patterns found in this codebase.\n");
            return out;
        }

        out.push('\n');

        // -- Boundary rows (deterministic order) --
        let mut entries = self.results.clone();
        // Sort by (channel_kind, direction, service_name, file_path, boundary_channel_uid)
        entries.sort_by(|a, b| {
            (
                &a.channel_kind,
                &a.direction,
                &a.service_name,
                &a.file_path,
                &a.boundary_channel_uid,
            )
                .cmp(&(
                    &b.channel_kind,
                    &b.direction,
                    &b.service_name,
                    &b.file_path,
                    &b.boundary_channel_uid,
                ))
        });

        // Full output, no truncation
        for entry in &entries {
            let identity = entry
                .service_name
                .as_deref()
                .or(entry.file_path.as_deref())
                .unwrap_or(&entry.boundary_channel_uid);

            let family = entry.protocol_family.as_deref().unwrap_or("-");

            out.push_str(&format!(
                "  {}  {}  {}  {}  {}\n",
                entry.channel_kind, entry.direction, entry.boundary_scope, family, identity
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_list_response() -> BoundariesListResponse {
        BoundariesListResponse {
            command: "boundaries list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![
                BoundaryListEntry {
                    boundary_channel_uid: "bc-1".to_string(),
                    channel_kind: "http_client".to_string(),
                    boundary_scope: "external".to_string(),
                    direction: "outbound".to_string(),
                    protocol_family: Some("REST".to_string()),
                    service_name: Some("UserService".to_string()),
                    file_path: Some("src/api/client.ts".to_string()),
                    symbol_key: None,
                    confidence: 0.9,
                    basis: Some("pattern".to_string()),
                    surface_uid: Some("surf-1".to_string()),
                    surface_display_name: Some("api".to_string()),
                },
                BoundaryListEntry {
                    boundary_channel_uid: "bc-2".to_string(),
                    channel_kind: "database".to_string(),
                    boundary_scope: "internal".to_string(),
                    direction: "bidirectional".to_string(),
                    protocol_family: Some("SQL".to_string()),
                    service_name: None,
                    file_path: Some("src/db/pool.ts".to_string()),
                    symbol_key: Some("DbPool.query".to_string()),
                    confidence: 0.8,
                    basis: Some("import".to_string()),
                    surface_uid: None,
                    surface_display_name: None,
                },
            ],
            count: 2,
            filter_kind: None,
            filter_scope: None,
            filter_direction: None,
            filter_family: None,
            filter_file: None,
            filter_file_prefix: None,
            filter_symbol: None,
        }
    }

    fn sample_empty_response() -> BoundariesListResponse {
        BoundariesListResponse {
            command: "boundaries list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![],
            count: 0,
            filter_kind: None,
            filter_scope: None,
            filter_direction: None,
            filter_family: None,
            filter_file: None,
            filter_file_prefix: None,
            filter_symbol: None,
        }
    }

    #[test]
    fn list_render_shows_header() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("Boundaries"));
    }

    #[test]
    fn list_render_shows_count() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("2 boundaries"));
    }

    #[test]
    fn list_render_shows_boundaries() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("http_client"));
        assert!(output.contains("database"));
        assert!(output.contains("UserService"));
    }

    #[test]
    fn list_render_shows_direction() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("outbound"));
        assert!(output.contains("bidirectional"));
    }

    #[test]
    fn list_render_shows_scope() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("external"));
        assert!(output.contains("internal"));
    }

    #[test]
    fn list_render_empty_shows_hint() {
        let resp = sample_empty_response();
        let output = resp.render_human();
        assert!(output.contains("hint:"));
        assert!(output.contains("boundaries are interactions"));
    }

    #[test]
    fn list_render_is_deterministic() {
        let resp = sample_list_response();
        let output = resp.render_human();
        // database comes before http_client alphabetically by channel_kind
        let db_pos = output.find("database").unwrap();
        let http_pos = output.find("http_client").unwrap();
        assert!(
            db_pos < http_pos,
            "Boundaries should be sorted by (kind, direction, ...)"
        );
    }

    #[test]
    fn list_render_shows_filter() {
        let mut resp = sample_empty_response();
        resp.filter_kind = Some("http_client".to_string());
        let output = resp.render_human();
        assert!(output.contains("Filtered by: kind=http_client"));
    }
}
