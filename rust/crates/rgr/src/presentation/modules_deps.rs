//! Presentation layer for module dependency command.
//!
//! # CLI-OUT-4 Group 3
//!
//! Response DTO and human renderer for:
//! - `modules deps` — cross-module dependency edges
//!
//! ## Change Axis
//!
//! This file changes when:
//! - Dependency summary language changes
//! - Edge presentation changes
//! - Direction filtering presentation changes
//!
//! It does NOT change when:
//! - `modules violations` output changes (different contract)
//! - Module catalog/inventory output changes (Groups 1-2)

use serde::Deserialize;

use super::module_shared::format_count;

// =============================================================================
// IMPORT DIAGNOSTICS (local copy - small struct, rendering differs by context)
// =============================================================================

/// Import analysis diagnostics from deps response.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportDiagnostics {
    #[serde(default, rename = "imports_total")]
    pub total_import_edges: u64,
    #[serde(default, rename = "imports_intra_module")]
    pub intra_module_edges: u64,
    #[serde(default, rename = "imports_cross_module")]
    pub cross_module_edges: u64,
    #[serde(default, rename = "imports_source_unowned")]
    pub from_unowned_edges: u64,
}

// =============================================================================
// MODULES DEPS RESPONSE
// =============================================================================

/// A module dependency edge in the deps response.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleDependencyEdge {
    #[serde(default, rename = "source")]
    pub source_module: String,
    #[serde(default, rename = "target")]
    pub target_module: String,
    #[serde(default)]
    pub import_count: u64,
    #[serde(default)]
    pub source_file_count: u64,
}

/// Response structure for modules deps command.
#[derive(Debug, Deserialize)]
pub struct ModulesDepsResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub results: Vec<ModuleDependencyEdge>,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub diagnostics: Option<ImportDiagnostics>,
    /// MODULES-DEPS-SUMMARY-SCOPE-1: the universe of `diagnostics` — `module` (the named
    /// module's imports the direction selects) or `repo` (the whole repository). Absent = a
    /// daemon that predates the key, whose `diagnostics` are always the repository's.
    #[serde(default)]
    pub diagnostics_scope: Option<String>,
    /// TEST-EDGE-SCOPE-1B (D-TESB-08): the import view. Absent = a daemon that predates the
    /// partition (stated, never a zero remainder).
    #[serde(default)]
    pub import_view: Option<serde_json::Value>,
    /// TEST-EDGE-SCOPE-1B: the imports the view excludes over the relations this answer covers.
    #[serde(default)]
    pub import_remainder: Option<serde_json::Value>,
    /// TEST-EDGE-SCOPE-1B (D-TESB-09): the undetermined files among the answer's importers.
    #[serde(default)]
    pub importer_test_status_undetermined: Option<serde_json::Value>,
}

impl ModulesDepsResponse {
    /// TEST-EDGE-SCOPE-1B (RG-REQ-004-L12): the remainder lines and the importer UNDETERMINED
    /// line (shared wording modules).
    fn partition_lines(&self) -> String {
        use super::import_partition::{
            partition_lines_from, surface_partition, EdgeNoun, Partition, Requires,
        };
        let mut out = String::new();
        // One decode rule (D-TESB-17 row U9).
        let partition = surface_partition(
            self.import_view.as_ref(),
            self.import_remainder.as_ref(),
            None,
            self.importer_test_status_undetermined.as_ref(),
            Requires::MODULE_EDGES,
        );
        let stated = matches!(partition, Partition::Stated(_));
        for line in partition_lines_from(partition, EdgeNoun::CrossModule) {
            out.push_str(&format!("{line}\n"));
        }
        if stated {
            if let Some(line) = super::test_status::undetermined_files_line(
                self.importer_test_status_undetermined.as_ref(),
            ) {
                out.push_str(&format!("{line}\n"));
            }
        }
        out
    }

