//! Presentation layer for boundaries summary command.
//!
//! # CLI-OUT-4 Group 5
//!
//! Response DTO and human renderer for `boundaries summary`.
//!
//! ## Change Axis
//!
//! This file changes when:
//! - Summary aggregate format changes
//! - Grouping categories change
//! - Count display changes
//!
//! It does NOT change when:
//! - boundaries list changes
//! - boundaries show changes

use serde::Deserialize;

// =============================================================================
// BOUNDARIES SUMMARY RESPONSE
// =============================================================================

/// Count by category entry.
#[derive(Debug, Clone, Deserialize)]
pub struct CategoryCount {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub count: u64,
}

/// File with boundaries entry.
#[derive(Debug, Clone, Deserialize)]
pub struct FileWithBoundaries {
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub boundary_count: u64,
}

/// Summary data object.
#[derive(Debug, Clone, Deserialize)]
pub struct BoundarySummary {
    #[serde(default, rename = "totalSurfaces")]
    pub total_surfaces: u64,
    #[serde(default, rename = "totalChannels")]
    pub total_channels: u64,
    #[serde(default, rename = "byChannelKind")]
    pub by_channel_kind: Vec<CategoryCount>,
    #[serde(default, rename = "byBoundaryScope")]
    pub by_boundary_scope: Vec<CategoryCount>,
    #[serde(default, rename = "byDirection")]
    pub by_direction: Vec<CategoryCount>,
    #[serde(default, rename = "byProtocolFamily")]
    pub by_protocol_family: Vec<CategoryCount>,
    #[serde(default, rename = "byBasis")]
    pub by_basis: Vec<CategoryCount>,
    #[serde(default, rename = "filesWithBoundaries")]
    pub files_with_boundaries: Vec<FileWithBoundaries>,
}

/// Response structure for boundaries summary command.
#[derive(Debug, Deserialize)]
pub struct BoundariesSummaryResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub summary: Option<BoundarySummary>,
}

