//! Presentation layer for the `coverage` command.
//!
//! # CLI-OUT-6 Group 3
//!
//! Transforms coverage import results into human-readable plain text.
//! Shows import summary and per-file coverage data.
//!
//! Coverage is a write command (import operation), not a read query.
//! The output summarizes what was imported and highlights path mismatches.
//!
//! # Backend-Bounded Diagnostics
//!
//! The `unnormalized_paths_sample` and `unmatched_indexed_paths_sample` fields
//! are already bounded by the backend (max 10 each). This is NOT renderer-side
//! clipping. The renderer presents them honestly as "backend samples" to help
//! users debug path mismatches without pretending the lists are exhaustive.
//!
//! # Output Contract
//!
//! - Imported file rows: full output, no truncation
//! - Sample-path diagnostics: backend-bounded, rendered as samples
//! - Deterministic ordering (by file_path for imported rows)
//! - `--json` preserved for machine mode

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Response DTO for `coverage` command.
///
/// Envelope fields from `build_envelope` plus import-specific fields.
#[derive(Debug, Deserialize)]
pub struct CoverageResponse {
    #[allow(dead_code)]
    pub command: String,
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    pub results: Vec<CoverageEntry>,
    pub count: usize,
    pub imported_count: usize,
    pub unnormalized_count: usize,
    pub unmatched_indexed_count: usize,
    /// Backend-bounded sample (max 10) of paths that couldn't be normalized.
    #[serde(default)]
    pub unnormalized_paths_sample: Vec<String>,
    /// Backend-bounded sample (max 10) of indexed files without coverage.
    #[serde(default)]
    pub unmatched_indexed_paths_sample: Vec<String>,
}