    /// MODULES-DEPS-SUMMARY-SCOPE-1 (SLICE_DOC 2.1 item 3, 2.2): the Summary header, decided by
    /// (`diagnostics_scope`, `module`). Each header states the universe of the figures below it.
    fn summary_header(&self, direction_label: &str) -> String {
        match (self.diagnostics_scope.as_deref(), self.module.as_deref()) {
            // The figures count module X's imports that the direction selects.
            (Some("module"), Some(module)) => {
                format!("Summary (module {module}, {direction_label}):")
            }
            (Some("module"), None) => {
                "Summary (diagnostics scope \"module\" but the answer names no module):".to_string()
            }
            // Repo-wide figures: `repo`, or absent (a daemon older than the key, whose
            // diagnostics are always the unfiltered derivation counters).
            (None | Some("repo"), None) => "Summary:".to_string(),
            (None | Some("repo"), Some(module)) => {
                format!("Summary (whole repository, not module {module}):")
            }
            (Some(other), _) => {
                format!("Summary (diagnostics scope \"{other}\" — not a scope this build reads):")
            }
        }
    }

    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // -- Header --
        out.push_str("Module Dependencies\n\n");

        // -- Query context --
        let direction_label = match self.direction.as_str() {
            "outbound" => "outbound only",
            "inbound" => "inbound only",
            _ => "all directions",
        };
        out.push_str(&format!("Queried: {}\n", direction_label));

        if let Some(ref module) = self.module {
            out.push_str(&format!("Module: {}\n", module));
        }

        // -- Summary from diagnostics --
        if let Some(ref diag) = self.diagnostics {
            out.push_str(&format!("\n{}\n", self.summary_header(direction_label)));
            out.push_str(&format!(
                "  {} cross-module dependencies\n",
                diag.cross_module_edges
            ));
            out.push_str(&format!(
                "  {} intra-module imports\n",
                diag.intra_module_edges
            ));
            out.push_str(&format!(
                "  {} imports from unowned sources\n",
                diag.from_unowned_edges
            ));
        }

        // -- Dependency edges --
        if self.results.is_empty() {
            out.push_str("\nNo cross-module dependencies exist.\n");
            out.push_str(&self.partition_lines());
            out.push_str("\nhint: if this is unexpected, module boundaries may need refinement.\n");
            out.push_str("      Run 'rmap modules list' to see module coverage.\n");
            return out;
        }

        out.push_str(&format!(
            "\n{}\n\n",
            format_count(self.results.len(), "dependency edge", "dependency edges")
        ));

        // Sort edges deterministically: (source, target)
        let mut edges = self.results.clone();
        edges.sort_by(|a, b| {
            (&a.source_module, &a.target_module).cmp(&(&b.source_module, &b.target_module))
        });

        // Full output, no truncation
        for edge in &edges {
            out.push_str(&format!(
                "  {} -> {}  ({} imports from {} files)\n",
                edge.source_module, edge.target_module, edge.import_count, edge.source_file_count
            ));
        }
        out.push_str(&self.partition_lines());

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_deps_response() -> ModulesDepsResponse {
        ModulesDepsResponse {
            command: "modules deps".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            direction: "all".to_string(),
            module: None,
            results: vec![
                ModuleDependencyEdge {
                    source_module: "packages/cli".to_string(),
                    target_module: "packages/core".to_string(),
                    import_count: 5,
                    source_file_count: 2,
                },
                ModuleDependencyEdge {
                    source_module: "packages/api".to_string(),
                    target_module: "packages/core".to_string(),
                    import_count: 10,
                    source_file_count: 3,
                },
            ],
            count: 2,
            diagnostics: Some(ImportDiagnostics {
                total_import_edges: 100,
                intra_module_edges: 80,
                cross_module_edges: 15,
                from_unowned_edges: 5,
            }),
            diagnostics_scope: None,
            import_view: Some(
                serde_json::json!({"include_tests": false, "include_inferred": false}),
            ),
            // A current daemon's complete payload, nothing excluded (D-TESB-17 fixture rule).
            import_remainder: Some(serde_json::json!({
                "tests": {"imports": 0, "edges": 0},
                "inferred": {"imports": 0, "edges": 0},
                "tests_and_inferred": {"imports": 0, "edges": 0}
            })),
            importer_test_status_undetermined: Some(serde_json::json!({
                "count": 0, "paths": [], "universe": "cross_module_importers",
                "universe_count": 0, "unknown_count": 0
            })),
        }
    }

