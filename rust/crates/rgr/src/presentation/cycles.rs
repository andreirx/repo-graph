//! Presentation layer for the `cycles` command.
//!
//! # CLI-OUT-2B
//!
//! Transforms daemon cycles response into human-readable plain text.
//!
//! ## Human Output Structure
//!
//! ```text
//! Cycles: billing-service
//! Snapshot: snap_01kr...
//!
//! 4 module-level cycles found
//!
//! Cycle 1 (10 modules):
//!   src/core -> src/services -> src/adapters -> ... -> src/core
//!
//! Cycle 2 (3 modules):
//!   src/auth -> src/user -> src/auth
//!
//! ...
//!
//! Run: rmap modules deps <module> to see specific import edges
//! ```

use serde::Deserialize;

use crate::presentation::kv_line;

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized cycles response from daemon.
#[derive(Debug, Deserialize)]
pub struct CyclesResponse {
    pub repo_uid: String,
    /// Human-readable repo name for CLI display.
    #[serde(default)]
    pub display_name: Option<String>,
    pub snapshot_uid: String,
    pub cycles: Vec<Cycle>,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct Cycle {
    pub nodes: Vec<CycleNode>,
}

#[derive(Debug, Deserialize)]
pub struct CycleNode {
    #[allow(dead_code)]
    pub node_id: String,
    pub name: String,
    #[allow(dead_code)]
    pub file: Option<String>,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl CyclesResponse {
    /// Render the cycles response as human-readable plain text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        let repo_display = self.display_name.as_deref().unwrap_or(&self.repo_uid);
        out.push_str(&kv_line("Cycles", repo_display));
        out.push_str(&kv_line("Snapshot", &truncate_uid(&self.snapshot_uid)));
        out.push('\n');

        // ── Summary ────────────────────────────────────────────────
        if self.count == 0 {
            out.push_str("No module-level cycles found.\n");
            return out.trim_end().to_string();
        }

        out.push_str(&format!(
            "{} module-level cycle{} found\n\n",
            self.count,
            if self.count == 1 { "" } else { "s" }
        ));

        // ── Cycles ─────────────────────────────────────────────────
        for (i, cycle) in self.cycles.iter().enumerate() {
            let size = cycle.nodes.len();

            out.push_str(&format!("Cycle {} ({} modules):\n", i + 1, size));

            // Show cycle members as arrow chain
            let members = self.render_cycle_chain(cycle);
            out.push_str(&format!("  {}\n", members));

            out.push('\n');
        }

        // ── Next step hint ─────────────────────────────────────────
        if !self.cycles.is_empty() {
            out.push_str("Run: rmap modules deps <module> to see specific import edges\n");
        }

        out.trim_end().to_string()
    }

    fn render_cycle_chain(&self, cycle: &Cycle) -> String {
        if cycle.nodes.is_empty() {
            return "(empty cycle)".to_string();
        }

        // Show first 4 members + ellipsis + back to first
        let names: Vec<&str> = cycle.nodes.iter().map(|n| n.name.as_str()).collect();

        if names.len() <= 5 {
            // Show full chain
            let mut chain = names.join(" -> ");
            chain.push_str(&format!(" -> {}", names[0]));
            chain
        } else {
            // Truncate: first 3 -> ... -> last -> first
            let mut chain = names[..3].join(" -> ");
            chain.push_str(" -> ...");
            chain.push_str(&format!(" -> {} -> {}", names[names.len() - 1], names[0]));
            chain
        }
    }
}

fn truncate_uid(uid: &str) -> String {
    if uid.len() > 20 {
        format!("{}...", &uid[..17])
    } else {
        uid.to_string()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_response() -> CyclesResponse {
        CyclesResponse {
            repo_uid: "repo_01kr12345678".to_string(),
            display_name: Some("test-repo".to_string()),
            snapshot_uid: "snap_01kr12345678".to_string(),
            cycles: vec![],
            count: 0,
        }
    }

    #[test]
    fn render_shows_repo_display_name() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("Cycles: test-repo"));
    }

    #[test]
    fn render_shows_no_cycles_message() {
        let r = minimal_response();
        let out = r.render_human();
        assert!(out.contains("No module-level cycles found"));
    }

    #[test]
    fn render_shows_cycle_count() {
        let mut r = minimal_response();
        r.count = 3;
        r.cycles = vec![
            Cycle {
                nodes: vec![
                    CycleNode {
                        node_id: "n1".to_string(),
                        name: "src/a".to_string(),
                        file: None,
                    },
                    CycleNode {
                        node_id: "n2".to_string(),
                        name: "src/b".to_string(),
                        file: None,
                    },
                ],
            },
            Cycle {
                nodes: vec![
                    CycleNode {
                        node_id: "n3".to_string(),
                        name: "src/c".to_string(),
                        file: None,
                    },
                    CycleNode {
                        node_id: "n4".to_string(),
                        name: "src/d".to_string(),
                        file: None,
                    },
                ],
            },
            Cycle {
                nodes: vec![
                    CycleNode {
                        node_id: "n5".to_string(),
                        name: "src/e".to_string(),
                        file: None,
                    },
                    CycleNode {
                        node_id: "n6".to_string(),
                        name: "src/f".to_string(),
                        file: None,
                    },
                ],
            },
        ];
        let out = r.render_human();
        assert!(out.contains("3 module-level cycles found"));
    }

    #[test]
    fn render_shows_cycle_chain() {
        let mut r = minimal_response();
        r.count = 1;
        r.cycles = vec![Cycle {
            nodes: vec![
                CycleNode {
                    node_id: "n1".to_string(),
                    name: "src/a".to_string(),
                    file: None,
                },
                CycleNode {
                    node_id: "n2".to_string(),
                    name: "src/b".to_string(),
                    file: None,
                },
                CycleNode {
                    node_id: "n3".to_string(),
                    name: "src/c".to_string(),
                    file: None,
                },
            ],
        }];
        let out = r.render_human();
        assert!(out.contains("src/a -> src/b -> src/c -> src/a"));
    }

    #[test]
    fn render_shows_large_cycle_size() {
        let mut r = minimal_response();
        r.count = 1;
        // Create a 10-node cycle
        let nodes: Vec<CycleNode> = (0..10)
            .map(|i| CycleNode {
                node_id: format!("n{}", i),
                name: format!("src/mod{}", i),
                file: None,
            })
            .collect();
        r.cycles = vec![Cycle { nodes }];
        let out = r.render_human();
        assert!(out.contains("(10 modules)"));
    }

    #[test]
    fn render_falls_back_to_repo_uid_when_no_display_name() {
        let mut r = minimal_response();
        r.display_name = None;
        let out = r.render_human();
        assert!(out.contains("Cycles: repo_01kr12345678"));
    }
}
