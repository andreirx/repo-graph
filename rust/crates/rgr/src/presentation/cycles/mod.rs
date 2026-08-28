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
//!
//! # Module layout
//!
//! The cycle-BODY rendering (the DFS walk over carried real edges, and the `members (unordered)` fallback —
//! CYCLE-HONESTY-1 §2.2) lives in the crate-private child [`walk`], split out to keep both files under the
//! 500-line guardrail. This file owns the response DTOs, the three response renderers, and the repo-level
//! type-only caveat footer.

use serde::Deserialize;

use crate::presentation::kv_line;

mod walk;
use walk::render_cycle_body;

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
    /// CYCLE-HONESTY-1 (§2.4, operator ruling C1 repo-level): the daemon set this true iff the repo's
    /// stored language facts show material TS/JS presence AND at least one cycle renders. Import edges do
    /// not distinguish `import type`, so a type-only import can create a cycle that vanishes at runtime;
    /// the renderer prints ONE repo-scoped footer when true. Absent/false on non-TS repos.
    #[serde(default)]
    pub ts_type_only_caveat: bool,
}

#[derive(Debug, Deserialize)]
pub struct Cycle {
    pub nodes: Vec<CycleNode>,
    /// CYCLE-HONESTY-1 (§2.1): the REAL intra-SCC directed IMPORTS edges among this cycle's members, keyed
    /// by member `node_id`. Present ONLY on the SQLite-served route (the LiveGraph route omits the field —
    /// an absent optional field is honest, operator ruling A1). The renderer draws an arrow ONLY between a
    /// pair present here; with no edges it renders `members (unordered)`. So no arrow can claim a
    /// nonexistent import.
    #[serde(default)]
    pub edges: Option<Vec<CycleEdge>>,
    /// CYCLE-HONESTY-1 (§2.1): true iff the daemon capped [`Cycle::edges`] at the sane bound (never a
    /// silent cut). Per §2.2 (operator ruling A1) a truncated edge set is a no-arrows fallback case: the
    /// carried edges are an incomplete subset, so a walk drawn over them could imply a chain the full set
    /// does not — the renderer therefore falls back to `members (unordered)` (no arrows). The member COUNT
    /// line stays complete.
    #[serde(default)]
    pub edges_truncated: Option<bool>,
}

/// CYCLE-HONESTY-1 (§2.1): one REAL directed import edge inside a cycle. A 2-field DTO mirroring the daemon
/// `edges` field byte-for-byte; sole consumer is the cycles renderer's walk finder. Named fields (not a
/// tuple) so serde matches the JSON keys `from_node_id`/`to_node_id`.
#[derive(Debug, Deserialize)]
pub struct CycleEdge {
    pub from_node_id: String,
    pub to_node_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CycleNode {
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

impl CycleNode {
    /// The human/agent-facing identity: the QUALIFIED module path when present (disambiguates the
    /// collision-prone short `name`), else the short `name`. Visible to the child [`walk`] module.
    fn display(&self) -> &str {
        self.qualified_name.as_deref().unwrap_or(self.name.as_str())
    }
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
            // CYCLE-HONESTY-1: a REAL walk over carried edges, else `members (unordered)` — never a
            // fabricated ring drawn from the (canonically-sorted, edge-less) member set.
            out.push_str(&render_cycle_body(cycle));
            out.push('\n');
        }

        // ── Next step hint ─────────────────────────────────────────
        if !self.cycles.is_empty() {
            out.push_str("Run: rmap modules deps <module> to see specific import edges\n");
        }

        self.push_ts_caveat(&mut out);
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
            out.push_str(&render_cycle_body(cycle));
            out.push('\n');
        }

