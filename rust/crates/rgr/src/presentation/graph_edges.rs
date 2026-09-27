//! Presentation layer for graph edge queries (callers, callees).
//!
//! # CLI-OUT-3
//!
//! Shared support module for callers and callees commands.
//! These commands have identical response structures and change for the same reasons,
//! so they share rendering logic with thin command-specific wrappers.
//!
//! ## Human Output Structure
//!
//! ```text
//! Callers of OpenXcom::State::State
//! File: src/Engine/State.cpp:51
//!
//! 5 callers found
//!
//!   OpenXcom::Game::run          src/Engine/Game.cpp:234     CALLS  static
//!   OpenXcom::Menu::init         src/Menu/Menu.cpp:45        CALLS  static
//!   ...
//! ```

use std::collections::BTreeMap;

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Target symbol information in callers/callees response.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetSymbol {
    #[serde(default)]
    pub stable_key: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub subtype: Option<String>,
    pub file: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
}

/// Edge symbol (caller or callee).
///
/// RECON-M-R2 — the presentation ACCEPTS UNKNOWN (recon-design-1 §3.7-4): `file`/`line`/`column`
/// are `Option` so a null/absent location (an S-minted union row — the LiveGraph carries no
/// definition locations) parses and renders as unknown instead of failing or inventing `:0`.
/// Every value served today (`Some(file)` + `Some(line)`, including the legacy `""`/`0`
/// placeholders) renders byte-identically; only genuinely-unknown values render the new forms.
/// `witness`/`occurrences` are the flag-gated union provenance fields (absent everywhere else).
#[derive(Debug, Clone, Deserialize)]
pub struct EdgeSymbol {
    #[serde(default)]
    pub stable_key: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub column: Option<u32>,
    #[serde(default)]
    pub edge_type: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    /// Union witness class (`both` | `semantic` | `syntactic` | `mixed`) — present ONLY on
    /// dual-measured rows of a flag-ON union answer (recon-design-1 §5.2). Absent = no claim.
    #[serde(default)]
    pub witness: Option<String>,
    /// Exact occurrence corroboration on a `mixed` row: `confirmed` of `total`.
    #[serde(default)]
    pub occurrences: Option<Occurrences>,
    /// PYTHON-RECEIVER-BINDING-1: on an `inferred` row, its recorded basis
    /// (`receiver_untyped_name_only`) or `unrecorded`. Absent on certain rows.
    #[serde(default)]
    pub inference_basis: Option<String>,
    /// PYTHON-RECEIVER-BINDING-1: on an `inferred` row, its writer — routing metadata, never
    /// rendered; it decides only whether an `unrecorded` row is the verified older promoter.
    #[serde(default)]
    pub inference_extractor: Option<String>,
}

/// PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11): the inferred remainder of a callers/callees
/// answer — rows bound only by an inferred binding, not in `count` by default.
#[derive(Debug, Clone, Deserialize)]
pub struct InferredRemainder {
    pub count: u64,
    pub by_basis: BTreeMap<String, u64>,
}

/// PYTHON-RECEIVER-BINDING-1 (RG-REQ-005-L09): the unresolved calls naming the symbol (`name`
/// set, callers) or leaving it (callees), per classifier basis code. `sites` are JSON routing
/// data (never rendered by the human line).
#[derive(Debug, Clone, Deserialize)]
pub struct UnresolvedRemainder {
    #[serde(default)]
    pub name: Option<String>,
    pub count: u64,
    pub by_basis: BTreeMap<String, u64>,
}

/// RECON-M-R2: a `mixed` row's exact occurrence corroboration (`confirmed` of `total` — the
/// R-RAT-5 never-claim-unconfirmed summary).
#[derive(Debug, Clone, Deserialize)]
pub struct Occurrences {
    pub confirmed: u64,
    pub total: u64,
}

/// RECON-M-R2: the union answer's instance-class composition (1:1 with the row multiset, §5.2).
/// Present only on flag-ON union answers; its presence is what turns on the witness rendering.
#[derive(Debug, Clone, Deserialize)]
pub struct WitnessCounts {
    pub both: u64,
    pub semantic_only: u64,
    pub syntactic_only: u64,
    pub unmeasured: u64,
}

