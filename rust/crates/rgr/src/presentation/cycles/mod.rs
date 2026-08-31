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
//! Three crate-private children keep every file under the 500-line guardrail:
//! - [`walk`] — the cycle-BODY rendering (the DFS walk over carried real edges, and the `members (unordered)`
//!   fallback — CYCLE-HONESTY-1 §2.2).
//! - [`composition`] — the FIXTURE-POLLUTION-1 §2.2 test-composition decode (`CycleComposition`).
//! - [`tests`] — the renderer unit tests.
//!
//! This file owns the response DTOs, the three response renderers, and the repo-level type-only caveat footer.

use serde::Deserialize;

use crate::presentation::kv_line;

mod composition;
mod walk;
use composition::CycleComposition;
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
    /// FIXTURE-POLLUTION-1 §2.3: set ONLY on the LiveGraph serving path (which lacks the
    /// `is_test` fact — deferred to CYCLE-FACTS-2). When present, the renderer prints the
    /// asymmetry honestly ("test-only cycles not evaluated on this serving path") rather
    /// than pretend uniformity. Absent on the SQLite route, which classifies per cycle instead.
    #[serde(default)]
    pub test_composition_note: Option<String>,
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
    /// FIXTURE-POLLUTION-1 §2.2 + binding direction rule: the daemon (SQLite route only)
    /// set this per-cycle discriminant (`test_only` / `production` / `unknown`) from the
    /// stored `is_test` fact, conservatively aggregated over the members. `test_only` →
    /// DEMOTED below the production headline; `production` → a real cycle; `unknown` (a
    /// member owns no tracked file, or a malformed node) → NEVER demoted, stays in the main
    /// listing carrying its reason. ABSENT → the LiveGraph route, which does not classify
    /// (§2.3 asymmetry, stated by [`CyclesResponse::test_composition_note`] instead).
    #[serde(default)]
    pub test_composition: Option<String>,
    /// The reader-framed reason present ONLY when `test_composition == "unknown"`.
    #[serde(default)]
    pub test_composition_unknown_reason: Option<String>,
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
        // FIXTURE-POLLUTION-1 §2.2 + binding direction rule: split POSITIVELY test-only
        // cycles (the daemon labeled them from the stored is_test fact on the SQLite route)
        // out of the production headline. They are DEMOTED to a trailing labeled section,
        // never hidden. UNKNOWN cycles (a member owns no tracked file) stay in the MAIN
        // listing carrying a marker — never demoted. The LiveGraph serving path does not
        // classify (§2.3) — every cycle is `NotEvaluated` there and the asymmetry note is
        // printed instead.
        let (fixtures, main): (Vec<&Cycle>, Vec<&Cycle>) = self
            .cycles
            .iter()
            .partition(|c| c.composition() == CycleComposition::TestOnly);

        if main.is_empty() && fixtures.is_empty() {
            out.push_str("No module-level cycles found.\n");
            self.push_test_composition_note(&mut out);
            return out.trim_end().to_string();
        }

        out.push_str(&format!(
            "{} module-level cycle{} found\n",
            main.len(),
            if main.len() == 1 { "" } else { "s" }
        ));
        if !fixtures.is_empty() {
            out.push_str(&format!(
                "+{} test-only cycle{} (excluded from the headline)\n",
                fixtures.len(),
                if fixtures.len() == 1 { "" } else { "s" }
            ));
        }
        out.push('\n');

        // ── Main-listing cycles (production + unknown) ─────────────
        for (i, cycle) in main.iter().enumerate() {
            let size = cycle.nodes.len();
            out.push_str(&format!("Cycle {} ({} modules):\n", i + 1, size));
            // Binding direction rule: an UNKNOWN cycle stays here (not demoted) carrying an
            // explicit unknown-with-reason marker — never a silent production placement.
            if let CycleComposition::Unknown(reason) = cycle.composition() {
                out.push_str(&format!("  [test-composition unknown: {reason}]\n"));
            }
            // CYCLE-HONESTY-1: a REAL walk over carried edges, else `members (unordered)` — never a
            // fabricated ring drawn from the (canonically-sorted, edge-less) member set.
            out.push_str(&render_cycle_body(cycle));
            out.push('\n');
        }

        // ── Next step hint ─────────────────────────────────────────
        if !main.is_empty() {
            out.push_str("Run: rmap modules deps <module> to see specific import edges\n");
        }

        // ── Demoted test-only cycles (§2.2) ────────────────────────
        if !fixtures.is_empty() {
            out.push_str(&format!(
                "\ntest-only cycles ({} — excluded from the headline):\n",
                fixtures.len()
            ));
            for (i, cycle) in fixtures.iter().enumerate() {
                let size = cycle.nodes.len();
                out.push_str(&format!("Test-only cycle {} ({} modules):\n", i + 1, size));
                out.push_str(&render_cycle_body(cycle));
                out.push('\n');
            }
        }

        self.push_ts_caveat(&mut out);
        self.push_test_composition_note(&mut out);
        out.trim_end().to_string()
    }

    /// FIXTURE-POLLUTION-1 §2.3: print the LiveGraph-route asymmetry note when present
    /// (that serving path cannot evaluate test composition — it lacks the `is_test` fact).
    /// Stated honestly rather than pretending uniformity; absent on the SQLite route.
    fn push_test_composition_note(&self, out: &mut String) {
        if let Some(note) = &self.test_composition_note {
            out.push_str(&format!("\nNote: {note}\n"));
        }
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
            // FIXTURE-POLLUTION-1 §2.3: even with no cycles, disclose that this LiveGraph
            // serving path did not evaluate test composition — never a silent "no fixtures".
            self.push_test_composition_note(&mut out);
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
        // FIXTURE-POLLUTION-1 §2.3: state the LiveGraph asymmetry (test composition not
        // evaluated on this serving path) rather than let the cycles read as production-vs-
        // test-classified. The daemon sets the note; absent on the SQLite route.
        self.push_test_composition_note(&mut out);
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
            // FIXTURE-POLLUTION-1 §2.3: even with no cycles, disclose that this LiveGraph
            // serving path did not evaluate test composition — never a silent "no fixtures".
            self.push_test_composition_note(&mut out);
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
        // FIXTURE-POLLUTION-1 §2.3: state the LiveGraph asymmetry (test composition not
        // evaluated on this serving path) rather than let the cycles read as production-vs-
        // test-classified. The daemon sets the note; absent on the SQLite route.
        self.push_test_composition_note(&mut out);
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
mod tests;
