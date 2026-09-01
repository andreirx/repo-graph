//! Presentation layer for human-readable CLI output.
//!
//! # CLI-OUT-1 Architecture
//!
//! The daemon returns structured DTOs. This module transforms them into
//! human-readable plain text. The transformation is one-way: CLI reads
//! daemon DTO, renders for human consumption.
//!
//! ## Responsibilities
//!
//! - Parse daemon JSON into typed response structs
//! - Render typed responses as plain text
//! - Provide shared formatting helpers
//! - Hide internal envelope fields from human output
//!
//! ## Non-Responsibilities
//!
//! - Terminal styling / ANSI colors (future slice)
//! - Width-aware formatting (future slice)
//! - Daemon protocol changes
//!
//! # Usage
//!
//! ```rust,ignore
//! use presentation::orient::OrientResponse;
//!
//! let result = client.request("orient", params)?;
//!
//! if args.json {
//!     println!("{}", serde_json::to_string_pretty(&result)?);
//! } else {
//!     let response: OrientResponse = serde_json::from_value(result)?;
//!     println!("{}", response.render_human());
//! }
//! ```

pub mod check;
pub mod cycles;
pub mod deps_list;
// DEPS-ATTRIB-2 §2.4: secondary-ecosystem view. `pub(crate)`, not `pub` (review-2 fix):
// the packet freezes new PUBLIC Rust APIs beyond the additive JSON field; this is a
// guardrail-driven crate-private extraction from `deps_list.rs`, consumed only by the
// sibling `deps_list` presenter within this crate — same convention as `orient_types`,
// `orient_seg2`, `http_boundary` above.
pub(crate) mod deps_list_secondary;
pub mod explain;
pub mod explain_sections;
pub mod graph_edges;
pub mod imports;
pub mod map;
// MODULES-IDENTITY-2 §2.1: crate-local — all items are `pub(crate)`; its only users
// (`orient_seg2`, `modules_list`) are in this crate, so the module is not part of the
// crate's public API (review-0: `pub(crate)` is the ratified visibility).
pub(crate) mod module_disambiguation;
pub mod module_inventory;
pub mod module_shared;
pub mod modules_deps;
pub mod modules_list;
pub mod modules_show;
pub mod modules_violations;
pub mod orient;
// ORIENT-SEGMENT-2 (review-3 §3): guardrail SPLIT modules, not new public APIs — pure
// relocations of `impl OrientResponse` blocks / response DTOs out of `orient.rs` /
// `orient_sections.rs` to hold each file under the 500-line guardrail. They are consumed
// only within this crate; `orient::*` re-exports (`pub use super::orient_types::…`)
// preserve every prior public `presentation::orient::<Type>` path, so nothing public
// changes. `pub(crate)` keeps the split from minting a new public surface (the packet
// freezes new public APIs; only crate-private splits are pre-ratified).
pub(crate) mod orient_guidance;
pub mod orient_reliability;
pub(crate) mod orient_reliability_caveats;
pub mod orient_sections;
// ORIENT-SEGMENT-2 (operator ruling 2, 2026-08-28): rgr-INTERNAL presentation, not a
// new public API — the seg2 renderers/DTOs are consumed only by the sibling orient
// presenters within this crate.
pub(crate) mod orient_seg2;
pub(crate) mod orient_types;
pub mod path;
pub mod reliability;
pub mod seed;
pub mod stats;
pub mod surfaces;
pub mod trust;
/// RECON-M-R3a: shared witness-block rendering (union accounting → reader lines) for
/// trust/orient/stats — one client-side projection, no per-surface phrasing drift.
pub mod witnesses;

// HTTP-BOUNDARY-1: HTTP/REST boundary-map DTO + rendering shared by the
// `surfaces list` and `modules list` presenters (kept off those 500+-line files).
pub(crate) mod http_boundary;

// Group 5: Boundaries (CLI-OUT-4)
pub mod boundaries_list;
pub mod boundaries_show;
pub mod boundaries_summary;

// CLI-OUT-5: Inventory
pub mod docs;
pub mod policy;
pub mod resources;

// CLI-OUT-6: Quality/Risk
pub mod churn;
pub mod coverage;
pub mod hotspots;
pub mod risk;

// CLI-OUT-7: Governance
pub mod assess;
pub mod gate;
pub mod violations;

// ── Shared Helpers ───────────────────────────────────────────────────────────

/// Render a section heading.
///
/// Format: "Heading\n" (with newline for separation from content).
pub fn heading(title: &str) -> String {
    format!("{}\n", title)
}

