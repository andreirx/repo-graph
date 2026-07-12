//! Presentation layer for the `stats` command.
//!
//! # CLI-OUT-2C
//!
//! Transforms daemon module stats into human-readable plain text.
//! Renders sorted sections to point the reader in the right direction. No
//! threshold-based "at risk" labeling. MODULE-MODEL-2 §13 D7: every per-group
//! table is bounded to the top [`STATS_SECTION_CAP`] rows + an honest omission
//! line; the COMPLETE set always rides `stats --json`.
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
//! By size
//!   src/handlers           files=45  symbols=892
//!   src/models             files=38  symbols=634
//!   ...
//!
//! By fan-in
//!   src/utils              fan_in=18  fan_out=2
//!   src/models             fan_in=15  fan_out=4
//!   ...
//!
//! By fan-out
//!   src/handlers           fan_out=12  fan_in=3
//!   src/api                fan_out=9   fan_in=5
//!   ...
//!
//! By distance from main sequence
//!   src/legacy             D=0.89  I=0.11  A=0.00
//!   src/adapters           D=0.72  I=0.28  A=0.00
//!   ...
//! ```

use serde::Deserialize;

use crate::presentation::heading;

/// MODULE-MODEL-2 §13 D7: the top-N cap for EVERY per-group table on the human
/// `stats` surface — the folded "Package groups" table AND the four per-directory
/// "By size / fan-in / fan-out / distance" views. `stats` has no orient-style
/// budget ladder, so it bounds at a single generous tier (aligned with orient's
/// `large`) followed by an honest omission line. `stats --json` carries the
/// COMPLETE set (the daemon folds + ships every group via the shared
/// `rollup_package_groups`), so bounding the human tables loses nothing.
///
/// ONE cap for ALL sections is the "same budget notion" the surface uses
/// (MODULE-MODEL-2 review-0 #1): the four `By …` tables enumerate the SAME
/// `self.stats` population, so bounding only one would leave its omission line
/// FALSE-in-context — the "omitted" groups would reappear in full three sections
/// down. Bounding every table keeps each omission line TRUE (the tail rows live
/// ONLY in `stats --json`).
const STATS_SECTION_CAP: usize = 50;

