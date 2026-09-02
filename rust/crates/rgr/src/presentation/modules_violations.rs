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
    /// GOV-ARMED-1: whether any module boundary declaration is configured for
    /// this repo. `Some(false)` → unarmed one-liner; `Some(true)` → the
    /// violation render; `None` → daemon did not report it → unknown-with-reason.
    /// Configuration-presence fact, never inferred from `count == 0`.
    #[serde(default)]
    pub armed: Option<bool>,
    /// GOV-ARMED-1: number of active boundary declarations checked, for the
    /// explicit armed-and-clean render.
    #[serde(default)]
    pub declarations_checked: Option<u64>,
}

impl ModulesViolationsResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // -- Header --
        out.push_str("Module Violations\n\n");

        // GOV-ARMED-1: unarmed / unknown short-circuits before the zero counts
        // and import analysis.
        match self.armed {
            Some(false) => {
                out.push_str(
                    "Violations: not armed — no module boundary declarations exist for \
                     this repo; nothing has been checked.\n",
                );
                out.push_str(
                    "To arm: declare a boundary \
                     (`rmap declare boundary <module_path> --forbids <target>`).\n",
                );
                return out;
            }
            None => {
                out.push_str(&super::armed_unknown_line("Violations"));
            }
            Some(true) => {}
        }

        // -- Counts --
        // COHERENCE-POLISH-1 §3(b): this surface reports discovered-module (module-graph-derived)
        // violations ONLY (same fact class the top-level `rmap violations` renders as its
        // "discovered module violation(s)" section). Share that one noun phrase across the two
        // surfaces so an agent reads one vocabulary, not "0 violations" here vs "0 discovered module
        // violations" there for the same fact.
        out.push_str(&format!(
            "{}\n",
            format_count(
                self.count as usize,
                "discovered module violation",
                "discovered module violations"
            )
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
        // GOV-ARMED-1: armed and clean — declarations exist, nothing violated.
        // Name the declarations checked so it never reads as unarmed.
        if self.results.violations.is_empty() && self.results.stale_declarations.is_empty() {
            match self.declarations_checked {
                Some(n) => out.push_str(&format!(
                    "\n{} boundary {} checked — no violations.\n",
                    n,
                    if n == 1 {
                        "declaration"
                    } else {
                        "declarations"
                    }
                )),
                None => out.push_str("\nNo boundary violations detected.\n"),
            }
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
            armed: Some(true),
            declarations_checked: Some(3),
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
            // Armed and clean: declarations exist, none violated.
            armed: Some(true),
            declarations_checked: Some(5),
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
        assert!(output.contains("2 discovered module violations"));
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

    // GOV-ARMED-1: armed and clean names the declarations checked.
    #[test]
    fn violations_render_empty_shows_message() {
        let resp = sample_empty_violations_response();
        let output = resp.render_human();
        assert!(output.contains("5 boundary declarations checked — no violations."));
        assert!(!output.contains("not armed"));
    }

    // GOV-ARMED-1: no module boundary declarations → one honest line + CTA, no
    // zero counts / import analysis dump.
    #[test]
    fn violations_render_unarmed() {
        let mut resp = sample_empty_violations_response();
        resp.armed = Some(false);
        resp.declarations_checked = Some(0);
        let output = resp.render_human();
        assert!(output.contains(
            "Violations: not armed — no module boundary declarations exist for this repo; \
             nothing has been checked."
        ));
        assert!(output.contains("rmap declare boundary"));
        assert!(!output.contains("Import analysis:"));
        assert!(!output.contains("0 discovered module violations"));
    }

    // GOV-ARMED-1: determination fact absent → unknown-with-reason.
    #[test]
    fn violations_render_armed_unknown() {
        let mut resp = sample_empty_violations_response();
        resp.armed = None;
        resp.declarations_checked = None;
        let output = resp.render_human();
        assert!(output.contains("Violations: armed state unknown"));
        assert!(!output.contains("not armed"));
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
