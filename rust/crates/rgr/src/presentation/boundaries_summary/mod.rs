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
//!
//! # Module layout
//!
//! Two crate-private children hold what would otherwise push this file over the 500-line
//! guardrail (review-1 finding #3): [`partition`] — the FIXTURE-POLLUTION-1 §2.2
//! production/test-only partition (subtraction + trailing render); and `tests` (`tests.rs`)
//! — the renderer unit tests. This file owns the DTOs and the `render_human` orchestration.

use serde::Deserialize;

// `pub(crate)` (not private) so crate-mates — the presentation smoke tests in the sibling
// `presentation` module — can name `partition::Additive` when constructing a response.
pub(crate) mod partition;

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
    /// §2.3 — the UNIFIED HTTP provider/consumer counts (same aggregation the surfaces
    /// footer prints). `None` = the union read degraded.
    #[serde(default)]
    http_surface_providers: Option<usize>,
    #[serde(default)]
    http_surface_consumers: Option<usize>,
    #[serde(default)]
    http_surface_degraded: Option<String>,
    /// FIXTURE-POLLUTION-1 §2.2 (review-1 #2b) — the additive test-only sub-summary, captured
    /// RAW so [`partition::Additive::parse`] can enforce a strict field contract (review-2 #2:
    /// a partial payload degrades, it never zero-fills). Absent for a repo with no test-only
    /// surface (byte-identical pre-slice output).
    #[serde(default)]
    test_only_summary: Option<serde_json::Value>,
    /// FIXTURE-POLLUTION-1 §2.4 + binding direction rule (review-2 #1) — the additive
    /// unknown-composition disclosure, captured RAW for the same strict parse. Absent when no
    /// reconciled surface has unknown test-composition.
    #[serde(default)]
    unknown_composition: Option<serde_json::Value>,
}

/// Response structure for boundaries summary command (normalized).
#[derive(Debug)]
pub struct BoundariesSummaryResponse {
    pub command: String,
    pub repo: String,
    pub snapshot: String,
    pub summary: Option<BoundarySummary>,
    /// §2.3 — unified HTTP provider/consumer counts (see the DTO fields). Both `Some` =
    /// counts available; `http_degraded` set = the union read failed.
    pub http_providers: Option<usize>,
    pub http_consumers: Option<usize>,
    pub http_degraded: Option<String>,
    /// FIXTURE-POLLUTION-1 §2.2 (review-1 #2b) — the test-only sub-summary: subtracted from
    /// the headline and rendered as a trailing disclosure when `Ready`; the headline stays the
    /// FULL summary (nothing subtracted) when `Absent` or `Degraded`. `pub(crate)`: an
    /// internal presentation detail (the partition types are crate-private).
    pub(crate) test_only: partition::Additive<partition::TestOnlySummary>,
    /// FIXTURE-POLLUTION-1 §2.4 + binding direction rule (review-2 #1) — the unknown-
    /// composition disclosure. Unknown surfaces are NEVER subtracted from the headline; this
    /// annotates them so the headline reads as production+unknown, not confirmed production.
    pub(crate) unknown: partition::Additive<partition::UnknownComposition>,
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
            http_providers: dto.http_surface_providers,
            http_consumers: dto.http_surface_consumers,
            http_degraded: dto.http_surface_degraded,
            // Strict parse (review-2 #2): a present-but-malformed disclosure becomes
            // `Degraded`, disclosed rather than silently zero-filled.
            test_only: partition::Additive::parse(dto.test_only_summary, "test-only"),
            unknown: partition::Additive::parse(dto.unknown_composition, "unknown-composition"),
        })
    }
}