/// MODULE-MODEL-2 §13 D7: the honest omission tail for a bounded `stats` section.
///
/// Returns `Some("  … and N more {noun}s — see {drill}\n")` ONLY when `total`
/// exceeds [`STATS_SECTION_CAP`] (else `None` — nothing was hidden, so no line).
/// `total` is the COMPLETE group count (never the displayed subset), so the
/// omission count is always TRUE. `drill` names where the full set lives:
/// `stats --json` for every table; the package-group table additionally points at
/// `modules` (review-0 #3, parity with `orient`'s line). One helper backs all five
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
    /// MODULE-MODEL-2 §13 D7: every per-group table (the folded "Package groups"
    /// view and the four per-directory "By …" views) is bounded to
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

        // ── By size (per directory group — MODULE-MODEL-2 §13 D7 bounded) ──
        // Top-N by file count + a TRUE omission line (review-0 #1). The four
        // per-directory `By …` views all bound the SAME `self.stats` population,
        // so each omission line is honest: the omitted directory groups are NOT
        // shown elsewhere on the human surface — they ride `stats --json`.
        out.push_str(&heading("By size"));
        let mut by_size = self.stats.clone();
        by_size.sort_by(|a, b| {
            b.file_count
                .cmp(&a.file_count)
                // MODULE-MODEL-2 review-2 #2 + §13 D7: file count DESC then
                // lexicographic module path — and NO symbol-count key. D7 defines the
                // topology ranking as "top-N by file count, lexicographic-path
                // tie-break"; a secondary symbol-count key would order two equal-file
                // groups by an unrelated metric (and could change which tied rows
                // cross the cap boundary). Module paths are unique, so path alone
                // makes the order TOTAL → `.take(cap)` is deterministic regardless of
                // input row order (the review-1 #2 property, preserved).
                .then_with(|| a.module.cmp(&b.module))
        });
        for m in by_size.iter().take(STATS_SECTION_CAP) {
            out.push_str(&format!(
                "  {}  files={}  symbols={}\n",
                m.module, m.file_count, m.symbol_count
            ));
        }
        if let Some(line) =
            section_omission_line(by_size.len(), "directory group", "`stats --json`")
        {
            out.push_str(&line);
        }
        out.push('\n');

        // ── Dependency-section reliability caveat (HONEST-DEGRADATION-IMPL-1 D1) ──
        // fan-in/out, instability, and distance all ride the resolved IMPORTS graph. When that graph
        // is not fully resolved (import-graph axis != HIGH), the coupling numbers below are partial —
        // attach a reason-specific reader-context caveat (mirroring orient/trust's posture) so the
        // reader treats them as directional, not as measured architectural fact.
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

        // ── By fan-in (MODULE-MODEL-2 §13 D7 bounded — see "By size") ──
        out.push_str(&heading("By fan-in"));
        let mut by_fan_in = self.stats.clone();
        // review-1 #2: module-path tie-break → TOTAL order → deterministic `.take(cap)`.
        by_fan_in.sort_by(|a, b| {
            b.fan_in
                .cmp(&a.fan_in)
                .then_with(|| a.module.cmp(&b.module))
        });
        for m in by_fan_in.iter().take(STATS_SECTION_CAP) {
            out.push_str(&format!(
                "  {}  fan_in={}  fan_out={}\n",
                m.module, m.fan_in, m.fan_out
            ));
        }
        if let Some(line) =
            section_omission_line(by_fan_in.len(), "directory group", "`stats --json`")
        {
            out.push_str(&line);
        }
        out.push('\n');

        // ── By fan-out (MODULE-MODEL-2 §13 D7 bounded — see "By size") ──
        out.push_str(&heading("By fan-out"));
        let mut by_fan_out = self.stats.clone();
        // review-1 #2: module-path tie-break → TOTAL order → deterministic `.take(cap)`.
        by_fan_out.sort_by(|a, b| {
            b.fan_out
                .cmp(&a.fan_out)
                .then_with(|| a.module.cmp(&b.module))
        });
        for m in by_fan_out.iter().take(STATS_SECTION_CAP) {
            out.push_str(&format!(
                "  {}  fan_out={}  fan_in={}\n",
                m.module, m.fan_out, m.fan_in
            ));
        }
        if let Some(line) =
            section_omission_line(by_fan_out.len(), "directory group", "`stats --json`")
        {
            out.push_str(&line);
        }
        out.push('\n');

        // ── By distance from main sequence (MODULE-MODEL-2 §13 D7 bounded) ──
        out.push_str(&heading("By distance from main sequence"));
        let mut by_distance = self.stats.clone();
        by_distance.sort_by(|a, b| {
            b.distance_from_main_sequence
                .partial_cmp(&a.distance_from_main_sequence)
                .unwrap_or(std::cmp::Ordering::Equal)
                // review-1 #2: module-path tie-break → TOTAL order → deterministic
                // `.take(cap)` (also pins the order of the many `None`/`Equal` rows,
                // which `partial_cmp` alone leaves at input order).
                .then_with(|| a.module.cmp(&b.module))
        });
        for m in by_distance.iter().take(STATS_SECTION_CAP) {
            // HONEST-DEGRADATION-IMPL-1 (D1): a degenerate (zero-degree) module has unknown
            // instability/distance — render "unknown", never "0.00" read as "on the main sequence".
            // Abstractness is import-graph-independent, so it stays a concrete number.
            out.push_str(&format!(
                "  {}  D={}  I={}  A={:.2}\n",
                m.module,
                fmt_metric(m.distance_from_main_sequence),
                fmt_metric(m.instability),
                m.abstractness
            ));
        }
        if let Some(line) =
            section_omission_line(by_distance.len(), "directory group", "`stats --json`")
        {
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

    #[test]
    fn render_human_sorts_by_size_descending() {
        let resp = sample_stats();
        let output = resp.render_human();
        // src/handlers (45 files) should come before src/models (38 files)
        let by_size_section = output
            .split("By size\n")
            .nth(1)
            .unwrap()
            .split("By fan-in")
            .next()
            .unwrap();
        let handlers_pos = by_size_section.find("src/handlers").unwrap();
        let models_pos = by_size_section.find("src/models").unwrap();
        assert!(handlers_pos < models_pos);
    }

    #[test]
    fn render_human_sorts_by_fan_in_descending() {
        let resp = sample_stats();
        let output = resp.render_human();
        // src/utils (fan_in=18) should come first
        let by_fan_in_section = output
            .split("By fan-in\n")
            .nth(1)
            .unwrap()
            .split("By fan-out")
            .next()
            .unwrap();
        let utils_pos = by_fan_in_section.find("src/utils").unwrap();
        let models_pos = by_fan_in_section.find("src/models").unwrap();
        assert!(utils_pos < models_pos);
    }

    #[test]
    fn render_human_sorts_by_distance_descending() {
        let resp = sample_stats();
        let output = resp.render_human();
        // src/models (D=0.86) should come before src/utils (D=0.5)
        let by_distance_section = output
            .split("By distance from main sequence\n")
            .nth(1)
            .unwrap();
        let models_pos = by_distance_section.find("src/models").unwrap();
        let utils_pos = by_distance_section.find("src/utils").unwrap();
        assert!(models_pos < utils_pos);
    }

    #[test]
    fn render_human_includes_all_modules_in_each_section() {
        let resp = sample_stats();
        let output = resp.render_human();
        // All 3 modules should appear in each section
        for section in ["By size", "By fan-in", "By fan-out", "By distance"] {
            let section_text = output.split(section).nth(1).unwrap_or("");
            assert!(
                section_text.contains("src/handlers"),
                "section {} missing src/handlers",
                section
            );
            assert!(
                section_text.contains("src/models"),
                "section {} missing src/models",
                section
            );
            assert!(
                section_text.contains("src/utils"),
                "section {} missing src/utils",
                section
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
            package_groups: vec![],
            stats: vec![degenerate_module()],
        };
        let output = resp.render_human();
        let distance_section = output
            .split("By distance from main sequence")
            .nth(1)
            .expect("distance section present");
        assert!(
            distance_section.contains("D=unknown") && distance_section.contains("I=unknown"),
            "degenerate coupling must render unknown: {output}"
        );
        assert!(
            !distance_section.contains("D=0.00") && !distance_section.contains("I=0.00"),
            "must NOT render a bare 0.00 for an undefined metric: {output}"
        );
        // Abstractness (the kept classification metric) is still a concrete number.
        assert!(distance_section.contains("A=1.00"), "{output}");
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
            package_groups: vec![],
            stats: vec![degenerate_module()],
        };
        let output = resp.render_human();
        // The caveat sits ABOVE the dependency sections and names the real count + the direction.
        let caveat_pos = output
            .find("1090 imports are unresolved")
            .expect("unresolved-imports caveat present");
        let fanin_pos = output.find("By fan-in").expect("fan-in section present");
        assert!(
            caveat_pos < fanin_pos,
            "caveat must precede the sections: {output}"
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
        let fanin_pos = output.find("By fan-in").expect("fan-in section present");
        assert!(
            action_pos < fanin_pos,
            "next-action must precede fan-in: {output}"
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
    fn render_human_bounds_directory_group_tables_with_true_omission() {
        // review-0 #1: the four per-directory "By …" tables share ONE population
        // (`self.stats`), so EACH must bound to STATS_SECTION_CAP + a TRUE omission
        // line — otherwise a group omitted from "By size" reappears in full under
        // "By fan-in", making the omission line false-in-context. Here N = cap + 15
        // directory groups: every table shows the cap + "… 15 more directory
        // groups", the Summary counts the COMPLETE N, and the full set rides JSON.
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
        // test isolates the directory-group tables.
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

        // Each of the 4 per-directory tables carries the SAME true omission line
        // (15 beyond each table's own top-cap). Exactly 4 occurrences.
        let omission = "… and 15 more directory groups — see `stats --json`";
        assert_eq!(
            output.matches(omission).count(),
            4,
            "each of the 4 per-directory tables must carry the true omission line:\n{output}"
        );

        // Spot-check "By size": the largest row (d000) is shown; the row ranked at
        // the cap boundary (d050 by size) is omitted; exactly the cap is shown.
        let by_size = output
            .split("By size\n")
            .nth(1)
            .unwrap()
            .split("By fan-in")
            .next()
            .unwrap();
        assert!(
            by_size.contains("d000  files="),
            "top row shown:\n{by_size}"
        );
        assert!(
            !by_size.contains(&format!("d{:03}  files=", STATS_SECTION_CAP)),
            "over-cap row omitted from By size:\n{by_size}"
        );
        // `symbols=` is unique to the By-size row format → exactly cap rows shown.
        assert_eq!(
            by_size.matches("symbols=").count(),
            STATS_SECTION_CAP,
            "By size must show exactly the cap:\n{by_size}"
        );
    }

    #[test]
    fn render_human_directory_tables_tie_break_is_deterministic() {
        // review-1 #2: when MORE THAN the cap of directory groups are TIED on a
        // table's sort metric, WHICH rows survive the `.take(cap)` must be a pure
        // function of the SET, not the input row order. Here 60 groups (`m00`..`m59`)
        // are identical on EVERY metric and fed in REVERSE (`m59` first). The
        // lexicographic module-path tie-break makes each of the four `By …` sorts a
        // TOTAL order, so every table shows the lexicographically-smallest cap
        // (`m00`..`m49`) and omits `m50`..`m59` — regardless of input order. Without
        // the tie-break the stable sort would keep input order and show `m59`..`m10`
        // (the opposite selection), so this pins the fix, not merely the bound.
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
        // One package group keeps the package table off the omission path.
        resp.package_groups = vec![StatsPackageGroup {
            name: "only".to_string(),
            file_count: 1,
            test_file_count: 0,
        }];
        let output = resp.render_human();

        // Summary counts ALL 60 directory groups.
        assert!(
            output.contains(&format!("directory groups: {n}")),
            "summary count must be the complete total:\n{output}"
        );

        // Each of the 4 per-directory tables carries the TRUE omission line (10 = 60-cap).
        assert_eq!(
            output
                .matches("… and 10 more directory groups — see `stats --json`")
                .count(),
            4,
            "each of the 4 tables carries the true omission count:\n{output}"
        );

        // Deterministic selection: the lexicographically-smallest cap survives in
        // EVERY table; the 10 largest-by-name rows are omitted everywhere. `m00` is
        // shown and `m50` omitted ONLY under the path tie-break (the old stable-sort
        // input order would show `m59`..`m10`, i.e. the inverse).
        let sections = [
            ("By size\n", "By fan-in"),
            ("By fan-in\n", "By fan-out"),
            ("By fan-out\n", "By distance"),
            ("By distance from main sequence\n", "\u{0}"), // to end
        ];
        for (start, end) in sections {
            let after = output.split(start).nth(1).unwrap_or("");
            let section = after.split(end).next().unwrap_or(after);
            assert!(
                section.contains("m00") && section.contains("m49"),
                "smallest-by-path cap must be shown in section starting {start:?}:\n{section}"
            );
            assert!(
                !section.contains("m50") && !section.contains("m59"),
                "largest-by-path rows must be omitted in section starting {start:?}:\n{section}"
            );
        }
    }

    // ── MODULE-MODEL-2 review-2 #2 — "By size" tie-break is path, NOT symbol count ──

    #[test]
    fn render_human_by_size_ties_break_on_path_not_symbol_count() {
        // review-2 #2 + §13 D7: three groups TIED on file_count but with DIFFERENT
        // symbol counts, whose path order is the INVERSE of their symbol-count order.
        // Under D7 (file DESC, then lexicographic path — no symbol key) the rows come
        // out `src/aaa, src/bbb, src/ccc`. With the removed symbol-count key they
        // would come out `src/bbb (999), src/ccc (500), src/aaa (1)` — so this
        // fixture DETECTS the key, unlike the earlier all-tied fixture.
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
                symbol_count: 999, // largest symbols — would rank FIRST under old key
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
        let by_size = output
            .split("By size\n")
            .nth(1)
            .unwrap()
            .split("By fan-in")
            .next()
            .unwrap();
        let a = by_size.find("src/aaa").expect("aaa present");
        let b = by_size.find("src/bbb").expect("bbb present");
        let c = by_size.find("src/ccc").expect("ccc present");
        assert!(
            a < b && b < c,
            "By size must order tied-file rows by path (aaa<bbb<ccc), NOT by symbol \
             count (which would give bbb<ccc<aaa):\n{by_size}"
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