/// Response structure for callers command.
#[derive(Debug, Deserialize)]
pub struct CallersResponse {
    pub target: TargetSymbol,
    pub callers: Vec<EdgeSymbol>,
    pub count: usize,
    /// RECON-M-R2 (additive; union answers only).
    #[serde(default)]
    pub witness_counts: Option<WitnessCounts>,
    /// PYTHON-RECEIVER-BINDING-1: absent = no inferred row (or an older daemon) — never a zero.
    #[serde(default)]
    pub inferred: Option<InferredRemainder>,
    /// PYTHON-RECEIVER-BINDING-1: absent = no unresolved call bears on the symbol.
    #[serde(default)]
    pub unresolved_naming: Option<UnresolvedRemainder>,
}

/// Response structure for callees command.
#[derive(Debug, Deserialize)]
pub struct CalleesResponse {
    pub target: TargetSymbol,
    pub callees: Vec<EdgeSymbol>,
    pub count: usize,
    /// RECON-M-R2 (additive; union answers only).
    #[serde(default)]
    pub witness_counts: Option<WitnessCounts>,
    /// PYTHON-RECEIVER-BINDING-1: absent = no inferred row (or an older daemon) — never a zero.
    #[serde(default)]
    pub inferred: Option<InferredRemainder>,
    /// PYTHON-RECEIVER-BINDING-1: absent = no unresolved call bears on the symbol.
    #[serde(default)]
    pub unresolved_from: Option<UnresolvedRemainder>,
}

// ── Direction for shared rendering ───────────────────────────────────────────

/// Direction label for shared renderer.
#[derive(Debug, Clone, Copy)]
pub enum EdgeDirection {
    Callers,
    Callees,
}

impl EdgeDirection {
    fn label(&self) -> &'static str {
        match self {
            Self::Callers => "Callers",
            Self::Callees => "Callees",
        }
    }

    fn count_label(&self, count: usize) -> String {
        let noun = match self {
            Self::Callers => "caller",
            Self::Callees => "callee",
        };
        if count == 1 {
            format!("1 {} found", noun)
        } else {
            format!("{} {}s found", count, noun)
        }
    }
}

// ── Remainder wording (PYTHON-RECEIVER-BINDING-1) ────────────────────────────

/// The recorded basis of a name-only binding.
const NAME_ONLY_BASIS: &str = repo_graph_agent::reliability::NAME_ONLY_BASIS;
/// The verified older writer of compiler-promoted rows labelled `inferred` (D-PRB-ENRICH-1 as
/// corrected): its unrecorded rows name the rebuild remedy; every other unrecorded row is
/// `basis unknown`.
const LEGACY_PROMOTER: &str = "compiler-promotion:0.1.0";

/// Every inferred row's basis is the name-only binding (the "(name-only)" rule).
pub(crate) fn all_name_only(by_basis: &BTreeMap<String, u64>) -> bool {
    !by_basis.is_empty() && by_basis.keys().all(|b| b == NAME_ONLY_BASIS)
}

/// The end-of-row marker of one inferred row, in the reader's words (RG-REQ-002-L07/L08): the
/// name-only binding, the verified older promoter (with the runnable rebuild remedy), or — for any
/// other unrecorded row — `basis unknown`. The writer never renders.
pub(crate) fn inferred_row_marker(
    basis: Option<&str>,
    extractor: Option<&str>,
    repo: Option<&str>,
) -> String {
    match basis {
        Some(NAME_ONLY_BASIS) => " (inferred: name-only binding — investigate)".to_string(),
        Some("unrecorded") if extractor == Some(LEGACY_PROMOTER) => format!(
            " (inferred: compiler-resolved call recorded by an older rmap — run rmap repo rebuild {})",
            repo.map(super::shell_quote_arg)
                .unwrap_or_else(|| "<path>".to_string())
        ),
        _ => " (inferred: basis unknown — investigate)".to_string(),
    }
}

/// `"(<reader label>: <n>, …)"` over basis-code counts — the labels and order of the shared
/// attribution breakdown; an unrecognized code folds into `unrecognized: <n>` (never dropped,
/// never the raw code).
pub(crate) fn reader_class_counts(by_basis: &BTreeMap<String, u64>) -> String {
    let breakdown = repo_graph_agent::attribution::attribution_breakdown(
        by_basis.iter().map(|(k, v)| (k.as_str(), *v)),
    );
    let mut parts: Vec<String> = breakdown
        .classes
        .iter()
        .map(|(class, n)| format!("{}: {}", class.reader_label(), n))
        .collect();
    if breakdown.other > 0 {
        parts.push(format!("unrecognized: {}", breakdown.other));
    }
    format!("({})", parts.join(", "))
}

