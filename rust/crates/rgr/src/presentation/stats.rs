//! Presentation layer for the `stats` command.
//!
//! # CLI-OUT-2C
//!
//! Transforms daemon module stats into human-readable plain text. QUANT-MECH-1
//! §2.3 (audit #10): the directory population is rendered ONCE — a single
//! "Directory groups" table with one row per directory carrying every metric as a
//! column, replacing the four former per-metric sections that each re-listed the
//! same ~50 directories. No threshold-based "at risk" labeling. MODULE-MODEL-2 §13
//! D7: the package-group table and the directory table are each bounded to the top
//! [`STATS_SECTION_CAP`] rows + an honest omission line; the COMPLETE set (and every
//! metric) always rides `stats --json`.
//!
//! ## Human Output Structure
//!
//! MODULE-MODEL-1 (D1/D4): the Summary is self-labelled — `package groups`
//! (logical packages, main+test merged; agrees with `orient`) and `directory
//! groups` (the leaf-directory rows below) — never a bare `modules: N`. The
//! declared/inferred `module_candidates` notion lives in `modules` / `trust`.
//!
//! ```text
//! Module Stats: billing-service
//!
//! Summary
//!   package groups: 18
//!   directory groups: 24
//!   total_files: 312
//!   total_symbols: 4521
//!
//! Package groups (by size — directory/package topology, Layer 0/1)
//!   handlers               files=45 (12 test)
//!   models                 files=38
//!   ...
//!
//! Directory groups (by size)
//!   path            files  symbols  fan_in  fan_out        I        D     A
//!   src/handlers       45      892       3       12     0.80     0.20  0.00
//!   src/models         38      634       4        2     0.33     0.67  0.00
//!   ...
//! ```

use serde::Deserialize;

use crate::presentation::heading;

/// MODULE-MODEL-2 §13 D7: the top-N cap for EACH per-group table on the human
/// `stats` surface — the folded "Package groups" table AND the single "Directory
/// groups" table (QUANT-MECH-1 §2.3 collapsed the four former per-metric views into
/// one). `stats` has no orient-style budget ladder, so it bounds at a single generous
/// tier (aligned with orient's `large`) followed by an honest omission line.
/// `stats --json` carries the COMPLETE set (the daemon folds + ships every group via
/// the shared `rollup_package_groups`), so bounding the human tables loses nothing.
///
/// ONE cap for both tables is the "same budget notion" the surface uses
/// (MODULE-MODEL-2 review-0 #1). Since QUANT-MECH-1 the directory population is a
/// SINGLE table (not four re-listing views), so its one omission line is
/// unambiguously TRUE — the tail rows live ONLY in `stats --json`.
const STATS_SECTION_CAP: usize = 50;

/// MODULE-MODEL-2 §13 D7: the honest omission tail for a bounded `stats` section.
///
/// Returns `Some("  … and N more {noun}s — see {drill}\n")` ONLY when `total`
/// exceeds [`STATS_SECTION_CAP`] (else `None` — nothing was hidden, so no line).
/// `total` is the COMPLETE group count (never the displayed subset), so the
/// omission count is always TRUE. `drill` names where the full set lives:
/// `stats --json` for every table; the package-group table additionally points at
/// `modules` (review-0 #3, parity with `orient`'s line). One helper backs both
/// bounded tables so the tail wording + count stay uniform — and so a future edit
/// cannot bound one table while leaving a sibling unbounded (the review-0 #1 bug).
fn section_omission_line(total: usize, noun: &str, drill: &str) -> Option<String> {
    if total <= STATS_SECTION_CAP {
        return None;
    }
    let more = total - STATS_SECTION_CAP;
    Some(format!(
        "  … and {} more {}{} — see {}\n",
        more,
        noun,
        if more == 1 { "" } else { "s" },
        drill,
    ))
}

// ── Response Types ───────────────────────────────────────────────────────────

/// Deserialized stats response from daemon.
#[derive(Debug, Deserialize)]
pub struct StatsResponse {
    #[serde(default)]
    pub repo_uid: String,
    #[serde(default)]
    pub snapshot_uid: String,
    /// Human-readable repo name for CLI display (CLI-OUT-2C).
    #[serde(default)]
    pub display_name: Option<String>,
    pub stats: Vec<ModuleStats>,
    #[serde(default)]
    pub count: usize,
    /// HONEST-DEGRADATION-IMPL-1 (D4): the repo-level all-SYMBOL count — the canonical "symbols"
    /// number `orient` also shows, sourced from `compute_repo_summary` (NOT a per-module row-sum,
    /// which loses symbols in files owned by no module). `None` on older/explicit paths → the
    /// renderer falls back to summing the rows (so a missing field never reads as a false zero).
    #[serde(default)]
    pub total_symbols: Option<i64>,
    /// HONEST-DEGRADATION-IMPL-1 (D1): the import-graph reliability posture for this snapshot (the
    /// SAME axis `trust`/`orient` consume). When `level != HIGH`, the dependency sections carry a
    /// reason-specific caveat. `None` → the daemon could not assemble the overlay → no caveat (we
    /// never fabricate a posture we did not compute).
    #[serde(default)]
    pub import_graph_reliability: Option<StatsReliabilityAxis>,
    /// HONEST-DEGRADATION-IMPL-2 (D5): the daemon's toolchain-aware honest next-action line, present only
    /// when relationship reliability is LOW. Rendered beneath the dependency caveat. `None` (absent on the
    /// wire) on a resolved repo or when no honest statement applies. Coherent with `orient`'s line (same
    /// daemon helper).
    #[serde(default)]
    pub relationship_next_action: Option<String>,
    /// MODULE-MODEL-2 §13 D4/D7: the COMPLETE folded package-group set, folded
    /// daemon-side via the SAME shared `rollup_package_groups` + manifest facts
    /// `orient` uses (so the two surfaces agree). The human table bounds it (top-N
    /// then an omission line); this field is the full set (`stats --json` exposes
    /// it whole). `#[serde(default)]` keeps it empty on an older daemon.
    #[serde(default)]
    pub package_groups: Vec<StatsPackageGroup>,
    /// MODULE-MODEL-2 (ROOT-MANIFEST-POLYGLOT, ratified 2026-07-12): the one-line
    /// reader-frame limitation marker the daemon attaches when a repo-root manifest
    /// was suppressed by the conservative rule (nested manifest roots coexist, so
    /// "." folds nothing and its directories degrade to directory groups). The SAME
    /// string `orient` carries (both from the shared `root_manifest_limitation`), so
    /// the two surfaces agree. `None` (absent on the wire) when nothing is
    /// suppressed — a genuine single-package or manifest-less repo carries no marker.
    #[serde(default)]
    pub root_manifest_limitation: Option<String>,
    /// RECON-M-R3a (g1u): the daemon's ADDITIVE union-accounting call block (opaque JSON —
    /// rendered through the shared `presentation::witnesses` projection). Present ONLY in
    /// W-BOTH with a current measured ledger; absent on the wire otherwise (R-0).
    #[serde(default)]
    pub witnesses: Option<serde_json::Value>,
}

