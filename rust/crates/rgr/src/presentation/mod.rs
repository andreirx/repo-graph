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
pub mod explain;
pub mod explain_sections;
pub mod graph_edges;
pub mod imports;
pub mod map;
pub mod module_inventory;
pub mod module_shared;
pub mod modules_deps;
pub mod modules_list;
pub mod modules_show;
pub mod modules_violations;
pub mod orient;
pub mod orient_reliability;
pub mod orient_sections;
pub mod path;
pub mod stats;
pub mod surfaces;
pub mod trust;

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
