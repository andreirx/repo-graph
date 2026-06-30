//! Presentation layer for the `stats` command.
//!
//! # CLI-OUT-2C
//!
//! Transforms daemon module stats into human-readable plain text.
//! Renders full sorted sections to point the reader in the right direction.
//! No arbitrary top-N clipping or threshold-based "at risk" labeling.
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

use repo_graph_agent::{rollup_package_groups, DirGroup};
use serde::Deserialize;

use crate::presentation::heading;

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
    /// Renders full sorted sections. No arbitrary top-N clipping.
    /// The caller can pipe to `head` or redirect to file if needed.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        let repo_display = self
            .display_name
            .as_deref()
            .unwrap_or_else(|| &self.repo_uid);
        out.push_str(&format!("Module Stats: {}\n\n", repo_display));

        // ── Package groups (MODULE-MODEL-1 D2(i)/D4) ───────────────
        // Fold the per-directory rows into logical package groups via the SAME
        // shared roll-up `orient` uses — so the two commands cannot report
        // divergent topology numbers. The rows below are the directory-level
        // detail (Martin metrics are per-directory); the package groups are the
        // merged main+test view the agent orients by.
        let package_groups = rollup_package_groups(
            &self
                .stats
                .iter()
                .map(|m| DirGroup {
                    path: m.module.clone(),
                    file_count: m.file_count.max(0) as u64,
                })
                .collect::<Vec<_>>(),
        );

        // ── Summary ────────────────────────────────────────────────
        // Self-labelled, never a bare "modules: N" (MODULE-MODEL-1 D1): the
        // package-group count agrees with `orient`; the directory-group count is
        // the number of leaf-directory rows enumerated below. The
        // declared/inferred `module_candidates` notion lives in `modules`/`trust`.
        out.push_str(&heading("Summary"));
        out.push_str(&format!("  package groups: {}\n", package_groups.len()));
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

        // ── Package groups (by size, main+test merged) ─────────────
        out.push_str(&heading(
            "Package groups (by size — directory/package topology, Layer 0/1)",
        ));
        for g in &package_groups {
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
        out.push('\n');

        // ── By size (per directory group) ──────────────────────────
        out.push_str(&heading("By size"));
        let mut by_size = self.stats.clone();
        by_size.sort_by(|a, b| {
            b.file_count
                .cmp(&a.file_count)
                .then_with(|| b.symbol_count.cmp(&a.symbol_count))
        });
        for m in &by_size {
            out.push_str(&format!(
                "  {}  files={}  symbols={}\n",
                m.module, m.file_count, m.symbol_count
            ));
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

        // ── By fan-in ──────────────────────────────────────────────
        out.push_str(&heading("By fan-in"));
        let mut by_fan_in = self.stats.clone();
        by_fan_in.sort_by(|a, b| b.fan_in.cmp(&a.fan_in));
        for m in &by_fan_in {
            out.push_str(&format!(
                "  {}  fan_in={}  fan_out={}\n",
                m.module, m.fan_in, m.fan_out
            ));
        }
        out.push('\n');

        // ── By fan-out ─────────────────────────────────────────────
        out.push_str(&heading("By fan-out"));
        let mut by_fan_out = self.stats.clone();
        by_fan_out.sort_by(|a, b| b.fan_out.cmp(&a.fan_out));
        for m in &by_fan_out {
            out.push_str(&format!(
                "  {}  fan_out={}  fan_in={}\n",
                m.module, m.fan_out, m.fan_in
            ));
        }
        out.push('\n');

        // ── By distance from main sequence ─────────────────────────
        out.push_str(&heading("By distance from main sequence"));
        let mut by_distance = self.stats.clone();
        by_distance.sort_by(|a, b| {
            b.distance_from_main_sequence
                .partial_cmp(&a.distance_from_main_sequence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for m in &by_distance {
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
}