/// MODULE-MODEL-2 §13 D4/D7: one folded package group in the stats response — the
/// reader-side mirror of `repo_graph_agent::PackageGroup`, deserialized from the
/// `package_groups` array the daemon attaches (the daemon owns the fold).
#[derive(Debug, Clone, Deserialize)]
pub struct StatsPackageGroup {
    pub name: String,
    #[serde(default)]
    pub file_count: u64,
    #[serde(default)]
    pub test_file_count: u64,
}

/// HONEST-DEGRADATION-IMPL-1 (D1): the import-graph reliability axis carried inline on the stats
/// response. A reader-side mirror of `repo_graph_trust::ReliabilityAxisScore` (deserialized from the
/// daemon JSON — `level` is the serialized `ReliabilityLevel`, e.g. "HIGH"/"LOW"; `reasons` are the
/// machine reason codes the renderer humanizes per-reason, e.g. `unresolved_imports=944`).
#[derive(Debug, Clone, Deserialize)]
pub struct StatsReliabilityAxis {
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleStats {
    pub module: String,
    pub fan_in: i64,
    pub fan_out: i64,
    /// HONEST-DEGRADATION-IMPL-1 (D1): `None` (JSON `null` = unknown) when the module has zero
    /// resolved import degree — the `0/0` instability is undefined, never a bare `0.0`.
    pub instability: Option<f64>,
    pub abstractness: f64,
    /// HONEST-DEGRADATION-IMPL-1 (D1): `None` (JSON `null`) when instability is unknown.
    pub distance_from_main_sequence: Option<f64>,
    pub file_count: i64,
    pub symbol_count: i64,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl StatsResponse {
    /// Render the stats response as human-readable plain text.
    ///
    /// MODULE-MODEL-2 §13 D7 + QUANT-MECH-1 §2.3: both per-group tables (the folded
    /// "Package groups" view and the single "Directory groups" table) are bounded to
    /// [`STATS_SECTION_CAP`] rows followed by an honest omission line; the COMPLETE
    /// set always rides `stats --json`. Headline counts (the Summary block) count
    /// ALL groups, never the displayed subset, so the omission counts are TRUE.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        let repo_display = self
            .display_name
            .as_deref()
            .unwrap_or_else(|| &self.repo_uid);
        out.push_str(&format!("Module Stats: {}\n\n", repo_display));

        // ── Summary (MODULE-MODEL-1 D1; MODULE-MODEL-2 §13 D4) ─────
        // Self-labelled, never a bare "modules: N": the package-group count is the
        // COMPLETE set the daemon folded via the SAME shared `rollup_package_groups`
        // + manifest facts `orient` uses (so the two surfaces agree); the
        // directory-group count is the leaf-directory rows enumerated below. The
        // declared/inferred `module_candidates` notion lives in `modules`/`trust`.
        out.push_str(&heading("Summary"));
        out.push_str(&format!(
            "  package groups: {}\n",
            self.package_groups.len()
        ));
        out.push_str(&format!("  directory groups: {}\n", self.stats.len()));

        let total_files: i64 = self.stats.iter().map(|m| m.file_count).sum();
        // HONEST-DEGRADATION-IMPL-1 (D4): prefer the repo-level all-SYMBOL count the daemon attaches
        // (== orient's headline); fall back to summing the module-owned rows only when the field is
        // absent. The row-sum can undercount (it omits symbols in files no module owns), so the
        // repo-level count is the canonical, cross-surface-coherent number.
        let total_symbols: i64 = self
            .total_symbols
            .unwrap_or_else(|| self.stats.iter().map(|m| m.symbol_count).sum());
        out.push_str(&format!("  total_files: {}\n", total_files));
        out.push_str(&format!("  total_symbols: {}\n", total_symbols));
        out.push('\n');

        if self.stats.is_empty() {
            out.push_str("No directory groups found.\n");
            return out;
        }

        // ── Package groups (by size — MODULE-MODEL-2 §13 D7 bounded) ─
        // Top-N by file count (the daemon returns them size-DESC, name-ASC), then
        // an honest omission line. The COMPLETE set rides `stats --json`.
        out.push_str(&heading(
            "Package groups (by size — directory/package topology, Layer 0/1)",
        ));
        // ROOT-MANIFEST-POLYGLOT (ratified 2026-07-12): the reader-frame limitation
        // marker renders here (before the rows) when a repo-root manifest was
        // suppressed — the SAME line `orient` carries (shared daemon helper), so the
        // two surfaces agree. Absent (None) = nothing suppressed → no note.
        if let Some(note) = &self.root_manifest_limitation {
            out.push_str(&format!("  {note}\n"));
        }
        let pkg_total = self.package_groups.len();
        for g in self.package_groups.iter().take(STATS_SECTION_CAP) {
            let test_suffix = if g.test_file_count > 0 {
                format!(" ({} test)", g.test_file_count)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "  {}  files={}{}\n",
                g.name, g.file_count, test_suffix
            ));
        }
        // review-0 #3: parity with `orient`'s omission line — package groups ARE the
        // physical topology of the `modules` notion, so the drill-down names BOTH
        // `stats --json` (the complete folded set) and `modules` (the declared/
        // inferred candidate view). Directory-group tables below point only at
        // `stats --json` (they are the raw per-directory rows, not the module view).
        if let Some(line) = section_omission_line(pkg_total, "group", "`stats --json` / `modules`")
        {
            out.push_str(&line);
        }
        out.push('\n');

        // ── Dependency-section reliability caveat (HONEST-DEGRADATION-IMPL-1 D1) ──
        // The Directory-groups table below carries the import-graph-derived columns
        // (fan_in/fan_out/I/D). When the imports graph is not fully resolved
        // (import-graph axis != HIGH), those columns are partial — attach a
        // reason-specific reader-context caveat (mirroring orient/trust) so the reader
        // treats them as directional, not as measured architectural fact. It renders
        // ONCE, above the single table (QUANT-MECH-1 §2.3: state each fact once).
        if let Some(caveat) = self.dependency_reliability_caveat() {
            out.push_str(&caveat);
            out.push('\n');
        }
        // HONEST-DEGRADATION-IMPL-2 (D5): the toolchain-aware honest next-action, beneath the dependency
        // caveat (the daemon emits it only when relationship reliability is LOW; coherent with `orient`).
        if let Some(line) = &self.relationship_next_action {
            out.push_str(line);
            out.push('\n');
        }
        // RECON-M-R3a (g1u, §5.3.2): the ADDITIVE reconciled union-call line — rendered ONLY
        // when the daemon attached the coverage-labeled block (W-BOTH with a current measured
        // ledger); absent otherwise (byte-identical output, R-0). Same shared renderer as
        // orient, so the two surfaces carry one phrasing.
        if let Some(line) = self
            .witnesses
            .as_ref()
            .and_then(crate::presentation::witnesses::g1u_line)
        {
            out.push_str(&line);
            out.push('\n');
        }

        // ── Directory groups (QUANT-MECH-1 §2.3: one row per directory, stated ONCE) ──
        // Audit #10 de-dup: the four former per-metric sections ("By size / By fan-in
        // / By fan-out / By distance") each RE-LISTED the SAME `self.stats` directory
        // population — the same ~50 directories printed four times. Collapsed to a
        // SINGLE table: one row per directory group carrying every metric as a column,
        // so each directory appears exactly once. No metric is dropped (files, symbols,
        // fan_in, fan_out, I, D, A are all columns); the per-metric ranked *views* are
        // traded for the honesty of a single listing — the reader/agent sorts the
        // column it needs, and the COMPLETE set (every metric) rides `stats --json`.
        //
        // Sorted by file_count DESC then module path ASC — the former "By size"
        // primary, and a TOTAL order (module paths are unique) so `.take(cap)` is
        // deterministic regardless of input row order. Bounded to STATS_SECTION_CAP +
        // a TRUE omission line (the count is the COMPLETE directory-group total).
        out.push_str(&heading("Directory groups (by size)"));
        let mut dirs = self.stats.clone();
        dirs.sort_by(|a, b| {
            b.file_count
                .cmp(&a.file_count)
                .then_with(|| a.module.cmp(&b.module))
        });
        let shown = &dirs[..STATS_SECTION_CAP.min(dirs.len())];

        // Column widths over the SHOWN rows (aligns to what is printed). Each width
        // has a floor = its header label width so the header never overflows.
        let path_w = shown
            .iter()
            .map(|m| m.module.len())
            .max()
            .unwrap_or(0)
            .max(4);
        let files_w = col_width(shown.iter().map(|m| m.file_count.to_string()), 5);
        let symbols_w = col_width(shown.iter().map(|m| m.symbol_count.to_string()), 7);
        let fanin_w = col_width(shown.iter().map(|m| m.fan_in.to_string()), 6);
        let fanout_w = col_width(shown.iter().map(|m| m.fan_out.to_string()), 7);
        // I/D render via `fmt_metric` → "unknown" (7 chars) on a degenerate row, so
        // their column floor is 7 to hold the widest token.
        let metric_w = 7;

        out.push_str(&format!(
            "  {:<path_w$}  {:>files_w$}  {:>symbols_w$}  {:>fanin_w$}  {:>fanout_w$}  {:>metric_w$}  {:>metric_w$}  {:>4}\n",
            "path", "files", "symbols", "fan_in", "fan_out", "I", "D", "A",
            path_w = path_w, files_w = files_w, symbols_w = symbols_w,
            fanin_w = fanin_w, fanout_w = fanout_w, metric_w = metric_w,
        ));
        for m in shown {
            // HONEST-DEGRADATION-IMPL-1 (D1): a degenerate (zero-degree) module has
            // unknown instability/distance — `fmt_metric` renders "unknown", never a
            // bare "0.00" that reads as "on the main sequence". Abstractness is
            // import-graph-independent, so it stays a concrete number.
            out.push_str(&format!(
                "  {:<path_w$}  {:>files_w$}  {:>symbols_w$}  {:>fanin_w$}  {:>fanout_w$}  {:>metric_w$}  {:>metric_w$}  {:>4.2}\n",
                m.module,
                m.file_count,
                m.symbol_count,
                m.fan_in,
                m.fan_out,
                fmt_metric(m.instability),
                fmt_metric(m.distance_from_main_sequence),
                m.abstractness,
                path_w = path_w, files_w = files_w, symbols_w = symbols_w,
                fanin_w = fanin_w, fanout_w = fanout_w, metric_w = metric_w,
            ));
        }
        if let Some(line) = section_omission_line(dirs.len(), "directory group", "`stats --json`") {
            out.push_str(&line);
        }

        out
    }

