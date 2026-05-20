//! Presentation layer for module violations command.
//!
//! # CLI-OUT-4 Group 3
//!
//! Response DTO and human renderer for:
//! - `modules violations` — boundary violation diagnostics
//!
//! ## Change Axis
//!
//! This file changes when:
//! - Stale declaration presentation changes
//! - Violation reasoning changes
//! - Policy/boundary breach framing changes
//!
//! It does NOT change when:
//! - `modules deps` output changes (different contract)
//! - Module catalog/inventory output changes (Groups 1-2)

use serde::Deserialize;

use super::module_shared::format_count;

// =============================================================================
// IMPORT DIAGNOSTICS (local copy - small struct, rendering differs by context)
// =============================================================================

/// Import analysis diagnostics from violations response.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportDiagnostics {
    #[serde(default)]
    pub total_import_edges: u64,
    #[serde(default)]
    pub intra_module_edges: u64,
    #[serde(default)]
    pub cross_module_edges: u64,
    #[serde(default)]
    pub from_unowned_edges: u64,
}

// =============================================================================
// MODULES VIOLATIONS RESPONSE
// =============================================================================

/// A boundary violation entry.
#[derive(Debug, Clone, Deserialize)]
pub struct ViolationEntry {
    #[serde(default)]
    pub declaration_uid: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub import_count: u64,
    #[serde(default)]
    pub source_file_count: u64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// A stale declaration entry.
#[derive(Debug, Clone, Deserialize)]
pub struct StaleDeclaration {
    #[serde(default)]
    pub declaration_uid: String,
    #[serde(default)]
    pub stale_side: String,
    #[serde(default)]
    pub missing_paths: Vec<String>,
}

/// Nested results structure for violations.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ViolationsResults {
    #[serde(default)]
    pub violations: Vec<ViolationEntry>,
    #[serde(default)]
    pub stale_declarations: Vec<StaleDeclaration>,
}

/// Response structure for modules violations command.
#[derive(Debug, Deserialize)]
pub struct ModulesViolationsResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub results: ViolationsResults,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub stale_count: u64,
    #[serde(default)]
    pub diagnostics: Option<ImportDiagnostics>,
}

