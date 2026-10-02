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
    /// CPP-INCLUDE-BASENAME-1 (RG-REQ-002-L11): why the row is bound, as the daemon recorded it
    /// (`None` = no `reason` key).
    #[serde(default)]
    pub reason: Option<ImportEntryReason>,
}

/// CPP-INCLUDE-BASENAME-1: the decoded `reason` of an import row — the storage read's three
/// forms, plus any other shape (JSON this build did not produce), kept as found so the row still
/// renders and says the reason is unreadable.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ImportEntryReason {
    Recorded {
        basis: String,
        candidates: Vec<String>,
    },
    Unreadable {
        unreadable: String,
    },
    Missing {
        missing: String,
    },
    Other(serde_json::Value),
}

/// The one row form of an `imports <file>` listing, shared by the default and the compare
/// listings: `  <symbol>  depth=<d>  <resolution>`, where an `inferred` row's resolution states
/// its reason ([`inferred_row_state`]).
fn format_import_row(imp: &ImportEntry) -> String {
    let resolution = if imp.resolution == "inferred" {
        inferred_row_state(imp)
    } else if imp.resolution.is_empty() {
        "-".to_string()
    } else {
        imp.resolution.clone()
    };
    format!(
        "  {}  depth={}  {}
",
        imp.symbol, imp.depth, resolution
    )
}

/// CPP-INCLUDE-BASENAME-1 (RG-REQ-002-L11, RG-REQ-002-L04): what an inferred row says about its
/// reason. The renderer prints JSON it did not produce, so it re-checks the one agreement it prints
/// — a `unique_basename` reason with exactly one candidate equal to the row's own `file` — and
/// states every other reason as unreadable, never as evidence. A C/C++ inferred row without a
/// reason (its reason carrier was lost) says so with the remedy; any other row without a reason
/// prints `inferred`, as before.
fn inferred_row_state(imp: &ImportEntry) -> String {
    const MISSING: &str = "inferred (reason missing — re-index)";
    let unreadable = |what: &str| format!("inferred: reason unreadable ({what})");
    match &imp.reason {
        Some(ImportEntryReason::Recorded { basis, candidates }) => {
            if basis != "unique_basename" {
                return unreadable(&format!("basis {basis} on an inferred row"));
            }
            match candidates.as_slice() {
                [one] if *one == imp.file => format!("inferred: unique basename → {one}"),
                [one] => unreadable(&format!("candidate {one} is not the row's file")),
                many => unreadable(&format!(
                    "{} candidates under a unique basename",
                    many.len()
                )),
            }
        }
        Some(ImportEntryReason::Unreadable { unreadable: what }) => unreadable(what),
        Some(ImportEntryReason::Missing { .. }) => MISSING.to_string(),
        Some(ImportEntryReason::Other(value)) => {
            if value.is_object() && value.get("basis").is_none() {
                unreadable("the reason has no basis")
            } else {
                unreadable("a reason this build does not read")
            }
        }
        None if imp
            .evidence
            .iter()
            .any(|e| e.starts_with("c-core:") || e.starts_with("cpp-core:")) =>
        {
            MISSING.to_string()
        }
        None => "inferred".to_string(),
    }
}

