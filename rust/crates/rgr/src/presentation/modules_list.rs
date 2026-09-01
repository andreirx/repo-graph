//! Presentation layer for `modules list` command.
//!
//! # CLI-OUT-4 Group 1
//!
//! Response DTO and human renderer for module catalog listing.
//!
//! ## Change Axis
//!
//! This file changes when:
//! - `modules list` output format changes
//! - `modules list` daemon response structure changes
//!
//! It does NOT change when:
//! - `modules show` changes
//! - Shared formatting changes (see `module_shared.rs`)

use serde::Deserialize;

use super::module_disambiguation::{collision_disambiguator, ModuleRow};
use super::module_shared::{
    format_count, format_dead_compact, format_files_compact, format_kind_confidence,
};
use super::{budget_remainder_line, HUMAN_ROW_BUDGET};

/// MODULE-EDGES-1 §2.1: one cross-module dependency edge, projected by the daemon
/// from the SAME module dependency graph (`module-queries`) the module rows and the
/// count come from. The presenter renders the edge list AND derives its count from
/// the SAME array (`ModulesListResponse::edges`), so a count that disagrees with its
/// own list is impossible by construction. Additive; the whole array is `Option` on
/// the response so an OLDER daemon (no `edges` key) reads as UNKNOWN and is labelled
/// unavailable — never a false zero from an absent field (honesty rule #1).
///
/// review-0 item 3 (honesty): the three RENDERED scalars carry NO `#[serde(default)]`.
/// A malformed edge (missing `source`/`target`/`import_count`) therefore FAILS the
/// response parse with the concrete serde reason (surfaced by `commands/modules/list.rs`
/// as `error: failed to parse modules list response: missing field \`source\``), never a
/// fabricated blank endpoint or `0 file-level imports`. The daemon (same version) emits
/// all three for every edge from the frozen graph, so this only trips on genuine wire
/// corruption / version skew — the honest outcome. `source_file_count` is intentionally
/// NOT modelled here: it is not part of the §2.1 row (`source → target (N imports)`), so
/// carrying it would be an unrendered, unearned field.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleEdgeEntry {
    pub source: String,
    pub target: String,
    pub import_count: u64,
}

/// A module entry in the list response.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleListEntry {
    #[serde(default)]
    pub module_uid: String,
    #[serde(default)]
    pub module_key: String,
    #[serde(default)]
    pub canonical_root_path: String,
    #[serde(default)]
    pub module_kind: String,
    #[serde(default)]
    pub display_name: String,
    /// MODULES-IDENTITY-2 §2.1: the owning manifest filename (`pyproject.toml`,
    /// `package.json`, `Cargo.toml`, …), derived by the daemon from the
    /// `module_key` source prefix via the SAME shared helper `orient` uses
    /// (`repo_graph_storage::manifest_for_module_key`). `None` for
    /// inferred/directory modules (no manifest declared them) — honest, never
    /// guessed. Used ONLY to disambiguate twin display names; additive and
    /// otherwise byte-compatible for existing consumers.
    #[serde(default)]
    pub manifest: Option<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub owned_file_count: usize,
    #[serde(default)]
    pub owned_test_file_count: usize,
    /// RECON-M-R3a (g2u-a): the daemon's per-module REDUCTION-ONLY unref overlay
    /// (`{fewer_flagged, …}` — see `modules_show`). Absent unless measured and nonzero.
    #[serde(default)]
    pub unref_reduction: Option<serde_json::Value>,
    #[serde(default)]
    pub outbound_dependency_count: usize,
    #[serde(default)]
    pub outbound_import_count: usize,
    #[serde(default)]
    pub inbound_dependency_count: usize,
    #[serde(default)]
    pub inbound_import_count: usize,
    #[serde(default)]
    pub violation_count: usize,
    #[serde(default)]
    pub dead_symbol_count: usize,
    #[serde(default)]
    pub dead_test_symbol_count: usize,
}

