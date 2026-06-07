//! Presentation layer for the `imports` command.
//!
//! # CLI-OUT-3
//!
//! Renders file/module dependency listing as human-readable text.
//! Shows direct imports with resolution status.
//!
//! ## Human Output Structure
//!
//! ```text
//! Imports: src/Engine/State.cpp
//!
//! 19 imports
//!
//!   src/Engine/Game.h                  depth=1  static
//!   src/Engine/InteractiveSurface.h    depth=1  static
//!   src/Engine/Language.h              depth=1  static
//!   ...
//! ```

use serde::Deserialize;
use std::collections::BTreeMap;

// ── Response Types ───────────────────────────────────────────────────────────

/// An import edge in the response.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImportEntry {
    #[serde(default)]
    pub node_id: String,
    /// The imported symbol/file path.
    pub symbol: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub subtype: String,
    /// The file being imported (often same as symbol for file imports).
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
    #[serde(default)]
    pub edge_type: String,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub depth: u32,
}

/// Response structure for imports command.
#[derive(Debug, Deserialize)]
pub struct ImportsResponse {
    /// The file being queried.
    pub file: String,
    /// List of imports.
    pub imports: Vec<ImportEntry>,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl ImportsResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────
        out.push_str(&format!("Imports: {}\n\n", self.file));

        // ── Count ──────────────────────────────────────────────────
        let count = self.imports.len();
        if count == 1 {
            out.push_str("1 import\n");
        } else {
            out.push_str(&format!("{} imports\n", count));
        }

        if self.imports.is_empty() {
            return out;
        }

        out.push('\n');

        // ── Import list ────────────────────────────────────────────
        for imp in &self.imports {
            // Use symbol as the display name (usually the imported file path)
            let name = &imp.symbol;
            let depth = imp.depth;
            let resolution = if imp.resolution.is_empty() {
                "-"
            } else {
                &imp.resolution
            };

            out.push_str(&format!("  {}  depth={}  {}\n", name, depth, resolution));
        }

        out
    }
}

// ── IMPORTS-LIVEGRAPH-CLI-1: the LiveGraph import read-model response (D2/D4) ──────────────────

/// A captured FILE -> FILE import edge (a graph fact) in the `--engine livegraph` response.
#[derive(Debug, Clone, Deserialize)]
pub struct LgImportEdge {
    #[serde(default)]
    pub src_file: String,
    #[serde(default)]
    pub dst_file: String,
    #[serde(default)]
    pub basis: String,
    #[serde(default)]
    pub raw_specifier: Option<String>,
}

/// A classified non-edge import observation (completeness evidence) in the `--engine livegraph` response.
#[derive(Debug, Clone, Deserialize)]
pub struct LgImportObservation {
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub raw_specifier: String,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub blocking: bool,
}

/// The `imports --engine livegraph` response: captured EDGES (facts) + classified OBSERVATIONS (evidence),
/// SEPARATED (D2), plus the module-cycle trust signals named after their SOURCE (NOT a generic
/// import-listing-completeness claim).
#[derive(Debug, Deserialize)]
pub struct LivegraphImportsResponse {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub file_filter: Option<String>,
    #[serde(default)]
    pub edges: Vec<LgImportEdge>,
    #[serde(default)]
    pub edge_count: usize,
    #[serde(default)]
    pub observations: Vec<LgImportObservation>,
    #[serde(default)]
    pub observation_count: usize,
    #[serde(default)]
    pub blocking_observation_count: usize,
    #[serde(default)]
    pub observation_class_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub module_cycle_completeness: String,
    #[serde(default)]
    pub module_cycle_answer_class: String,
    #[serde(default)]
    pub freshness: String,
    #[serde(default)]
    pub missing_partitions: Vec<String>,
}

