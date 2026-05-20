//! Presentation layer for module ownership inventory commands.
//!
//! # CLI-OUT-4 Group 2
//!
//! Response DTOs and human renderers for:
//! - `modules files` — files owned by a specific module
//! - `modules unowned` — files not assigned to any module
//!
//! ## Change Axis
//!
//! This file changes when:
//! - `modules files` or `modules unowned` output format changes
//! - Daemon response structure for these commands changes
//!
//! It does NOT change when:
//! - `modules list` or `modules show` changes (Group 1)
//! - `modules deps` or `modules violations` changes (Group 3)

use std::collections::HashMap;

use serde::Deserialize;

use super::module_shared::format_count;

// ═══════════════════════════════════════════════════════════════════════════════
// MODULES FILES RESPONSE
// ═══════════════════════════════════════════════════════════════════════════════

/// Module identity in files response (subset of full identity).
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleRef {
    #[serde(default)]
    pub module_uid: String,
    #[serde(default)]
    pub module_key: String,
    #[serde(default)]
    pub canonical_root_path: String,
}

/// A file entry in the files response.
#[derive(Debug, Clone, Deserialize)]
pub struct OwnedFileEntry {
    #[serde(default)]
    pub file_uid: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub assignment_kind: String,
    #[serde(default)]
    pub confidence: f64,
}

/// Response structure for modules files command.
#[derive(Debug, Deserialize)]
pub struct ModulesFilesResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub module: Option<ModuleRef>,
    #[serde(default)]
    pub results: Vec<OwnedFileEntry>,
}

impl ModulesFilesResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        let module_name = self
            .module
            .as_ref()
            .map(|m| m.canonical_root_path.as_str())
            .unwrap_or("unknown");
        out.push_str(&format!("Files: {}\n\n", module_name));

        // ── Count ──────────────────────────────────────────────────
        let count = self.results.len();
        out.push_str(&format_count(count, "file", "files"));
        out.push('\n');

        if self.results.is_empty() {
            out.push_str("\nNo files owned by this module.\n");
            return out;
        }

        out.push('\n');

        // ── File rows ──────────────────────────────────────────────
        // Full output, no truncation. Caller can pipe to head.
        for file in &self.results {
            out.push_str(&format!(
                "  {}  {}  {}\n",
                file.path, file.language, file.assignment_kind,
            ));
        }

        out
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MODULES UNOWNED RESPONSE
// ═══════════════════════════════════════════════════════════════════════════════

/// An unowned file entry.
#[derive(Debug, Clone, Deserialize)]
pub struct UnownedFileEntry {
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub reason: String,
}

/// Response structure for modules unowned command.
#[derive(Debug, Deserialize)]
pub struct ModulesUnownedResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub results: Vec<UnownedFileEntry>,
}