/// Individual coverage entry for an imported file.
#[derive(Debug, Deserialize, Clone)]
pub struct CoverageEntry {
    pub file_path: String,
    pub line_coverage: f64,
    pub covered_statements: u64,
    pub total_statements: u64,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl CoverageResponse {
    /// Render the coverage response as human-readable plain text.
    ///
    /// Outputs full imported file list. No truncation.
    /// Sample-path diagnostics are backend-bounded and labeled as such.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Coverage Import\n\n");

        // Import summary
        let file_word = if self.imported_count == 1 {
            "file"
        } else {
            "files"
        };
        out.push_str(&format!("{} {} imported\n", self.imported_count, file_word));

        // Unnormalized count
        if self.unnormalized_count > 0 {
            let word = if self.unnormalized_count == 1 {
                "file"
            } else {
                "files"
            };
            out.push_str(&format!(
                "{} {} could not be normalized (paths outside repo)\n",
                self.unnormalized_count, word
            ));
        }

        // Unmatched indexed count
        if self.unmatched_indexed_count > 0 {
            let word = if self.unmatched_indexed_count == 1 {
                "file has"
            } else {
                "files have"
            };
            out.push_str(&format!(
                "{} indexed {} no coverage data\n",
                self.unmatched_indexed_count, word
            ));
        }

        // Imported files section (full output, sorted for determinism)
        if !self.results.is_empty() {
            out.push_str("\nImported files:\n");

            let mut entries = self.results.clone();
            entries.sort_by(|a, b| a.file_path.cmp(&b.file_path));

            // Compute column widths
            let max_path_len = entries.iter().map(|e| e.file_path.len()).max().unwrap_or(0);

            for entry in &entries {
                let coverage_pct = format!("{:.1}%", entry.line_coverage * 100.0);
                let statements = format!("{}/{}", entry.covered_statements, entry.total_statements);
                out.push_str(&format!(
                    "  {:<width$}  {:>6}  {} statements\n",
                    entry.file_path,
                    coverage_pct,
                    statements,
                    width = max_path_len,
                ));
            }
        }

        // Backend-bounded diagnostic samples
        // These are NOT exhaustive - explicitly label them as samples
        if !self.unnormalized_paths_sample.is_empty() {
            out.push_str(&format!(
                "\nUnnormalized paths ({} of {}, backend sample):\n",
                self.unnormalized_paths_sample.len(),
                self.unnormalized_count
            ));
            for path in &self.unnormalized_paths_sample {
                out.push_str(&format!("  {}\n", path));
            }
        }

        if !self.unmatched_indexed_paths_sample.is_empty() {
            out.push_str(&format!(
                "\nUnmatched indexed files ({} of {}, backend sample):\n",
                self.unmatched_indexed_paths_sample.len(),
                self.unmatched_indexed_count
            ));
            for path in &self.unmatched_indexed_paths_sample {
                out.push_str(&format!("  {}\n", path));
            }
        }

        // Hint for empty import
        if self.imported_count == 0 {
            out.push_str("\nhint: no files were imported.\n");
            if self.unnormalized_count > 0 {
                out.push_str("      Check that coverage report paths are relative to repo root.\n");
            }
        }

        out
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(
        entries: Vec<CoverageEntry>,
        unnormalized_count: usize,
        unmatched_indexed_count: usize,
        unnormalized_sample: Vec<String>,
        unmatched_sample: Vec<String>,
    ) -> CoverageResponse {
        CoverageResponse {
            command: "coverage".to_string(),
            repo: "test-repo".to_string(),
            snapshot: "snap_123".to_string(),
            results: entries.clone(),
            count: entries.len(),
            imported_count: entries.len(),
            unnormalized_count,
            unmatched_indexed_count,
            unnormalized_paths_sample: unnormalized_sample,
            unmatched_indexed_paths_sample: unmatched_sample,
        }
    }

    fn make_entry(path: &str, coverage: f64, covered: u64, total: u64) -> CoverageEntry {
        CoverageEntry {
            file_path: path.to_string(),
            line_coverage: coverage,
            covered_statements: covered,
            total_statements: total,
        }
    }

    #[test]
    fn render_empty_import() {
        let resp = make_response(vec![], 0, 0, vec![], vec![]);
        let out = resp.render_human();

        assert!(out.contains("Coverage Import"));
        assert!(out.contains("0 files imported"));
        assert!(out.contains("hint: no files were imported"));
    }

    #[test]
    fn render_single_import() {
        let resp = make_response(
            vec![make_entry("src/main.ts", 0.85, 170, 200)],
            0,
            0,
            vec![],
            vec![],
        );
        let out = resp.render_human();

        assert!(out.contains("1 file imported"));
        assert!(out.contains("src/main.ts"));
        assert!(out.contains("85.0%"));
        assert!(out.contains("170/200 statements"));
    }

    #[test]
    fn render_multiple_imports_sorted() {
        let resp = make_response(
            vec![
                make_entry("src/z.ts", 0.90, 90, 100),
                make_entry("src/a.ts", 0.80, 80, 100),
                make_entry("src/m.ts", 0.70, 70, 100),
            ],
            0,
            0,
            vec![],
            vec![],
        );
        let out = resp.render_human();

        assert!(out.contains("3 files imported"));

        // Verify alphabetical ordering
        let a_pos = out.find("src/a.ts").unwrap();
        let m_pos = out.find("src/m.ts").unwrap();
        let z_pos = out.find("src/z.ts").unwrap();

        assert!(a_pos < m_pos, "a should come before m");
        assert!(m_pos < z_pos, "m should come before z");
    }

    #[test]
    fn render_with_unnormalized_paths() {
        let resp = make_response(
            vec![make_entry("src/main.ts", 0.85, 170, 200)],
            3,
            0,
            vec!["../outside/foo.js".to_string(), "/abs/path.js".to_string()],
            vec![],
        );
        let out = resp.render_human();

        assert!(out.contains("3 files could not be normalized"));
        assert!(out.contains("Unnormalized paths (2 of 3, backend sample)"));
        assert!(out.contains("../outside/foo.js"));
        assert!(out.contains("/abs/path.js"));
    }

    #[test]
    fn render_with_unmatched_indexed() {
        let resp = make_response(
            vec![make_entry("src/main.ts", 0.85, 170, 200)],
            0,
            5,
            vec![],
            vec!["src/old.ts".to_string(), "src/legacy.ts".to_string()],
        );
        let out = resp.render_human();

        assert!(out.contains("5 indexed files have no coverage data"));
        assert!(out.contains("Unmatched indexed files (2 of 5, backend sample)"));
        assert!(out.contains("src/old.ts"));
        assert!(out.contains("src/legacy.ts"));
    }

    #[test]
    fn render_with_both_mismatch_types() {
        let resp = make_response(
            vec![make_entry("src/main.ts", 0.85, 170, 200)],
            3,
            5,
            vec!["../foo.js".to_string()],
            vec!["src/old.ts".to_string()],
        );
        let out = resp.render_human();

        assert!(out.contains("3 files could not be normalized"));
        assert!(out.contains("5 indexed files have no coverage data"));
        assert!(out.contains("Unnormalized paths (1 of 3, backend sample)"));
        assert!(out.contains("Unmatched indexed files (1 of 5, backend sample)"));
    }

    #[test]
    fn sample_labels_indicate_backend_bounded() {
        let resp = make_response(
            vec![],
            15,
            20,
            (0..10).map(|i| format!("path{}.js", i)).collect(),
            (0..10).map(|i| format!("file{}.ts", i)).collect(),
        );
        let out = resp.render_human();

        // Backend bounded samples should be labeled with "of N, backend sample"
        assert!(out.contains("(10 of 15, backend sample)"));
        assert!(out.contains("(10 of 20, backend sample)"));
    }

    #[test]
    fn empty_import_with_unnormalized_shows_hint() {
        let resp = make_response(vec![], 5, 0, vec!["../outside.js".to_string()], vec![]);
        let out = resp.render_human();

        assert!(out.contains("hint: no files were imported"));
        assert!(out.contains("Check that coverage report paths are relative to repo root"));
    }

    #[test]
    fn grammar_singular_file() {
        let resp = make_response(
            vec![make_entry("src/main.ts", 0.85, 170, 200)],
            1,
            1,
            vec!["../foo.js".to_string()],
            vec!["src/old.ts".to_string()],
        );
        let out = resp.render_human();

        assert!(out.contains("1 file imported"));
        assert!(out.contains("1 file could not be normalized"));
        assert!(out.contains("1 indexed file has no coverage data"));
    }
}