        self.push_ts_caveat(&mut out);
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
            out.push_str(&render_cycle_body(cycle));
            out.push('\n');
        }

        self.push_ts_caveat(&mut out);
        out.trim_end().to_string()
    }

    /// CYCLE-HONESTY-1 (§2.4, operator ruling C1 repo-level): append the ONE repo-scoped type-only caveat
    /// footer when the daemon flagged material TS/JS presence (the flag already ANDs "≥1 cycle renders").
    /// Repo-scoped wording, no per-cycle claim.
    fn push_ts_caveat(&self, out: &mut String) {
        if self.ts_type_only_caveat {
            out.push_str(
                "\nNote: this repo contains TypeScript/JavaScript; import edges do not distinguish \
                 `import type` — some cycles may vanish at runtime.\n",
            );
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
            ts_type_only_caveat: false,
        }
    }

    /// A MODULE cycle node; `qualified_name` defaults to `None` (exercises the `name` fallback).
    fn cnode(node_id: &str, name: &str) -> CycleNode {
        CycleNode {
            node_id: node_id.to_string(),
            name: name.to_string(),
            qualified_name: None,
            file: None,
        }
    }

    /// A cycle with NO carried edges (the LiveGraph route + older daemon reply) -> unordered render.
    fn cyc(nodes: Vec<CycleNode>) -> Cycle {
        Cycle {
            nodes,
            edges: None,
            edges_truncated: None,
        }
    }

    #[test]
    fn render_shows_repo_display_name() {
        let out = minimal_response().render_human();
        assert!(out.contains("Cycles: test-repo"));
    }

    #[test]
    fn render_shows_no_cycles_message() {
        let out = minimal_response().render_human();
        assert!(out.contains("No module-level cycles found"));
    }

    #[test]
    fn render_shows_cycle_count() {
        let mut r = minimal_response();
        r.count = 3;
        r.cycles = vec![
            cyc(vec![cnode("n1", "src/a"), cnode("n2", "src/b")]),
            cyc(vec![cnode("n3", "src/c"), cnode("n4", "src/d")]),
            cyc(vec![cnode("n5", "src/e"), cnode("n6", "src/f")]),
        ];
        let out = r.render_human();
        assert!(out.contains("3 module-level cycles found"));
    }

    #[test]
    fn render_shows_large_cycle_size() {
        let mut r = minimal_response();
        r.count = 1;
        let nodes: Vec<CycleNode> = (0..10)
            .map(|i| cnode(&format!("n{i}"), &format!("src/mod{i}")))
            .collect();
        r.cycles = vec![cyc(nodes)];
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

    // ── CYCLE-HONESTY-1 (§2.4): repo-level type-only caveat footer ──

    #[test]
    fn ts_caveat_footer_present_when_flagged() {
        let mut r = minimal_response();
        r.count = 1;
        r.ts_type_only_caveat = true;
        r.cycles = vec![cyc(vec![cnode("a", "a"), cnode("b", "b")])];
        let out = r.render_human();
        assert!(
            out.contains("this repo contains TypeScript/JavaScript")
                && out.contains("import type")
                && out.contains("vanish at runtime"),
            "repo-scoped type-only caveat footer present: {out}"
        );
    }

    #[test]
    fn ts_caveat_footer_absent_when_not_flagged() {
        let mut r = minimal_response();
        r.count = 1;
        r.cycles = vec![cyc(vec![cnode("a", "a"), cnode("b", "b")])];
        let out = r.render_human();
        assert!(
            !out.contains("import type"),
            "no caveat on a non-TS repo: {out}"
        );
    }

    // ── CYCLES-FILE-IMPORT-RENDER-1: FILE-import vocabulary (LiveGraph route -> no edges -> unordered) ──

    fn two_file_cycle() -> CyclesResponse {
        let mut r = minimal_response();
        r.count = 1;
        r.cycles = vec![cyc(vec![
            cnode("repo:packages/a/src/main.ts:FILE", "packages/a/src/main.ts"),
            cnode("repo:packages/b/src/foo.ts:FILE", "packages/b/src/foo.ts"),
        ])];
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
        // LiveGraph route carries no edges -> unordered listing, NO fabricated arrows.
        assert!(
            out.contains("members (unordered): packages/a/src/main.ts, packages/b/src/foo.ts"),
            "{out}"
        );
        assert!(
            !out.contains(" -> "),
            "no arrows on the edge-less route: {out}"
        );
        assert!(!out.contains("module"), "no module vocab: {out}");
        assert!(
            !out.contains("rmap modules deps"),
            "no module-deps hint: {out}"
        );
    }

    #[test]
    fn sqlite_module_render_keeps_vocabulary() {
        // The SQLite path uses render_human (MODULE) vocabulary + the module-deps hint (unchanged).
        let out = two_file_cycle().render_human();
        assert!(out.contains("1 module-level cycle found"), "{out}");
        assert!(out.contains("(2 modules)"), "{out}");
        assert!(
            out.contains("Run: rmap modules deps <module>"),
            "module-deps hint retained for SQLite: {out}"
        );
    }

    // ── MODULE-CYCLES-CLI-1: dedicated MODULE-import renderer (module paths; LiveGraph -> unordered) ──

    fn two_module_cycle() -> CyclesResponse {
        let mut r = minimal_response();
        r.count = 1;
        r.cycles = vec![cyc(vec![
            cnode("repo:packages/a/src:MODULE", "packages/a/src"),
            cnode("repo:packages/b/src:MODULE", "packages/b/src"),
        ])];
        r
    }

    #[test]
    fn module_import_render_says_modules_with_paths() {
        let out = two_module_cycle().render_human_module_import();
        assert!(out.contains("1 MODULE import cycle found"), "{out}");
        assert!(out.contains("(2 modules)"), "{out}");
        // LiveGraph route -> unordered member PATHS, no fabricated arrows.
        assert!(
            out.contains("members (unordered): packages/a/src, packages/b/src"),
            "members are module PATHS: {out}"
        );
        assert!(
            !out.contains(" -> "),
            "no arrows on the edge-less route: {out}"
        );
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