impl ModulesViolationsResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // -- Header --
        out.push_str("Module Violations\n\n");

        // -- Counts --
        out.push_str(&format!(
            "{}\n",
            format_count(self.count as usize, "violation", "violations")
        ));
        out.push_str(&format!(
            "{}\n",
            format_count(
                self.stale_count as usize,
                "stale declaration",
                "stale declarations"
            )
        ));

        // -- Import analysis from diagnostics --
        if let Some(ref diag) = self.diagnostics {
            let total = diag.total_import_edges;
            if total > 0 {
                let intra_pct = (diag.intra_module_edges as f64 / total as f64 * 100.0) as u64;
                let unowned_pct = (diag.from_unowned_edges as f64 / total as f64 * 100.0) as u64;

                out.push_str("\nImport analysis:\n");
                out.push_str(&format!("  {} total import edges\n", total));
                out.push_str(&format!(
                    "  {} intra-module ({}%)\n",
                    diag.intra_module_edges, intra_pct
                ));
                out.push_str(&format!("  {} cross-module\n", diag.cross_module_edges));
                out.push_str(&format!(
                    "  {} from unowned sources ({}%)\n",
                    diag.from_unowned_edges, unowned_pct
                ));
            }
        }

        // -- Violations list --
        if self.results.violations.is_empty() && self.results.stale_declarations.is_empty() {
            out.push_str("\nNo boundary violations detected.\n");
            return out;
        }

        // Full output, deterministic ordering
        if !self.results.violations.is_empty() {
            out.push_str("\nViolations:\n");

            let mut violations = self.results.violations.clone();
            violations.sort_by(|a, b| (&a.source, &a.target).cmp(&(&b.source, &b.target)));

            for v in &violations {
                out.push_str(&format!(
                    "  {} -> {}  ({} imports from {} files)\n",
                    v.source, v.target, v.import_count, v.source_file_count
                ));
                if let Some(ref reason) = v.reason {
                    out.push_str(&format!("    reason: {}\n", reason));
                }
            }
        }

        // -- Stale declarations --
        if !self.results.stale_declarations.is_empty() {
            out.push_str("\nStale declarations:\n");

            let mut stale = self.results.stale_declarations.clone();
            stale.sort_by(|a, b| a.declaration_uid.cmp(&b.declaration_uid));

            for s in &stale {
                out.push_str(&format!(
                    "  {} (stale: {})\n",
                    s.declaration_uid, s.stale_side
                ));
                for path in &s.missing_paths {
                    out.push_str(&format!("    missing: {}\n", path));
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_violations_response() -> ModulesViolationsResponse {
        ModulesViolationsResponse {
            command: "modules violations".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: ViolationsResults {
                violations: vec![
                    ViolationEntry {
                        declaration_uid: "decl-1".to_string(),
                        source: "packages/cli".to_string(),
                        target: "packages/internal".to_string(),
                        import_count: 3,
                        source_file_count: 1,
                        reason: Some("internal module".to_string()),
                    },
                    ViolationEntry {
                        declaration_uid: "decl-2".to_string(),
                        source: "packages/api".to_string(),
                        target: "packages/internal".to_string(),
                        import_count: 7,
                        source_file_count: 2,
                        reason: None,
                    },
                ],
                stale_declarations: vec![StaleDeclaration {
                    declaration_uid: "decl-old".to_string(),
                    stale_side: "target".to_string(),
                    missing_paths: vec!["packages/legacy".to_string()],
                }],
            },
            count: 2,
            stale_count: 1,
            diagnostics: Some(ImportDiagnostics {
                total_import_edges: 100,
                intra_module_edges: 90,
                cross_module_edges: 5,
                from_unowned_edges: 5,
            }),
        }
    }

    fn sample_empty_violations_response() -> ModulesViolationsResponse {
        ModulesViolationsResponse {
            command: "modules violations".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: ViolationsResults {
                violations: vec![],
                stale_declarations: vec![],
            },
            count: 0,
            stale_count: 0,
            diagnostics: Some(ImportDiagnostics {
                total_import_edges: 3918,
                intra_module_edges: 3775,
                cross_module_edges: 0,
                from_unowned_edges: 143,
            }),
        }
    }

    #[test]
    fn violations_render_shows_header() {
        let resp = sample_violations_response();
        let output = resp.render_human();
        assert!(output.contains("Module Violations"));
    }

    #[test]
    fn violations_render_shows_counts() {
        let resp = sample_violations_response();
        let output = resp.render_human();
        assert!(output.contains("2 violations"));
        assert!(output.contains("1 stale declaration"));
    }

    #[test]
    fn violations_render_shows_import_analysis() {
        let resp = sample_violations_response();
        let output = resp.render_human();
        assert!(output.contains("Import analysis:"));
        assert!(output.contains("100 total import edges"));
        assert!(output.contains("90 intra-module"));
        assert!(output.contains("5 cross-module"));
    }

    #[test]
    fn violations_render_shows_violations() {
        let resp = sample_violations_response();
        let output = resp.render_human();
        assert!(output.contains("Violations:"));
        assert!(output.contains("packages/cli -> packages/internal"));
        assert!(output.contains("packages/api -> packages/internal"));
    }

    #[test]
    fn violations_render_shows_reason() {
        let resp = sample_violations_response();
        let output = resp.render_human();
        assert!(output.contains("reason: internal module"));
    }

    #[test]
    fn violations_render_shows_stale() {
        let resp = sample_violations_response();
        let output = resp.render_human();
        assert!(output.contains("Stale declarations:"));
        assert!(output.contains("decl-old (stale: target)"));
        assert!(output.contains("missing: packages/legacy"));
    }

    #[test]
    fn violations_render_empty_shows_message() {
        let resp = sample_empty_violations_response();
        let output = resp.render_human();
        assert!(output.contains("No boundary violations detected"));
    }

    #[test]
    fn violations_render_is_deterministic() {
        let resp = sample_violations_response();
        let output = resp.render_human();
        // api comes before cli alphabetically
        let api_pos = output.find("packages/api -> packages/internal").unwrap();
        let cli_pos = output.find("packages/cli -> packages/internal").unwrap();
        assert!(
            api_pos < cli_pos,
            "Violations should be sorted by (source, target)"
        );
    }
}
