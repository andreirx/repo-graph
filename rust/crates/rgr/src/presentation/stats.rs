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
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleStats {
    pub module: String,
    pub fan_in: i64,
    pub fan_out: i64,
    pub instability: f64,
    pub abstractness: f64,
    pub distance_from_main_sequence: f64,
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
        let total_symbols: i64 = self.stats.iter().map(|m| m.symbol_count).sum();
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
            out.push_str(&format!(
                "  {}  D={:.2}  I={:.2}  A={:.2}\n",
                m.module, m.distance_from_main_sequence, m.instability, m.abstractness
            ));
        }

        out
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
            stats: vec![
                ModuleStats {
                    module: "src/handlers".to_string(),
                    fan_in: 5,
                    fan_out: 8,
                    instability: 0.62,
                    abstractness: 0.0,
                    distance_from_main_sequence: 0.38,
                    file_count: 45,
                    symbol_count: 892,
                },
                ModuleStats {
                    module: "src/models".to_string(),
                    fan_in: 12,
                    fan_out: 2,
                    instability: 0.14,
                    abstractness: 0.0,
                    distance_from_main_sequence: 0.86,
                    file_count: 38,
                    symbol_count: 634,
                },
                ModuleStats {
                    module: "src/utils".to_string(),
                    fan_in: 18,
                    fan_out: 0,
                    instability: 0.0,
                    abstractness: 0.5,
                    distance_from_main_sequence: 0.5,
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
            stats: vec![],
        };
        let output = resp.render_human();
        assert!(output.contains("Module Stats: repo_abc123"));
    }
}