/// Render a sub-heading (indented section).
///
/// Format: "  SubHeading\n"
pub fn sub_heading(title: &str) -> String {
    format!("  {}\n", title)
}

/// Render a bulleted list item.
///
/// Format: "  - item text\n"
pub fn bullet(item: &str) -> String {
    format!("  - {}\n", item)
}

/// Render multiple items as a bulleted list.
///
/// Returns empty string if items is empty.
pub fn bullet_list(items: &[String]) -> String {
    items.iter().map(|s| bullet(s)).collect()
}

/// Render a key-value line.
///
/// Format: "Key: value\n"
pub fn kv_line(key: &str, value: &str) -> String {
    format!("{}: {}\n", key, value)
}

/// Render a key-value line with indentation.
///
/// Format: "  Key: value\n"
pub fn kv_line_indented(key: &str, value: &str) -> String {
    format!("  {}: {}\n", key, value)
}

/// Render a "next steps" section with suggested commands.
///
/// Commands are rendered as bullet items. Returns empty string if no commands.
pub fn next_steps(commands: &[&str]) -> String {
    if commands.is_empty() {
        return String::new();
    }
    let mut out = heading("Next steps");
    for cmd in commands {
        out.push_str(&bullet(cmd));
    }
    out
}

/// GOV-ARMED-1: the shared "unknown" degradation line for the governance
/// quartet (`gate` / `assess` / `violations` / `modules violations`).
///
/// Each of those surfaces determines whether it is "armed" (any policy /
/// requirement / boundary declaration configured) from an additive
/// configuration-presence field in the daemon response. If that field is
/// ABSENT — e.g. a CLI newer than the daemon it is talking to — the armed
/// state is genuinely UNKNOWN. Per the VISION's honesty rules we never guess
/// "armed" or "not armed" from a missing fact; we say the state is unknown and
/// name the fix. The suffix is identical across all four surfaces, so it lives
/// here to prevent wording drift; only `subject` (the surface's display name)
/// varies.
///
/// GOV-ARMED-1 (review-0 fix): `pub(crate)`, not `pub`. All four callers
/// (gate/assess/violations/modules_violations presenters) live inside this
/// crate; the packet freezes new PUBLIC APIs, and a crate-private helper is the
/// smallest compliant form. Verified via ripgrep across all crates: one
/// definition + exactly these four in-crate call sites, no external reference.
pub(crate) fn armed_unknown_line(subject: &str) -> String {
    format!(
        "{}: armed state unknown — the daemon did not report configuration \
         presence; upgrade rmap and the daemon to matching versions.\n",
        subject
    )
}

/// QUANT-MECH-1 §2.2: the house default row budget for the quantity surfaces
/// (`churn`, `hotspots`) — the top-N rows the human render shows before the
/// remainder line. `--full` renders every row uncapped; the COMPLETE set always
/// rides `--json` (budgets are a HUMAN-render concern only). 25 matches the audit's
/// house standard cap.
pub(crate) const HUMAN_ROW_BUDGET: usize = 25;

/// QUANT-MECH-1 §2.2: the shared "(+N more — --full)" remainder line for the
/// budgeted quantity surfaces (`churn`, `hotspots`).
///
/// Returns `Some` ONLY when the human render bounded the list (`total > shown`);
/// `None` when everything shown (nothing hidden → never a "+0 more" line). `total`
/// is the COMPLETE row count and `shown` the rendered subset, so the remainder
/// count is always TRUE.
///
/// `pub(crate)`: both callers (churn/hotspots presenters) live in this crate; the
/// packet freezes new PUBLIC APIs. One helper backs both so the wording + count stay
/// identical. Rejected simpler: inline the identical line in each renderer — the
/// wording-drift smell that `stats::section_omission_line` already documents.
pub(crate) fn budget_remainder_line(total: usize, shown: usize) -> Option<String> {
    if total <= shown {
        return None;
    }
    Some(format!("  (+{} more — --full)\n", total - shown))
}

/// Severity levels for display grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisplaySeverity {
    High,
    Medium,
    Low,
}