/// RG-REQ-005-L09: the unresolved-calls line — `"<U> unresolved call(s) name(s) `<name>` (…)"`
/// (callers) or `"<U> unresolved call(s) from this symbol (…)"` (callees).
pub(crate) fn unresolved_line(
    count: u64,
    name: Option<&str>,
    by_basis: &BTreeMap<String, u64>,
) -> String {
    let calls = if count == 1 { "call" } else { "calls" };
    let classes = reader_class_counts(by_basis);
    match name {
        Some(n) => {
            let verb = if count == 1 { "names" } else { "name" };
            format!("{count} unresolved {calls} {verb} `{n}` {classes}")
        }
        None => format!("{count} unresolved {calls} from this symbol {classes}"),
    }
}

// ── Human Rendering ──────────────────────────────────────────────────────────

/// Render one edge's location, honestly (RECON-M-R2 — unknown is never rendered as zero):
/// known file + known line → today's exact `file:line` (including the legacy `""`/`0`
/// placeholder rows, which keep rendering `:0` byte-identically); known file + unknown line →
/// the file alone (never an invented `:0`); unknown file → `-` (the existing absent-value
/// convention used for edge_type/resolution).
fn render_location(edge: &EdgeSymbol) -> String {
    match (&edge.file, edge.line) {
        (Some(f), Some(l)) => format!("{}:{}", f, l),
        (Some(f), None) => f.clone(),
        (None, _) => "-".to_string(),
    }
}

/// The compact per-row witness marker (recon-design-1 §5.2 — reader labels, §3.1): rendered ONLY
/// when the answer carries witness data. `mixed` renders its exact corroborated fraction
/// ("confirmed by both analyses — N of M occurrences"). A row with NO witness field makes no
/// claim (unmeasured) and renders no marker.
fn render_witness_marker(edge: &EdgeSymbol) -> String {
    match edge.witness.as_deref() {
        Some("both") => "  [both]".to_string(),
        Some("semantic") => "  [compiler-only]".to_string(),
        Some("syntactic") => "  [syntax-only]".to_string(),
        Some("mixed") => match &edge.occurrences {
            Some(o) => format!("  [both {}/{}]", o.confirmed, o.total),
            None => "  [both n/m]".to_string(),
        },
        // Forward-honest: an unknown class renders verbatim rather than being dropped.
        Some(other) => format!("  [{}]", other),
        None => String::new(),
    }
}

/// PYTHON-RECEIVER-BINDING-1: what a callers/callees answer states beside its certain rows.
/// Every field `None` renders byte-identically to before (an absent field is never a zero).
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeRemainders<'a> {
    pub inferred: Option<&'a InferredRemainder>,
    pub unresolved: Option<&'a UnresolvedRemainder>,
    /// The repository path, for the rebuild remedy of a legacy promoted row.
    pub repo: Option<&'a str>,
}

