//! Presentation layer for the `path` command.
//!
//! # CLI-OUT-3
//!
//! Renders shortest path between two symbols as human-readable text.
//! Shows ordered route with edge types between hops.
//!
//! ## Human Output Structure
//!
//! When path found:
//! ```text
//! Path: OpenXcom::State::State -> OpenXcom::Game::getCursor
//!
//! 1 hop
//!
//!   OpenXcom::State::State       src/Engine/State.cpp:51
//!     -> CALLS
//!   OpenXcom::Game::getCursor    src/Engine/Game.cpp:417
//! ```
//!
//! When not found:
//! ```text
//! Path: OpenXcom::State::State -> OpenXcom::Game::run
//!
//! No path found.
//! ```

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// A node in the path.
#[derive(Debug, Clone, Deserialize)]
pub struct PathNode {
    #[serde(default)]
    pub node_id: String,
    pub symbol: String,
    pub file: String,
    #[serde(default)]
    pub line: u32,
    /// Edge type leading TO this node (empty for first node).
    #[serde(default)]
    pub edge_type: String,
}

/// The path result within the response.
#[derive(Debug, Clone, Deserialize)]
pub struct PathResult {
    pub found: bool,
    pub path_length: usize,
    pub path: Vec<PathNode>,
}

/// Response structure for path command.
#[derive(Debug, Deserialize)]
pub struct PathResponse {
    #[serde(default)]
    pub repo_uid: String,
    #[serde(default)]
    pub snapshot_uid: String,
    pub path: PathResult,
    pub found: bool,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl PathResponse {
    /// Render as human-readable text with explicit query terms.
    ///
    /// When path is not found, the header still shows the user's original query
    /// rather than "? -> ?".
    pub fn render_human_with_query(&self, from_query: &str, to_query: &str) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        // Use resolved symbols from path if found, otherwise preserve query terms
        let (from_name, to_name) = if self.path.path.len() >= 2 {
            (
                self.path
                    .path
                    .first()
                    .map(|n| n.symbol.as_str())
                    .unwrap_or(from_query),
                self.path
                    .path
                    .last()
                    .map(|n| n.symbol.as_str())
                    .unwrap_or(to_query),
            )
        } else if self.path.path.len() == 1 {
            let name = &self.path.path[0].symbol;
            (name.as_str(), name.as_str())
        } else {
            // No path found - preserve user's query terms in header
            (from_query, to_query)
        };

        out.push_str(&format!("Path: {} -> {}\n\n", from_name, to_name));

        // ── Not found ──────────────────────────────────────────────
        if !self.found || !self.path.found {
            out.push_str("No path found.\n");
            return out;
        }

        // ── Hop count ──────────────────────────────────────────────
        let hops = self.path.path_length;
        if hops == 1 {
            out.push_str("1 hop\n\n");
        } else {
            out.push_str(&format!("{} hops\n\n", hops));
        }

        // ── Path nodes ─────────────────────────────────────────────
        for (i, node) in self.path.path.iter().enumerate() {
            let location = format!("{}:{}", node.file, node.line);
            out.push_str(&format!("  {}  {}\n", node.symbol, location));

            // Show edge to next node (if not last)
            if i < self.path.path.len() - 1 {
                // Edge type is on the NEXT node (edge leading to it)
                if let Some(next) = self.path.path.get(i + 1) {
                    let edge = if next.edge_type.is_empty() {
                        "->".to_string()
                    } else {
                        format!("-> {}", next.edge_type)
                    };
                    out.push_str(&format!("    {}\n", edge));
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path_found() -> PathResponse {
        PathResponse {
            repo_uid: "repo_123".to_string(),
            snapshot_uid: "snap_456".to_string(),
            found: true,
            path: PathResult {
                found: true,
                path_length: 2,
                path: vec![
                    PathNode {
                        node_id: "n1".to_string(),
                        symbol: "Foo::start".to_string(),
                        file: "src/foo.cpp".to_string(),
                        line: 10,
                        edge_type: "".to_string(),
                    },
                    PathNode {
                        node_id: "n2".to_string(),
                        symbol: "Bar::process".to_string(),
                        file: "src/bar.cpp".to_string(),
                        line: 20,
                        edge_type: "CALLS".to_string(),
                    },
                    PathNode {
                        node_id: "n3".to_string(),
                        symbol: "Baz::finish".to_string(),
                        file: "src/baz.cpp".to_string(),
                        line: 30,
                        edge_type: "CALLS".to_string(),
                    },
                ],
            },
        }
    }

    fn sample_path_not_found() -> PathResponse {
        PathResponse {
            repo_uid: "repo_123".to_string(),
            snapshot_uid: "snap_456".to_string(),
            found: false,
            path: PathResult {
                found: false,
                path_length: 0,
                path: vec![],
            },
        }
    }

    #[test]
    fn render_path_found_shows_header() {
        let resp = sample_path_found();
        let output = resp.render_human_with_query("Foo::start", "Baz::finish");
        assert!(output.contains("Path: Foo::start -> Baz::finish"));
    }

    #[test]
    fn render_path_found_shows_hop_count() {
        let resp = sample_path_found();
        let output = resp.render_human_with_query("Foo::start", "Baz::finish");
        assert!(output.contains("2 hops"));
    }

    #[test]
    fn render_path_found_shows_nodes() {
        let resp = sample_path_found();
        let output = resp.render_human_with_query("Foo::start", "Baz::finish");
        assert!(output.contains("Foo::start"));
        assert!(output.contains("src/foo.cpp:10"));
        assert!(output.contains("Bar::process"));
        assert!(output.contains("src/bar.cpp:20"));
        assert!(output.contains("Baz::finish"));
        assert!(output.contains("src/baz.cpp:30"));
    }

    #[test]
    fn render_path_found_shows_edges() {
        let resp = sample_path_found();
        let output = resp.render_human_with_query("Foo::start", "Baz::finish");
        assert!(output.contains("-> CALLS"));
    }

    #[test]
    fn render_path_not_found_preserves_query_terms() {
        let resp = sample_path_not_found();
        // Key test: query terms preserved in header even when path not found
        let output = resp.render_human_with_query("MyClass::start", "OtherClass::end");
        assert!(output.contains("Path: MyClass::start -> OtherClass::end"));
        assert!(output.contains("No path found."));
    }

    #[test]
    fn render_single_hop() {
        let resp = PathResponse {
            repo_uid: "r".to_string(),
            snapshot_uid: "s".to_string(),
            found: true,
            path: PathResult {
                found: true,
                path_length: 1,
                path: vec![
                    PathNode {
                        node_id: "n1".to_string(),
                        symbol: "A".to_string(),
                        file: "a.cpp".to_string(),
                        line: 1,
                        edge_type: "".to_string(),
                    },
                    PathNode {
                        node_id: "n2".to_string(),
                        symbol: "B".to_string(),
                        file: "b.cpp".to_string(),
                        line: 2,
                        edge_type: "CALLS".to_string(),
                    },
                ],
            },
        };
        let output = resp.render_human_with_query("A", "B");
        assert!(output.contains("1 hop"));
        assert!(!output.contains("hops")); // singular
    }
}