    /// HONEST-DEGRADATION-IMPL-1 (D1): the reader-context caveat for the dependency sections, or
    /// `None` when the import-graph posture is HIGH / unavailable (no noise on a resolved graph).
    /// One clause per non-HIGH reason actually present (each gated on its own trigger), mirroring
    /// `orient`'s reason-by-reason humanization — so it states only what is true and can NEVER print
    /// "0 imports unresolved". When the level is non-HIGH but no reason is recognized, a generic
    /// directional note (claiming no count) is emitted.
    fn dependency_reliability_caveat(&self) -> Option<String> {
        let axis = self.import_graph_reliability.as_ref()?;
        if axis.level == "HIGH" {
            return None;
        }
        let mut clauses: Vec<String> = Vec::new();
        for reason in &axis.reasons {
            if let Some(rest) = reason.strip_prefix("unresolved_imports=") {
                // The ONLY clause that names a count — and only when N > 0 (never "0 imports unresolved").
                if let Ok(n) = rest.parse::<u64>() {
                    if n > 0 {
                        clauses.push(format!(
                            "{n} imports are unresolved (e.g. external libraries / unresolved #include), so module coupling is under-counted"
                        ));
                    }
                }
            } else if reason == "alias_resolution_suspicion" {
                clauses.push(
                    "some import paths use aliases that may resolve to the wrong module, so coupling may be misattributed".to_string(),
                );
            } else if reason == "registry_pattern_suspicion" {
                clauses.push(
                    "registry/factory wiring does not appear as imports, so coupling may be under-counted through that indirection".to_string(),
                );
            }
        }
        let detail = if clauses.is_empty() {
            String::new()
        } else {
            format!(" — {}", clauses.join("; "))
        };
        Some(format!(
            "Dependency metrics below reflect only the imports resolved on this index{detail}; treat these as directional.\n"
        ))
    }
}