impl LivegraphImportsResponse {
    /// Render as COMPACT human text (D4): edge count + the edge list; observation per-class counts + the
    /// BLOCKING evidence. Benign (external/asset) observations are NOT listed individually UNLESS the query is
    /// filtered to a single file (rule 4). JSON (`--json`) carries the full evidence.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        let scope = match &self.file_filter {
            Some(f) => format!("file={f}"),
            None => "repo-wide".to_string(),
        };
        out.push_str(&format!(
            "Imports (livegraph): {}  [{}]\n",
            self.display_name, scope
        ));
        // Module-cycle trust, named after its SOURCE (never a generic import-completeness claim).
        out.push_str(&format!(
            "module-cycle: completeness={}  answer_class={}  freshness={}\n",
            self.module_cycle_completeness, self.module_cycle_answer_class, self.freshness
        ));
        if !self.missing_partitions.is_empty() {
            out.push_str(&format!(
                "  missing partitions: {}\n",
                self.missing_partitions.join(", ")
            ));
        }
        out.push('\n');

        // EDGES (graph facts) — listed in full.
        out.push_str(&format!(
            "Edges: {} captured FILE->FILE import edges\n",
            self.edge_count
        ));
        for e in &self.edges {
            let spec = e
                .raw_specifier
                .as_deref()
                .map(|s| format!("  \"{s}\""))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {} -> {}  [{}]{}\n",
                e.src_file, e.dst_file, e.basis, spec
            ));
        }
        out.push('\n');

        // OBSERVATIONS (completeness evidence) — counts always; per-row only for blocking (or all, if
        // file-filtered).
        out.push_str(&format!(
            "Observations: {} (blocking: {})\n",
            self.observation_count, self.blocking_observation_count
        ));
        if !self.observation_class_counts.is_empty() {
            let counts: Vec<String> = self
                .observation_class_counts
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            out.push_str(&format!("  by class: {}\n", counts.join("  ")));
        }
        let show_benign = self.file_filter.is_some();
        let mut shown_header = false;
        for o in &self.observations {
            if o.blocking || show_benign {
                if !shown_header {
                    out.push_str("  evidence:\n");
                    shown_header = true;
                }
                let tag = if o.blocking { "BLOCKING" } else { "benign" };
                out.push_str(&format!(
                    "    {}  {}  [{}] {}\n",
                    o.source_file, o.raw_specifier, o.class, tag
                ));
            }
        }
        out
    }
}

// ── IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1: the `imports --engine compare` response (D6) ────────

/// A LiveGraph edge SQLite lacks (an improvement) in the compare sidecar.
#[derive(Debug, Clone, Deserialize)]
pub struct CompareExtraEdge {
    #[serde(default)]
    pub dst_file: String,
    #[serde(default)]
    pub basis: String,
    #[serde(default)]
    pub raw_specifier: Option<String>,
}

/// A blocking LiveGraph observation reported by the compare sidecar.
#[derive(Debug, Clone, Deserialize)]
pub struct CompareBlockingObs {
    #[serde(default)]
    pub raw_specifier: String,
    #[serde(default)]
    pub class: String,
}

/// The D3 precondition (the file's partition residency) in the compare sidecar.
#[derive(Debug, Clone, Deserialize)]
pub struct ComparePrecondition {
    #[serde(default)]
    pub partition: String,
    #[serde(default)]
    pub resident: bool,
    #[serde(default)]
    pub fresh: bool,
    #[serde(default)]
    pub ts_primary: bool,
    #[serde(default)]
    pub precondition_met: bool,
}

/// The directional-compare sidecar (SQLite-vs-LiveGraph) for one file.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImportsComparison {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub matched: Vec<String>,
    #[serde(default)]
    pub missing_in_livegraph: Vec<String>,
    #[serde(default)]
    pub extra_livegraph_edges: Vec<CompareExtraEdge>,
    #[serde(default)]
    pub blocking_observations: Vec<CompareBlockingObs>,
    #[serde(default)]
    pub sqlite_resolved_local_count: usize,
    #[serde(default)]
    pub livegraph_edge_count: usize,
    #[serde(default)]
    pub precondition: Option<ComparePrecondition>,
}

