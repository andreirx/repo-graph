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
    /// SHORT module name (legacy default). Collision-prone across packages; kept for back-compat.
    pub name: String,
    /// CYCLES-OUTPUT-CONTRACT-1 (D1=B/D2=B): the QUALIFIED module path (e.g. `packages/a/src`). The default
    /// human render PREFERS it over the short `name`. Absent for non-module cycles -> falls back to `name`.
    #[serde(default)]
    pub qualified_name: Option<String>,
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

    /// Render FILE-import cycles (CYCLES-FILE-IMPORT-RENDER-1): the `--engine livegraph --kind
    /// file-import` answer is over the FILE import graph, NOT the MODULE graph, so it must NOT borrow the
    /// MODULE vocabulary of [`render_human`] ("module-level cycle", "(N modules)", "rmap modules deps").
    /// Owns BOTH the empty and non-empty cases (so both are unit-testable here, not in the command). No
    /// "rmap modules deps" follow-up hint — the cycle chain already lists the files, and no such relation
    /// command applies to FILE-import cycles.
    pub fn render_human_file_import(&self) -> String {
        let mut out = String::new();

        let repo_display = self.display_name.as_deref().unwrap_or(&self.repo_uid);
        out.push_str(&kv_line("Cycles", repo_display));
        out.push_str(&kv_line("Snapshot", &truncate_uid(&self.snapshot_uid)));
        out.push('\n');

        if self.count == 0 {
            out.push_str("No FILE import cycles found within the captured scope.\n");
            return out.trim_end().to_string();
        }

        out.push_str(&format!(
            "{} FILE import cycle{} found\n\n",
            self.count,
            if self.count == 1 { "" } else { "s" }
        ));

        for (i, cycle) in self.cycles.iter().enumerate() {
            let size = cycle.nodes.len();
            out.push_str(&format!(
                "Cycle {} ({} file{}):\n",
                i + 1,
                size,
                if size == 1 { "" } else { "s" }
            ));
            let members = self.render_cycle_chain(cycle);
            out.push_str(&format!("  {}\n", members));
            out.push('\n');
        }

        out.trim_end().to_string()
    }

    /// Render MODULE-import cycles (MODULE-CYCLES-CLI-1 D3): the `--engine livegraph --kind module-import`
    /// answer is over the DIRECTORY-aggregated MODULE import graph, so it uses MODULE vocabulary (members
    /// are module PATHS) and — like the FILE renderer — NOT the generic SQLite "module-level" / "rmap
    /// modules deps" text. Owns empty + non-empty (both unit-testable here).
    pub fn render_human_module_import(&self) -> String {
        let mut out = String::new();

        let repo_display = self.display_name.as_deref().unwrap_or(&self.repo_uid);
        out.push_str(&kv_line("Cycles", repo_display));
        out.push_str(&kv_line("Snapshot", &truncate_uid(&self.snapshot_uid)));
        out.push('\n');

        if self.count == 0 {
            out.push_str("No MODULE import cycles found within the captured scope.\n");
            return out.trim_end().to_string();
        }

        out.push_str(&format!(
            "{} MODULE import cycle{} found\n\n",
            self.count,
            if self.count == 1 { "" } else { "s" }
        ));

        for (i, cycle) in self.cycles.iter().enumerate() {
            let size = cycle.nodes.len();
            out.push_str(&format!(
                "Cycle {} ({} module{}):\n",
                i + 1,
                size,
                if size == 1 { "" } else { "s" }
            ));
            let members = self.render_cycle_chain(cycle);
            out.push_str(&format!("  {}\n", members));
            out.push('\n');
        }

        out.trim_end().to_string()
    }

    fn render_cycle_chain(&self, cycle: &Cycle) -> String {
        if cycle.nodes.is_empty() {
            return "(empty cycle)".to_string();
        }

        // Show first 4 members + ellipsis + back to first. CYCLES-OUTPUT-CONTRACT-1: prefer the QUALIFIED
        // module path (disambiguates the collision-prone short `name`); fall back to `name` when absent.
        let names: Vec<&str> = cycle
            .nodes
            .iter()
            .map(|n| n.qualified_name.as_deref().unwrap_or(n.name.as_str()))
            .collect();

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

    /// CYCLES-OUTPUT-CONTRACT-1: a MODULE cycle node for tests; `qualified_name` defaults to `None`, so these
    /// legacy fixtures exercise the renderer's `name` fallback (unchanged behavior).
    fn cnode(node_id: &str, name: &str) -> CycleNode {
        CycleNode {
            node_id: node_id.to_string(),
            name: name.to_string(),
            qualified_name: None,
            file: None,
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
                nodes: vec![cnode("n1", "src/a"), cnode("n2", "src/b")],
            },
            Cycle {
                nodes: vec![cnode("n3", "src/c"), cnode("n4", "src/d")],
            },
            Cycle {
                nodes: vec![cnode("n5", "src/e"), cnode("n6", "src/f")],
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
                cnode("n1", "src/a"),
                cnode("n2", "src/b"),
                cnode("n3", "src/c"),
            ],
        }];
        let out = r.render_human();
        assert!(out.contains("src/a -> src/b -> src/c -> src/a"));
    }

    #[test]
    fn render_prefers_qualified_name_over_short_name() {
        // CYCLES-OUTPUT-CONTRACT-1 (D1=B): with qualified_name present, the chain shows the QUALIFIED path, not
        // the short, collision-prone `name` (here both modules are short-named "src").
        let mut r = minimal_response();
        r.count = 1;
        r.cycles = vec![Cycle {
            nodes: vec![
                CycleNode {
                    node_id: "repo:packages/a/src:MODULE".to_string(),
                    name: "src".to_string(),
                    qualified_name: Some("packages/a/src".to_string()),
                    file: None,
                },
                CycleNode {
                    node_id: "repo:packages/b/src:MODULE".to_string(),
                    name: "src".to_string(),
                    qualified_name: Some("packages/b/src".to_string()),
                    file: None,
                },
            ],
        }];
        let out = r.render_human();
        assert!(
            out.contains("packages/a/src -> packages/b/src -> packages/a/src"),
            "qualified path shown, not the ambiguous short name: {out}"
        );
        assert!(
            !out.contains("src -> src"),
            "the collision-prone short name must NOT be what renders: {out}"
        );
    }

    #[test]
    fn render_shows_large_cycle_size() {
        let mut r = minimal_response();
        r.count = 1;
        // Create a 10-node cycle
        let nodes: Vec<CycleNode> = (0..10)
            .map(|i| cnode(&format!("n{}", i), &format!("src/mod{}", i)))
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

    // ── CYCLES-FILE-IMPORT-RENDER-1: FILE-import vocabulary (not MODULE) ──

    fn two_file_cycle() -> CyclesResponse {
        let mut r = minimal_response();
        r.count = 1;
        r.cycles = vec![Cycle {
            nodes: vec![
                cnode("repo:packages/a/src/main.ts:FILE", "packages/a/src/main.ts"),
                cnode("repo:packages/b/src/foo.ts:FILE", "packages/b/src/foo.ts"),
            ],
        }];
        r
    }

    #[test]
    fn file_import_render_empty_says_files_not_modules() {
        let out = minimal_response().render_human_file_import(); // count 0
        assert!(
            out.contains("No FILE import cycles found within the captured scope"),
            "{out}"
        );
        assert!(!out.contains("module"), "empty must not say module: {out}");
    }

    #[test]
    fn file_import_render_nonempty_says_files_not_modules() {
        let out = two_file_cycle().render_human_file_import();
        assert!(out.contains("1 FILE import cycle found"), "{out}");
        assert!(out.contains("(2 files)"), "{out}");
        assert!(
            out.contains(
                "packages/a/src/main.ts -> packages/b/src/foo.ts -> packages/a/src/main.ts"
            ),
            "{out}"
        );
        // Requirements 1-3: no MODULE vocabulary and no module-deps follow-up hint.
        assert!(!out.contains("module"), "no module vocab: {out}");
        assert!(
            !out.contains("rmap modules deps"),
            "no module-deps hint: {out}"
        );
    }

    #[test]
    fn sqlite_module_render_unchanged() {
        // Requirement 4: the SQLite path uses render_human (MODULE), which must stay verbatim.
        let out = two_file_cycle().render_human();
        assert!(out.contains("1 module-level cycle found"), "{out}");
        assert!(out.contains("(2 modules)"), "{out}");
        assert!(
            out.contains("Run: rmap modules deps <module>"),
            "module-deps hint retained for SQLite: {out}"
        );
    }

    // ── MODULE-CYCLES-CLI-1: dedicated MODULE-import renderer (module paths) ──

    fn two_module_cycle() -> CyclesResponse {
        let mut r = minimal_response();
        r.count = 1;
        r.cycles = vec![Cycle {
            nodes: vec![
                cnode("repo:packages/a/src:MODULE", "packages/a/src"),
                cnode("repo:packages/b/src:MODULE", "packages/b/src"),
            ],
        }];
        r
    }

    #[test]
    fn module_import_render_says_modules_with_paths() {
        let out = two_module_cycle().render_human_module_import();
        assert!(out.contains("1 MODULE import cycle found"), "{out}");
        assert!(out.contains("(2 modules)"), "{out}");
        assert!(
            out.contains("packages/a/src -> packages/b/src -> packages/a/src"),
            "members are module PATHS: {out}"
        );
        // Requirements 7: precise MODULE-import wording -- NOT the generic SQLite "module-level" text, NOT
        // the file-import renderer, NO "rmap modules deps" hint.
        assert!(!out.contains("module-level"), "{out}");
        assert!(!out.contains("FILE import"), "{out}");
        assert!(!out.contains("rmap modules deps"), "{out}");
    }

    #[test]
    fn module_import_render_empty() {
        let out = minimal_response().render_human_module_import();
        assert!(
            out.contains("No MODULE import cycles found within the captured scope"),
            "{out}"
        );
        assert!(!out.contains("module-level"), "{out}");
    }
}