/// QUANT-MECH-1 §2.3: the render width for one numeric column of the Directory-groups
/// table — the widest rendered token, floored at the column's header-label width so the
/// header never overflows its own column. A tiny local helper for the four numeric
/// columns of the single table (its only callers); the alternative (four inline
/// `map().max().unwrap_or(0).max(floor)` chains) merely repeats this exact expression.
fn col_width(vals: impl Iterator<Item = String>, floor: usize) -> usize {
    vals.map(|s| s.len()).max().unwrap_or(0).max(floor)
}

/// HONEST-DEGRADATION-IMPL-1 (D1): render a coupling metric as a fixed-2dp number, or `unknown` when
/// it is `None` (a value that is a pure artifact of an unresolved import graph — architecture Rule #6:
/// `null` = unknown, never a bare `0`).
fn fmt_metric(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{v:.2}"),
        None => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stats() -> StatsResponse {
        StatsResponse {
            repo_uid: "repo_123".to_string(),
            snapshot_uid: "snap_456".to_string(),
            display_name: Some("my-service".to_string()),
            count: 3,
            total_symbols: None,
            import_graph_reliability: None,
            relationship_next_action: None,
            root_manifest_limitation: None,
            witnesses: None,
            // The daemon folds these 3 dirs (no manifest, shared `src` prefix) into
            // 3 package groups — pre-computed here since the client no longer folds.
            package_groups: vec![
                StatsPackageGroup {
                    name: "handlers".to_string(),
                    file_count: 45,
                    test_file_count: 0,
                },
                StatsPackageGroup {
                    name: "models".to_string(),
                    file_count: 38,
                    test_file_count: 0,
                },
                StatsPackageGroup {
                    name: "utils".to_string(),
                    file_count: 10,
                    test_file_count: 0,
                },
            ],
            stats: vec![
                ModuleStats {
                    module: "src/handlers".to_string(),
                    fan_in: 5,
                    fan_out: 8,
                    instability: Some(0.62),
                    abstractness: 0.0,
                    distance_from_main_sequence: Some(0.38),
                    file_count: 45,
                    symbol_count: 892,
                },
                ModuleStats {
                    module: "src/models".to_string(),
                    fan_in: 12,
                    fan_out: 2,
                    instability: Some(0.14),
                    abstractness: 0.0,
                    distance_from_main_sequence: Some(0.86),
                    file_count: 38,
                    symbol_count: 634,
                },
                ModuleStats {
                    module: "src/utils".to_string(),
                    fan_in: 18,
                    fan_out: 0,
                    instability: Some(0.0),
                    abstractness: 0.5,
                    distance_from_main_sequence: Some(0.5),
                    file_count: 10,
                    symbol_count: 120,
                },
            ],
        }
    }

    /// QUANT-MECH-1 §2.3: the single de-duped "Directory groups" table substring
    /// (heading through end of output). One place tests slice the table.
    fn directory_table(output: &str) -> &str {
        output
            .split("Directory groups (by size)\n")
            .nth(1)
            .expect("directory groups table present")
    }

    #[test]
    fn render_human_includes_repo_name() {
        let resp = sample_stats();
        let output = resp.render_human();
        assert!(output.contains("Module Stats: my-service"));
    }

    #[test]
    fn render_human_includes_summary() {
        let resp = sample_stats();
        let output = resp.render_human();
        // MODULE-MODEL-1 D1: self-labelled, never a bare "modules: N". The sample
        // dirs (src/handlers, src/models, src/utils) share the `src` prefix and
        // have no main/test split → 3 package groups == 3 directory groups.
        assert!(output.contains("package groups: 3"), "{output}");
        assert!(output.contains("directory groups: 3"), "{output}");
        assert!(
            !output.contains("  modules: "),
            "bare module count: {output}"
        );
        assert!(output.contains("total_files: 93")); // 45 + 38 + 10
        assert!(output.contains("total_symbols: 1646")); // 892 + 634 + 120
    }

    #[test]
    fn render_human_names_package_groups() {
        // The merged package-group view (prefix collapsed to last segment).
        let resp = sample_stats();
        let output = resp.render_human();
        let pkg_section = output
            .split("Package groups (by size")
            .nth(1)
            .expect("package groups section present");
        for name in ["handlers", "models", "utils"] {
            assert!(pkg_section.contains(name), "missing {name}: {output}");
        }
    }

    /// COHERENCE-2 §2.4 (review-0 #2): the `stats` package-group headline prints `files=N (M test)`
    /// where `(M test)` is a SUBSET of the total N (total-inclusive headline) — the product-wide
    /// meaning `modules list`/`modules show`/`orient`/`check` share. A group with 100 files, 10 of
    /// them tests, renders `files=100 (10 test)` — never `files=90` (the non-test count) and never
    /// `110` (an addend). Positive-count proof (the sample fixture's groups are all zero-test).
    #[test]
    fn render_human_package_group_test_count_is_a_subset_of_the_total() {
        let mut resp = sample_stats();
        // 10 of 100 files are tests — a genuine subset on the largest group.
        resp.package_groups = vec![
            StatsPackageGroup {
                name: "core".to_string(),
                file_count: 100,
                test_file_count: 10,
            },
            StatsPackageGroup {
                name: "util".to_string(),
                file_count: 20,
                test_file_count: 0,
            },
        ];
        let output = resp.render_human();
        let pkg_section = output
            .split("Package groups (by size")
            .nth(1)
            .expect("package groups section present");
        assert!(
            pkg_section.contains("files=100 (10 test)"),
            "the group headline is the TOTAL (100) with the test count as a subset (10):\n{output}"
        );
        // The addend defect would show the non-test count (90) as the headline — forbidden.
        assert!(
            !pkg_section.contains("files=90"),
            "the non-test count must NOT be the headline (addend defect):\n{output}"
        );
        // A zero-test group renders the bare total with no `(0 test)` noise.
        assert!(
            pkg_section.contains("files=20") && !pkg_section.contains("files=20 (0 test)"),
            "a zero-test group renders the bare total:\n{output}"
        );
    }

    #[test]
    fn render_human_sorts_directory_table_by_size_descending() {
        let resp = sample_stats();
        let output = resp.render_human();
        // The single directory table is sorted by file_count DESC: src/handlers (45)
        // before src/models (38) before src/utils (10).
        let table = directory_table(&output);
        let handlers = table.find("src/handlers").unwrap();
        let models = table.find("src/models").unwrap();
        let utils = table.find("src/utils").unwrap();
        assert!(handlers < models && models < utils, "{table}");
    }

    #[test]
    fn render_human_directory_table_carries_every_metric_column() {
        // QUANT-MECH-1 §2.3: no metric is dropped by the de-dup — the single table
        // carries files, symbols, fan_in, fan_out, I, D, A as columns. src/utils has
        // fan_in=18, fan_out=0 — both must be present on its one row.
        let resp = sample_stats();
        let output = resp.render_human();
        for col in [
            "path", "files", "symbols", "fan_in", "fan_out", "I", "D", "A",
        ] {
            assert!(
                directory_table(&output).contains(col),
                "missing column {col}:\n{output}"
            );
        }
        // src/utils's coupling values appear on its single row (fan_in=18, fan_out=0).
        let table = directory_table(&output);
        let utils_row = table
            .lines()
            .find(|l| l.contains("src/utils"))
            .expect("utils row");
        assert!(
            utils_row.contains("18") && utils_row.contains('0'),
            "{utils_row}"
        );
    }

    #[test]
    fn render_human_states_each_directory_exactly_once() {
        // QUANT-MECH-1 §2.3 DoD: each directory group is rendered ONCE (the audit
        // measured the same population printed across FOUR per-metric sections). The
        // fully-qualified path (`src/handlers`, unlike the short package-group name
        // `handlers`) is unique to the directory table, so each occurs exactly once.
        let resp = sample_stats();
        let output = resp.render_human();
        for dir in ["src/handlers", "src/models", "src/utils"] {
            assert_eq!(
                output.matches(dir).count(),
                1,
                "{dir} must appear exactly once (de-duped):\n{output}"
            );
        }
    }

    #[test]
    fn render_human_empty_stats() {
        let resp = StatsResponse {
            repo_uid: "r1".to_string(),
            snapshot_uid: "s1".to_string(),
            display_name: Some("empty-repo".to_string()),
            count: 0,
            total_symbols: None,
            import_graph_reliability: None,
            relationship_next_action: None,
            root_manifest_limitation: None,
            witnesses: None,
            package_groups: vec![],
            stats: vec![],
        };
        let output = resp.render_human();
        assert!(output.contains("package groups: 0"));
        assert!(output.contains("directory groups: 0"));
        assert!(output.contains("No directory groups found."));
    }

    #[test]
    fn render_human_fallback_to_repo_uid_without_display_name() {
        let resp = StatsResponse {
            repo_uid: "repo_abc123".to_string(),
            snapshot_uid: "s1".to_string(),
            display_name: None,
            count: 0,
            total_symbols: None,
            import_graph_reliability: None,
            relationship_next_action: None,
            root_manifest_limitation: None,
            witnesses: None,
            package_groups: vec![],
            stats: vec![],
        };
        let output = resp.render_human();
        assert!(output.contains("Module Stats: repo_abc123"));
    }

    // ── HONEST-DEGRADATION-IMPL-1 (D4) — total_symbols uses the canonical repo count ──────────────

    #[test]
    fn render_human_prefers_repo_level_total_symbols_over_row_sum() {
        // D4: when the daemon attaches the repo-level all-SYMBOL count, the Summary shows THAT
        // (== orient), not the per-module row-sum. Here rows sum to 1646 but the repo count is 3977
        // (the divergence ownership-incompleteness produces) — the renderer must show 3977.
        let mut resp = sample_stats();
        resp.total_symbols = Some(3977);
        let output = resp.render_human();
        assert!(
            output.contains("total_symbols: 3977"),
            "Summary must use the repo-level count, not the row sum: {output}"
        );
        assert!(
            !output.contains("total_symbols: 1646"),
            "row-sum must NOT win when the repo-level count is present: {output}"
        );
    }

    #[test]
    fn render_human_falls_back_to_row_sum_when_total_absent() {
        // Defensive: a missing field must never read as a false zero — fall back to the row sum.
        let resp = sample_stats(); // total_symbols: None
        let output = resp.render_human();
        assert!(output.contains("total_symbols: 1646"), "{output}"); // 892 + 634 + 120
    }

    // ── HONEST-DEGRADATION-IMPL-1 (D1) — LOW caveat + degenerate-unknown (human side) ─────────────

    /// A single degenerate (zero-degree) module — its coupling metrics are unknown.
    fn degenerate_module() -> ModuleStats {
        ModuleStats {
            module: "src/core".to_string(),
            fan_in: 0,
            fan_out: 0,
            instability: None, // 0/0 — undefined
            abstractness: 1.0, // a classification artifact (kept, scoped out of D1)
            distance_from_main_sequence: None,
            file_count: 12,
            symbol_count: 340,
        }
    }

    fn axis(level: &str, reasons: &[&str]) -> StatsReliabilityAxis {
        StatsReliabilityAxis {
            level: level.to_string(),
            reasons: reasons.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn render_human_degenerate_coupling_is_unknown_not_zero() {
        // The nginx face of C1: fan_in=fan_out=0 must render I/D as `unknown`, never `0.00` (which
        // reads as "on the main sequence"). Abstractness stays a number (import-graph-independent).
        let resp = StatsResponse {
            repo_uid: "r".into(),
            snapshot_uid: "s".into(),
            display_name: Some("nginx".into()),
            count: 1,
            total_symbols: Some(3977),
            import_graph_reliability: Some(axis("LOW", &["unresolved_imports=1090"])),
            relationship_next_action: None,
            root_manifest_limitation: None,
            witnesses: None,
            package_groups: vec![],
            stats: vec![degenerate_module()],
        };
        let output = resp.render_human();
        let table = directory_table(&output);
        let core_row = table
            .lines()
            .find(|l| l.contains("src/core"))
            .expect("core row present");
        // Degenerate coupling (fan_in=fan_out=0) → I and D render "unknown", never a
        // bare 0.00 that reads as "on the main sequence".
        assert!(
            core_row.contains("unknown"),
            "degenerate coupling must render unknown: {core_row}"
        );
        // The row must not present a fabricated 0.00 for the undefined I/D metrics.
        // (Abstractness is import-graph-independent → stays a concrete number, 1.00.)
        assert!(
            core_row.contains("1.00"),
            "abstractness stays a concrete number: {core_row}"
        );
        // Exactly two "unknown" tokens on the row: I and D.
        assert_eq!(
            core_row.matches("unknown").count(),
            2,
            "both I and D must be unknown: {core_row}"
        );
    }

    #[test]
    fn render_human_low_import_graph_attaches_reason_specific_caveat() {
        let resp = StatsResponse {
            repo_uid: "r".into(),
            snapshot_uid: "s".into(),
            display_name: Some("nginx".into()),
            count: 1,
            total_symbols: Some(3977),
            import_graph_reliability: Some(axis("LOW", &["unresolved_imports=1090"])),
            relationship_next_action: None,
            root_manifest_limitation: None,
            witnesses: None,
            package_groups: vec![],
            stats: vec![degenerate_module()],
        };
        let output = resp.render_human();
        // The caveat sits ABOVE the directory table (which carries the coupling
        // columns) and names the real count + the direction.
        let caveat_pos = output
            .find("1090 imports are unresolved")
            .expect("unresolved-imports caveat present");
        let table_pos = output
            .find("Directory groups (by size)")
            .expect("directory table present");
        assert!(
            caveat_pos < table_pos,
            "caveat must precede the directory table: {output}"
        );
        assert!(output.contains("under-counted"), "{output}");
        assert!(output.contains("treat these as directional"), "{output}");
    }

    #[test]
    fn render_human_high_import_graph_shows_no_caveat() {
        // No noise on a fully-resolved repo: HIGH posture → no dependency caveat.
        let mut resp = sample_stats();
        resp.import_graph_reliability = Some(axis("HIGH", &[]));
        let output = resp.render_human();
        assert!(
            !output.contains("treat these as directional"),
            "HIGH posture must not emit a caveat: {output}"
        );
    }

    #[test]
    fn render_human_caveat_never_prints_zero_imports_unresolved() {
        // review-1 #1 guard: a non-HIGH axis whose cause is alias-suspicion (unresolved count = 0)
        // must show the alias clause and NEVER the false string "0 imports unresolved".
        let mut resp = sample_stats();
        resp.import_graph_reliability = Some(axis("LOW", &["alias_resolution_suspicion"]));
        let output = resp.render_human();
        assert!(
            output.contains("treat these as directional"),
            "caveat present: {output}"
        );
        assert!(output.contains("aliases"), "alias clause present: {output}");
        assert!(
            !output.contains("imports are unresolved") && !output.contains("0 imports"),
            "must never manufacture an unresolved-count claim: {output}"
        );
    }

    // ── HONEST-DEGRADATION-IMPL-2 (D5) — toolchain-aware next-action line renders ──────────────────

    #[test]
    fn render_human_renders_relationship_next_action_beneath_caveat() {
        // D5: the daemon-supplied next-action renders in the reliability-context area (after the
        // dependency caveat, before the fan-in section). The daemon owns WHICH line; the renderer just
        // surfaces it (here the C no-path case).
        let mut resp = sample_stats();
        resp.import_graph_reliability = Some(axis("LOW", &["unresolved_imports=1090"]));
        resp.relationship_next_action = Some(
            "no semantic-resolution path exists for C on this build; these relationship facts remain \
             low-confidence"
                .to_string(),
        );
        let output = resp.render_human();
        let action_pos = output
            .find("no semantic-resolution path exists for C")
            .expect("next-action present");
        let table_pos = output
            .find("Directory groups (by size)")
            .expect("directory table present");
        assert!(
            action_pos < table_pos,
            "next-action must precede the directory table: {output}"
        );
    }

    #[test]
    fn render_human_omits_next_action_when_absent() {
        // None (resolved repo / no honest statement) → nothing rendered, no noise.
        let resp = sample_stats(); // relationship_next_action: None
        let output = resp.render_human();
        assert!(!output.contains("semantic-resolution"), "{output}");
        assert!(!output.contains("rmap enrich"), "{output}");
    }

    // ── MODULE-MODEL-2 §13 D7 — bounded package-group table + true omission ────────

    #[test]
    fn render_human_bounds_package_groups_with_true_omission() {
        // A >cap-group repo renders the top-STATS_SECTION_CAP by size + an
        // honest omission line; the Summary count is the COMPLETE total. The full
        // set rides `stats --json` (the daemon ships it whole).
        let mut resp = sample_stats(); // non-empty `stats` (skips the empty return)
        let n = STATS_SECTION_CAP + 20;
        resp.package_groups = (0..n)
            .map(|i| StatsPackageGroup {
                name: format!("g{i:03}"),
                file_count: (1000 - i) as u64,
                test_file_count: 0,
            })
            .collect();
        let output = resp.render_human();

        // Summary count is the COMPLETE total, never the displayed count.
        assert!(
            output.contains(&format!("package groups: {n}")),
            "summary count must be the complete total:\n{output}"
        );
        // Top group shown; the group at the cap boundary omitted.
        assert!(
            output.contains("g000  files=1000"),
            "top group shown:\n{output}"
        );
        assert!(
            output.contains(&format!("g{:03}  files=", STATS_SECTION_CAP - 1)),
            "last in-cap group shown:\n{output}"
        );
        assert!(
            !output.contains(&format!("g{:03}  files=", STATS_SECTION_CAP)),
            "first over-cap group omitted:\n{output}"
        );
        // True omission line (n - cap = 20) WITH the `/ modules` drill-down
        // (review-0 #3, parity with orient's line).
        assert!(
            output.contains("… and 20 more groups — see `stats --json` / `modules`"),
            "true omission line with modules drill-down:\n{output}"
        );
    }

    #[test]
    fn render_human_no_omission_when_within_cap() {
        // At or below the cap, every group is shown and there is NO omission line.
        let mut resp = sample_stats();
        resp.package_groups = (0..STATS_SECTION_CAP)
            .map(|i| StatsPackageGroup {
                name: format!("g{i:03}"),
                file_count: 1,
                test_file_count: 0,
            })
            .collect();
        let output = resp.render_human();
        assert!(
            !output.contains("more groups — see"),
            "no omission at/under cap:\n{output}"
        );
    }

    #[test]
    fn render_human_bounds_directory_table_with_true_omission() {
        // QUANT-MECH-1 §2.3: the single directory table bounds to STATS_SECTION_CAP +
        // ONE true omission line (the audit's four re-listing sections collapsed to
        // one). Here N = cap + 15 directory groups: the table shows the cap, carries
        // "… 15 more directory groups", the Summary counts the COMPLETE N, and the
        // full set rides `stats --json`.
        let mut resp = sample_stats();
        let n = STATS_SECTION_CAP + 15;
        resp.stats = (0..n)
            .map(|i| ModuleStats {
                module: format!("d{i:03}"),
                fan_in: (n - i) as i64,
                fan_out: i as i64,
                instability: Some(0.5),
                abstractness: 0.0,
                distance_from_main_sequence: Some((n - i) as f64),
                file_count: (n - i) as i64,
                symbol_count: (n - i) as i64,
            })
            .collect();
        // One package group keeps the package table OFF the omission path, so this
        // test isolates the directory table.
        resp.package_groups = vec![StatsPackageGroup {
            name: "only".to_string(),
            file_count: 1,
            test_file_count: 0,
        }];
        let output = resp.render_human();

        // Summary counts ALL directory groups, never the displayed subset.
        assert!(
            output.contains(&format!("directory groups: {n}")),
            "summary count must be the complete total:\n{output}"
        );

        // EXACTLY ONE omission line now (not four) — the de-dup's whole point.
        let omission = "… and 15 more directory groups — see `stats --json`";
        assert_eq!(
            output.matches(omission).count(),
            1,
            "the single directory table carries exactly one true omission line:\n{output}"
        );

        // The largest row (d000) is shown; the row ranked at the cap boundary (d050 by
        // size) is omitted; exactly the cap of rows is shown.
        let table = directory_table(&output);
        assert!(table.contains("d000 "), "top row shown:\n{table}");
        assert!(
            !table.contains(&format!("d{:03} ", STATS_SECTION_CAP)),
            "over-cap row omitted:\n{table}"
        );
        // Count data rows: lines that start with "  d" (path column). Exactly cap.
        let data_rows = table
            .lines()
            .filter(|l| l.trim_start().starts_with('d'))
            .count();
        assert_eq!(
            data_rows, STATS_SECTION_CAP,
            "table must show exactly the cap:\n{table}"
        );
    }

    #[test]
    fn render_human_directory_table_tie_break_is_deterministic() {
        // review-1 #2 (carried to the single table): when MORE THAN the cap of
        // directory groups are TIED on file_count, WHICH rows survive `.take(cap)`
        // must be a pure function of the SET, not the input row order. Here 60 groups
        // (`m00`..`m59`) are identical on EVERY metric and fed in REVERSE (`m59`
        // first). The lexicographic module-path tie-break makes the sort a TOTAL
        // order, so the table shows the lexicographically-smallest cap (`m00`..`m49`)
        // and omits `m50`..`m59` — regardless of input order.
        let mut resp = sample_stats();
        let n = STATS_SECTION_CAP + 10; // 60
        resp.stats = (0..n)
            .rev() // input order m59, m58, …, m00 — deliberately NOT the display order
            .map(|i| ModuleStats {
                module: format!("m{i:02}"),
                fan_in: 3,
                fan_out: 3,
                instability: Some(0.5),
                abstractness: 0.0,
                distance_from_main_sequence: Some(0.5),
                file_count: 7,
                symbol_count: 7,
            })
            .collect();
        resp.package_groups = vec![StatsPackageGroup {
            name: "only".to_string(),
            file_count: 1,
            test_file_count: 0,
        }];
        let output = resp.render_human();

        assert!(
            output.contains(&format!("directory groups: {n}")),
            "summary count must be the complete total:\n{output}"
        );
        assert_eq!(
            output
                .matches("… and 10 more directory groups — see `stats --json`")
                .count(),
            1,
            "the single table carries one true omission line:\n{output}"
        );

        let table = directory_table(&output);
        assert!(
            table.contains("m00") && table.contains("m49"),
            "smallest-by-path cap must be shown:\n{table}"
        );
        assert!(
            !table.contains("m50") && !table.contains("m59"),
            "largest-by-path rows must be omitted:\n{table}"
        );
    }

    // ── QUANT-MECH-1 §2.3 — directory table tie-break is path, NOT symbol count ──

    #[test]
    fn render_human_directory_table_ties_break_on_path_not_symbol_count() {
        // Three groups TIED on file_count but with DIFFERENT symbol counts, whose
        // path order is the INVERSE of their symbol-count order. Under the size sort
        // (file DESC, then lexicographic path — no symbol key) the rows come out
        // `src/aaa, src/bbb, src/ccc`. A stray symbol-count key would give
        // `src/bbb (999), src/ccc (500), src/aaa (1)` — so this fixture DETECTS a key.
        let mut resp = sample_stats();
        resp.stats = vec![
            ModuleStats {
                module: "src/aaa".to_string(),
                fan_in: 0,
                fan_out: 0,
                instability: Some(0.5),
                abstractness: 0.0,
                distance_from_main_sequence: Some(0.5),
                file_count: 10,
                symbol_count: 1, // smallest symbols, first by path
            },
            ModuleStats {
                module: "src/bbb".to_string(),
                fan_in: 0,
                fan_out: 0,
                instability: Some(0.5),
                abstractness: 0.0,
                distance_from_main_sequence: Some(0.5),
                file_count: 10,
                symbol_count: 999, // largest symbols — would rank FIRST under a symbol key
            },
            ModuleStats {
                module: "src/ccc".to_string(),
                fan_in: 0,
                fan_out: 0,
                instability: Some(0.5),
                abstractness: 0.0,
                distance_from_main_sequence: Some(0.5),
                file_count: 10,
                symbol_count: 500,
            },
        ];
        let output = resp.render_human();
        let table = directory_table(&output);
        let a = table.find("src/aaa").expect("aaa present");
        let b = table.find("src/bbb").expect("bbb present");
        let c = table.find("src/ccc").expect("ccc present");
        assert!(
            a < b && b < c,
            "directory table must order tied-file rows by path (aaa<bbb<ccc), NOT by \
             symbol count (which would give bbb<ccc<aaa):\n{table}"
        );
    }

    // ── MODULE-MODEL-2 ROOT-MANIFEST-POLYGLOT — visible limitation marker ──────────

    #[test]
    fn render_human_renders_root_manifest_limitation_marker() {
        // When the daemon attaches the marker (a root manifest was suppressed by the
        // conservative rule), it renders as a visible reader-frame note inside the
        // Package groups section, ABOVE the group rows (tells the reader what to
        // expect). The daemon owns the exact wording (shared with orient); the
        // renderer just surfaces it.
        let mut resp = sample_stats();
        resp.root_manifest_limitation = Some(
            "root package.json not folded — nested toolchains present; \
             root-owned directories shown as directory groups"
                .to_string(),
        );
        let output = resp.render_human();
        assert!(
            output.contains("root package.json not folded"),
            "marker must render:\n{output}"
        );
        // It sits inside the Package groups section, before the first group row.
        let pkg_section = output
            .split("Package groups (by size")
            .nth(1)
            .expect("package groups section present");
        let marker_pos = pkg_section
            .find("root package.json not folded")
            .expect("marker inside package-groups section");
        let first_row = pkg_section
            .find("handlers")
            .expect("first group row present");
        assert!(
            marker_pos < first_row,
            "marker must precede the group rows:\n{pkg_section}"
        );
    }

    #[test]
    fn render_human_omits_marker_when_no_root_manifest_limitation() {
        // None (single-package / manifest-less repo, nothing suppressed) → no note,
        // no noise.
        let resp = sample_stats(); // root_manifest_limitation: None
        let output = resp.render_human();
        assert!(
            !output.contains("not folded"),
            "no marker when nothing is suppressed:\n{output}"
        );
    }
}
