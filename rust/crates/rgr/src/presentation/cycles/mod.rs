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
//! This file owns the response DTOs, the three response renderers, the per-cycle type-only verdict
//! ([`CycleTypeOnly`], TYPE-ONLY-IMPORTS-1) + its narrowed Unknown footer, and the LiveGraph-route blanket
//! type-only caveat footer (retained where the per-module-edge fact is not reachable).

use repo_graph_agent::{partition_counts, CyclePartition, CycleTestComposition};
use serde::Deserialize;

use crate::presentation::{cycle_exclusion_clause, kv_line};

mod composition;
mod walk;
use composition::CycleComposition;
use walk::render_cycle_body;

/// ORIENT-CYCLES-DISAGREE-1 (review-4 #1): the exclusion-aware split of the RENDERED module cycles,
/// produced by the SHARED `repo_graph_agent::partition_counts` — the ONE partition function
/// `orient`'s cycle leaf also uses. No count arithmetic lives in this renderer, so the two surfaces
/// cannot state different numbers for one snapshot (the slice's DoD). `Some` iff EVERY cycle
/// carries a decoded test-composition (the SQLite route, where the stored `is_test` fact is
/// reachable); a single `NotEvaluated` cycle (the LiveGraph route — §2.3 asymmetry) ⇒ `None`, and
/// the renderer falls back to the raw main count + the asymmetry note, exactly as `orient`'s
/// `headline_split` returns `None` there.
fn headline_partition(cycles: &[Cycle]) -> Option<CyclePartition> {
    let comps = cycles
        .iter()
        .map(|c| c.composition().into_agent())
        .collect::<Option<Vec<CycleTestComposition>>>()?;
    Some(partition_counts(&comps))
}

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
    /// CYCLE-HONESTY-1 (§2.4) — the BLANKET repo-level `import type` caveat, now ROUTE-CONDITIONAL
    /// (TYPE-ONLY-IMPORTS-1): true ONLY on the LiveGraph serving route, which cannot reach the stored
    /// per-module-edge `is_type_only` fact and so falls back to the coarse "some cycles may vanish at
    /// runtime" hedge. The SQLite route sets this FALSE and instead carries the precise per-cycle
    /// [`Cycle::type_only`] verdict (with a narrowed footer only where genuine Unknown remains). The
    /// renderer prints the blanket footer only when this is true. Absent/false on non-TS repos.
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
    /// TYPE-ONLY-IMPORTS-1: the per-cycle runtime-vs-type-only verdict, computed by the SQLite route
    /// from the stored per-module-edge `is_type_only` fact (a cycle is type-only iff EVERY edge in its
    /// walk is type-only). ABSENT (`None`) when the fact is not reachable on this serving route (the
    /// LiveGraph cache route — it states the asymmetry via the blanket caveat instead) OR the cycle has
    /// no TS/JS member (§5: other languages' import edges are runtime by definition, label absent).
    #[serde(default)]
    pub type_only: Option<CycleTypeOnly>,
}