/// Response structure for imports command.
#[derive(Debug, Deserialize)]
pub struct ImportsResponse {
    /// The file being queried.
    pub file: String,
    /// List of imports.
    pub imports: Vec<ImportEntry>,
    /// TEST-EDGE-SCOPE-1B (RG-REQ-002-L11): the view the listing answers (`None` = a daemon that
    /// predates the partition — the unavailable line, never a zero remainder).
    #[serde(default)]
    pub import_view: Option<serde_json::Value>,
    /// The imports the view left out (the inferred ones unless `--include-inferred`).
    #[serde(default)]
    pub import_remainder: Option<serde_json::Value>,
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
            self.push_partition_lines(&mut out);
            return out;
        }

        out.push('\n');

        // ── Import list ────────────────────────────────────────────
        // The symbol is the display name (usually the imported file path).
        for imp in &self.imports {
            out.push_str(&format_import_row(imp));
        }

        self.push_partition_lines(&mut out);
        out
    }

    /// TEST-EDGE-SCOPE-1B: the inferred imports not listed, with the flag that lists them (worded
    /// by `import_partition`). Nothing when nothing is left out.
    fn push_partition_lines(&self, out: &mut String) {
        use crate::presentation::import_partition as ip;
        let partition =
            match ip::partition_from(self.import_view.as_ref(), self.import_remainder.as_ref()) {
                ip::Partition::Stated(r) => ip::Partition::Stated(ip::per_file_remainder(&r)),
                other => other,
            };
        for line in ip::partition_lines_from(partition, ip::EdgeNoun::None) {
            out.push_str(&line);
            out.push('\n');
        }
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
                out.push_str(&format_import_row(imp));
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

// ── IMPORTS-LIVEGRAPH-REPOWIDE-READINESS-1: the repo-wide aggregate report (D6) ────────────────

/// The repo-wide readiness metrics (D3).
#[derive(Debug, Default, Deserialize)]
pub struct ReadinessMetrics {
    #[serde(default)]
    pub files_total: usize,
    #[serde(default)]
    pub files_precondition_met: usize,
    #[serde(default)]
    pub files_fallback_required: usize,
    #[serde(default)]
    pub files_regression: usize,
    #[serde(default)]
    pub missing_in_livegraph_total: usize,
    #[serde(default)]
    pub extra_livegraph_edges_total: usize,
    #[serde(default)]
    pub blocking_observation_total: usize,
    #[serde(default)]
    pub blocking_observation_by_class: BTreeMap<String, usize>,
    #[serde(default)]
    pub unknown_total: usize,
    #[serde(default)]
    pub sqlite_import_bearing_files: usize,
    #[serde(default)]
    pub livegraph_import_bearing_files: usize,
    #[serde(default)]
    pub fallback_share: f64,
    #[serde(default)]
    pub fallback_heavy: bool,
}

/// A per-file regression in the report (a SQLite resolved-local import LiveGraph lost).
#[derive(Debug, Deserialize)]
pub struct ReadinessRegression {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub missing: Vec<String>,
}

/// An ambiguous SQLite import the harness could not classify.
#[derive(Debug, Deserialize)]
pub struct ReadinessUnknown {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub resolution: String,
}

/// The `imports --engine compare` NO-FILE (repo-wide) aggregate readiness report.
#[derive(Debug, Deserialize)]
pub struct ImportsReadinessReport {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub coverage_complete: bool,
    #[serde(default)]
    pub metrics: ReadinessMetrics,
    #[serde(default)]
    pub regressions: Vec<ReadinessRegression>,
    #[serde(default)]
    pub unknowns: Vec<ReadinessUnknown>,
}

impl ImportsReadinessReport {
    /// Render the repo-wide readiness summary (the verdict + the D3 metrics; regressions / unknowns are LOUD).
    pub fn render_human(&self) -> String {
        let m = &self.metrics;
        let mut out = String::new();
        out.push_str(&format!(
            "Imports readiness (repo-wide): {}\n",
            self.display_name
        ));
        out.push_str(&format!("VERDICT: {}\n\n", self.verdict));
        out.push_str(&format!("  files_total            = {}\n", m.files_total));
        out.push_str(&format!(
            "  precondition_met       = {}\n",
            m.files_precondition_met
        ));
        out.push_str(&format!(
            "  fallback_required      = {}  (share {:.1}%{})\n",
            m.files_fallback_required,
            m.fallback_share * 100.0,
            if m.fallback_heavy {
                " FALLBACK-HEAVY"
            } else {
                ""
            }
        ));
        out.push_str(&format!(
            "  REGRESSIONS            = {}\n",
            m.files_regression
        ));
        out.push_str(&format!("  unknown                = {}\n", m.unknown_total));
        out.push_str(&format!(
            "  missing_in_livegraph   = {}\n",
            m.missing_in_livegraph_total
        ));
        out.push_str(&format!(
            "  extra_livegraph_edges  = {}\n",
            m.extra_livegraph_edges_total
        ));
        out.push_str(&format!(
            "  blocking_observations  = {}\n",
            m.blocking_observation_total
        ));
        if !m.blocking_observation_by_class.is_empty() {
            let by: Vec<String> = m
                .blocking_observation_by_class
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            out.push_str(&format!("    by class: {}\n", by.join("  ")));
        }
        out.push_str(&format!(
            "  import-bearing files (sqlite / livegraph) = {} / {}\n",
            m.sqlite_import_bearing_files, m.livegraph_import_bearing_files
        ));
        out.push_str(&format!(
            "  coverage_complete      = {}\n",
            self.coverage_complete
        ));
        if !self.regressions.is_empty() {
            out.push_str("\n  REGRESSION DETAIL (SQLite resolved-local imports LiveGraph lost):\n");
            for r in &self.regressions {
                out.push_str(&format!("    {} -> missing {:?}\n", r.file, r.missing));
            }
        }
        if !self.unknowns.is_empty() {
            out.push_str("\n  UNKNOWN DETAIL (ambiguous SQLite imports):\n");
            for u in &self.unknowns {
                out.push_str(&format!(
                    "    {} -> {} [{}]\n",
                    u.file, u.target, u.resolution
                ));
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
                    reason: None,
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
                    reason: None,
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
                    reason: None,
                },
            ],
            import_view: Some(default_view()),
            import_remainder: Some(zero_remainder()),
        }
    }

    /// The view a partitioned daemon states for a default request.
    fn default_view() -> serde_json::Value {
        serde_json::json!({"include_tests": false, "include_inferred": false})
    }

    /// A current daemon's zero remainder (D-TESB-17 fixture rule).
    fn zero_remainder() -> serde_json::Value {
        serde_json::json!({
            "tests": {"imports": 0, "edges": 0},
            "inferred": {"imports": 0, "edges": 0},
            "tests_and_inferred": {"imports": 0, "edges": 0}
        })
    }

    fn sample_empty_imports() -> ImportsResponse {
        ImportsResponse {
            file: "src/standalone.cpp".to_string(),
            imports: vec![],
            import_view: Some(default_view()),
            import_remainder: Some(zero_remainder()),
        }
    }

    #[test]
    fn imports_renders_certain_rows_and_the_inferred_remainder() {
        // RG-REQ-002-L11 (§2.4 kafka): the default lists the certain rows and states the inferred
        // ones with the flag that lists them; with the flag the rows carry `inferred`.
        let resp: ImportsResponse = serde_json::from_value(serde_json::json!({
            "file": "tests/kafkatest/services/streams.py",
            "imports": [{"symbol": "tests/kafkatest/services/monitor/jmx.py", "resolution": "static", "depth": 1}],
            "count": 1,
            "import_view": {"include_tests": false, "include_inferred": false},
            "import_remainder": {
                "tests": {"imports": 0, "edges": 0},
                "inferred": {"imports": 2, "edges": 0},
                "tests_and_inferred": {"imports": 0, "edges": 0}
            }
        }))
        .unwrap();
        let out = resp.render_human();
        assert!(out.contains("1 import\n"), "{out}");
        assert!(
            out.contains("  tests/kafkatest/services/monitor/jmx.py  depth=1  static"),
            "{out}"
        );
        assert!(
            out.contains("+2 inferred imports, not shown — --include-inferred"),
            "{out}"
        );
        assert!(!out.contains("streams_property.py"), "{out}");

        let with: ImportsResponse = serde_json::from_value(serde_json::json!({
            "file": "tests/kafkatest/services/streams.py",
            "imports": [
                {"symbol": "tests/kafkatest/services/streams_property.py", "resolution": "inferred", "depth": 1},
                {"symbol": "tests/kafkatest/services/verifiable_consumer.py", "resolution": "inferred", "depth": 1}
            ],
            "import_view": {"include_tests": false, "include_inferred": true},
            "import_remainder": {
                "tests": {"imports": 0, "edges": 0},
                "inferred": {"imports": 0, "edges": 0},
                "tests_and_inferred": {"imports": 0, "edges": 0}
            }
        }))
        .unwrap();
        let out = with.render_human();
        assert!(
            out.contains("streams_property.py  depth=1  inferred"),
            "{out}"
        );
        assert!(!out.contains("not shown"), "{out}");

        // An older daemon (no view): the unavailable line, never a zero remainder.
        let old: ImportsResponse = serde_json::from_value(serde_json::json!({
            "file": "a.py", "imports": []
        }))
        .unwrap();
        assert!(old
            .render_human()
            .contains(crate::presentation::import_partition::PARTITION_UNAVAILABLE));
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

    #[test]
    fn readiness_report_renders_verdict_and_metrics() {
        let json = serde_json::json!({
            "display_name": "amodx",
            "verdict": "GREEN",
            "coverage_complete": true,
            "metrics": {
                "files_total": 100, "files_precondition_met": 100, "files_fallback_required": 0,
                "files_regression": 0, "missing_in_livegraph_total": 0, "extra_livegraph_edges_total": 50,
                "blocking_observation_total": 5,
                "blocking_observation_by_class": {"WorkspaceLocalUnedgeable": 5},
                "unknown_total": 0, "sqlite_import_bearing_files": 30,
                "livegraph_import_bearing_files": 100, "fallback_share": 0.0, "fallback_heavy": false
            },
            "regressions": [], "unknowns": []
        });
        let r: ImportsReadinessReport = serde_json::from_value(json).unwrap();
        let out = r.render_human();
        assert!(out.contains("Imports readiness (repo-wide): amodx"));
        assert!(out.contains("VERDICT: GREEN"));
        assert!(out.contains("files_total            = 100"));
        assert!(out.contains("REGRESSIONS            = 0"));
        assert!(out.contains("WorkspaceLocalUnedgeable=5"));
    }

    #[test]
    fn readiness_report_regression_detail_is_loud() {
        let json = serde_json::json!({
            "display_name": "x", "verdict": "RED", "coverage_complete": true,
            "metrics": { "files_regression": 1, "missing_in_livegraph_total": 1 },
            "regressions": [{"file": "a.ts", "missing": ["b.ts"]}],
            "unknowns": []
        });
        let r: ImportsReadinessReport = serde_json::from_value(json).unwrap();
        let out = r.render_human();
        assert!(out.contains("VERDICT: RED"));
        assert!(out.contains("REGRESSION DETAIL"));
        assert!(out.contains("a.ts -> missing [\"b.ts\"]"));
    }

    // ── TEST-EDGE-SCOPE-1B: D-TESB-17 (rows U9, W1) on `imports` ──

    #[test]
    fn imports_partial_partition_payload_renders_unreadable_never_nothing_excluded() {
        let mut resp = sample_imports();
        resp.import_remainder = None;
        let out = resp.render_human();
        assert!(
            out.contains(crate::presentation::import_partition::PARTITION_UNREADABLE),
            "{out}"
        );
        assert!(!out.contains("not shown"), "{out}");
    }

    #[test]
    fn imports_wrong_typed_remainder_renders_unreadable_never_zero() {
        for bad in [
            serde_json::json!([]),
            serde_json::json!({"tests": {"imports": 0, "edges": 0},
                               "inferred": {"imports": "2", "edges": 0},
                               "tests_and_inferred": {"imports": 0, "edges": 0}}),
        ] {
            let mut resp = sample_imports();
            resp.import_remainder = Some(bad.clone());
            let out = resp.render_human();
            assert!(
                out.contains(crate::presentation::import_partition::PARTITION_UNREADABLE),
                "{bad}: {out}"
            );
            assert!(!out.contains("not shown"), "{out}");
        }
    }

    // ── CPP-INCLUDE-BASENAME-1: the reason on an inferred row (D-CIB-REASON-1) ──────────

    /// A row as the daemon serializes it.
    fn row_json(
        path: &str,
        resolution: &str,
        evidence: &str,
        reason: Option<serde_json::Value>,
    ) -> serde_json::Value {
        let mut row = serde_json::json!({
            "node_id": "n", "symbol": path, "kind": "FILE", "file": path,
            "edge_type": "IMPORTS", "resolution": resolution,
            "evidence": if evidence.is_empty() { vec![] } else { vec![evidence] }, "depth": 1
        });
        if let Some(reason) = reason {
            row["reason"] = reason;
        }
        row
    }

    /// An `--include-inferred` listing of `src/core/ngx_config.h` over the given rows.
    fn listing_with_inferred(rows: Vec<serde_json::Value>) -> ImportsResponse {
        serde_json::from_value(serde_json::json!({
            "file": "src/core/ngx_config.h",
            "imports": rows,
            "import_view": {"include_tests": false, "include_inferred": true},
            "import_remainder": zero_remainder(),
        }))
        .unwrap()
    }

    /// The single row line of a one-row listing.
    fn only_row(resp: &ImportsResponse) -> String {
        let out = resp.render_human();
        let rows: Vec<&str> = out.lines().filter(|l| l.starts_with("  ")).collect();
        assert_eq!(rows.len(), 1, "{out}");
        rows[0].to_string()
    }

    const LINUX: &str = "src/os/unix/ngx_linux_config.h";

    #[test]
    fn inferred_unique_basename_row_renders_its_reason_and_candidate() {
        let resp = listing_with_inferred(vec![row_json(
            LINUX,
            "inferred",
            "c-core:0.1.0",
            Some(serde_json::json!({"basis": "unique_basename", "candidates": [LINUX]})),
        )]);
        assert_eq!(
            resp.render_human(),
            format!(
                "Imports: src/core/ngx_config.h\n\n1 import\n\n  {LINUX}  depth=1  inferred: unique basename → {LINUX}\n"
            )
        );
    }

    #[test]
    fn inferred_row_with_an_unreadable_reason_says_so_never_bare_inferred() {
        let resp = listing_with_inferred(vec![row_json(
            LINUX,
            "inferred",
            "c-core:0.1.0",
            Some(serde_json::json!({"unreadable": "candidates is empty"})),
        )]);
        assert_eq!(
            only_row(&resp),
            format!("  {LINUX}  depth=1  inferred: reason unreadable (candidates is empty)")
        );
    }

    #[test]
    fn inferred_row_whose_reason_disagrees_with_the_row_is_unreadable_never_evidence() {
        // JSON this build did not produce: the renderer re-checks the one agreement it prints.
        let disagreements = [
            serde_json::json!({"basis": "unique_basename", "candidates": ["src/os/win32/ngx_win32_config.h"]}),
            serde_json::json!({"basis": "unique_basename", "candidates": [LINUX, "src/x.h"]}),
            serde_json::json!({"basis": "unique_basename", "candidates": []}),
            serde_json::json!({"basis": "unique_suffix", "candidates": [LINUX]}),
            serde_json::json!({"basis": "python_submodule", "candidates": [LINUX]}),
            serde_json::json!({"candidates": [LINUX]}),
            serde_json::json!("unique_basename"),
        ];
        for reason in disagreements {
            let resp = listing_with_inferred(vec![row_json(
                LINUX,
                "inferred",
                "c-core:0.1.0",
                Some(reason.clone()),
            )]);
            let line = only_row(&resp);
            let prefix = format!("  {LINUX}  depth=1  inferred: reason unreadable (");
            assert!(
                line.starts_with(&prefix) && line.ends_with(')'),
                "{reason}: {line}"
            );
            assert!(
                !line.contains('→'),
                "{reason}: never printed as evidence: {line}"
            );
        }
    }

    #[test]
    fn row_without_a_rendered_reason_prints_exactly_as_before() {
        // A static row (its reason stays in JSON), another producer's inferred row without a
        // reason, and a row whose evidence names no producer.
        let resp = listing_with_inferred(vec![
            row_json(
                "lib/src/util/foo.h",
                "static",
                "cpp-core:0.2.0",
                Some(
                    serde_json::json!({"basis": "unique_suffix", "candidates": ["lib/src/util/foo.h"]}),
                ),
            ),
            row_json("pkg/sub.py", "inferred", "python-core:0.2.0", None),
            row_json("src/x.h", "inferred", "", None),
            row_json("src/y.h", "static", "c-core:0.1.0", None),
        ]);
        assert_eq!(
            resp.render_human(),
            "Imports: src/core/ngx_config.h\n\n4 imports\n\n  \
             lib/src/util/foo.h  depth=1  static\n  \
             pkg/sub.py  depth=1  inferred\n  \
             src/x.h  depth=1  inferred\n  \
             src/y.h  depth=1  static\n"
        );
    }

    #[test]
    fn compare_listing_prints_the_same_rows_as_the_default_listing() {
        let rows = vec![
            row_json(
                LINUX,
                "inferred",
                "c-core:0.1.0",
                Some(serde_json::json!({"basis": "unique_basename", "candidates": [LINUX]})),
            ),
            row_json("src/core/ngx_core.h", "inferred", "c-core:0.1.0", None),
            row_json("src/event/ngx_event.h", "static", "c-core:0.1.0", None),
        ];
        let default = listing_with_inferred(rows.clone());
        let compare: ImportsCompareResponse = serde_json::from_value(serde_json::json!({
            "file": "src/core/ngx_config.h",
            "imports": rows,
            "comparison": {"status": "SqliteFallback"},
        }))
        .unwrap();
        let row_lines = |out: String| -> Vec<String> {
            out.lines()
                .filter(|l| l.starts_with("  ") && l.contains("depth="))
                .map(str::to_string)
                .collect()
        };
        let d = row_lines(default.render_human());
        assert_eq!(d.len(), 3);
        assert_eq!(d, row_lines(compare.render_human()));
    }

    #[test]
    fn import_entry_decodes_a_reason_its_unreadable_form_its_missing_form_and_its_absence() {
        let decode = |reason: Option<serde_json::Value>| -> ImportEntry {
            serde_json::from_value(row_json(LINUX, "inferred", "c-core:0.1.0", reason)).unwrap()
        };
        assert_eq!(
            decode(Some(
                serde_json::json!({"basis": "unique_basename", "candidates": [LINUX]})
            ))
            .reason,
            Some(ImportEntryReason::Recorded {
                basis: "unique_basename".into(),
                candidates: vec![LINUX.into()],
            })
        );
        assert_eq!(
            decode(Some(
                serde_json::json!({"unreadable": "basis is not a string"})
            ))
            .reason,
            Some(ImportEntryReason::Unreadable {
                unreadable: "basis is not a string".into()
            })
        );
        assert_eq!(
            decode(Some(serde_json::json!({"missing": "no reason carrier"}))).reason,
            Some(ImportEntryReason::Missing {
                missing: "no reason carrier".into()
            })
        );
        assert_eq!(decode(None).reason, None);
    }

    #[test]
    fn inferred_row_whose_reason_is_missing_for_a_null_carrier_says_re_index() {
        let resp = listing_with_inferred(vec![row_json(
            LINUX,
            "inferred",
            "c-core:0.1.0",
            Some(serde_json::json!({"missing": "no reason carrier"})),
        )]);
        assert_eq!(
            only_row(&resp),
            format!("  {LINUX}  depth=1  inferred (reason missing — re-index)")
        );
    }

    #[test]
    fn inferred_row_whose_reason_is_missing_for_a_carrier_without_basis_says_re_index() {
        let resp = listing_with_inferred(vec![row_json(
            LINUX,
            "inferred",
            "cpp-core:0.2.0",
            Some(serde_json::json!({"missing": "reason carrier has no basis"})),
        )]);
        assert_eq!(
            only_row(&resp),
            format!("  {LINUX}  depth=1  inferred (reason missing — re-index)")
        );
    }

    #[test]
    fn c_family_inferred_row_without_a_reason_key_says_reason_missing_never_bare_inferred() {
        for evidence in ["c-core:0.1.0", "cpp-core:0.2.0"] {
            let resp = listing_with_inferred(vec![row_json(LINUX, "inferred", evidence, None)]);
            assert_eq!(
                only_row(&resp),
                format!("  {LINUX}  depth=1  inferred (reason missing — re-index)"),
                "{evidence}"
            );
        }
    }
}