/// The `imports <file> --engine compare` response: the SQLite listing (PRIMARY) + the directional-compare
/// sidecar. The SQLite part renders byte-compatibly with the default; the compare summary follows.
#[derive(Debug, Deserialize)]
pub struct ImportsCompareResponse {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub imports: Vec<ImportEntry>,
    #[serde(default)]
    pub comparison: ImportsComparison,
}

impl ImportsCompareResponse {
    /// Render the SQLite listing (primary, default-compatible) then the directional-compare summary.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Imports: {}\n\n", self.file));
        let count = self.imports.len();
        out.push_str(&format!(
            "{} import{}\n",
            count,
            if count == 1 { "" } else { "s" }
        ));
        if !self.imports.is_empty() {
            out.push('\n');
            for imp in &self.imports {
                let resolution = if imp.resolution.is_empty() {
                    "-"
                } else {
                    &imp.resolution
                };
                out.push_str(&format!(
                    "  {}  depth={}  {}\n",
                    imp.symbol, imp.depth, resolution
                ));
            }
        }
        // ── Compare summary (the sidecar) ──
        let c = &self.comparison;
        out.push_str(&format!("\nCompare (sqlite vs livegraph): {}\n", c.status));
        match &c.precondition {
            Some(p) => out.push_str(&format!(
                "  precondition: partition={} resident={} fresh={} ts={} -> met={}\n",
                p.partition, p.resident, p.fresh, p.ts_primary, p.precondition_met
            )),
            None => out.push_str(
                "  precondition: no resident TS partition for this file -> SQLite fallback\n",
            ),
        }
        out.push_str(&format!(
            "  sqlite resolved-local={}  livegraph edges={}  matched={}\n",
            c.sqlite_resolved_local_count,
            c.livegraph_edge_count,
            c.matched.len()
        ));
        if !c.missing_in_livegraph.is_empty() {
            out.push_str(&format!(
                "  MISSING in livegraph (REGRESSIONS): {}\n",
                c.missing_in_livegraph.len()
            ));
            for m in &c.missing_in_livegraph {
                out.push_str(&format!("    - {m}\n"));
            }
        }
        if !c.extra_livegraph_edges.is_empty() {
            out.push_str(&format!(
                "  extra livegraph edges (improvements): {}\n",
                c.extra_livegraph_edges.len()
            ));
            for e in &c.extra_livegraph_edges {
                let spec = e
                    .raw_specifier
                    .as_deref()
                    .map(|s| format!(" \"{s}\""))
                    .unwrap_or_default();
                out.push_str(&format!("    + {} [{}]{}\n", e.dst_file, e.basis, spec));
            }
        }
        if !c.blocking_observations.is_empty() {
            out.push_str(&format!(
                "  blocking observations: {}\n",
                c.blocking_observations.len()
            ));
            for o in &c.blocking_observations {
                out.push_str(&format!("    ! {} [{}]\n", o.raw_specifier, o.class));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_imports() -> ImportsResponse {
        ImportsResponse {
            file: "src/main.cpp".to_string(),
            imports: vec![
                ImportEntry {
                    node_id: "n1".to_string(),
                    symbol: "src/foo.h".to_string(),
                    kind: "FILE".to_string(),
                    subtype: "SOURCE".to_string(),
                    file: "src/foo.h".to_string(),
                    line: 1,
                    column: 0,
                    edge_type: "IMPORTS".to_string(),
                    resolution: "static".to_string(),
                    evidence: vec!["cpp-core:0.1.0".to_string()],
                    depth: 1,
                },
                ImportEntry {
                    node_id: "n2".to_string(),
                    symbol: "src/bar.h".to_string(),
                    kind: "FILE".to_string(),
                    subtype: "SOURCE".to_string(),
                    file: "src/bar.h".to_string(),
                    line: 1,
                    column: 0,
                    edge_type: "IMPORTS".to_string(),
                    resolution: "static".to_string(),
                    evidence: vec![],
                    depth: 1,
                },
                ImportEntry {
                    node_id: "n3".to_string(),
                    symbol: "external/lib.h".to_string(),
                    kind: "FILE".to_string(),
                    subtype: "EXTERNAL".to_string(),
                    file: "external/lib.h".to_string(),
                    line: 1,
                    column: 0,
                    edge_type: "IMPORTS".to_string(),
                    resolution: "unresolved".to_string(),
                    evidence: vec![],
                    depth: 1,
                },
            ],
        }
    }

    fn sample_empty_imports() -> ImportsResponse {
        ImportsResponse {
            file: "src/standalone.cpp".to_string(),
            imports: vec![],
        }
    }

    #[test]
    fn render_imports_shows_header() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("Imports: src/main.cpp"));
    }