/// Response structure for modules list command.
#[derive(Debug, Deserialize)]
pub struct ModulesListResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub results: Vec<ModuleListEntry>,
    /// HTTP-BOUNDARY-1 (review-0 item 2): count of persisted HTTP provider↔consumer
    /// links. `Some(n)` nonzero means modules talk over HTTP/REST even when the
    /// import graph is intra-module — so the "boundaries may not be meaningful"
    /// hint is wrong. `None` = the link read FAILED (unknown), never 0
    /// (review-4 item 2): a read error must not restore that claim.
    #[serde(default)]
    pub http_boundary_link_count: Option<usize>,
    /// HTTP-BOUNDARY-1 (review-4 item 2): reader-framed degradation when the HTTP
    /// link read failed. Present → the boundary-meaningfulness hint is suppressed
    /// (unknown, not "meaningless") and this is shown instead.
    #[serde(default)]
    pub http_boundary_link_degraded: Option<String>,
    /// MODULE-EDGES-1 §2.1: the cross-module dependency edge list, from the daemon's
    /// SAME `load_module_graph_facts` read the rollups/count come from. `Some` (even
    /// empty) = authoritative single-read fact; `None` = older daemon (UNKNOWN) → the
    /// pre-slice rollup-derived count line is preserved, never a false zero.
    #[serde(default)]
    pub edges: Option<Vec<ModuleEdgeEntry>>,
}

impl ModulesListResponse {
    /// Render as human-readable text at the default (budgeted) presentation.
    ///
    /// review-0 item 1: this is the PRE-SLICE public signature (`pub fn
    /// render_human(&self)`), preserved because `rgr` re-exports `presentation`
    /// (`pub mod presentation` in `lib.rs`) — changing it is a public API break beyond
    /// the ratified additive DTO field. The `--full` budget lever lives on the
    /// crate-private [`render_human_budgeted`](Self::render_human_budgeted).
    pub fn render_human(&self) -> String {
        self.render_human_budgeted(false)
    }

    /// Budget-aware human render. `full` uncaps the cross-module edge list (default
    /// budgets it to [`HUMAN_ROW_BUDGET`] with an honest "(+N more — --full)").
    ///
    /// `pub(crate)`: the sole caller is `commands/modules/list.rs` (which parses
    /// `--full`); the `--full` lever is a CLI concern, not part of the public renderer
    /// contract (review-0 item 1).
    pub(crate) fn render_human_budgeted(&self, full: bool) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        // Note: daemon doesn't provide display_name for modules_list,
        // so we use a generic header. Future: request display_name field.
        out.push_str("Modules\n\n");

        // ── Count ──────────────────────────────────────────────────
        let count = self.results.len();
        out.push_str(&format_count(count, "module", "modules"));
        out.push('\n');

        if self.results.is_empty() {
            out.push_str("\nNo modules detected.\n");
            out.push_str(
                "\nhint: modules are inferred from directory structure or declared in manifests.\n",
            );
            return out;
        }

        out.push('\n');

        // ── Twin-name disambiguation (MODULES-IDENTITY-2 §2.1) ──────
        // Two modules can share a display name (django declares TWO `Django`
        // modules, both rooted at `.`) — indistinguishable rows mislead the
        // reader. Append the owning manifest suffix (`Django [pyproject.toml]` /
        // `Django [package.json]`) ONLY on a genuine collision, via the SAME
        // shared helper `orient` uses — one implementation, never a second copy.
        // A UNIQUE display name gets NO suffix, so unique-name repos (glamCRM's
        // modules are unique) stay byte-identical to today.
        let rows: Vec<ModuleRow> = self
            .results
            .iter()
            .map(|m| ModuleRow {
                path: &m.canonical_root_path,
                name: Some(m.display_name.as_str()).filter(|n| !n.is_empty()),
                manifest: m.manifest.as_deref(),
            })
            .collect();
        let effective_names: Vec<&str> = rows.iter().map(|r| r.effective_name()).collect();
        let labels: Vec<String> = self
            .results
            .iter()
            .enumerate()
            .map(
                |(i, m)| match collision_disambiguator(&rows, &effective_names, i) {
                    Some(token) => format!("{} [{token}]", m.display_name),
                    None => m.display_name.clone(),
                },
            )
            .collect();

