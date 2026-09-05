//! Presentation layer for `modules show` command.
//!
//! # CLI-OUT-4 Group 1
//!
//! Response DTOs and human renderer for single module detail view.
//!
//! ## Change Axis
//!
//! This file changes when:
//! - `modules show` output format changes
//! - `modules show` daemon response structure changes
//!
//! It does NOT change when:
//! - `modules list` changes
//! - Shared formatting changes (see `module_shared.rs`)

use serde::Deserialize;

use super::module_shared::{format_count, format_kind_confidence};

/// Module identity in show response.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleIdentity {
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
    #[serde(default)]
    pub confidence: f64,
}

/// Rollup statistics in show response.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleRollups {
    #[serde(default)]
    pub owned_file_count: usize,
    #[serde(default)]
    pub owned_test_file_count: usize,
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
    /// RECON-M-R3a (g2u-a): the daemon's REDUCTION-ONLY unref overlay
    /// (`{fewer_flagged, accounting, coverage, basis}`) — flagged symbols the compiler
    /// witnessed incoming references for (known false positives of the syntax-only
    /// estimate). Absent unless measured and nonzero (R-0).
    #[serde(default)]
    pub unref_reduction: Option<serde_json::Value>,
}

/// Module dependency edge (from daemon inbound/outbound_dependencies).
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleDependency {
    #[serde(default)]
    pub module_uid: String,
    #[serde(default)]
    pub module_key: String,
    #[serde(default)]
    pub canonical_root_path: String,
    #[serde(default)]
    pub module_kind: String,
    #[serde(default)]
    pub import_count: usize,
    #[serde(default)]
    pub source_file_count: usize,
}

/// Module violation entry.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleViolation {
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub target_file: String,
    #[serde(default)]
    pub violation_kind: String,
}

/// Evidence for module inference.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleEvidence {
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub evidence_kind: String,
    #[serde(default)]
    pub evidence_strength: String,
    #[serde(default)]
    pub dominant_language: String,
}

/// Response structure for modules show command.
#[derive(Debug, Deserialize)]
pub struct ModulesShowResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    pub module: ModuleIdentity,
    pub rollups: ModuleRollups,
    #[serde(default)]
    pub outbound_dependencies: Vec<ModuleDependency>,
    #[serde(default)]
    pub inbound_dependencies: Vec<ModuleDependency>,
    #[serde(default)]
    pub violations: Vec<ModuleViolation>,
    #[serde(default)]
    pub rollups_degraded: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<ModuleEvidence>,
    // trust field omitted - complex nested structure, not needed for basic rendering
}