impl DisplaySeverity {
    pub fn parse(s: &str) -> Self {
        match s {
            "high" => Self::High,
            "medium" => Self::Medium,
            _ => Self::Low,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_formats_with_newline() {
        assert_eq!(heading("Signals"), "Signals\n");
    }

    #[test]
    fn bullet_formats_with_indent_and_dash() {
        assert_eq!(bullet("item one"), "  - item one\n");
    }

    #[test]
    fn bullet_list_empty_returns_empty() {
        assert_eq!(bullet_list(&[]), "");
    }

    #[test]
    fn bullet_list_formats_multiple() {
        let items = vec!["one".to_string(), "two".to_string()];
        assert_eq!(bullet_list(&items), "  - one\n  - two\n");
    }

    #[test]
    fn kv_line_formats_correctly() {
        assert_eq!(kv_line("Repo", "my-app"), "Repo: my-app\n");
    }

    #[test]
    fn kv_line_indented_formats_correctly() {
        assert_eq!(kv_line_indented("count", "5"), "  count: 5\n");
    }

    #[test]
    fn next_steps_empty_returns_empty() {
        assert_eq!(next_steps(&[]), "");
    }

    #[test]
    fn next_steps_formats_commands() {
        let out = next_steps(&["rmap check", "rmap explain src/foo.ts"]);
        assert!(out.contains("Next steps\n"));
        assert!(out.contains("  - rmap check\n"));
        assert!(out.contains("  - rmap explain src/foo.ts\n"));
    }

    #[test]
    fn display_severity_parse() {
        assert_eq!(DisplaySeverity::parse("high"), DisplaySeverity::High);
        assert_eq!(DisplaySeverity::parse("medium"), DisplaySeverity::Medium);
        assert_eq!(DisplaySeverity::parse("low"), DisplaySeverity::Low);
        assert_eq!(DisplaySeverity::parse("unknown"), DisplaySeverity::Low);
    }
}

/// HTTP-SURFACE-COHERENCE-1 §2.3 — cross-renderer count coherence.
///
/// The slice mandates ONE shared HTTP aggregation feeding `surfaces list`
/// (headline + footer), `boundaries summary`, and `boundaries list`. In
/// production all three read `unified_http_surfaces` in the daemon; this test
/// closes the loop at the PRESENTATION boundary: given ONE logical HTTP row set,
/// it drives all three human renderers, PARSES their rendered output, and asserts
/// the provider/consumer counts agree — the audit's headline-vs-footer-vs-summary
/// contradiction is impossible if this passes.
#[cfg(test)]
mod http_count_coherence {
    use super::boundaries_list::{BoundariesListResponse, BoundaryListEntry};
    use super::boundaries_summary::partition::Additive;
    use super::boundaries_summary::{BoundariesSummaryResponse, BoundarySummary};
    use super::http_boundary::{render_surfaces, HttpBoundarySurfaceEntry};

    /// One logical HTTP surface, shared by every renderer under test.
    struct Surface {
        direction: &'static str,
        method: &'static str,
        route: Option<&'static str>,
        file: &'static str,
    }

    fn fixture() -> Vec<Surface> {
        vec![
            Surface {
                direction: "provider",
                method: "GET",
                route: Some("/api/a"),
                file: "app/a/route.ts",
            },
            Surface {
                direction: "provider",
                method: "POST",
                route: Some("/api/a"),
                file: "app/a/route.ts",
            },
            Surface {
                direction: "provider",
                method: "GET",
                route: Some("/api/b"),
                file: "app/b/route.ts",
            },
            Surface {
                direction: "provider",
                method: "GET",
                route: None,
                file: "app/c/route.ts",
            },
            Surface {
                direction: "consumer",
                method: "GET",
                route: Some("/api/a"),
                file: "web/client.ts",
            },
            Surface {
                direction: "consumer",
                method: "GET",
                route: Some("/ext"),
                file: "web/other.ts",
            },
        ]
    }

    /// The single source of truth every renderer's count derives from (the
    /// daemon's `http_surface_union::counts` uses this exact rule).
    fn counts(rows: &[Surface]) -> (usize, usize) {
        (
            rows.iter().filter(|r| r.direction == "provider").count(),
            rows.iter().filter(|r| r.direction == "consumer").count(),
        )
    }

    fn to_surfaces_entries(rows: &[Surface]) -> Vec<HttpBoundarySurfaceEntry> {
        rows.iter()
            .map(|r| HttpBoundarySurfaceEntry {
                direction: r.direction.to_string(),
                http_method: r.method.to_string(),
                route: r.route.map(str::to_string),
                source_file: r.file.to_string(),
                is_test: None,
                framework: None,
                route_unknown_reason: None,
                module: None,
                conflict: None,
            })
            .collect()
    }

    fn to_boundaries_entries(rows: &[Surface]) -> Vec<BoundaryListEntry> {
        rows.iter()
            .map(|r| {
                let route = r.route.unwrap_or("<dynamic>");
                BoundaryListEntry {
                    boundary_channel_uid: format!("http:{}:{}", r.method, r.file),
                    channel_kind: "http".to_string(),
                    boundary_scope: "unknown".to_string(),
                    direction: r.direction.to_string(),
                    protocol_family: Some("http".to_string()),
                    service_name: None,
                    file_path: Some(r.file.to_string()),
                    symbol_key: None,
                    confidence: 0.9,
                    basis: None,
                    surface_uid: None,
                    surface_display_name: Some(format!("{} {}", r.method, route)),
                    test_composition: "production".to_string(),
                    test_composition_unknown_reason: None,
                }
            })
            .collect()
    }

    /// Parse "P provider(s), C consumer(s)" out of any of the three renderers'
    /// count phrases.
    fn parse_phrase(line: &str) -> (usize, usize) {
        let after = line.rsplit(':').next().unwrap_or(line);
        let num_before = |kw: &str| -> usize {
            after
                .split(kw)
                .next()
                .and_then(|s| {
                    s.trim()
                        .trim_end_matches(", ")
                        .split_whitespace()
                        .next_back()
                })
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("no {kw} count in {line:?}"))
        };
        (num_before("provider"), num_before("consumer"))
    }

    #[test]
    fn surfaces_footer_summary_line_and_boundaries_rows_agree() {
        let rows = fixture();
        let (p, c) = counts(&rows);

        // 1) surfaces list — parse the footer phrase.
        let surfaces_out = render_surfaces(&to_surfaces_entries(&rows));
        let footer = surfaces_out
            .lines()
            .find(|l| l.starts_with('—'))
            .expect("surfaces footer");
        assert_eq!(
            parse_phrase(footer),
            (p, c),
            "surfaces footer:\n{surfaces_out}"
        );

        // 2) boundaries summary — HTTP line built from the SAME (p, c) the daemon
        //    computes via the shared aggregation.
        let summary = BoundariesSummaryResponse {
            command: "boundaries summary".to_string(),
            repo: "r".to_string(),
            snapshot: "s".to_string(),
            summary: Some(BoundarySummary {
                total_surfaces: (p + c) as u64,
                total_channels: 0,
                by_channel_kind: vec![],
                by_boundary_scope: vec![],
                by_direction: vec![],
                by_protocol_family: vec![],
                by_basis: vec![],
                files_with_boundaries: vec![],
            }),
            http_providers: Some(p),
            http_consumers: Some(c),
            http_degraded: None,
            test_only: Additive::Absent,
            unknown: Additive::Absent,
        };
        let summary_out = summary.render_human();
        let http_line = summary_out
            .lines()
            .find(|l| l.contains("HTTP/REST surfaces:"))
            .expect("summary HTTP line");
        assert_eq!(
            parse_phrase(http_line),
            (p, c),
            "summary line:\n{summary_out}"
        );

        // 3) boundaries list — the grouped view of the SAME rows: sum the ×N
        //    counts split by direction and confirm they reconstruct (p, c).
        let list = BoundariesListResponse {
            command: "boundaries list".to_string(),
            repo: "r".to_string(),
            snapshot: "s".to_string(),
            results: to_boundaries_entries(&rows),
            count: (p + c) as u64,
            filter_kind: None,
            filter_scope: None,
            filter_direction: None,
            filter_family: None,
            filter_file: None,
            filter_file_prefix: None,
            filter_symbol: None,
        };
        let list_out = list.render_human();
        let (mut lp, mut lc) = (0usize, 0usize);
        for line in list_out.lines() {
            // grouped rows are indented `  <direction>  <file>  ×N  <routes>`;
            // the `N file×direction groups` header also contains `×` but is not
            // indented, so require the two-space row prefix.
            if !line.starts_with("  ") {
                continue;
            }
            let Some(times_idx) = line.find('×') else {
                continue;
            };
            let n: usize = line[times_idx + '×'.len_utf8()..]
                .split_whitespace()
                .next()
                .and_then(|t| t.parse().ok())
                .expect("×N count");
            if line.contains("provider") {
                lp += n;
            } else if line.contains("consumer") {
                lc += n;
            }
        }
        assert_eq!((lp, lc), (p, c), "boundaries grouped rows:\n{list_out}");

        // All three renderers reconstructed the identical (providers, consumers).
        assert_eq!((p, c), (4, 2));
    }
}