/// Render graph edge response as human-readable text.
///
/// Shared implementation for both callers and callees. `witness_counts` (RECON-M-R2) is `Some`
/// ONLY on flag-ON union answers — its presence adds ONE section line + the per-row markers;
/// absent, the output is byte-identical to the pre-M-R2 renderer (data-driven, R-0/R-1).
///
/// PYTHON-RECEIVER-BINDING-1: by default the count line names the inferred remainder with the flag
/// that lists it (`; M inferred (name-only) — investigate with --include-inferred`); when the rows
/// include inferred ones (the flag was given) it reads `N callers found; M of them inferred
/// (name-only) — marked below` and each inferred row ends with its marker. The unresolved calls
/// bearing on the symbol follow on their own line.
pub fn render_graph_edges(
    direction: EdgeDirection,
    target: &TargetSymbol,
    edges: &[EdgeSymbol],
    count: usize,
    witness_counts: Option<&WitnessCounts>,
    remainders: EdgeRemainders<'_>,
) -> String {
    let mut out = String::new();

    // ── Header ─────────────────────────────────────────────────
    let target_name = target.qualified_name.as_deref().unwrap_or(&target.name);
    out.push_str(&format!("{} of {}\n", direction.label(), target_name));
    out.push_str(&format!("File: {}:{}\n\n", target.file, target.line));

    // ── Count ──────────────────────────────────────────────────
    out.push_str(&direction.count_label(count));
    let listed_inferred = edges
        .iter()
        .filter(|e| e.resolution.as_deref() == Some("inferred"))
        .count() as u64;
    if let Some(inf) = remainders.inferred {
        let label = if all_name_only(&inf.by_basis) {
            "inferred (name-only)"
        } else {
            "inferred"
        };
        if listed_inferred > 0 {
            out.push_str(&format!("; {} of them {label} — marked below", inf.count));
        } else {
            out.push_str(&format!(
                "; {} {label} — investigate with --include-inferred",
                inf.count
            ));
        }
    }
    out.push('\n');
    if let Some(u) = remainders.unresolved {
        out.push_str(&unresolved_line(u.count, u.name.as_deref(), &u.by_basis));
        out.push('\n');
    }

    // ── Witness section line (union answers only — recon-design-1 §5.2; the one-time
    //    "call sites are syntax-detected" clarification lives HERE, never per-row) ──
    if let Some(w) = witness_counts {
        out.push_str(&format!(
            "{} confirmed by both analyses · {} compiler-only · {} syntax-only · {} not measured \
             by the compiler — call sites are syntax-detected\n",
            w.both, w.semantic_only, w.syntactic_only, w.unmeasured
        ));
    }

    if edges.is_empty() {
        return out;
    }

    out.push('\n');

    // ── Edge list ──────────────────────────────────────────────
    let render_markers = witness_counts.is_some();
    for edge in edges {
        let name = edge.qualified_name.as_deref().unwrap_or(&edge.name);
        let location = render_location(edge);
        let edge_type = edge.edge_type.as_deref().unwrap_or("-");
        let resolution = edge.resolution.as_deref().unwrap_or("-");
        let mut marker = if render_markers {
            render_witness_marker(edge)
        } else {
            String::new()
        };
        if edge.resolution.as_deref() == Some("inferred") {
            marker.push_str(&inferred_row_marker(
                edge.inference_basis.as_deref(),
                edge.inference_extractor.as_deref(),
                remainders.repo,
            ));
        }

        out.push_str(&format!(
            "  {}  {}  {}  {}{}\n",
            name, location, edge_type, resolution, marker
        ));
    }

    out
}

impl CallersResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        self.render_human_for_repo(None)
    }

    /// Render as human-readable text; `repo` names the repository in a rebuild remedy.
    pub fn render_human_for_repo(&self, repo: Option<&str>) -> String {
        render_graph_edges(
            EdgeDirection::Callers,
            &self.target,
            &self.callers,
            self.count,
            self.witness_counts.as_ref(),
            EdgeRemainders {
                inferred: self.inferred.as_ref(),
                unresolved: self.unresolved_naming.as_ref(),
                repo,
            },
        )
    }
}

impl CalleesResponse {
    /// Render as human-readable text.
    pub fn render_human(&self) -> String {
        self.render_human_for_repo(None)
    }