    #[test]
    fn render_imports_shows_count() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("3 imports"));
    }

    #[test]
    fn render_imports_singular_count() {
        let mut resp = sample_imports();
        resp.imports.truncate(1);
        let output = resp.render_human();
        assert!(output.contains("1 import"));
        assert!(!output.contains("imports")); // no plural
    }

    #[test]
    fn render_imports_shows_entries() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("src/foo.h"));
        assert!(output.contains("src/bar.h"));
        assert!(output.contains("external/lib.h"));
    }

    #[test]
    fn render_imports_shows_depth() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("depth=1"));
    }

    #[test]
    fn render_imports_shows_resolution() {
        let resp = sample_imports();
        let output = resp.render_human();
        assert!(output.contains("static"));
        assert!(output.contains("unresolved"));
    }

    #[test]
    fn render_empty_imports() {
        let resp = sample_empty_imports();
        let output = resp.render_human();
        assert!(output.contains("0 imports"));
        // No import lines after count
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3); // header, blank, count
    }

    fn sample_lg() -> LivegraphImportsResponse {
        LivegraphImportsResponse {
            display_name: "amodx".to_string(),
            file_filter: None,
            edges: vec![LgImportEdge {
                src_file: "a/src/x.ts".to_string(),
                dst_file: "a/src/y.ts".to_string(),
                basis: "AstImportFileInventoryResolved".to_string(),
                raw_specifier: Some("../y".to_string()),
            }],
            edge_count: 1,
            observations: vec![
                LgImportObservation {
                    source_file: "a/src/x.ts".to_string(),
                    raw_specifier: "react".to_string(),
                    class: "ExternalNonLocal".to_string(),
                    blocking: false,
                },
                LgImportObservation {
                    source_file: "a/src/x.ts".to_string(),
                    raw_specifier: "@scope/wslocal".to_string(),
                    class: "WorkspaceLocalUnedgeable".to_string(),
                    blocking: true,
                },
            ],
            observation_count: 2,
            blocking_observation_count: 1,
            observation_class_counts: BTreeMap::from([
                ("ExternalNonLocal".to_string(), 1),
                ("WorkspaceLocalUnedgeable".to_string(), 1),
            ]),
            module_cycle_completeness: "IncompleteImportClasses".to_string(),
            module_cycle_answer_class: "Exact".to_string(),
            freshness: "Fresh".to_string(),
            missing_partitions: vec![],
        }
    }

    #[test]
    fn lg_render_shows_edges_and_named_module_cycle_trust() {
        let out = sample_lg().render_human();
        assert!(out.contains("Imports (livegraph): amodx"));
        assert!(out.contains("[repo-wide]"));
        // module-cycle trust named after its source (not a bare completeness claim).
        assert!(out.contains("completeness=IncompleteImportClasses"));
        assert!(out.contains("answer_class=Exact"));
        assert!(out.contains("a/src/x.ts -> a/src/y.ts"));
        assert!(out.contains("AstImportFileInventoryResolved"));
        assert!(out.contains("by class:"));
    }

    #[test]
    fn lg_render_repo_wide_suppresses_benign_lists_blocking() {
        let out = sample_lg().render_human();
        // the BLOCKING workspace-local observation is listed individually.
        assert!(out.contains("@scope/wslocal"));
        assert!(out.contains("[WorkspaceLocalUnedgeable] BLOCKING"));
        // the benign external specifier is NOT listed individually in repo-wide mode (rule 4).
        assert!(
            !out.contains("react"),
            "benign external suppressed in repo-wide human output"
        );
    }

    #[test]
    fn lg_render_file_filtered_lists_benign_too() {
        let mut r = sample_lg();
        r.file_filter = Some("a/src/x.ts".to_string());
        let out = r.render_human();
        assert!(out.contains("[file=a/src/x.ts]"));
        // file-filtered -> the benign external is listed too.
        assert!(out.contains("react"));
        assert!(out.contains("[ExternalNonLocal] benign"));
    }

    #[test]
    fn compare_render_shows_sqlite_listing_and_summary() {
        let r = ImportsCompareResponse {
            file: "app/src/x.ts".to_string(),
            imports: vec![ImportEntry {
                symbol: "app/src/y.ts".to_string(),
                resolution: "static".to_string(),
                depth: 1,
                ..Default::default()
            }],
            comparison: ImportsComparison {
                status: "NoLossLivegraphSuperset".to_string(),
                matched: vec!["app/src/y.ts".to_string()],
                missing_in_livegraph: vec![],
                extra_livegraph_edges: vec![CompareExtraEdge {
                    dst_file: "app/src/z.ts".to_string(),
                    basis: "AstImportTsconfigPathResolved".to_string(),
                    raw_specifier: Some("@/z".to_string()),
                }],
                blocking_observations: vec![CompareBlockingObs {
                    raw_specifier: "@scope/wslocal".to_string(),
                    class: "WorkspaceLocalUnedgeable".to_string(),
                }],
                sqlite_resolved_local_count: 1,
                livegraph_edge_count: 2,
                precondition: Some(ComparePrecondition {
                    partition: "app".to_string(),
                    resident: true,
                    fresh: true,
                    ts_primary: true,
                    precondition_met: true,
                }),
            },
        };
        let out = r.render_human();
        // SQLite listing PRIMARY (default-compatible).
        assert!(out.contains("Imports: app/src/x.ts"));
        assert!(out.contains("1 import"));
        assert!(out.contains("app/src/y.ts"));
        // compare summary.
        assert!(out.contains("Compare (sqlite vs livegraph): NoLossLivegraphSuperset"));
        assert!(out.contains("precondition: partition=app"));
        assert!(out.contains("extra livegraph edges (improvements): 1"));
        assert!(out.contains("+ app/src/z.ts [AstImportTsconfigPathResolved]"));
        assert!(out.contains("blocking observations: 1"));
        assert!(out.contains("@scope/wslocal"));
    }

    #[test]
    fn compare_render_regression_is_loud() {
        let r = ImportsCompareResponse {
            file: "app/src/x.ts".to_string(),
            imports: vec![],
            comparison: ImportsComparison {
                status: "Regression".to_string(),
                missing_in_livegraph: vec!["app/src/lost.ts".to_string()],
                sqlite_resolved_local_count: 1,
                livegraph_edge_count: 0,
                precondition: Some(ComparePrecondition {
                    partition: "app".to_string(),
                    resident: true,
                    fresh: true,
                    ts_primary: true,
                    precondition_met: true,
                }),
                ..Default::default()
            },
        };
        let out = r.render_human();
        assert!(out.contains("Compare (sqlite vs livegraph): Regression"));
        assert!(out.contains("MISSING in livegraph (REGRESSIONS): 1"));
        assert!(out.contains("- app/src/lost.ts"));
    }
}
