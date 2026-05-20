//! Presentation layer for `modules list` command.
//!
//! # CLI-OUT-4 Group 1
//!
//! Response DTO and human renderer for module catalog listing.
//!
//! ## Change Axis
//!
//! This file changes when:
//! - `modules list` output format changes
//! - `modules list` daemon response structure changes
//!
//! It does NOT change when:
//! - `modules show` changes
//! - Shared formatting changes (see `module_shared.rs`)

use serde::Deserialize;

use super::module_shared::{
    format_count, format_dead_compact, format_files_compact, format_kind_confidence,
};

/// A module entry in the list response.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleListEntry {
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
}

/// Response structure for modules list command.
#[derive(Debug, Deserialize)]
pub struct ModulesListResponse {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub results: Vec<ModuleListEntry>,
}

impl ModulesListResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        // Note: daemon doesn't provide display_name for modules_list,
        // so we use a generic header. Future: request display_name field.
        out.push_str("Modules\n\n");

        // ── Count ──────────────────────────────────────────────────
        let count = self.results.len();
        out.push_str(&format_count(count, "module", "modules"));
        out.push('\n');

        if self.results.is_empty() {
            out.push_str("\nNo modules detected.\n");
            out.push_str(
                "\nhint: modules are inferred from directory structure or declared in manifests.\n",
            );
            return out;
        }

        out.push('\n');

        // ── Module rows ────────────────────────────────────────────
        // Calculate column widths for alignment
        let max_name_len = self
            .results
            .iter()
            .map(|m| m.display_name.len())
            .max()
            .unwrap_or(10)
            .max(10);

        for module in &self.results {
            let name = &module.display_name;
            let files = format_files_compact(module.owned_file_count, module.owned_test_file_count);
            let dead = format_dead_compact(module.dead_symbol_count);
            let violations = format_count(module.violation_count, "violation", "violations");
            let kind_conf = format_kind_confidence(&module.module_kind, module.confidence);

            out.push_str(&format!(
                "  {:width$}  {:>20}  {:>10}  {:>14}  {}\n",
                name,
                files,
                dead,
                violations,
                kind_conf,
                width = max_name_len
            ));
        }

        // ── Cross-module dependency summary ────────────────────────
        let total_outbound: usize = self
            .results
            .iter()
            .map(|m| m.outbound_dependency_count)
            .sum();
        let total_inbound: usize = self
            .results
            .iter()
            .map(|m| m.inbound_dependency_count)
            .sum();

        out.push('\n');
        if total_outbound == 0 && total_inbound == 0 {
            out.push_str("No cross-module dependencies detected.\n");
            if self.results.len() > 1 {
                out.push_str(
                    "\nhint: all imports are intra-module. Module boundaries may not be meaningful yet.\n",
                );
            }
        } else {
            let dep_count = total_outbound.max(total_inbound) / 2; // rough dedup
            out.push_str(&format!(
                "{} cross-module dependencies detected.\n",
                dep_count
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_list_response() -> ModulesListResponse {
        ModulesListResponse {
            command: "modules list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![
                ModuleListEntry {
                    module_uid: "mod-1".to_string(),
                    module_key: "inferred:repo_123:src".to_string(),
                    canonical_root_path: "src".to_string(),
                    module_kind: "inferred".to_string(),
                    display_name: "src".to_string(),
                    confidence: 0.7,
                    owned_file_count: 100,
                    owned_test_file_count: 10,
                    outbound_dependency_count: 0,
                    outbound_import_count: 50,
                    inbound_dependency_count: 0,
                    inbound_import_count: 20,
                    violation_count: 0,
                    dead_symbol_count: 25,
                    dead_test_symbol_count: 5,
                },
                ModuleListEntry {
                    module_uid: "mod-2".to_string(),
                    module_key: "inferred:repo_123:lib".to_string(),
                    canonical_root_path: "lib".to_string(),
                    module_kind: "manifest".to_string(),
                    display_name: "lib".to_string(),
                    confidence: 1.0,
                    owned_file_count: 20,
                    owned_test_file_count: 0,
                    outbound_dependency_count: 1,
                    outbound_import_count: 5,
                    inbound_dependency_count: 1,
                    inbound_import_count: 10,
                    violation_count: 2,
                    dead_symbol_count: 3,
                    dead_test_symbol_count: 0,
                },
            ],
        }
    }

    fn sample_empty_list_response() -> ModulesListResponse {
        ModulesListResponse {
            command: "modules list".to_string(),
            repo: "repo_123".to_string(),
            snapshot: "snap_456".to_string(),
            results: vec![],
        }
    }

    #[test]
    fn list_render_shows_header() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.starts_with("Modules\n"));
    }

    #[test]
    fn list_render_shows_count() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("2 modules"));
    }

    #[test]
    fn list_render_shows_module_names() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("src"));
        assert!(output.contains("lib"));
    }

    #[test]
    fn list_render_shows_kind_confidence() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("inferred (0.7)"));
        assert!(output.contains("manifest")); // 1.0 hides decimal
    }

    #[test]
    fn list_render_shows_violations() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("2 violations"));
    }

    #[test]
    fn list_render_shows_cross_module_deps() {
        let resp = sample_list_response();
        let output = resp.render_human();
        assert!(output.contains("cross-module dependencies detected"));
    }

    #[test]
    fn list_render_empty_shows_hint() {
        let resp = sample_empty_list_response();
        let output = resp.render_human();
        assert!(output.contains("No modules detected"));
        assert!(output.contains("hint:"));
    }
}