impl BoundariesSummaryResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        out.push_str("Boundaries Summary\n\n");

        let full = match &self.summary {
            Some(s) => s,
            None => {
                out.push_str("No summary data available.\n");
                return out;
            }
        };

        // FIXTURE-POLLUTION-1 §2.2 (review-1 #2b): the HEADLINE is production+unknown — the
        // full reconciled summary MINUS the test-only sub-summary. Test-only architecture is
        // never in the headline totals/breakdowns; it is disclosed in a trailing section.
        // Only a `Ready` test-only sub-summary is subtracted: `Absent` (no test-only content)
        // and `Degraded` (malformed payload, review-2 #2) both leave the headline as the FULL
        // summary — a degraded partition never hides possibly-real architecture.
        let subtract = match &self.test_only {
            partition::Additive::Ready(t) => Some(t),
            partition::Additive::Absent | partition::Additive::Degraded(_) => None,
        };
        let headline = match subtract {
            Some(t) => t.headline_from(full),
            None => full.clone(),
        };
        // The HTTP callout is likewise headline-only (subtract the test-only unified split).
        let (http_providers, http_consumers) = match subtract {
            Some(t) => (
                self.http_providers
                    .map(|p| p.saturating_sub(t.http_providers)),
                self.http_consumers
                    .map(|c| c.saturating_sub(t.http_consumers)),
            ),
            None => (self.http_providers, self.http_consumers),
        };

        // -- Totals (headline) --
        out.push_str(&format!("{} surfaces\n", headline.total_surfaces));
        out.push_str(&format!("{} channels\n", headline.total_channels));

        // -- HTTP/REST (§2.3): the unified provider/consumer count, from the SAME read-time
        // union the `surfaces list` footer prints. A degraded union read is UNKNOWN, never a
        // silent zero.
        out.push_str(&render_http_line(
            http_providers,
            http_consumers,
            self.http_degraded.as_deref(),
        ));

        // -- Empty case -- (only when the headline is empty AND there is no additive
        // disclosure to render — a test-only or unknown disclosure must not be swallowed).
        if headline.total_surfaces == 0
            && headline.total_channels == 0
            && !self.test_only.has_content()
            && !self.unknown.has_content()
        {
            out.push_str("\nNo architectural boundaries detected.\n");
            out.push_str(
                "\nhint: boundaries connect surfaces to resources. Without detected surfaces,\n",
            );
            out.push_str("      no boundaries can be established.\n");
            return out;
        }

        push_breakdown(&mut out, "\nBy channel kind:\n", &headline.by_channel_kind);
        push_breakdown(&mut out, "\nBy scope:\n", &headline.by_boundary_scope);
        push_breakdown(&mut out, "\nBy direction:\n", &headline.by_direction);
        push_breakdown(&mut out, "\nBy protocol:\n", &headline.by_protocol_family);
        push_breakdown(&mut out, "\nBy basis:\n", &headline.by_basis);

        // -- Files with boundaries (headline) --
        if !headline.files_with_boundaries.is_empty() {
            out.push_str("\nFiles with boundaries:\n");
            let mut files = headline.files_with_boundaries.clone();
            files.sort();
            // Full output, no truncation (daemon only sends file paths, no counts)
            for file in &files {
                out.push_str(&format!("  {}\n", file));
            }
        }

        // -- Unknown-composition disclosure (§2.4 + binding direction rule, review-2 #1) --
        // These surfaces STAY in the headline; this only annotates that some are unprovable.
        match &self.unknown {
            partition::Additive::Ready(u) => out.push_str(&u.render_disclosure()),
            partition::Additive::Degraded(reason) => out.push_str(&format!(
                "\nnote: unknown test-composition disclosure unavailable — {} (headline may \
                 include unprovable surfaces).\n",
                reason
            )),
            partition::Additive::Absent => {}
        }

        // -- Trailing test-only disclosure (§2.2, review-1 #2b) --
        match &self.test_only {
            partition::Additive::Ready(t) => out.push_str(&t.render_trailing()),
            // A degraded test-only partition was NOT subtracted (headline is the full summary);
            // say so rather than silently dropping the demotion.
            partition::Additive::Degraded(reason) => out.push_str(&format!(
                "\nnote: test-only partition unavailable — {} (headline shown WITHOUT test-only \
                 demotion; some surfaces above may be test-only).\n",
                reason
            )),
            partition::Additive::Absent => {}
        }

        out
    }
}

/// A `count desc, then category asc` breakdown block under `header`, rendered only when
/// non-empty (mirrors the pre-slice per-breakdown block, extracted so the headline and the
/// trailing section share one ordering).
fn push_breakdown(out: &mut String, header: &str, items: &[CategoryCount]) {
    if items.is_empty() {
        return;
    }
    out.push_str(header);
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.category.cmp(&b.category))
    });
    for item in &sorted {
        out.push_str(&format!("  {}  {}\n", item.count, item.category));
    }
}

/// §2.3 — the unified HTTP provider/consumer line. Both counts present → the "P providers,
/// C consumers" phrase (matching the surfaces footer format); a degraded read → UNKNOWN with
/// the reason; neither present → empty (older daemons that don't send the field).
fn render_http_line(
    providers: Option<usize>,
    consumers: Option<usize>,
    degraded: Option<&str>,
) -> String {
    if let Some(reason) = degraded {
        return format!(
            "\nHTTP/REST surfaces: unknown — {} (not reporting 0; rerun after reindex).\n",
            reason
        );
    }
    match (providers, consumers) {
        // Genuinely no HTTP surfaces → no line (a repo with no HTTP stays clean and
        // byte-stable; the line only adds signal when there IS an HTTP story).
        (Some(0), Some(0)) => String::new(),
        (Some(p), Some(c)) => format!(
            "\nHTTP/REST surfaces: {} provider{}, {} consumer{}\n",
            p,
            if p == 1 { "" } else { "s" },
            c,
            if c == 1 { "" } else { "s" },
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
