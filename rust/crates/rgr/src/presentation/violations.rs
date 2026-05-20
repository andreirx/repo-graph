//! Presentation layer for the top-level `violations` command.
//!
//! # CLI-OUT-7 Group 2
//!
//! Transforms unified violations results into human-readable plain text.
//! Shows three sections: declared boundary, discovered module, stale declarations.
//!
//! # Scope Boundary
//!
//! This module handles the TOP-LEVEL `rmap violations` command only.
//! The `rmap modules violations` subcommand has its own renderer in
//! `presentation/modules_violations.rs` (CLI-OUT-4).
//!
//! # Output Contract
//!
//! - Three sections with counts
//! - Deterministic ordering per section
//! - Full output, no truncation
//! - `--json` preserved for machine mode

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Declared boundary violation entry.
#[derive(Debug, Deserialize, Clone)]
pub struct DeclaredViolation {
    pub boundary_module: String,
    pub forbidden_module: String,
    pub source_file: String,
    pub target_file: String,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Discovered module violation entry.
#[derive(Debug, Deserialize, Clone)]
pub struct DiscoveredViolation {
    pub declaration_uid: String,
    pub source: String,
    pub target: String,
    pub import_count: u64,
    pub source_file_count: u64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Stale declaration entry.
#[derive(Debug, Deserialize, Clone)]
pub struct StaleDeclaration {
    pub declaration_uid: String,
    pub stale_side: String,
    #[serde(default)]
    pub missing_paths: Vec<String>,
}

/// Nested results structure.
#[derive(Debug, Deserialize, Clone)]
pub struct ViolationsResults {
    #[serde(default)]
    pub declared_boundary_violations: Vec<DeclaredViolation>,
    #[serde(default)]
    pub discovered_module_violations: Vec<DiscoveredViolation>,
}

/// Response DTO for top-level `violations` command.
#[derive(Debug, Deserialize)]
pub struct ViolationsResponse {
    #[allow(dead_code)]
    pub command: String,
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    pub results: ViolationsResults,
    pub count: u64,
    pub declared_boundary_count: u64,
    pub discovered_module_count: u64,
    #[serde(default)]
    pub stale_declarations: Vec<StaleDeclaration>,
    pub stale_count: u64,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl ViolationsResponse {
    /// Render the violations response as human-readable plain text.
    ///
    /// Three sections: declared boundary, discovered module, stale declarations.
    /// Deterministic ordering per section.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Architectural Violations\n\n");

        // Counts summary
        out.push_str(&format!(
            "{}\n",
            format_count(
                self.declared_boundary_count as usize,
                "declared boundary violation",
                "declared boundary violations"
            )
        ));
        out.push_str(&format!(
            "{}\n",
            format_count(
                self.discovered_module_count as usize,
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

        // Empty case
        if self.count == 0 && self.stale_count == 0 {
            out.push_str("\nNo violations detected.\n");
            return out;
        }

        // Section 1: Declared boundary violations
        if !self.results.declared_boundary_violations.is_empty() {
            out.push_str("\nDeclared boundary violations:\n");

            let mut violations = self.results.declared_boundary_violations.clone();
            violations.sort_by(|a, b| {
                (&a.boundary_module, &a.forbidden_module, &a.source_file).cmp(&(
                    &b.boundary_module,
                    &b.forbidden_module,
                    &b.source_file,
                ))
            });

            for v in &violations {
                out.push_str(&format!(
                    "  {} -> {}\n",
                    v.boundary_module, v.forbidden_module
                ));
                let line_suffix = v.line.map(|l| format!(":{}", l)).unwrap_or_default();
                out.push_str(&format!("    source: {}{}\n", v.source_file, line_suffix));
                out.push_str(&format!("    target: {}\n", v.target_file));
                if let Some(ref reason) = v.reason {
                    out.push_str(&format!("    reason: {}\n", reason));
                }
            }
        }

        // Section 2: Discovered module violations
        if !self.results.discovered_module_violations.is_empty() {
            out.push_str("\nDiscovered module violations:\n");

            let mut violations = self.results.discovered_module_violations.clone();
            violations.sort_by(|a, b| (&a.source, &a.target).cmp(&(&b.source, &b.target)));

            for v in &violations {
                out.push_str(&format!("  {} -> {}\n", v.source, v.target));
                out.push_str(&format!(
                    "    {} imports from {} files\n",
                    v.import_count, v.source_file_count
                ));
                if let Some(ref reason) = v.reason {
                    out.push_str(&format!("    reason: {}\n", reason));
                }
            }
        }

        // Section 3: Stale declarations
        if !self.stale_declarations.is_empty() {
            out.push_str("\nStale declarations:\n");

            let mut stale = self.stale_declarations.clone();
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

/// Format a count with singular/plural grammar.
fn format_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{} {}", count, singular)
    } else {
        format!("{} {}", count, plural)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(
        declared: Vec<DeclaredViolation>,
        discovered: Vec<DiscoveredViolation>,
        stale: Vec<StaleDeclaration>,
    ) -> ViolationsResponse {
        let declared_count = declared.len() as u64;
        let discovered_count = discovered.len() as u64;
        let stale_count = stale.len() as u64;
        let count = declared_count + discovered_count;

        ViolationsResponse {
            command: "arch violations".to_string(),
            repo: "test-repo".to_string(),
            snapshot: "snap_abc123".to_string(),
            results: ViolationsResults {
                declared_boundary_violations: declared,
                discovered_module_violations: discovered,
            },
            count,
            declared_boundary_count: declared_count,
            discovered_module_count: discovered_count,
            stale_declarations: stale,
            stale_count,
        }
    }

    fn make_declared(
        boundary: &str,
        forbidden: &str,
        source: &str,
        target: &str,
        line: Option<u64>,
        reason: Option<&str>,
    ) -> DeclaredViolation {
        DeclaredViolation {
            boundary_module: boundary.to_string(),
            forbidden_module: forbidden.to_string(),
            source_file: source.to_string(),
            target_file: target.to_string(),
            line,
            reason: reason.map(|s| s.to_string()),
        }
    }

    fn make_discovered(
        source: &str,
        target: &str,
        import_count: u64,
        source_file_count: u64,
        reason: Option<&str>,
    ) -> DiscoveredViolation {
        DiscoveredViolation {
            declaration_uid: format!("decl-{}-{}", source, target),
            source: source.to_string(),
            target: target.to_string(),
            import_count,
            source_file_count,
            reason: reason.map(|s| s.to_string()),
        }
    }

    fn make_stale(uid: &str, side: &str, missing: Vec<&str>) -> StaleDeclaration {
        StaleDeclaration {
            declaration_uid: uid.to_string(),
            stale_side: side.to_string(),
            missing_paths: missing.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn render_empty_violations() {
        let resp = make_response(vec![], vec![], vec![]);
        let out = resp.render_human();

        assert!(out.contains("Architectural Violations"));
        assert!(out.contains("0 declared boundary violations"));
        assert!(out.contains("0 discovered module violations"));
        assert!(out.contains("0 stale declarations"));
        assert!(out.contains("No violations detected"));
    }

    #[test]
    fn render_declared_only() {
        let resp = make_response(
            vec![make_declared(
                "src/adapters",
                "src/core",
                "src/adapters/store.ts",
                "src/core/service.ts",
                Some(1),
                Some("adapters must not depend on core"),
            )],
            vec![],
            vec![],
        );
        let out = resp.render_human();

        assert!(out.contains("1 declared boundary violation"));
        assert!(out.contains("src/adapters -> src/core"));
        assert!(out.contains("source: src/adapters/store.ts:1"));
        assert!(out.contains("target: src/core/service.ts"));
        assert!(out.contains("reason: adapters must not depend on core"));
        assert!(!out.contains("No violations detected"));
    }

    #[test]
    fn render_discovered_only() {
        let resp = make_response(
            vec![],
            vec![make_discovered(
                "packages/cli",
                "packages/internal",
                3,
                1,
                Some("internal module"),
            )],
            vec![],
        );
        let out = resp.render_human();

        assert!(out.contains("1 discovered module violation"));
        assert!(out.contains("packages/cli -> packages/internal"));
        assert!(out.contains("3 imports from 1 files"));
        assert!(out.contains("reason: internal module"));
    }

    #[test]
    fn render_stale_only() {
        let resp = make_response(
            vec![],
            vec![],
            vec![make_stale("decl-old", "target", vec!["packages/legacy"])],
        );
        let out = resp.render_human();

        assert!(out.contains("1 stale declaration"));
        assert!(out.contains("decl-old (stale: target)"));
        assert!(out.contains("missing: packages/legacy"));
        // Stale alone does not trigger "No violations detected"
        assert!(!out.contains("No violations detected"));
    }

    #[test]
    fn render_all_sections() {
        let resp = make_response(
            vec![make_declared(
                "src/adapters",
                "src/core",
                "src/adapters/store.ts",
                "src/core/service.ts",
                None,
                None,
            )],
            vec![make_discovered("pkg/a", "pkg/b", 5, 2, None)],
            vec![make_stale("decl-x", "both", vec!["old/path"])],
        );
        let out = resp.render_human();

        assert!(out.contains("1 declared boundary violation"));
        assert!(out.contains("1 discovered module violation"));
        assert!(out.contains("1 stale declaration"));
        assert!(out.contains("Declared boundary violations:"));
        assert!(out.contains("Discovered module violations:"));
        assert!(out.contains("Stale declarations:"));
    }

    #[test]
    fn render_deterministic_ordering_declared() {
        let resp = make_response(
            vec![
                make_declared("z/mod", "a/mod", "z.ts", "a.ts", None, None),
                make_declared("a/mod", "z/mod", "a.ts", "z.ts", None, None),
            ],
            vec![],
            vec![],
        );
        let out = resp.render_human();

        let a_pos = out.find("a/mod -> z/mod").unwrap();
        let z_pos = out.find("z/mod -> a/mod").unwrap();
        assert!(a_pos < z_pos, "a/mod should come before z/mod");
    }

    #[test]
    fn render_deterministic_ordering_discovered() {
        let resp = make_response(
            vec![],
            vec![
                make_discovered("z/pkg", "a/pkg", 1, 1, None),
                make_discovered("a/pkg", "z/pkg", 1, 1, None),
            ],
            vec![],
        );
        let out = resp.render_human();

        let a_pos = out.find("a/pkg -> z/pkg").unwrap();
        let z_pos = out.find("z/pkg -> a/pkg").unwrap();
        assert!(a_pos < z_pos, "a/pkg should come before z/pkg");
    }

    #[test]
    fn render_singular_grammar() {
        let resp = make_response(
            vec![make_declared("a", "b", "a.ts", "b.ts", None, None)],
            vec![],
            vec![],
        );
        let out = resp.render_human();

        // Check singular form is used for the one violation
        assert!(out.contains("1 declared boundary violation\n"));
        // Verify it's not "1 declared boundary violations" (plural)
        assert!(!out.contains("1 declared boundary violations"));
    }

    #[test]
    fn render_plural_grammar() {
        let resp = make_response(
            vec![
                make_declared("a", "b", "a.ts", "b.ts", None, None),
                make_declared("c", "d", "c.ts", "d.ts", None, None),
            ],
            vec![],
            vec![],
        );
        let out = resp.render_human();

        assert!(out.contains("2 declared boundary violations"));
    }

    #[test]
    fn render_no_line_number() {
        let resp = make_response(
            vec![make_declared("a", "b", "a.ts", "b.ts", None, None)],
            vec![],
            vec![],
        );
        let out = resp.render_human();

        // Should not have ":None" or ":" with no number
        assert!(out.contains("source: a.ts\n"));
        assert!(!out.contains("a.ts:"));
    }
}