/// TYPE-ONLY-IMPORTS-1: the per-cycle type-only verdict sum type (mirrors the daemon `type_only` JSON —
/// `{kind[, reason]}`). Exhaustively matched by the renderer; no boolean+nullable.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CycleTypeOnly {
    /// EVERY edge in the cycle (SCC) is a TS/JS `import type` — the whole cycle vanishes at runtime.
    TypeOnly,
    /// COHERENCE-2 §2.2 (Option A): SOME (not all) edges are `import type` AND erasing them leaves NO
    /// directed cycle in the runtime subgraph — the cycle genuinely BREAKS at runtime (no runtime
    /// cycle remains). `type_only`/`of` are SCC-edge counts, rendered as such — never a "residual
    /// one-way coupling" claim the topology does not support.
    BreaksAtRuntime { type_only: usize, of: usize },
    /// A REAL runtime cycle: a directed cycle remains after every `import type` edge is erased. A
    /// PURE runtime cycle is `type_only == 0` (rendered WITHOUT a label); a MIXED SCC whose runtime
    /// subgraph is still cyclic carries `type_only > 0` as detail (COHERENCE-2 §2.2, Option A).
    ///
    /// COHERENCE-2 (review-1 #2): both counts are REQUIRED — no `#[serde(default)]`. The producer
    /// (`agent::CycleTypeOnly`, `Serialize` with no `skip_serializing_if`) ALWAYS emits both fields,
    /// even `type_only: 0` for a pure runtime cycle, so a missing field is producer/mirror schema
    /// drift, NOT a pure-runtime cycle. Defaulting the absent count to `0` would render that drift as
    /// the KNOWN pure-runtime state `{0, 0}` — a false-certainty Layer-0 claim that silently hides the
    /// unknown and could suppress the mixed-SCC `k of n` detail. Requiring the field makes typed
    /// `cycles` decoding fail into the handled response-parse `Result` (`error: failed to parse …`),
    /// and makes orient's raw-`Value` decode take its explicit unavailable-with-reason clause. (RULE #1.)
    HasRuntimeEdges { type_only: usize, of: usize },
    /// The verdict could not be computed (e.g. the snapshot was indexed before type-only tracking). The
    /// `reason` is reader-framed; the cycle is counted into the narrowed footer, NEVER demoted to runtime.
    Unknown { reason: String },
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
        //
        // ORIENT-CYCLES-DISAGREE-1 (review-4 #1): the headline INTEGERS come from the shared
        // `partition_counts` (via `headline_partition`) — the SAME function `orient` uses — never a
        // count local to this renderer. The `(fixtures, main)` split below is only the BODY
        // GROUPING (which cycles lead vs are demoted); it reads the SAME per-cycle `composition()`,
        // so `partition.production_count == main.len()` and `partition.test_only_count ==
        // fixtures.len()` by construction (pinned by `headline_counts_come_from_the_shared_partition`).
        let partition = headline_partition(&self.cycles);
        let (fixtures, main): (Vec<&Cycle>, Vec<&Cycle>) = self
            .cycles
            .iter()
            .partition(|c| c.composition() == CycleComposition::TestOnly);

        if main.is_empty() && fixtures.is_empty() {
            out.push_str("No module-level cycles found.\n");
            self.push_test_composition_note(&mut out);
            return out.trim_end().to_string();
        }

        // Headline count: the shared production count on the split (SQLite) route; the raw main
        // count on the unsplit (LiveGraph) route — where `partition` is `None` and no cycle is
        // demoted, so `main.len()` is the whole rendered set.
        let main_count = match partition {
            Some(p) => p.production_count,
            None => main.len() as u64,
        };
        out.push_str(&format!(
            "{} module-level cycle{} found",
            main_count,
            if main_count == 1 { "" } else { "s" }
        ));
        // ORIENT-CYCLES-DISAGREE-1 (operator ruling review-3 #2 + review-4 #3): the SAME combined
        // parenthetical `orient` renders — "(+M test-only excluded; test-composition unknown for
        // K)" — built by the ONE shared clause helper from the SAME shared partition, so the two
        // surfaces phrase the same integers identically. Present only on the split (SQLite) route;
        // the unsplit route states its asymmetry via `push_test_composition_note` instead.
        if let Some(p) = partition {
            if let Some(clause) =
                cycle_exclusion_clause(Some(p.test_only_count), Some(p.unknown_count))
            {
                out.push_str(&format!(" ({clause})"));
            }
        }
        out.push('\n');
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
            push_type_only_label(&mut out, cycle);
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
                // TYPE-ONLY-IMPORTS-1 (review-0 item 2): a demoted test-only cycle can ALSO be
                // type-only — a fixture cycle of pure `import type` edges vanishes at runtime just as
                // a production one does. Label it here too, so no type-only cycle goes unlabeled
                // regardless of which section renders it (the DoD: EACH type-only cycle labeled).
                push_type_only_label(&mut out, cycle);
                out.push_str(&render_cycle_body(cycle));
                out.push('\n');
            }
        }

        self.push_type_only_unknown_note(&mut out);
        self.push_ts_caveat(&mut out);
        self.push_test_composition_note(&mut out);
        out.trim_end().to_string()
    }

    /// TYPE-ONLY-IMPORTS-1: the NARROWED successor to the blanket `import type` caveat. On the SQLite
    /// route each cycle carries a per-cycle `type_only` verdict, so the blanket hedge is RETIRED and this
    /// footer is printed ONLY when genuine Unknown verdicts remain — naming HOW MANY, and grouped by the
    /// CARRIED reason. When every cycle carries a computed verdict (the fresh-index case) NOTHING is
    /// printed: an unlabeled cycle is then a confirmed runtime cycle, and a labeled one vanishes at
    /// runtime. Absent on the LiveGraph route (no per-cycle verdicts there — the blanket
    /// [`Self::push_ts_caveat`] states that asymmetry instead).
    ///
    /// Operator ruling 2026-09-03 item 2b: this renders the reason the `Unknown` sum type CARRIES, never
    /// a hard-coded string. A cycle whose verdict is `Unknown{"cycle import edges unavailable"}` and one
    /// whose verdict is `Unknown{"indexed before type-only tracking"}` (or `"type-only fact unreadable"`)
    /// are DISTINCT truths and render as distinct notes — no reason invention at the render site.
    fn push_type_only_unknown_note(&self, out: &mut String) {
        // Group the Unknown cycles by their carried reason (BTreeMap ⇒ deterministic note order).
        let mut by_reason: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for c in &self.cycles {
            if let Some(CycleTypeOnly::Unknown { reason }) = &c.type_only {
                *by_reason.entry(reason.as_str()).or_insert(0) += 1;
            }
        }
        for (reason, n) in by_reason {
            out.push_str(&format!(
                "\nNote: {n} cycle{} could not be evaluated for `import type` ({reason}) — {} may \
                 vanish at runtime.\n",
                if n == 1 { "" } else { "s" },
                if n == 1 { "it" } else { "some" },
            ));
        }
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

/// COHERENCE-2 §2.2: the ONE per-verdict type-only label string — the shared vocabulary home so
/// `cycles` and `orient` render the SAME state IDENTICALLY (orient calls this via
/// [`type_only_label`], pinned by the seam test). Exhaustive match; a new verdict variant
/// deliberately breaks it.
///
/// - `TypeOnly` → the whole cycle vanishes at runtime.
/// - `BreaksAtRuntime{k, n}` (Option A) → the cycle breaks at runtime (no runtime cycle remains);
///   `k of n` are SCC-edge counts, stated as such — NOT a "residual one-way coupling" claim.
/// - `HasRuntimeEdges{k, n}` with `k > 0` → a runtime cycle remains, `k of n` edges are type-only
///   (carried as detail).
/// - `HasRuntimeEdges{0, _}` → a pure runtime cycle → `None` (no label; byte-stable).
/// - `Unknown` → `None` (surfaced in the narrowed footer, not as a per-cycle label).
pub(crate) fn type_only_label(verdict: &CycleTypeOnly) -> Option<String> {
    match verdict {
        CycleTypeOnly::TypeOnly => Some("type-only (vanishes at runtime)".to_string()),
        CycleTypeOnly::BreaksAtRuntime { type_only, of } => Some(format!(
            "type-only breaks the cycle at runtime: {type_only} of {of} import edges are \
             `import type` (no runtime cycle remains)"
        )),
        CycleTypeOnly::HasRuntimeEdges { type_only, of } if *type_only > 0 => Some(format!(
            "runtime cycle remains: {type_only} of {of} import edges are `import type`"
        )),
        CycleTypeOnly::HasRuntimeEdges { .. } | CycleTypeOnly::Unknown { .. } => None,
    }
}

/// TYPE-ONLY-IMPORTS-1: render the per-cycle type-only label for ONE cycle, shared by the
/// production-listing loop AND the demoted test-only loop (review-0 item 2 — a test-only cycle can
/// itself be type-only). Delegates to the shared [`type_only_label`] so the wording cannot drift
/// from `orient`. `None` (a pure runtime cycle, an `Unknown`, or the route/§5 absence) prints
/// nothing here.
fn push_type_only_label(out: &mut String, cycle: &Cycle) {
    if let Some(verdict) = &cycle.type_only {
        if let Some(label) = type_only_label(verdict) {
            out.push_str("  ");
            out.push_str(&label);
            out.push('\n');
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