impl BoundariesSummaryResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        out.push_str("Boundaries Summary\n\n");

        let summary = match &self.summary {
            Some(s) => s,
            None => {
                out.push_str("No summary data available.\n");
                return out;
            }
        };

        // -- Totals --
        out.push_str(&format!("{} surfaces\n", summary.total_surfaces));
        out.push_str(&format!("{} channels\n", summary.total_channels));

        // -- Empty case --
        if summary.total_surfaces == 0 && summary.total_channels == 0 {
            out.push_str("\nNo architectural boundaries detected.\n");
            out.push_str(
                "\nhint: boundaries connect surfaces to resources. Without detected surfaces,\n",
            );
            out.push_str("      no boundaries can be established.\n");
            return out;
        }

        // -- By channel kind --
        if !summary.by_channel_kind.is_empty() {
            out.push_str("\nBy channel kind:\n");
            let mut items = summary.by_channel_kind.clone();
            items.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then_with(|| a.category.cmp(&b.category))
            });
            for item in &items {
                out.push_str(&format!("  {}  {}\n", item.count, item.category));
            }
        }

        // -- By scope --
        if !summary.by_boundary_scope.is_empty() {
            out.push_str("\nBy scope:\n");
            let mut items = summary.by_boundary_scope.clone();
            items.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then_with(|| a.category.cmp(&b.category))
            });
            for item in &items {
                out.push_str(&format!("  {}  {}\n", item.count, item.category));
            }
        }

        // -- By direction --
        if !summary.by_direction.is_empty() {
            out.push_str("\nBy direction:\n");
            let mut items = summary.by_direction.clone();
            items.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then_with(|| a.category.cmp(&b.category))
            });
            for item in &items {
                out.push_str(&format!("  {}  {}\n", item.count, item.category));
            }
        }

        // -- By protocol family --
        if !summary.by_protocol_family.is_empty() {
            out.push_str("\nBy protocol:\n");
            let mut items = summary.by_protocol_family.clone();
            items.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then_with(|| a.category.cmp(&b.category))
            });
            for item in &items {
                out.push_str(&format!("  {}  {}\n", item.count, item.category));
            }
        }

        // -- By basis --
        if !summary.by_basis.is_empty() {
            out.push_str("\nBy basis:\n");
            let mut items = summary.by_basis.clone();
            items.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then_with(|| a.category.cmp(&b.category))
            });
            for item in &items {
                out.push_str(&format!("  {}  {}\n", item.count, item.category));
            }
        }

        // -- Files with boundaries (top 10 if many) --
        if !summary.files_with_boundaries.is_empty() {
            out.push_str("\nFiles with boundaries:\n");
            let mut files = summary.files_with_boundaries.clone();
            files.sort_by(|a, b| {
                b.boundary_count
                    .cmp(&a.boundary_count)
                    .then_with(|| a.file_path.cmp(&b.file_path))
            });

            // Full output, no truncation
            for file in &files {
                out.push_str(&format!("  {}  {}\n", file.boundary_count, file.file_path));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summary_response() -> BoundariesSummaryResponse {
        BoundariesSummaryResponse {
            command: "boundaries summary".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            summary: Some(BoundarySummary {
                total_surfaces: 3,
                total_channels: 5,
                by_channel_kind: vec![
                    CategoryCount {
                        category: "http_client".to_string(),
                        count: 3,
                    },
                    CategoryCount {
                        category: "database".to_string(),
                        count: 2,
                    },
                ],
                by_boundary_scope: vec![
                    CategoryCount {
                        category: "external".to_string(),
                        count: 3,
                    },
                    CategoryCount {
                        category: "internal".to_string(),
                        count: 2,
                    },
                ],
                by_direction: vec![
                    CategoryCount {
                        category: "outbound".to_string(),
                        count: 4,
                    },
                    CategoryCount {
                        category: "inbound".to_string(),
                        count: 1,
                    },
                ],
                by_protocol_family: vec![CategoryCount {
                    category: "REST".to_string(),
                    count: 3,
                }],
                by_basis: vec![
                    CategoryCount {
                        category: "pattern".to_string(),
                        count: 4,
                    },
                    CategoryCount {
                        category: "import".to_string(),
                        count: 1,
                    },
                ],
                files_with_boundaries: vec![
                    FileWithBoundaries {
                        file_path: "src/api/client.ts".to_string(),
                        boundary_count: 3,
                    },
                    FileWithBoundaries {
                        file_path: "src/db/pool.ts".to_string(),
                        boundary_count: 2,
                    },
                ],
            }),
        }
    }

    fn sample_empty_summary_response() -> BoundariesSummaryResponse {
        BoundariesSummaryResponse {
            command: "boundaries summary".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            summary: Some(BoundarySummary {
                total_surfaces: 0,
                total_channels: 0,
                by_channel_kind: vec![],
                by_boundary_scope: vec![],
                by_direction: vec![],
                by_protocol_family: vec![],
                by_basis: vec![],
                files_with_boundaries: vec![],
            }),
        }
    }

    #[test]
    fn summary_render_shows_header() {
        let resp = sample_summary_response();
        let output = resp.render_human();
        assert!(output.contains("Boundaries Summary"));
    }

    #[test]
    fn summary_render_shows_totals() {
        let resp = sample_summary_response();
        let output = resp.render_human();
        assert!(output.contains("3 surfaces"));
        assert!(output.contains("5 channels"));
    }

    #[test]
    fn summary_render_shows_by_kind() {
        let resp = sample_summary_response();
        let output = resp.render_human();
        assert!(output.contains("By channel kind:"));
        assert!(output.contains("http_client"));
        assert!(output.contains("database"));
    }

    #[test]
    fn summary_render_shows_by_scope() {
        let resp = sample_summary_response();
        let output = resp.render_human();
        assert!(output.contains("By scope:"));
        assert!(output.contains("external"));
        assert!(output.contains("internal"));
    }

    #[test]
    fn summary_render_shows_by_direction() {
        let resp = sample_summary_response();
        let output = resp.render_human();
        assert!(output.contains("By direction:"));
        assert!(output.contains("outbound"));
        assert!(output.contains("inbound"));
    }

    #[test]
    fn summary_render_shows_files() {
        let resp = sample_summary_response();
        let output = resp.render_human();
        assert!(output.contains("Files with boundaries:"));
        assert!(output.contains("src/api/client.ts"));
        assert!(output.contains("src/db/pool.ts"));
    }

    #[test]
    fn summary_render_empty_shows_hint() {
        let resp = sample_empty_summary_response();
        let output = resp.render_human();
        assert!(output.contains("No architectural boundaries detected"));
        assert!(output.contains("hint:"));
    }

    #[test]
    fn summary_render_sorts_by_count_desc() {
        let resp = sample_summary_response();
        let output = resp.render_human();
        // http_client (3) should come before database (2)
        let http_pos = output.find("http_client").unwrap();
        let db_pos = output.find("database").unwrap();
        assert!(
            http_pos < db_pos,
            "Categories should be sorted by count descending"
        );
    }
}