    fn sample_empty_deps_response() -> ModulesDepsResponse {
        ModulesDepsResponse {
            command: "modules deps".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            direction: "all".to_string(),
            module: None,
            results: vec![],
            count: 0,
            diagnostics: Some(ImportDiagnostics {
                total_import_edges: 3918,
                intra_module_edges: 3775,
                cross_module_edges: 0,
                from_unowned_edges: 143,
            }),
            diagnostics_scope: None,
            import_view: Some(
                serde_json::json!({"include_tests": false, "include_inferred": false}),
            ),
            // A current daemon's complete payload, nothing excluded (D-TESB-17 fixture rule).
            import_remainder: Some(serde_json::json!({
                "tests": {"imports": 0, "edges": 0},
                "inferred": {"imports": 0, "edges": 0},
                "tests_and_inferred": {"imports": 0, "edges": 0}
            })),
            importer_test_status_undetermined: Some(serde_json::json!({
                "count": 0, "paths": [], "universe": "cross_module_importers",
                "universe_count": 0, "unknown_count": 0
            })),
        }
    }

    #[test]
    fn deps_render_shows_header() {
        let resp = sample_deps_response();
        let output = resp.render_human();
        assert!(output.contains("Module Dependencies"));
    }

    #[test]
    fn deps_render_shows_direction() {
        let resp = sample_deps_response();
        let output = resp.render_human();
        assert!(output.contains("Queried: all directions"));
    }

    #[test]
    fn deps_render_shows_summary() {
        let resp = sample_deps_response();
        let output = resp.render_human();
        assert!(output.contains("15 cross-module dependencies"));
        assert!(output.contains("80 intra-module imports"));
        assert!(output.contains("5 imports from unowned sources"));
    }

    #[test]
    fn deps_render_shows_edges() {
        let resp = sample_deps_response();
        let output = resp.render_human();
        assert!(output.contains("packages/cli -> packages/core"));
        assert!(output.contains("packages/api -> packages/core"));
    }

    #[test]
    fn deps_render_shows_edge_counts() {
        let resp = sample_deps_response();
        let output = resp.render_human();
        assert!(output.contains("5 imports from 2 files"));
        assert!(output.contains("10 imports from 3 files"));
    }

    #[test]
    fn deps_render_empty_shows_hint() {
        let resp = sample_empty_deps_response();
        let output = resp.render_human();
        assert!(output.contains("No cross-module dependencies exist"));
        assert!(output.contains("hint:"));
    }

    #[test]
    fn deps_render_is_deterministic() {
        let resp = sample_deps_response();
        let output = resp.render_human();
        // api comes before cli alphabetically
        let api_pos = output.find("packages/api -> packages/core").unwrap();
        let cli_pos = output.find("packages/cli -> packages/core").unwrap();
        assert!(
            api_pos < cli_pos,
            "Edges should be sorted by (source, target)"
        );
    }

    /// TEST-EDGE-SCOPE-1B (D-TESB-07): the remainder and importer lines follow the edges.
    #[test]
    fn modules_deps_renders_the_remainder() {
        let mut r = sample_deps_response();
        r.import_remainder = Some(serde_json::json!({
            "tests": {"imports": 415, "edges": 1},
            "inferred": {"imports": 0, "edges": 0},
            "tests_and_inferred": {"imports": 0, "edges": 0},
        }));
        r.importer_test_status_undetermined = Some(serde_json::json!({
            "count": 1, "paths": ["util/testutil.cc"], "universe": "cross_module_importers",
            "universe_count": 3, "unknown_count": 0,
        }));
        let out = r.render_human();
        assert!(out.contains(
            "+415 imports from test files, not shown (1 cross-module dependency only through them) — --include-tests\n"
        ), "{out}");
        assert!(out.contains(
            "1 file whose test status can't be determined — open it and look inside (of 3 files importing across modules)"
        ), "{out}");
        // A payload from a daemon that predates the partition says so.
        let mut old = sample_deps_response();
        old.import_view = None;
        assert!(old
            .render_human()
            .contains("import partition unavailable from this daemon"));
    }

    // ── Summary scope (MODULES-DEPS-SUMMARY-SCOPE-1, SLICE_DOC 2.1 item 3) ──

    /// The Summary block of a render: the header line and the three figure lines.
    fn summary_block(out: &str) -> Vec<&str> {
        let lines: Vec<&str> = out.lines().collect();
        let i = lines
            .iter()
            .position(|l| *l == "Summary:" || l.starts_with("Summary ("))
            .unwrap_or_else(|| panic!("no Summary block: {out}"));
        lines[i..i + 4].to_vec()
    }