impl ModulesShowResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        out.push_str(&format!("Module: {}\n\n", self.module.display_name));

        // ── Identity ───────────────────────────────────────────────
        out.push_str(&format!(
            "Kind: {}\n",
            format_kind_confidence(&self.module.module_kind, self.module.confidence)
        ));
        out.push_str(&format!("Root: {}/\n", self.module.canonical_root_path));

        // ── Ownership ──────────────────────────────────────────────
        // COHERENCE-2 §2.4: `owned_file_count` (non-test) and `owned_test_file_count` (test) are
        // disjoint; the headline is the TOTAL so `(N test files)` reads as a SUBSET of it, the same
        // meaning `modules list` and `stats` use — never an addend the reader must sum.
        out.push_str("\nOwnership:\n");
        let total_files = self.rollups.owned_file_count + self.rollups.owned_test_file_count;
        if self.rollups.owned_test_file_count > 0 {
            out.push_str(&format!(
                "  {} ({} test files)\n",
                format_count(total_files, "file", "files"),
                self.rollups.owned_test_file_count
            ));
        } else {
            out.push_str(&format!(
                "  {}\n",
                format_count(total_files, "file", "files")
            ));
        }

        // ── Relationships ──────────────────────────────────────────
        out.push_str("\nRelationships:\n");
        out.push_str(&format!(
            "  {}\n",
            format_count(
                self.rollups.outbound_dependency_count,
                "outbound dependency",
                "outbound dependencies"
            )
        ));
        out.push_str(&format!(
            "  {}\n",
            format_count(
                self.rollups.inbound_dependency_count,
                "inbound dependency",
                "inbound dependencies"
            )
        ));
        out.push_str(&format!(
            "  {}\n",
            format_count(self.rollups.violation_count, "violation", "violations")
        ));

        // ── Outbound dependencies (if any) ─────────────────────────
        if !self.outbound_dependencies.is_empty() {
            out.push_str("\nOutbound dependencies:\n");
            for dep in &self.outbound_dependencies {
                let name = if dep.canonical_root_path.is_empty() {
                    &dep.module_key
                } else {
                    &dep.canonical_root_path
                };
                out.push_str(&format!(
                    "  {} ({} imports from {} files)\n",
                    name, dep.import_count, dep.source_file_count
                ));
            }
        }

        // ── Inbound dependencies (if any) ──────────────────────────
        if !self.inbound_dependencies.is_empty() {
            out.push_str("\nInbound dependencies:\n");
            for dep in &self.inbound_dependencies {
                let name = if dep.canonical_root_path.is_empty() {
                    &dep.module_key
                } else {
                    &dep.canonical_root_path
                };
                out.push_str(&format!(
                    "  {} ({} imports from {} files)\n",
                    name, dep.import_count, dep.source_file_count
                ));
            }
        }

        // ── Violations (if any) ────────────────────────────────────
        if !self.violations.is_empty() {
            out.push_str("\nViolations:\n");
            for v in &self.violations {
                out.push_str(&format!(
                    "  {} -> {}  ({})\n",
                    v.source_file, v.target_file, v.violation_kind
                ));
            }
        }

        // ── Symbols ────────────────────────────────────────────────
        // OUTPUT-DOC-TRUTH-AUDIT-1: dead_symbol_count is a SYNTACTIC graph-orphan
        // estimate (no inbound reference in the modeled graph), a low-reliability
        // Layer-2 inference — NOT a Layer-0 "safe to delete" fact. Rendered as
        // "unreferenced" + a caveat so an agent never reads it as deletion-safe.
        // (`modules list` shows the same count compactly as `unref?`.)
        out.push_str("\nSymbols:\n");
        if self.rollups.dead_test_symbol_count > 0 {
            out.push_str(&format!(
                "  {} ({} in tests)\n",
                format_count(
                    self.rollups.dead_symbol_count,
                    "unreferenced symbol",
                    "unreferenced symbols"
                ),
                self.rollups.dead_test_symbol_count
            ));
        } else {
            out.push_str(&format!(
                "  {}\n",
                format_count(
                    self.rollups.dead_symbol_count,
                    "unreferenced symbol",
                    "unreferenced symbols"
                )
            ));
        }
        // RECON-M-R3a (g2u-a, §5.3.3a): the reduction-only compiler-witness line — rendered
        // ONLY when the daemon attached a nonzero reduction (W-BOTH with a current measured
        // ledger) AND the block passes the §5.3.0 labeling gate (review-2 item 1:
        // `accounting: "union"` + coverage basis, via the ONE shared gate) — the coverage
        // renders beside the reconciled value. It shrinks the false-positive claim; the
        // pipeline count above stays untouched.
        if let Some((n, coverage)) = self.rollups.unref_reduction.as_ref().and_then(|b| {
            let coverage = crate::presentation::witnesses::union_coverage_phrase(b)?;
            // Reduction-only truth: a zero/absent/malformed reduction renders NOTHING (the
            // daemon never attaches zero; defensive parity with the modules_list aggregate —
            // a zero line would be noise, a coerced zero an invented claim).
            let n = b
                .get("fewer_flagged")
                .and_then(|v| v.as_u64())
                .filter(|n| *n > 0)?;
            Some((n, coverage))
        }) {
            out.push_str(&format!(
                "  {n} fewer flagged: compiler-verified references found \
                 (reconciled — combined analyses; coverage: {coverage})\n"
            ));
        }
        out.push_str(
            "  note: unreferenced = no inbound reference in the indexed graph \
             (syntactic estimate); over-counts under low call-graph resolution; \
             run `rmap trust` for reliability.\n",
        );

        // ── Evidence ───────────────────────────────────────────────
        if !self.evidence.is_empty() {
            out.push_str("\nEvidence:\n");
            for e in &self.evidence {
                out.push_str(&format!(
                    "  {}  {}  {}\n",
                    e.source_type, e.source_path, e.evidence_strength
                ));
            }
        }

        // ── Warnings ───────────────────────────────────────────────
        if !self.warnings.is_empty() {
            out.push_str("\nWarnings:\n");
            for w in &self.warnings {
                out.push_str(&format!("  {}\n", w));
            }
        }

        // ── Hints for isolated modules ─────────────────────────────
        if self.rollups.outbound_dependency_count == 0
            && self.rollups.inbound_dependency_count == 0
            && self.rollups.violation_count == 0
        {
            out.push_str("\nNo dependencies detected. This module appears isolated.\n");
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_show_response() -> ModulesShowResponse {
        ModulesShowResponse {
            command: "modules show".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            module: ModuleIdentity {
                module_uid: "mod-1".to_string(),
                module_key: "inferred:repo_123:src".to_string(),
                canonical_root_path: "src".to_string(),
                module_kind: "inferred".to_string(),
                display_name: "src".to_string(),
                confidence: 0.7,
            },
            rollups: ModuleRollups {
                owned_file_count: 100,
                owned_test_file_count: 10,
                outbound_dependency_count: 2,
                outbound_import_count: 50,
                inbound_dependency_count: 1,
                inbound_import_count: 20,
                violation_count: 1,
                dead_symbol_count: 25,
                dead_test_symbol_count: 5,
                unref_reduction: None,
            },
            outbound_dependencies: vec![ModuleDependency {
                module_uid: "mod-lib".to_string(),
                module_key: "inferred:repo:lib".to_string(),
                canonical_root_path: "lib".to_string(),
                module_kind: "inferred".to_string(),
                import_count: 15,
                source_file_count: 3,
            }],
            inbound_dependencies: vec![ModuleDependency {
                module_uid: "mod-tests".to_string(),
                module_key: "inferred:repo:tests".to_string(),
                canonical_root_path: "tests".to_string(),
                module_kind: "inferred".to_string(),
                import_count: 8,
                source_file_count: 2,
            }],
            violations: vec![ModuleViolation {
                source_file: "src/foo.ts".to_string(),
                target_file: "lib/bar.ts".to_string(),
                violation_kind: "forbidden".to_string(),
            }],
            rollups_degraded: false,
            warnings: vec![],
            evidence: vec![ModuleEvidence {
                source_type: "directory_heuristic".to_string(),
                source_path: "src".to_string(),
                evidence_kind: "directory_structure".to_string(),
                evidence_strength: "basic".to_string(),
                dominant_language: "typescript".to_string(),
            }],
        }
    }

    fn sample_isolated_show_response() -> ModulesShowResponse {
        ModulesShowResponse {
            command: "modules show".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            module: ModuleIdentity {
                module_uid: "mod-1".to_string(),
                module_key: "inferred:repo_123:src".to_string(),
                canonical_root_path: "src".to_string(),
                module_kind: "inferred".to_string(),
                display_name: "src".to_string(),
                confidence: 0.7,
            },
            rollups: ModuleRollups {
                owned_file_count: 100,
                owned_test_file_count: 0,
                outbound_dependency_count: 0,
                outbound_import_count: 0,
                inbound_dependency_count: 0,
                inbound_import_count: 0,
                violation_count: 0,
                dead_symbol_count: 10,
                dead_test_symbol_count: 0,
                unref_reduction: None,
            },
            outbound_dependencies: vec![],
            inbound_dependencies: vec![],
            violations: vec![],
            rollups_degraded: false,
            warnings: vec![],
            evidence: vec![],
        }
    }

    #[test]
    fn show_render_shows_header() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Module: src"));
    }

    /// Review-1 item 3: the NONZERO g2u reduction through final human rendering — the
    /// labeled line renders beside (never instead of) the untouched pipeline count.
    #[test]
    fn show_render_nonzero_unref_reduction_renders_beside_pipeline_count() {
        let mut resp = sample_show_response();
        resp.rollups.unref_reduction = Some(serde_json::json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "fewer_flagged": 3,
            "basis": "compiler-verified references found",
        }));
        let output = resp.render_human();
        // Review-2 item 1: the coverage basis renders beside the reconciled value.
        assert!(
            output.contains(
                "3 fewer flagged: compiler-verified references found \
                 (reconciled — combined analyses; coverage: TypeScript (1 partition))"
            ),
            "{output}"
        );
        assert!(
            output.contains("25 unreferenced symbols"),
            "the pipeline count stays untouched: {output}"
        );
    }

    /// A zero or malformed reduction renders NOTHING — the daemon never attaches zero, and a
    /// coerced zero would be an invented claim (review-1 item 5's rule at this surface).
    /// Review-2 item 1 extends the malformed class: a NONZERO reduction missing its
    /// `accounting: "union"` marker or its coverage basis is suppressed too — the union
    /// value never renders unlabeled.
    #[test]
    fn show_render_zero_or_malformed_reduction_renders_nothing() {
        let mut resp = sample_show_response();
        resp.rollups.unref_reduction = Some(serde_json::json!({"fewer_flagged": 0}));
        assert!(!resp.render_human().contains("fewer flagged"));
        resp.rollups.unref_reduction = Some(serde_json::json!({"basis": "no count field"}));
        assert!(!resp.render_human().contains("fewer flagged"));
        // Nonzero count, accounting marker ABSENT (coverage well-formed) → suppressed.
        resp.rollups.unref_reduction = Some(serde_json::json!({
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "fewer_flagged": 3,
        }));
        assert!(!resp.render_human().contains("fewer flagged"));
        // Nonzero count, accounting present, coverage MALFORMED (no languages) → suppressed.
        resp.rollups.unref_reduction = Some(serde_json::json!({
            "accounting": "union",
            "coverage": {"partitions": ["p"], "fingerprint": "fp"},
            "fewer_flagged": 3,
        }));
        assert!(!resp.render_human().contains("fewer flagged"));
    }

    #[test]
    fn show_render_shows_kind() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Kind: inferred (0.7)"));
    }

    #[test]
    fn show_render_shows_root() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Root: src/"));
    }

    #[test]
    fn show_render_shows_ownership_with_test_as_subset() {
        // COHERENCE-2 §2.4: the fixture owns 100 non-test + 10 test files. The headline is the
        // TOTAL (110) and `(10 test files)` is the SUBSET of it — never the old addend rendering
        // that printed the non-test count (100) as the headline.
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Ownership:"));
        assert!(
            output.contains("110 files"),
            "total headline (100+10):\n{output}"
        );
        assert!(
            output.contains("10 test files"),
            "test subset clause:\n{output}"
        );
        assert!(
            !output.contains("100 files"),
            "the non-test count must NOT be the headline (addend defect):\n{output}"
        );
    }

    #[test]
    fn show_render_shows_relationships() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Relationships:"));
        assert!(output.contains("2 outbound dependencies"));
        assert!(output.contains("1 inbound dependency"));
        assert!(output.contains("1 violation"));
    }

    #[test]
    fn show_render_shows_outbound_deps() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Outbound dependencies:"));
        assert!(output.contains("lib (15 imports from 3 files)"));
    }

    #[test]
    fn show_render_shows_inbound_deps() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Inbound dependencies:"));
        assert!(output.contains("tests (8 imports from 2 files)"));
    }

    #[test]
    fn show_render_shows_violations() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Violations:"));
        assert!(output.contains("src/foo.ts -> lib/bar.ts"));
    }

    #[test]
    fn show_render_shows_unreferenced_symbols_with_caveat() {
        // OUTPUT-DOC-TRUTH-AUDIT-1: the same dead_symbol_count overclaim as `modules
        // list` — render it as the honest "unreferenced" + caveat, never the flat
        // "dead symbols" fact.
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Symbols:"));
        assert!(output.contains("25 unreferenced symbols"));
        assert!(output.contains("5 in tests"));
        assert!(
            !output.contains("dead"),
            "the overclaiming `dead` label must be absent:\n{output}"
        );
        assert!(
            output.contains("note: unreferenced = no inbound reference"),
            "caveat present:\n{output}"
        );
        assert!(
            output.contains("run `rmap trust` for reliability."),
            "caveat routes to the reliability surface:\n{output}"
        );
    }

    #[test]
    fn show_render_shows_evidence() {
        let resp = sample_show_response();
        let output = resp.render_human();
        assert!(output.contains("Evidence:"));
        assert!(output.contains("directory_heuristic"));
    }

    #[test]
    fn show_render_isolated_shows_hint() {
        let resp = sample_isolated_show_response();
        let output = resp.render_human();
        assert!(output.contains("No dependencies detected"));
        assert!(output.contains("appears isolated"));
    }
}