    /// Render as human-readable text; `repo` names the repository in a rebuild remedy.
    pub fn render_human_for_repo(&self, repo: Option<&str>) -> String {
        render_graph_edges(
            EdgeDirection::Callees,
            &self.target,
            &self.callees,
            self.count,
            self.witness_counts.as_ref(),
            EdgeRemainders {
                inferred: self.inferred.as_ref(),
                unresolved: self.unresolved_from.as_ref(),
                repo,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> TargetSymbol {
        TargetSymbol {
            stable_key: "repo_123:src/foo.cpp#Foo::bar:SYMBOL:METHOD".to_string(),
            name: "bar".to_string(),
            qualified_name: Some("Foo::bar".to_string()),
            kind: "SYMBOL".to_string(),
            subtype: Some("METHOD".to_string()),
            file: "src/foo.cpp".to_string(),
            line: 42,
            column: 0,
        }
    }

    fn sample_edges() -> Vec<EdgeSymbol> {
        vec![
            EdgeSymbol {
                inference_basis: None,
                inference_extractor: None,
                stable_key: "repo_123:src/main.cpp#main:SYMBOL:FUNCTION".to_string(),
                name: "main".to_string(),
                qualified_name: Some("main".to_string()),
                kind: "SYMBOL".to_string(),
                subtype: Some("FUNCTION".to_string()),
                file: Some("src/main.cpp".to_string()),
                line: Some(10),
                column: Some(0),
                edge_type: Some("CALLS".to_string()),
                resolution: Some("static".to_string()),
                witness: None,
                occurrences: None,
            },
            EdgeSymbol {
                inference_basis: None,
                inference_extractor: None,
                stable_key: "repo_123:src/helper.cpp#Helper::run:SYMBOL:METHOD".to_string(),
                name: "run".to_string(),
                qualified_name: Some("Helper::run".to_string()),
                kind: "SYMBOL".to_string(),
                subtype: Some("METHOD".to_string()),
                file: Some("src/helper.cpp".to_string()),
                line: Some(55),
                column: Some(0),
                edge_type: Some("CALLS".to_string()),
                resolution: Some("static".to_string()),
                witness: None,
                occurrences: None,
            },
        ]
    }

    #[test]
    fn render_callers_includes_header() {
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: sample_edges(),
            count: 2,
            witness_counts: None,
        };
        let output = resp.render_human();
        assert!(output.contains("Callers of Foo::bar"));
        assert!(output.contains("File: src/foo.cpp:42"));
    }

    #[test]
    fn render_callers_includes_count() {
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: sample_edges(),
            count: 2,
            witness_counts: None,
        };
        let output = resp.render_human();
        assert!(output.contains("2 callers found"));
    }

    #[test]
    fn render_callers_singular_count() {
        let mut edges = sample_edges();
        edges.pop();
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: edges,
            count: 1,
            witness_counts: None,
        };
        let output = resp.render_human();
        assert!(output.contains("1 caller found"));
    }