    #[test]
    fn deps_render_module_summary_states_the_module_and_direction_with_the_modules_figures() {
        for (direction, label) in [
            ("all", "all directions"),
            ("outbound", "outbound only"),
            ("inbound", "inbound only"),
        ] {
            let mut r = sample_deps_response();
            r.direction = direction.to_string();
            r.module = Some("packages/core".to_string());
            r.diagnostics_scope = Some("module".to_string());
            let out = r.render_human();
            assert!(out.contains(&format!("Queried: {label}\n")), "{out}");
            assert!(out.contains("Module: packages/core\n"), "{out}");
            assert_eq!(
                summary_block(&out),
                vec![
                    format!("Summary (module packages/core, {label}):").as_str(),
                    "  15 cross-module dependencies",
                    "  80 intra-module imports",
                    "  5 imports from unowned sources",
                ],
                "{out}"
            );
        }
    }

    #[test]
    fn deps_render_unfiltered_without_a_scope_key_renders_as_today() {
        // The render at HEAD 4e7bd983 for this fixture, written by hand.
        let today = "Module Dependencies\n\
                     \n\
                     Queried: all directions\n\
                     \n\
                     Summary:\n  \
                     15 cross-module dependencies\n  \
                     80 intra-module imports\n  \
                     5 imports from unowned sources\n\
                     \n\
                     2 dependency edges\n\
                     \n  \
                     packages/api -> packages/core  (10 imports from 3 files)\n  \
                     packages/cli -> packages/core  (5 imports from 2 files)\n";
        let absent = sample_deps_response();
        assert_eq!(absent.render_human(), today);
        let mut repo = sample_deps_response();
        repo.diagnostics_scope = Some("repo".to_string());
        assert_eq!(repo.render_human(), today);
    }

    #[test]
    fn deps_render_repo_wide_figures_beside_a_module_name_the_whole_repository() {
        // An older daemon: a module-filtered envelope with no `diagnostics_scope` key, whose
        // figures are the repository's (it copied the unfiltered derivation counters).
        let older: ModulesDepsResponse = serde_json::from_value(serde_json::json!({
            "command": "modules deps", "repo": "r", "snapshot": "s", "direction": "all",
            "module": "table",
            "diagnostics": {"imports_total": 343, "imports_cross_module": 185,
                            "imports_intra_module": 140, "imports_source_unowned": 18,
                            "imports_target_unowned": 0},
            "results": [], "count": 0,
        }))
        .expect("decodes");
        assert_eq!(older.diagnostics_scope, None);
        let mut repo = sample_deps_response();
        repo.module = Some("table".to_string());
        repo.diagnostics_scope = Some("repo".to_string());
        for (r, figures) in [(&older, ["185", "140", "18"]), (&repo, ["15", "80", "5"])] {
            let out = r.render_human();
            assert_eq!(
                summary_block(&out),
                vec![
                    "Summary (whole repository, not module table):".to_string(),
                    format!("  {} cross-module dependencies", figures[0]),
                    format!("  {} intra-module imports", figures[1]),
                    format!("  {} imports from unowned sources", figures[2]),
                ],
                "{out}"
            );
            assert!(!out.contains("\nSummary:\n"), "{out}");
            assert!(!out.contains("Summary (module"), "{out}");
        }
    }

    #[test]
    fn deps_render_module_scope_without_a_module_name_states_that_reason() {
        let mut r = sample_deps_response();
        r.diagnostics_scope = Some("module".to_string());
        let out = r.render_human();
        assert_eq!(
            summary_block(&out),
            vec![
                "Summary (diagnostics scope \"module\" but the answer names no module):",
                "  15 cross-module dependencies",
                "  80 intra-module imports",
                "  5 imports from unowned sources",
            ],
            "{out}"
        );
    }

    #[test]
    fn deps_render_unknown_scope_prints_the_stored_value_marked() {
        for module in [None, Some("table".to_string())] {
            let mut r = sample_deps_response();
            r.module = module;
            r.diagnostics_scope = Some("package".to_string());
            let out = r.render_human();
            assert_eq!(
                summary_block(&out)[0],
                "Summary (diagnostics scope \"package\" — not a scope this build reads):",
                "{out}"
            );
        }
    }
}
