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
///
/// PYTHON-RECEIVER-BINDING-1 (D-PRB-SCOPE-1 addendum): when no route through certain edges
/// exists and one exists through inferred call/import edges, the daemon adds
/// `inferred_edges_on_route` and `search_depth`. A count WITHOUT its depth is a decode error —
/// never a guessed bound.
#[derive(Debug, Deserialize)]
#[serde(try_from = "PathResponseWire")]
pub struct PathResponse {
    pub repo_uid: String,
    pub snapshot_uid: String,
    pub path: PathResult,
    pub found: bool,
    /// `(inferred edges on the uncertified route, the search depth both walks used)`.
    pub inferred_route: Option<(u64, u64)>,
}

/// The wire shape of [`PathResponse`] before the inferred-route pair is validated.
#[derive(Debug, Deserialize)]
struct PathResponseWire {
    #[serde(default)]
    repo_uid: String,
    #[serde(default)]
    snapshot_uid: String,
    path: PathResult,
    found: bool,
    #[serde(default)]
    inferred_edges_on_route: Option<u64>,
    #[serde(default)]
    search_depth: Option<u64>,
}

impl TryFrom<PathResponseWire> for PathResponse {
    type Error = String;

    fn try_from(w: PathResponseWire) -> Result<Self, Self::Error> {
        let inferred_route = match (w.inferred_edges_on_route, w.search_depth) {
            (Some(n), Some(d)) => Some((n, d)),
            (None, _) => None,
            (Some(_), None) => {
                return Err(
                    "path response carries inferred_edges_on_route without search_depth"
                        .to_string(),
                )
            }
        };
        Ok(Self {
            repo_uid: w.repo_uid,
            snapshot_uid: w.snapshot_uid,
            path: w.path,
            found: w.found,
            inferred_route,
        })
    }
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
            // PYTHON-RECEIVER-BINDING-1: an absent certain route is never presented as proof of
            // absence when inferred call/import edges connect the endpoints.
            // TEST-EDGE-SCOPE-1B (D-TESB-15): the next action is the runnable command with the
            // user's two queries (worded by `import_partition`).
            if let Some((n, depth)) = self.inferred_route {
                out.push_str(&format!(
                    "no route through certain edges within depth {depth}; {}\n",
                    crate::presentation::import_partition::path_no_route_line(
                        n, from_query, to_query
                    )
                ));
            }
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

        // TEST-EDGE-SCOPE-1B (D-TESB-15): a route found with `--include-inferred` states how many
        // of its hops are inferred call/import edges.
        if let Some((n, _)) = self.inferred_route {
            if n > 0 {
                out.push_str(&format!(
                    "{}\n",
                    crate::presentation::import_partition::path_inferred_hops_line(n, hops as u64)
                ));
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
            inferred_route: None,
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
            inferred_route: None,
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
            inferred_route: None,
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

    fn parse_path(
        extra: serde_json::Value,
        found: bool,
    ) -> Result<PathResponse, serde_json::Error> {
        let path = if found {
            serde_json::json!({"found": true, "path_length": 2, "path": [
                {"node_id": "n1", "symbol": "A.f", "file": "a.py", "line": 1, "edge_type": ""},
                {"node_id": "n2", "symbol": "M.h", "file": "m.py", "line": 5, "edge_type": "CALLS"},
                {"node_id": "n3", "symbol": "B.g", "file": "b.py", "line": 9, "edge_type": "IMPORTS"}
            ]})
        } else {
            serde_json::json!({"found": false, "path_length": 0, "path": []})
        };
        let mut v = serde_json::json!({
            "repo_uid": "r", "snapshot_uid": "s", "found": found, "path": path
        });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v)
    }

    #[test]
    fn render_path_not_found_names_the_runnable_include_inferred_command() {
        // D-TESB-15 (the new identity of `render_path_not_found_states_the_inferred_edges_on_route_
        // within_the_depth`): the next action is `rmap path <from> <to> --include-inferred` with
        // the user's two queries, quoted for the shell when needed.
        let one = parse_path(
            serde_json::json!({"inferred_edges_on_route": 1, "search_depth": 8}),
            false,
        )
        .unwrap();
        let out = one.render_human_with_query("A.f", "B.g");
        assert!(out.contains("No path found."), "{out}");
        assert!(
            out.contains("no route through certain edges within depth 8; a route exists using 1 inferred call/import edge — investigate with rmap path A.f B.g --include-inferred\n"),
            "{out}"
        );
        let three = parse_path(
            serde_json::json!({"inferred_edges_on_route": 3, "search_depth": 8}),
            false,
        )
        .unwrap();
        assert!(three
            .render_human_with_query("pkg mod::f", "B.g")
            .contains("a route exists using 3 inferred call/import edges — investigate with rmap path 'pkg mod::f' B.g --include-inferred"));
        // Absent → the plain not-found render (byte-identical).
        let plain = parse_path(serde_json::json!({}), false).unwrap();
        assert!(!plain
            .render_human_with_query("A.f", "B.g")
            .contains("inferred"));
        // A count without its depth is a decode error, never a guessed bound.
        assert!(parse_path(serde_json::json!({"inferred_edges_on_route": 1}), false).is_err());
    }

    #[test]
    fn render_path_found_with_inferred_hops_states_their_count() {
        // D-TESB-15: `path --include-inferred` answers the admitting walk's route and states how
        // many of its hops are inferred.
        let resp = parse_path(
            serde_json::json!({"include_inferred": true, "inferred_edges_on_route": 1, "search_depth": 8}),
            true,
        )
        .unwrap();
        let out = resp.render_human_with_query("A.f", "B.g");
        assert!(out.contains("2 hops"), "{out}");
        assert!(
            out.ends_with("1 of 2 hops is an inferred call/import edge — investigate\n"),
            "{out}"
        );
        let two = parse_path(
            serde_json::json!({"include_inferred": true, "inferred_edges_on_route": 2, "search_depth": 8}),
            true,
        )
        .unwrap();
        assert!(two
            .render_human_with_query("A.f", "B.g")
            .contains("2 of 2 hops are inferred call/import edges — investigate"));
        // No inferred hop: nothing added.
        let zero = parse_path(
            serde_json::json!({"include_inferred": true, "inferred_edges_on_route": 0, "search_depth": 8}),
            true,
        )
        .unwrap();
        let plain = parse_path(serde_json::json!({}), true).unwrap();
        assert_eq!(
            zero.render_human_with_query("A.f", "B.g"),
            plain.render_human_with_query("A.f", "B.g")
        );
    }
}
