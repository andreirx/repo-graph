//! Presentation layer for graph edge queries (callers, callees).
//!
//! # CLI-OUT-3
//!
//! Shared support module for callers and callees commands.
//! These commands have identical response structures and change for the same reasons,
//! so they share rendering logic with thin command-specific wrappers.
//!
//! ## Human Output Structure
//!
//! ```text
//! Callers of OpenXcom::State::State
//! File: src/Engine/State.cpp:51
//!
//! 5 callers found
//!
//!   OpenXcom::Game::run          src/Engine/Game.cpp:234     CALLS  static
//!   OpenXcom::Menu::init         src/Menu/Menu.cpp:45        CALLS  static
//!   ...
//! ```

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Target symbol information in callers/callees response.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetSymbol {
    #[serde(default)]
    pub stable_key: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub subtype: Option<String>,
    pub file: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
}

/// Edge symbol (caller or callee).
#[derive(Debug, Clone, Deserialize)]
pub struct EdgeSymbol {
    #[serde(default)]
    pub stable_key: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub subtype: Option<String>,
    pub file: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
    #[serde(default)]
    pub edge_type: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
}

/// Response structure for callers command.
#[derive(Debug, Deserialize)]
pub struct CallersResponse {
    pub target: TargetSymbol,
    pub callers: Vec<EdgeSymbol>,
    pub count: usize,
}

/// Response structure for callees command.
#[derive(Debug, Deserialize)]
pub struct CalleesResponse {
    pub target: TargetSymbol,
    pub callees: Vec<EdgeSymbol>,
    pub count: usize,
}

// ── Direction for shared rendering ───────────────────────────────────────────

/// Direction label for shared renderer.
#[derive(Debug, Clone, Copy)]
pub enum EdgeDirection {
    Callers,
    Callees,
}

impl EdgeDirection {
    fn label(&self) -> &'static str {
        match self {
            Self::Callers => "Callers",
            Self::Callees => "Callees",
        }
    }

    fn count_label(&self, count: usize) -> String {
        let noun = match self {
            Self::Callers => "caller",
            Self::Callees => "callee",
        };
        if count == 1 {
            format!("1 {} found", noun)
        } else {
            format!("{} {}s found", count, noun)
        }
    }
}

// ── Human Rendering ──────────────────────────────────────────────────────────

/// Render graph edge response as human-readable text.
///
/// Shared implementation for both callers and callees.
pub fn render_graph_edges(
    direction: EdgeDirection,
    target: &TargetSymbol,
    edges: &[EdgeSymbol],
    count: usize,
) -> String {
    let mut out = String::new();

    // ── Header ─────────────────────────────────────────────────
    let target_name = target.qualified_name.as_deref().unwrap_or(&target.name);
    out.push_str(&format!("{} of {}\n", direction.label(), target_name));
    out.push_str(&format!("File: {}:{}\n\n", target.file, target.line));

    // ── Count ──────────────────────────────────────────────────
    out.push_str(&direction.count_label(count));
    out.push('\n');

    if edges.is_empty() {
        return out;
    }

    out.push('\n');

    // ── Edge list ──────────────────────────────────────────────
    for edge in edges {
        let name = edge.qualified_name.as_deref().unwrap_or(&edge.name);
        let location = format!("{}:{}", edge.file, edge.line);
        let edge_type = edge.edge_type.as_deref().unwrap_or("-");
        let resolution = edge.resolution.as_deref().unwrap_or("-");

        out.push_str(&format!(
            "  {}  {}  {}  {}\n",
            name, location, edge_type, resolution
        ));
    }

    out
}

impl CallersResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        render_graph_edges(
            EdgeDirection::Callers,
            &self.target,
            &self.callers,
            self.count,
        )
    }
}

impl CalleesResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        render_graph_edges(
            EdgeDirection::Callees,
            &self.target,
            &self.callees,
            self.count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> TargetSymbol {
        TargetSymbol {
            stable_key: "repo_123:src/foo.cpp#Foo::bar:SYMBOL:METHOD".to_string(),
            name: "bar".to_string(),
            qualified_name: Some("Foo::bar".to_string()),
            kind: "SYMBOL".to_string(),
            subtype: Some("METHOD".to_string()),
            file: "src/foo.cpp".to_string(),
            line: 42,
            column: 0,
        }
    }

    fn sample_edges() -> Vec<EdgeSymbol> {
        vec![
            EdgeSymbol {
                stable_key: "repo_123:src/main.cpp#main:SYMBOL:FUNCTION".to_string(),
                name: "main".to_string(),
                qualified_name: Some("main".to_string()),
                kind: "SYMBOL".to_string(),
                subtype: Some("FUNCTION".to_string()),
                file: "src/main.cpp".to_string(),
                line: 10,
                column: 0,
                edge_type: Some("CALLS".to_string()),
                resolution: Some("static".to_string()),
            },
            EdgeSymbol {
                stable_key: "repo_123:src/helper.cpp#Helper::run:SYMBOL:METHOD".to_string(),
                name: "run".to_string(),
                qualified_name: Some("Helper::run".to_string()),
                kind: "SYMBOL".to_string(),
                subtype: Some("METHOD".to_string()),
                file: "src/helper.cpp".to_string(),
                line: 55,
                column: 0,
                edge_type: Some("CALLS".to_string()),
                resolution: Some("static".to_string()),
            },
        ]
    }

    #[test]
    fn render_callers_includes_header() {
        let resp = CallersResponse {
            target: sample_target(),
            callers: sample_edges(),
            count: 2,
        };
        let output = resp.render_human();
        assert!(output.contains("Callers of Foo::bar"));
        assert!(output.contains("File: src/foo.cpp:42"));
    }

    #[test]
    fn render_callers_includes_count() {
        let resp = CallersResponse {
            target: sample_target(),
            callers: sample_edges(),
            count: 2,
        };
        let output = resp.render_human();
        assert!(output.contains("2 callers found"));
    }

    #[test]
    fn render_callers_singular_count() {
        let mut edges = sample_edges();
        edges.pop();
        let resp = CallersResponse {
            target: sample_target(),
            callers: edges,
            count: 1,
        };
        let output = resp.render_human();
        assert!(output.contains("1 caller found"));
    }

    #[test]
    fn render_callers_includes_edges() {
        let resp = CallersResponse {
            target: sample_target(),
            callers: sample_edges(),
            count: 2,
        };
        let output = resp.render_human();
        assert!(output.contains("main"));
        assert!(output.contains("src/main.cpp:10"));
        assert!(output.contains("CALLS"));
        assert!(output.contains("Helper::run"));
    }

    #[test]
    fn render_callees_uses_callees_label() {
        let resp = CalleesResponse {
            target: sample_target(),
            callees: sample_edges(),
            count: 2,
        };
        let output = resp.render_human();
        assert!(output.contains("Callees of Foo::bar"));
        assert!(output.contains("2 callees found"));
    }

    #[test]
    fn render_empty_callers() {
        let resp = CallersResponse {
            target: sample_target(),
            callers: vec![],
            count: 0,
        };
        let output = resp.render_human();
        assert!(output.contains("0 callers found"));
        // No edge lines after count
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 4); // header, file, blank, count
    }
}
