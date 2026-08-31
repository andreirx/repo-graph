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
//!
//! # Module layout
//!
//! Two crate-private children hold what would otherwise push this file over the 500-line
//! guardrail (review-1 finding #3): [`group`] — the (file × direction) rollup and the
//! FIXTURE-POLLUTION-1 §2.2 production/test-only partition; and `tests` (`tests.rs`) — the
//! renderer unit tests. This file owns the DTO and the `render_human` orchestration.

use serde::Deserialize;

use super::module_shared::format_count;

mod group;

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
    /// FIXTURE-POLLUTION-1 §2.2 + binding direction rule: the row's test-composition
    /// discriminant emitted by the daemon (`test_only` / `production` / `unknown`). A
    /// `test_only` row is DEMOTED below the production headline; an `unknown` row (no
    /// reachable `is_test` fact) is NEVER demoted — it stays in the main listing carrying
    /// its reason. Absent (a payload predating this field, or a serving path that does not
    /// classify) parses to `Unknown` via [`BoundaryListEntry::composition`], never a silent
    /// production default.
    #[serde(default, rename = "test_composition")]
    pub test_composition: String,
    /// The reader-framed reason present ONLY when `test_composition == "unknown"`.
    #[serde(default)]
    pub test_composition_unknown_reason: Option<String>,
}

/// FIXTURE-POLLUTION-1 binding direction rule (presentation mirror of the daemon
/// `TestComposition`): a surface is positively test-only, positively production, or
/// unprovable — three mutually-exclusive states, so demotion (TestOnly only) is never
/// confused with the conservative main-listing placement (Production or Unknown). This is a
/// NEUTRAL test-only classification from the stored `is_test` fact, not a provenance claim.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RowComposition {
    TestOnly,
    Production,
    Unknown(String),
}

impl BoundaryListEntry {
    /// Decode the daemon discriminant into the three-state domain value. An unrecognized or
    /// ABSENT discriminant is `Unknown` WITH a reason — never a production default (the
    /// review-0 collapse). The reason falls back to a fixed explanation only when the daemon
    /// omitted it (kept explicit — this is a display string, not a classification).
    fn composition(&self) -> RowComposition {
        match self.test_composition.as_str() {
            "test_only" => RowComposition::TestOnly,
            "production" => RowComposition::Production,
            _ => RowComposition::Unknown(match &self.test_composition_unknown_reason {
                Some(reason) => reason.clone(),
                None => "test-composition fact not provided by the serving path".to_string(),
            }),
        }
    }
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

        // Group ONCE (review-1 #2a): the headline counts GROUPS (file × direction), not
        // rows, so multiple rows sharing a group do not inflate the "N real groups" count.
        // Binding direction rule: only positively-test-only groups are demoted; UNKNOWN
        // groups stay in the main listing (counted here) carrying their marker.
        let grouped = group::group_and_render(&self.results);

        // -- Count (main headline; test-only groups demoted below, §2.2) --
        out.push_str(&format!(
            "{}\n",
            format_count(grouped.main_group_count, "boundary", "boundaries")
        ));
        if grouped.test_only_group_count > 0 {
            out.push_str(&format!(
                "+{} test-only surface{} (excluded from the headline)\n",
                grouped.test_only_group_count,
                if grouped.test_only_group_count == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }

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

        out.push_str(&grouped.body);
        out
    }
}

#[cfg(test)]
mod tests;