        // ── Module rows ────────────────────────────────────────────
        // Column width is computed over the FINAL rendered labels (which include
        // any disambiguation suffix), so an added suffix never breaks alignment;
        // with no collisions the labels equal the display names → byte-identical.
        let max_name_len = labels.iter().map(|l| l.len()).max().unwrap_or(10).max(10);

        for (module, name) in self.results.iter().zip(labels.iter()) {
            let files = format_files_compact(module.owned_file_count, module.owned_test_file_count);
            let dead = format_dead_compact(module.dead_symbol_count);
            let violations = format_count(module.violation_count, "violation", "violations");
            let kind_conf = format_kind_confidence(&module.module_kind, module.confidence);

            out.push_str(&format!(
                "  {:width$}  {:>20}  {:>10}  {:>14}  {}\n",
                name,
                files,
                dead,
                violations,
                kind_conf,
                width = max_name_len
            ));
        }

        // ── Unreferenced-symbol caveat (OUTPUT-DOC-TRUTH-AUDIT-1) ───
        // The `unref?` column is dead_symbol_count: a SYNTACTIC graph-orphan
        // estimate (no inbound reference in the modeled graph), a low-reliability
        // Layer-2 inference — NOT a Layer-0 "safe to delete" fact. The bare label
        // "dead" overclaimed it; this footnote keeps the (useful) count while
        // making its certainty class honest and pointing at the trust surface.
        out.push('\n');
        out.push_str(
            "note: unref? = symbols with no inbound reference in the indexed graph \
             (syntactic estimate); over-counts under low call-graph resolution; \
             run `rmap trust` for reliability.\n",
        );
        // RECON-M-R3a (g2u-a): the reduction-only compiler-witness aggregate — rendered ONLY
        // when the daemon attached nonzero reductions (W-BOTH with a current measured ledger)
        // that pass the §5.3.0 labeling gate (review-2 item 1: `accounting: "union"` +
        // coverage basis, per row, via the ONE shared gate) — the coverage renders beside the
        // aggregate. A row failing the gate contributes NOTHING (its union value is
        // suppressed, never rendered unlabeled). Per-module figures ride the JSON. Reduction
        // only: the `unref?` column is untouched.
        let mut reduced = 0u64;
        let mut modules_with = 0usize;
        // All rows come from the ONE shared projection, so today the set holds one phrase;
        // collecting a set keeps the join honest if that ever diverges.
        let mut coverages: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for m in &self.results {
            if let Some((n, coverage)) = m.unref_reduction.as_ref().and_then(|b| {
                let coverage = crate::presentation::witnesses::union_coverage_phrase(b)?;
                let n = b
                    .get("fewer_flagged")
                    .and_then(|v| v.as_u64())
                    .filter(|n| *n > 0)?;
                Some((n, coverage))
            }) {
                reduced += n;
                modules_with += 1;
                coverages.insert(coverage);
            }
        }
        if reduced > 0 {
            out.push_str(&format!(
                "reconciled: {} fewer flagged across {} module{} — compiler-verified \
                 references found — combined analyses (coverage: {}).\n",
                reduced,
                modules_with,
                if modules_with == 1 { "" } else { "s" },
                coverages.into_iter().collect::<Vec<_>>().join("; "),
            ));
        }

        // ── Cross-module dependency edges (MODULE-EDGES-1 §2.1) ─────
        // The count line and the edge list below come from ONE array (`self.edges`),
        // so they can never disagree. An OLDER daemon omits the field (UNKNOWN) → the
        // edge list is labelled unavailable-with-reason, never a false zero.
        match &self.edges {
            Some(edges) => self.render_edge_list(&mut out, edges, full),
            None => self.render_edges_unavailable(&mut out),
        }

