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

/// Generic count by category entry (for rendering).
#[derive(Debug, Clone)]
pub struct CategoryCount {
    pub category: String,
    pub count: u64,
}

// Category-specific DTOs matching daemon's camelCase field names
#[derive(Debug, Clone, Deserialize)]
struct ChannelKindCount {
    #[serde(default, rename = "channelKind")]
    kind: String,
    #[serde(default)]
    count: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct BoundaryScopeCount {
    #[serde(default, rename = "boundaryScope")]
    scope: String,
    #[serde(default)]
    count: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DirectionCount {
    #[serde(default)]
    direction: String,
    #[serde(default)]
    count: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct ProtocolFamilyCount {
    #[serde(default, rename = "protocolFamily")]
    family: String,
    #[serde(default)]
    count: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct BasisCount {
    #[serde(default)]
    basis: String,
    #[serde(default)]
    count: u64,
}

/// Summary data object (internal DTO matching daemon response).
#[derive(Debug, Clone, Deserialize)]
struct BoundarySummaryDto {
    #[serde(default, rename = "totalSurfaces")]
    total_surfaces: u64,
    #[serde(default, rename = "totalChannels")]
    total_channels: u64,
    #[serde(default, rename = "byChannelKind")]
    by_channel_kind: Vec<ChannelKindCount>,
    #[serde(default, rename = "byBoundaryScope")]
    by_boundary_scope: Vec<BoundaryScopeCount>,
    #[serde(default, rename = "byDirection")]
    by_direction: Vec<DirectionCount>,
    #[serde(default, rename = "byProtocolFamily")]
    by_protocol_family: Vec<ProtocolFamilyCount>,
    #[serde(default, rename = "byBasis")]
    by_basis: Vec<BasisCount>,
    #[serde(default, rename = "filesWithBoundaries")]
    files_with_boundaries: Vec<String>,
}

/// Summary data object (normalized for rendering).
#[derive(Debug, Clone)]
pub struct BoundarySummary {
    pub total_surfaces: u64,
    pub total_channels: u64,
    pub by_channel_kind: Vec<CategoryCount>,
    pub by_boundary_scope: Vec<CategoryCount>,
    pub by_direction: Vec<CategoryCount>,
    pub by_protocol_family: Vec<CategoryCount>,
    pub by_basis: Vec<CategoryCount>,
    pub files_with_boundaries: Vec<String>,
}

impl From<BoundarySummaryDto> for BoundarySummary {
    fn from(dto: BoundarySummaryDto) -> Self {
        BoundarySummary {
            total_surfaces: dto.total_surfaces,
            total_channels: dto.total_channels,
            by_channel_kind: dto
                .by_channel_kind
                .into_iter()
                .map(|c| CategoryCount {
                    category: c.kind,
                    count: c.count,
                })
                .collect(),
            by_boundary_scope: dto
                .by_boundary_scope
                .into_iter()
                .map(|c| CategoryCount {
                    category: c.scope,
                    count: c.count,
                })
                .collect(),
            by_direction: dto
                .by_direction
                .into_iter()
                .map(|c| CategoryCount {
                    category: c.direction,
                    count: c.count,
                })
                .collect(),
            by_protocol_family: dto
                .by_protocol_family
                .into_iter()
                .map(|c| CategoryCount {
                    category: c.family,
                    count: c.count,
                })
                .collect(),
            by_basis: dto
                .by_basis
                .into_iter()
                .map(|c| CategoryCount {
                    category: c.basis,
                    count: c.count,
                })
                .collect(),
            files_with_boundaries: dto.files_with_boundaries,
        }
    }
}

/// Response DTO for deserialization (matches daemon response).
#[derive(Debug, Deserialize)]
struct BoundariesSummaryResponseDto {
    #[serde(default)]
    command: String,
    #[serde(default)]
    repo: String,
    #[serde(default)]
    snapshot: String,
    #[serde(default)]
    summary: Option<BoundarySummaryDto>,
}

/// Response structure for boundaries summary command (normalized).
#[derive(Debug)]
pub struct BoundariesSummaryResponse {
    pub command: String,
    pub repo: String,
    pub snapshot: String,
    pub summary: Option<BoundarySummary>,
}

impl BoundariesSummaryResponse {
    /// Parse from JSON value (daemon response).
    pub fn from_json(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        let dto: BoundariesSummaryResponseDto = serde_json::from_value(value)?;
        Ok(BoundariesSummaryResponse {
            command: dto.command,
            repo: dto.repo,
            snapshot: dto.snapshot,
            summary: dto.summary.map(BoundarySummary::from),
        })
    }
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

        // -- Files with boundaries --
        if !summary.files_with_boundaries.is_empty() {
            out.push_str("\nFiles with boundaries:\n");
            let mut files = summary.files_with_boundaries.clone();
            files.sort();

            // Full output, no truncation (daemon only sends file paths, no counts)
            for file in &files {
                out.push_str(&format!("  {}\n", file));
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
                    "src/api/client.ts".to_string(),
                    "src/db/pool.ts".to_string(),
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