    #[test]
    fn render_callers_includes_edges() {
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: sample_edges(),
            count: 2,
            witness_counts: None,
        };
        let output = resp.render_human();
        assert!(output.contains("main"));
        assert!(output.contains("src/main.cpp:10"));
        assert!(output.contains("CALLS"));
        assert!(output.contains("Helper::run"));
    }

    #[test]
    fn render_callees_uses_callees_label() {
        let resp = CalleesResponse {
            inferred: None,
            unresolved_from: None,
            target: sample_target(),
            callees: sample_edges(),
            count: 2,
            witness_counts: None,
        };
        let output = resp.render_human();
        assert!(output.contains("Callees of Foo::bar"));
        assert!(output.contains("2 callees found"));
    }

    #[test]
    fn render_empty_callers() {
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: vec![],
            count: 0,
            witness_counts: None,
        };
        let output = resp.render_human();
        assert!(output.contains("0 callers found"));
        // No edge lines after count
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 4); // header, file, blank, count
    }

    // ── RECON-M-R2: the presentation accepts unknown (recon-design-1 §3.7-4) ─────────────

    /// A union answer with an S-minted row (null line/column) — the exact daemon shape — must
    /// PARSE and render the unknown location honestly (file alone, no invented `:0`).
    #[test]
    fn union_answer_with_null_locations_parses_and_renders_unknown_honestly() {
        let json = serde_json::json!({
            "target": {
                "stable_key": "r:src/b.ts#calleeFn:SYMBOL:FUNCTION",
                "name": "calleeFn", "qualified_name": "calleeFn", "kind": "SYMBOL",
                "subtype": "FUNCTION", "file": "src/b.ts", "line": 1, "column": 0
            },
            "callers": [
                { "stable_key": "r:src/a.ts#callerFn:SYMBOL:FUNCTION", "name": "callerFn",
                  "qualified_name": "callerFn", "kind": "SYMBOL", "subtype": "FUNCTION",
                  "file": "src/a.ts", "line": 3, "column": 0,
                  "edge_type": "CALLS", "resolution": "resolved", "witness": "both" },
                { "stable_key": "r:src/c.ts#helper:SYMBOL:FUNCTION", "name": "helper",
                  "qualified_name": null, "kind": "", "subtype": null,
                  "file": "src/c.ts", "line": null, "column": null,
                  "edge_type": "CALLS", "resolution": "livegraph", "witness": "semantic" }
            ],
            "count": 2,
            "witness_counts": { "both": 1, "semantic_only": 1, "syntactic_only": 0, "unmeasured": 0 }
        });
        let resp: CallersResponse =
            serde_json::from_value(json).expect("null locations must parse (accepts unknown)");
        let out = resp.render_human();
        assert!(
            out.contains("src/a.ts:3"),
            "known location renders as today"
        );
        assert!(
            out.contains("  helper  src/c.ts  CALLS"),
            "unknown line renders the file ALONE — never an invented :0 (got:\n{out})"
        );
        assert!(
            !out.contains("src/c.ts:0"),
            "unknown is never rendered as zero"
        );
    }

    /// The legacy LG placeholder rows (`""`/`0`) keep rendering byte-identically (`:0`) — the
    /// flag-off byte-parity mandate: the Option-ization changes NO existing rendering.
    #[test]
    fn legacy_placeholder_rows_render_byte_identically() {
        let mut edges = sample_edges();
        edges[0].file = Some(String::new());
        edges[0].line = Some(0);
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: edges,
            count: 2,
            witness_counts: None,
        };
        let out = resp.render_human();
        assert!(
            out.contains("  main  :0  CALLS  static\n"),
            "the shipped placeholder shape `:0` is byte-preserved (got:\n{out})"
        );
        assert!(
            !out.contains('['),
            "no witness markers without witness_counts"
        );
    }

    /// Unknown file renders `-` (the existing absent-value convention).
    #[test]
    fn unknown_file_renders_dash() {
        let mut edges = sample_edges();
        edges[0].file = None;
        edges[0].line = None;
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: edges,
            count: 2,
            witness_counts: None,
        };
        let out = resp.render_human();
        assert!(out.contains("  main  -  CALLS  static\n"));
    }

    /// RECON-M-R2 §5.2 human contract: witness data present → ONE section line (with the
    /// one-time "call sites are syntax-detected" clarification) + compact per-row markers;
    /// `mixed` renders its exact corroborated fraction; an unmeasured row renders no marker.
    ///
    /// Iteration 2 (review-1): the fixture is CONTRACT-VALID with NONZERO `unmeasured` — the
    /// counts are 1:1 with the row multiset (§5.2): a `mixed` pair with `occurrences {1, 2}`
    /// serves TWO rows (1 `both` + 1 `syntactic` instance), and the witness-less row is the
    /// `unmeasured` instance (§3.6: a per-symbol-unanswerable projection's measurable-side fact).
    /// Four counts {1, 0, 1, 1} sum to `rows.len()` == 3.
    #[test]
    fn witness_counts_render_section_line_and_row_markers() {
        let mut edges = sample_edges();
        edges[0].witness = Some("mixed".to_string());
        edges[0].occurrences = Some(Occurrences {
            confirmed: 1,
            total: 2,
        });
        // The mixed pair's second served row (multiplicity 2 — same pair, same label; the daemon
        // serves one row per P instance).
        let second_mixed = edges[0].clone();
        edges.insert(1, second_mixed);
        edges[2].witness = None; // the unmeasured instance — makes no claim
        let counts = WitnessCounts {
            both: 1,
            semantic_only: 0,
            syntactic_only: 1,
            unmeasured: 1,
        };
        assert_eq!(
            (counts.both + counts.semantic_only + counts.syntactic_only + counts.unmeasured)
                as usize,
            edges.len(),
            "fixture sanity: the four counts are 1:1 with the row multiset (§5.2)"
        );
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: edges,
            count: 3,
            witness_counts: Some(counts),
        };
        let out = resp.render_human();
        assert!(
            out.contains(
                "1 confirmed by both analyses · 0 compiler-only · 1 syntax-only · 1 not measured \
                 by the compiler — call sites are syntax-detected"
            ),
            "the ONE section line, unmeasured rendered honestly (got:\n{out})"
        );
        assert!(
            out.contains("[both 1/2]"),
            "mixed renders its exact fraction (got:\n{out})"
        );
        assert!(
            out.contains("  Helper::run  src/helper.cpp:55  CALLS  static\n"),
            "an unmeasured row carries NO marker (got:\n{out})"
        );
    }

    /// Without witness data the render is byte-identical to the pre-M-R2 renderer (R-0/R-1:
    /// data-driven absence, not suppression).
    #[test]
    fn no_witness_data_renders_exactly_the_legacy_shape() {
        let resp = CallersResponse {
            inferred: None,
            unresolved_naming: None,
            target: sample_target(),
            callers: sample_edges(),
            count: 2,
            witness_counts: None,
        };
        let out = resp.render_human();
        let expected = "Callers of Foo::bar\nFile: src/foo.cpp:42\n\n2 callers found\n\n  \
                        main  src/main.cpp:10  CALLS  static\n  Helper::run  src/helper.cpp:55  \
                        CALLS  static\n";
        assert_eq!(out, expected, "byte-identical legacy render");
    }

    // ── PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11, RG-REQ-005-L09) ──────────

    fn inferred_edge(name: &str, line: u32, basis: &str, extractor: &str) -> EdgeSymbol {
        EdgeSymbol {
            stable_key: format!("r:x.py#{name}:SYMBOL:FUNCTION"),
            name: name.to_string(),
            qualified_name: Some(name.to_string()),
            kind: "SYMBOL".to_string(),
            subtype: Some("FUNCTION".to_string()),
            file: Some("x.py".to_string()),
            line: Some(line),
            column: Some(4),
            edge_type: Some("CALLS".to_string()),
            resolution: Some("inferred".to_string()),
            witness: None,
            occurrences: None,
            inference_basis: Some(basis.to_string()),
            inference_extractor: Some(extractor.to_string()),
        }
    }

    fn remainder(count: u64, basis: &[(&str, u64)]) -> InferredRemainder {
        InferredRemainder {
            count,
            by_basis: basis.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }

    fn unresolved(name: Option<&str>, count: u64, basis: &[(&str, u64)]) -> UnresolvedRemainder {
        UnresolvedRemainder {
            name: name.map(str::to_string),
            count,
            by_basis: basis.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }

    fn callers(
        edges: Vec<EdgeSymbol>,
        count: usize,
        inferred: Option<InferredRemainder>,
        naming: Option<UnresolvedRemainder>,
    ) -> CallersResponse {
        CallersResponse {
            target: sample_target(),
            callers: edges,
            count,
            witness_counts: None,
            inferred,
            unresolved_naming: naming,
        }
    }

    #[test]
    fn render_callers_states_the_inferred_remainder_on_the_count_line() {
        let out = callers(
            sample_edges(),
            2,
            Some(remainder(266, &[("receiver_untyped_name_only", 266)])),
            None,
        )
        .render_human();
        assert!(
            out.contains(
                "2 callers found; 266 inferred (name-only) — investigate with --include-inferred\n"
            ),
            "{out}"
        );
        assert!(
            !out.contains("(inferred:"),
            "no inferred row is listed by default:\n{out}"
        );
    }

    #[test]
    fn render_callers_include_inferred_marks_each_inferred_row() {
        let mut edges = sample_edges();
        edges.push(inferred_edge(
            "_get_dependencies_for_model",
            1737,
            "receiver_untyped_name_only",
            "python-core:0.2.0",
        ));
        let out = callers(
            edges,
            3,
            Some(remainder(1, &[("receiver_untyped_name_only", 1)])),
            None,
        )
        .render_human();
        assert!(
            out.contains("3 callers found; 1 of them inferred (name-only) — marked below\n"),
            "{out}"
        );
        assert!(
            out.contains("_get_dependencies_for_model  x.py:1737  CALLS  inferred (inferred: name-only binding — investigate)"),
            "{out}"
        );
        assert_eq!(
            out.matches("(inferred:").count(),
            1,
            "certain rows carry no marker:\n{out}"
        );
    }

    #[test]
    fn render_callers_include_inferred_marks_a_0_1_0_promoted_row_as_stored_by_an_older_rmap_with_the_rebuild_command(
    ) {
        let edges = vec![inferred_edge(
            "registry",
            54,
            "unrecorded",
            "compiler-promotion:0.1.0",
        )];
        let resp = callers(edges, 1, Some(remainder(1, &[("unrecorded", 1)])), None);
        let out = resp.render_human_for_repo(Some("/work/repo-graph"));
        assert!(
            out.contains("(inferred: compiler-resolved call recorded by an older rmap — run rmap repo rebuild /work/repo-graph)"),
            "{out}"
        );
        assert!(
            !out.contains("compiler-promotion"),
            "the writer never renders:\n{out}"
        );
        assert!(resp
            .render_human()
            .contains("run rmap repo rebuild <path>)"));
    }

    #[test]
    fn render_callers_include_inferred_marks_an_unrecorded_row_of_any_other_writer_as_basis_unknown(
    ) {
        for writer in [
            "compiler-promotion:0.2.0",
            "compiler-promotion:0.3.0",
            "some-writer:1.0.0",
        ] {
            let edges = vec![inferred_edge("f", 9, "unrecorded", writer)];
            let out =
                callers(edges, 1, Some(remainder(1, &[("unrecorded", 1)])), None).render_human();
            assert!(
                out.contains("(inferred: basis unknown — investigate)"),
                "{writer}: {out}"
            );
            assert!(
                !out.contains("older rmap") && !out.contains(writer),
                "{writer}: {out}"
            );
        }
    }

    #[test]
    fn render_callers_count_line_drops_name_only_when_an_inferred_row_is_unrecorded() {
        let out = callers(
            sample_edges(),
            2,
            Some(remainder(
                3,
                &[("receiver_untyped_name_only", 2), ("unrecorded", 1)],
            )),
            None,
        )
        .render_human();
        assert!(
            out.contains("2 callers found; 3 inferred — investigate with --include-inferred\n"),
            "{out}"
        );
        assert!(!out.contains("(name-only)"), "{out}");
    }

    #[test]
    fn render_callees_states_the_inferred_remainder_on_the_count_line() {
        let resp = CalleesResponse {
            target: sample_target(),
            callees: sample_edges()[..1].to_vec(),
            count: 1,
            witness_counts: None,
            inferred: Some(remainder(1, &[("receiver_untyped_name_only", 1)])),
            unresolved_from: None,
        };
        let out = resp.render_human();
        assert!(
            out.contains(
                "1 callee found; 1 inferred (name-only) — investigate with --include-inferred\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn render_callers_states_the_unresolved_calls_naming_the_symbol() {
        let out = callers(
            sample_edges(),
            2,
            None,
            Some(unresolved(
                Some("extend"),
                3,
                &[("self_call_hierarchy_miss", 3)],
            )),
        )
        .render_human();
        assert!(
            out.contains("3 unresolved calls name `extend` (couldn't attribute: 3)\n"),
            "{out}"
        );
        let one = callers(
            sample_edges(),
            2,
            None,
            Some(unresolved(
                Some("bar"),
                1,
                &[("specifier_matches_runtime_module", 1)],
            )),
        )
        .render_human();
        assert!(
            one.contains("1 unresolved call names `bar` (standard library / runtime module: 1)"),
            "{one}"
        );
    }

    #[test]
    fn render_callees_states_the_unresolved_calls_from_the_symbol() {
        let resp = CalleesResponse {
            target: sample_target(),
            callees: vec![],
            count: 0,
            witness_counts: None,
            inferred: None,
            unresolved_from: Some(unresolved(
                None,
                3,
                &[("no_supporting_signal", 2), ("a_newer_basis", 1)],
            )),
        };
        let out = resp.render_human();
        assert!(
            out.contains(
                "3 unresolved calls from this symbol (couldn't attribute: 2, unrecognized: 1)\n"
            ),
            "an unrecognized basis folds, never renders raw:\n{out}"
        );
        assert!(!out.contains("a_newer_basis"));
    }

    #[test]
    fn render_callers_zero_certain_with_unresolved_names_the_count() {
        // RG-REQ-005-L09: a zero never reads as absence.
        let out = callers(
            vec![],
            0,
            None,
            Some(unresolved(
                Some("Recover"),
                2,
                &[("no_supporting_signal", 2)],
            )),
        )
        .render_human();
        assert!(
            out.contains(
                "0 callers found\n2 unresolved calls name `Recover` (couldn't attribute: 2)\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn render_callers_without_remainder_fields_is_unchanged() {
        // An older daemon / LiveGraph payload: no remainder fields → today's bytes; an absent
        // field is never read as a zero.
        let json = serde_json::json!({
            "target": {"name": "bar", "qualified_name": "Foo::bar", "kind": "SYMBOL", "file": "src/foo.cpp", "line": 42},
            "callers": [{"name": "main", "qualified_name": "main", "kind": "SYMBOL", "file": "src/main.cpp", "line": 10, "edge_type": "CALLS", "resolution": "static"}],
            "count": 1
        });
        let resp: CallersResponse = serde_json::from_value(json).unwrap();
        assert_eq!(
            resp.render_human(),
            "Callers of Foo::bar\nFile: src/foo.cpp:42\n\n1 caller found\n\n  main  src/main.cpp:10  CALLS  static\n"
        );
    }
}