        out
    }

    /// MODULE-EDGES-1 §2.1: render the cross-module edge list under a count line
    /// derived from the SAME array. Sorted by reference count DESC then (source,
    /// target) ASC (deterministic); budgeted to [`HUMAN_ROW_BUDGET`] with an honest
    /// remainder unless `full`. Zero-state keeps the pre-slice honest handling
    /// (the "boundaries may not be meaningful" / HTTP-link note).
    fn render_edge_list(&self, out: &mut String, edges: &[ModuleEdgeEntry], full: bool) {
        out.push('\n');
        if edges.is_empty() {
            out.push_str("No cross-module dependencies detected.\n");
            if self.results.len() > 1 {
                // HTTP-BOUNDARY-1: the Layer-3 boundary note (heuristic-HTTP-link
                // vs meaningless-boundaries vs failed-read-unknown) is decided in
                // the crate-private `http_boundary` presenter — kept off this file.
                out.push_str(&super::http_boundary::render_modules_note(
                    self.http_boundary_link_count,
                    self.http_boundary_link_degraded.as_deref(),
                ));
            }
            return;
        }

        // Count == list length, from the SAME array → disagreement impossible.
        out.push_str(&format_count(
            edges.len(),
            "cross-module dependency",
            "cross-module dependencies",
        ));
        out.push_str(" detected.\n");

        let mut sorted: Vec<&ModuleEdgeEntry> = edges.iter().collect();
        sorted.sort_by(|a, b| {
            b.import_count
                .cmp(&a.import_count)
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.target.cmp(&b.target))
        });

        let shown = if full {
            sorted.len()
        } else {
            sorted.len().min(HUMAN_ROW_BUDGET)
        };
        for e in &sorted[..shown] {
            out.push_str(&format!(
                "  {} \u{2192} {} ({} file-level import{})\n",
                e.source,
                e.target,
                e.import_count,
                if e.import_count == 1 { "" } else { "s" },
            ));
        }
        if let Some(remainder) = budget_remainder_line(sorted.len(), shown) {
            out.push_str(&remainder);
        }
    }

    /// MODULE-EDGES-1 §2.1 (review-0 item 4): honest degradation for an OLDER daemon
    /// that sent NO `edges` array. The authoritative edge list is UNAVAILABLE — say so,
    /// and WHY (the connected daemon predates the field). We do NOT mint a false zero
    /// from the absent field, and we do NOT present the pre-slice rollup-derived count
    /// as an authoritative edge-list count (standing honesty rule #1): the rough rollup
    /// figure is surfaced only when nonzero and is LABELLED a rough estimate, distinct
    /// from the edge-list count it cannot stand in for.
    fn render_edges_unavailable(&self, out: &mut String) {
        let total_outbound: usize = self
            .results
            .iter()
            .map(|m| m.outbound_dependency_count)
            .sum();
        let total_inbound: usize = self
            .results
            .iter()
            .map(|m| m.inbound_dependency_count)
            .sum();

        out.push('\n');
        out.push_str(
            "Cross-module edge list unavailable — the connected daemon did not provide it \
             (older daemon predating MODULE-EDGES-1; upgrade the daemon to see the named edges).\n",
        );
        if total_outbound == 0 && total_inbound == 0 {
            // The rollups the older daemon DID compute report no cross-module deps — a
            // rollup-derived fact, labelled as such (never re-cast as the edge list).
            out.push_str("Module rollups report no cross-module dependencies.\n");
            if self.results.len() > 1 {
                out.push_str(&super::http_boundary::render_modules_note(
                    self.http_boundary_link_count,
                    self.http_boundary_link_degraded.as_deref(),
                ));
            }
        } else {
            let rough = total_outbound.max(total_inbound) / 2; // rough rollup dedup
            out.push_str(&format!(
                "Module rollups suggest ~{rough} cross-module dependencies \
                 (rough rollup estimate, not the authoritative edge-list count).\n",
            ));
        }
    }
}

// review-0 item 2: the test body lives in a sibling file (via `#[path]`, the
// `orient_tests.rs` idiom) so this renderer file stays under the >500-line structural
// guardrail. Still a child module of `modules_list` — `use super::*` reaches the
// response structs and `HUMAN_ROW_BUDGET` exactly as an inline `mod tests` would.
#[cfg(test)]
#[path = "modules_list_tests.rs"]
mod tests;