impl ModulesUnownedResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        out.push_str("Unowned Files\n\n");

        // ── Count ──────────────────────────────────────────────────
        let count = self.results.len();
        out.push_str(&format!(
            "{} not assigned to any module\n",
            format_count(count, "file", "files")
        ));

        if self.results.is_empty() {
            out.push_str("\nAll source files are assigned to modules.\n");
            return out;
        }

        // ── Group by reason ────────────────────────────────────────
        let mut by_reason: HashMap<String, Vec<&UnownedFileEntry>> = HashMap::new();
        for entry in &self.results {
            by_reason
                .entry(entry.reason.clone())
                .or_default()
                .push(entry);
        }

        // Sort reasons by (count desc, reason asc) for deterministic output
        let mut reasons: Vec<_> = by_reason.into_iter().collect();
        reasons.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

        // Sort files within each reason alphabetically
        for (_reason, files) in &mut reasons {
            files.sort_by(|a, b| a.file_path.cmp(&b.file_path));
        }

        out.push_str("\nBy reason:\n");
        for (reason, files) in &reasons {
            out.push_str(&format!(
                "  {}  {}\n",
                reason,
                format_count(files.len(), "file", "files")
            ));
        }

        // ── Full file lists per reason ─────────────────────────────
        // Full output, no sampling. Caller can pipe to head.
        for (reason, files) in &reasons {
            out.push_str(&format!("\n{}:\n", reason));
            for file in files.iter() {
                out.push_str(&format!("  {}\n", file.file_path));
            }
        }

        // ── Hint ───────────────────────────────────────────────────
        // Check if all unowned are from excluded directories
        let all_excluded = reasons
            .iter()
            .all(|(r, _)| r.starts_with("excluded_directory:"));

        if all_excluded {
            out.push_str(
                "\nhint: excluded directories are intentional. Check 'rmap modules list' for true gaps.\n",
            );
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ModulesFilesResponse tests ─────────────────────────────────

    fn sample_files_response() -> ModulesFilesResponse {
        ModulesFilesResponse {
            command: "modules files".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            module: Some(ModuleRef {
                module_uid: "mod-1".to_string(),
                module_key: "inferred:repo:src".to_string(),
                canonical_root_path: "src".to_string(),
            }),
            results: vec![
                OwnedFileEntry {
                    file_uid: "f1".to_string(),
                    path: "src/main.ts".to_string(),
                    language: "ts".to_string(),
                    assignment_kind: "manifest_prefix".to_string(),
                    confidence: 1.0,
                },
                OwnedFileEntry {
                    file_uid: "f2".to_string(),
                    path: "src/lib/helper.ts".to_string(),
                    language: "ts".to_string(),
                    assignment_kind: "manifest_prefix".to_string(),
                    confidence: 1.0,
                },
            ],
        }
    }

    fn sample_empty_files_response() -> ModulesFilesResponse {
        ModulesFilesResponse {
            command: "modules files".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            module: Some(ModuleRef {
                module_uid: "mod-1".to_string(),
                module_key: "inferred:repo:empty".to_string(),
                canonical_root_path: "empty".to_string(),
            }),
            results: vec![],
        }
    }

    #[test]
    fn files_render_shows_header() {
        let resp = sample_files_response();
        let output = resp.render_human();
        assert!(output.contains("Files: src"));
    }

    #[test]
    fn files_render_shows_count() {
        let resp = sample_files_response();
        let output = resp.render_human();
        assert!(output.contains("2 files"));
    }

    #[test]
    fn files_render_shows_paths() {
        let resp = sample_files_response();
        let output = resp.render_human();
        assert!(output.contains("src/main.ts"));
        assert!(output.contains("src/lib/helper.ts"));
    }

    #[test]
    fn files_render_shows_language() {
        let resp = sample_files_response();
        let output = resp.render_human();
        assert!(output.contains("ts"));
    }

    #[test]
    fn files_render_shows_assignment_kind() {
        let resp = sample_files_response();
        let output = resp.render_human();
        assert!(output.contains("manifest_prefix"));
    }

    #[test]
    fn files_render_empty_shows_message() {
        let resp = sample_empty_files_response();
        let output = resp.render_human();
        assert!(output.contains("No files owned"));
    }

    // ── ModulesUnownedResponse tests ───────────────────────────────

    fn sample_unowned_response() -> ModulesUnownedResponse {
        ModulesUnownedResponse {
            command: "modules unowned".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![
                UnownedFileEntry {
                    file_path: "deps/lib/foo.h".to_string(),
                    language: "c".to_string(),
                    reason: "excluded_directory:deps".to_string(),
                },
                UnownedFileEntry {
                    file_path: "deps/lib/bar.h".to_string(),
                    language: "c".to_string(),
                    reason: "excluded_directory:deps".to_string(),
                },
                UnownedFileEntry {
                    file_path: "vendor/third.js".to_string(),
                    language: "js".to_string(),
                    reason: "excluded_directory:vendor".to_string(),
                },
            ],
        }
    }

    fn sample_empty_unowned_response() -> ModulesUnownedResponse {
        ModulesUnownedResponse {
            command: "modules unowned".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![],
        }
    }

    #[test]
    fn unowned_render_shows_header() {
        let resp = sample_unowned_response();
        let output = resp.render_human();
        assert!(output.contains("Unowned Files"));
    }

    #[test]
    fn unowned_render_shows_count() {
        let resp = sample_unowned_response();
        let output = resp.render_human();
        assert!(output.contains("3 files not assigned"));
    }

    #[test]
    fn unowned_render_groups_by_reason() {
        let resp = sample_unowned_response();
        let output = resp.render_human();
        assert!(output.contains("By reason:"));
        assert!(output.contains("excluded_directory:deps"));
        assert!(output.contains("excluded_directory:vendor"));
    }

    #[test]
    fn unowned_render_shows_reason_counts() {
        let resp = sample_unowned_response();
        let output = resp.render_human();
        // deps has 2 files, vendor has 1
        assert!(output.contains("2 files"));
        assert!(output.contains("1 file"));
    }

    #[test]
    fn unowned_render_shows_full_file_lists() {
        let resp = sample_unowned_response();
        let output = resp.render_human();
        // Full grouped output under each reason header
        assert!(output.contains("excluded_directory:deps:\n"));
        assert!(output.contains("deps/lib/foo.h"));
        assert!(output.contains("deps/lib/bar.h"));
        assert!(output.contains("excluded_directory:vendor:\n"));
        assert!(output.contains("vendor/third.js"));
    }

    #[test]
    fn unowned_render_shows_hint_for_excluded() {
        let resp = sample_unowned_response();
        let output = resp.render_human();
        assert!(output.contains("hint:"));
        assert!(output.contains("excluded directories are intentional"));
    }

    #[test]
    fn unowned_render_empty_shows_message() {
        let resp = sample_empty_unowned_response();
        let output = resp.render_human();
        assert!(output.contains("All source files are assigned"));
    }

    #[test]
    fn unowned_render_is_deterministic() {
        // Verify sort order: reasons by (count desc, reason asc), files alphabetically
        let resp = sample_unowned_response();
        let output = resp.render_human();

        // deps (2 files) should come before vendor (1 file) in summary
        let deps_summary_pos = output.find("excluded_directory:deps  2 files").unwrap();
        let vendor_summary_pos = output.find("excluded_directory:vendor  1 file").unwrap();
        assert!(
            deps_summary_pos < vendor_summary_pos,
            "Reasons should be sorted by count descending"
        );

        // Within deps section, bar.h should come before foo.h (alphabetical)
        let deps_section_start = output.find("excluded_directory:deps:\n").unwrap();
        let bar_pos = output[deps_section_start..].find("deps/lib/bar.h").unwrap();
        let foo_pos = output[deps_section_start..].find("deps/lib/foo.h").unwrap();
        assert!(
            bar_pos < foo_pos,
            "Files within reason should be sorted alphabetically"
        );
    }
}
