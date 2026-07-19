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
    /// RECON-M-R3a (g2u-a): the daemon's per-module REDUCTION-ONLY unref overlay
    /// (`{fewer_flagged, …}` — see `modules_show`). Absent unless measured and nonzero.
    #[serde(default)]
    pub unref_reduction: Option<serde_json::Value>,
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

        // ── Unreferenced-symbol caveat (OUTPUT-DOC-TRUTH-AUDIT-1) ───
        // The `unref?` column is dead_symbol_count: a SYNTACTIC graph-orphan
        // estimate (no inbound reference in the modeled graph), a low-reliability
        // Layer-2 inference — NOT a Layer-0 "safe to delete" fact. The bare label
        // "dead" overclaimed it; this footnote keeps the (useful) count while
        // making its certainty class honest and pointing at the trust surface.
        out.push('\n');
        out.push_str(
            "note: unref? = symbols with no inbound reference in the indexed graph \
             (syntactic estimate); over-counts under low call-graph resolution; \
             run `rmap trust` for reliability.\n",
        );
        // RECON-M-R3a (g2u-a): the reduction-only compiler-witness aggregate — rendered ONLY
        // when the daemon attached nonzero reductions (W-BOTH with a current measured ledger)
        // that pass the §5.3.0 labeling gate (review-2 item 1: `accounting: "union"` +
        // coverage basis, per row, via the ONE shared gate) — the coverage renders beside the
        // aggregate. A row failing the gate contributes NOTHING (its union value is
        // suppressed, never rendered unlabeled). Per-module figures ride the JSON. Reduction
        // only: the `unref?` column is untouched.
        let mut reduced = 0u64;
        let mut modules_with = 0usize;
        // All rows come from the ONE shared projection, so today the set holds one phrase;
        // collecting a set keeps the join honest if that ever diverges.
        let mut coverages: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for m in &self.results {
            if let Some((n, coverage)) = m.unref_reduction.as_ref().and_then(|b| {
                let coverage = crate::presentation::witnesses::union_coverage_phrase(b)?;
                let n = b
                    .get("fewer_flagged")
                    .and_then(|v| v.as_u64())
                    .filter(|n| *n > 0)?;
                Some((n, coverage))
            }) {
                reduced += n;
                modules_with += 1;
                coverages.insert(coverage);
            }
        }
        if reduced > 0 {
            out.push_str(&format!(
                "reconciled: {} fewer flagged across {} module{} — compiler-verified \
                 references found — combined analyses (coverage: {}).\n",
                reduced,
                modules_with,
                if modules_with == 1 { "" } else { "s" },
                coverages.into_iter().collect::<Vec<_>>().join("; "),
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
                    unref_reduction: None,
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
                    unref_reduction: None,
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

    /// Review-1 item 3: the NONZERO g2u aggregate through final human rendering — the
    /// reconciled footnote sums the per-module reductions beside the untouched `unref?`
    /// column figures.
    #[test]
    fn list_render_nonzero_reduction_renders_the_reconciled_footnote() {
        let mut resp = sample_list_response();
        resp.results[0].unref_reduction = Some(serde_json::json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "fewer_flagged": 2,
            "basis": "compiler-verified references found",
        }));
        let output = resp.render_human();
        // Review-2 item 1: the coverage basis renders beside the reconciled aggregate.
        assert!(
            output.contains(
                "reconciled: 2 fewer flagged across 1 module — compiler-verified \
                 references found — combined analyses (coverage: TypeScript (1 partition))."
            ),
            "{output}"
        );
        assert!(
            output.contains("25 unref?"),
            "the pipeline column stays untouched: {output}"
        );
    }

    /// Review-2 item 1 (negative): a row whose reduction block fails the §5.3.0 labeling
    /// gate — missing `accounting: "union"`, or missing/malformed coverage — contributes
    /// NOTHING: with no passing row the footnote is entirely absent, never an unlabeled
    /// reconciled figure.
    #[test]
    fn list_render_unlabeled_reduction_rows_render_no_footnote() {
        // Accounting marker absent (coverage well-formed).
        let mut resp = sample_list_response();
        resp.results[0].unref_reduction = Some(serde_json::json!({
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "fewer_flagged": 2,
        }));
        let output = resp.render_human();
        assert!(!output.contains("reconciled"), "{output}");
        assert!(!output.contains("fewer flagged"), "{output}");

        // Accounting present, coverage malformed (empty languages).
        resp.results[0].unref_reduction = Some(serde_json::json!({
            "accounting": "union",
            "coverage": {"languages": [], "partitions": ["p"], "fingerprint": "fp"},
            "fewer_flagged": 2,
        }));
        let output = resp.render_human();
        assert!(!output.contains("reconciled"), "{output}");
        assert!(!output.contains("fewer flagged"), "{output}");

        // Mixed: one labeled row + one unlabeled row → the gate is PER ROW: only the
        // labeled row's reduction aggregates; the unlabeled row's value is suppressed.
        resp.results[0].unref_reduction = Some(serde_json::json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "fewer_flagged": 2,
        }));
        resp.results[1].unref_reduction = Some(serde_json::json!({"fewer_flagged": 5}));
        let output = resp.render_human();
        assert!(
            output.contains(
                "reconciled: 2 fewer flagged across 1 module — compiler-verified \
                 references found — combined analyses (coverage: TypeScript (1 partition))."
            ),
            "{output}"
        );
        assert!(!output.contains("7 fewer"), "{output}");
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

    #[test]
    fn list_render_relabels_dead_as_unref_with_caveat() {
        // OUTPUT-DOC-TRUTH-AUDIT-1: dead_symbol_count is a low-reliability Layer-2
        // graph-orphan inference, not a Layer-0 fact. The column must read `unref?`
        // (never the overclaiming bare `dead`) and carry a caveat that points at the
        // reliability surface and never claims the count is safe-to-delete.
        let resp = sample_list_response();
        let output = resp.render_human();

        // Honest relabel present on the rows (25 -> "25 unref?", 3 -> "3 unref?").
        assert!(
            output.contains("unref?"),
            "honest column label present:\n{output}"
        );

        // The overclaiming bare label is GONE from the whole surface.
        assert!(
            !output.contains("dead"),
            "the overclaiming `dead` label must be absent:\n{output}"
        );

        // Caveat footnote present, scoped honestly, and routes to `rmap trust`.
        assert!(
            output.contains("note: unref? = symbols with no inbound reference"),
            "caveat footnote present:\n{output}"
        );
        assert!(
            output.contains("run `rmap trust` for reliability."),
            "caveat routes to the reliability surface:\n{output}"
        );
    }
}
