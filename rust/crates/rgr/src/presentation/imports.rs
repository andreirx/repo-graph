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
    } else if imp.resolution == "static" {
        static_row_state(imp)
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

/// CPP-INCLUDE-BASENAME-1 / TS-WORKSPACE-RESOLUTION-1 (RG-REQ-002-L11, RG-REQ-002-L04): what an
/// inferred row says about its reason. The renderer prints JSON it did not produce, so it re-checks
/// the agreement it prints — a `unique_basename` reason with exactly one candidate equal to the
/// row's own `file`; a `workspace_source_entry` reason whose first candidate is the row's own
/// `file` followed by one or more declared entries — and states every other reason as unreadable,
/// never as evidence. An inferred row of a producer that must carry a reason (C/C++, TS) without
/// one says so with the remedy; any other row without a reason prints `inferred`, as before.
fn inferred_row_state(imp: &ImportEntry) -> String {
    const MISSING: &str = "inferred (reason missing — re-index)";
    let unreadable = |what: &str| format!("inferred: reason unreadable ({what})");
    match &imp.reason {
        Some(ImportEntryReason::Recorded { basis, candidates })
            if basis == "workspace_source_entry" =>
        {
            match candidates.as_slice() {
                [source, declared @ ..] if *source == imp.file && !declared.is_empty() => {
                    let (noun, list) = match declared {
                        [one] => ("entry", one.clone()),
                        many => ("entries", many.join(", ")),
                    };
                    format!(
                        "inferred: workspace source entry → {source} (declared {noun} {list} not indexed)"
                    )
                }
                [source, ..] if *source != imp.file => {
                    unreadable(&format!("candidate {source} is not the row's file"))
                }
                [_] => unreadable("no declared entry beside the source entry"),
                _ => unreadable("candidates is empty"),
            }
        }
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
        None if imp.evidence.iter().any(|e| {
            e.starts_with("c-core:") || e.starts_with("cpp-core:") || e.starts_with("ts-core:")
        }) =>
        {
            MISSING.to_string()
        }
        None => "inferred".to_string(),
    }
}

/// TS-ALIAS-RESOLUTION-1 (D-TSA-RECORD-CONFLICT-1: "output wording to clarify if needed"): what a
/// static row says about the rule that bound it. A row whose recorded reason is `tsconfig_paths`
/// with exactly one candidate equal to the row's own `file` — the `paths` stage bound it
/// (`indexer/src/resolver.rs` `resolve_tsconfig_paths_import`) — prints
/// `static (resolved through tsconfig paths)`; a `tsconfig_paths` reason that disagrees with the row
/// is unreadable, never evidence (the [`inferred_row_state`] re-check applied to a static row); a
/// static row whose stored reason storage decoded as `Unreadable` prints
/// `static: reason unreadable (<storage's text>)` for every basis (oracle-corrections.md OC-3);
/// every static row with no reason, or a readable reason other than `tsconfig_paths`, prints
/// `static`, as before.
fn static_row_state(imp: &ImportEntry) -> String {
    let unreadable = |what: &str| format!("static: reason unreadable ({what})");
    match &imp.reason {
        Some(ImportEntryReason::Recorded { basis, candidates }) if basis == "tsconfig_paths" => {
            match candidates.as_slice() {
                [one] if *one == imp.file => "static (resolved through tsconfig paths)".to_string(),
                [one] => unreadable(&format!("candidate {one} is not the row's file")),
                [] => unreadable("candidates is empty"),
                many => unreadable(&format!(
                    "{} candidates under a tsconfig paths binding",
                    many.len()
                )),
            }
        }
        // OC-3: storage decoded the carrier as present but malformed or contradicting its edge;
        // the unreadable state is the carrier's, whatever basis it named.
        Some(ImportEntryReason::Unreadable { unreadable: what }) => unreadable(what),
        _ => "static".to_string(),
    }
}

/// TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04): the LiveGraph class of a bare import naming this
/// repository's own workspace package. The LiveGraph route keeps the label until one rule serves
/// both routes; every human render that prints it explains it once ([`workspace_label_note`]).
const WORKSPACE_LOCAL_UNEDGEABLE: &str = "WorkspaceLocalUnedgeable";

/// The one line that explains the `WorkspaceLocalUnedgeable` label. It states only what holds for
/// EVERY import with that label (D-TWR-NOTE): the renderer sees the label, not the default route's
/// outcome for each import — the label also covers workspace subpath imports and root imports whose
/// declared entry is indexed, which the workspace-source rule leaves unresolved. So the line says
/// the default route MAY record such an import as inferred, only under the rule's condition, names
/// where to look (the file's listing under `--include-inferred` when the render is about one file,
/// the module view under `--include-inferred` otherwise), and sends the agent to open any other.
fn workspace_label_note(file: Option<&str>) -> String {
    let command = match file {
        Some(f) => format!(
            "rmap imports {} --include-inferred",
            crate::presentation::import_partition::shell_quote(f)
        ),
        None => "rmap modules deps --include-inferred".to_string(),
    };
    format!(
        "  note: [{WORKSPACE_LOCAL_UNEDGEABLE}] is an import of this repo's own workspace package; \
         this route forms no edge for it until one rule serves both routes — the default route \
         may record it as an inferred import of the package's source entry, but only for an \
         import of the package root whose declared entries are not indexed ({command}); for any \
         other import with this label, open the import and look inside\n"
    )
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
    /// IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12): the file's imports without a confirmed
    /// target. `None` = the key is ABSENT (a daemon that predates this read — the unavailable
    /// line, never a zero); a present value of any shape, `null` included, is decoded at render so
    /// a malformed one is a named unreadable line, never a parse failure of the whole listing.
    #[serde(default, deserialize_with = "present_value")]
    pub unresolved: Option<serde_json::Value>,
    /// The number of `unresolved` rows (checked against them at render).
    #[serde(default, deserialize_with = "present_value")]
    pub unresolved_count: Option<serde_json::Value>,
    /// The file's language (a non-empty string, else rendered `unknown`).
    #[serde(default, deserialize_with = "present_value")]
    pub language: Option<serde_json::Value>,
    /// `{"omitted_forms": [..] | null}`: the import forms this listing omits for the language.
    #[serde(default, deserialize_with = "present_value")]
    pub listing_coverage: Option<serde_json::Value>,
}

/// A present key is `Some(value)` — `null` included — so an explicit `null` is never read as an
/// absent key (with `#[serde(default)]`, an absent key is `None`).
fn present_value<'de, D>(d: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <serde_json::Value as Deserialize>::deserialize(d).map(Some)
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
        let unresolved =
            unresolved::decode(self.unresolved.as_ref(), self.unresolved_count.as_ref());
        if self.is_zero_state(&unresolved) {
            // IMPORTS-UNRESOLVED-REMAINDER-1: no row of either table and nothing left out — the count
            // line names the language and the listing coverage instead of a bare zero.
            out.push_str(&unresolved::zero_state_line(
                self.language.as_ref(),
                self.listing_coverage.as_ref(),
            ));
            return out;
        }
        if count == 1 {
            out.push_str("1 import\n");
        } else {
            out.push_str(&format!("{} imports\n", count));
        }

        if self.imports.is_empty() {
            self.push_partition_lines(&mut out);
            self.push_unresolved_lines(&mut out, &unresolved);
            return out;
        }

        out.push('\n');

        // ── Import list ────────────────────────────────────────────
        // The symbol is the display name (usually the imported file path).
        for imp in &self.imports {
            out.push_str(&format_import_row(imp));
        }

        self.push_partition_lines(&mut out);
        self.push_unresolved_lines(&mut out, &unresolved);
        out
    }

    /// IMPORTS-UNRESOLVED-REMAINDER-1: the zero-state — no listed row, a stated partition with no
    /// inferred remainder, and a readable zero of rows without a confirmed target.
    fn is_zero_state(&self, unresolved: &unresolved::Decoded) -> bool {
        use crate::presentation::import_partition as ip;
        let nothing_left_out = matches!(
            ip::partition_from(self.import_view.as_ref(), self.import_remainder.as_ref()),
            ip::Partition::Stated(r) if ip::per_file_remainder(&r).inferred.imports == 0
        );
        self.imports.is_empty()
            && nothing_left_out
            && matches!(unresolved, unresolved::Decoded::Rows(rows) if rows.is_empty())
    }

    /// IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12, RG-REQ-002-L04): outside the zero-state, the
    /// block of rows without a confirmed target (or its measured zero, or its unavailable /
    /// unreadable line), then the one coverage line.
    fn push_unresolved_lines(&self, out: &mut String, unresolved: &unresolved::Decoded) {
        out.push_str(&unresolved::block(unresolved));
        out.push_str(&unresolved::coverage_line(
            self.language.as_ref(),
            self.listing_coverage.as_ref(),
        ));
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

// ── IMPORTS-UNRESOLVED-REMAINDER-1: the rows without a confirmed target (SLICE_DOC §2.2) ────────

/// IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12, RG-REQ-002-L04, RG-REQ-002-L11,
/// D-AGENT-USEFULNESS-FRAME-1): decode and word the `unresolved` rows of an `imports <file>` answer.
///
/// Every reader phrase restates the PREDICATE the stored code's assignment site tests (the site is
/// named beside each phrase); the inference is carried only as the classification QUESTION and the
/// candidates. A stored string this build has no phrase for is printed verbatim and marked — never
/// mapped to a known phrase. Absent evidence renders unavailable, malformed evidence unreadable —
/// never zero, never dropped.
mod unresolved {
    use serde_json::Value;

    /// One readable row.
    pub(super) struct Row {
        target_key: String,
        recorded_specifier: Option<String>,
        decoded_target_path: Option<String>,
        line: Option<u64>,
        category: String,
        classification: String,
        basis_code: String,
        /// The metadata `basis` (the candidate-set reason), `None` when not recorded.
        basis: Option<String>,
        candidates: Candidates,
    }

    /// The candidate carrier, as the daemon serialized it.
    enum Candidates {
        NotRecorded,
        Paths(Vec<String>),
        Unreadable(String),
    }

    /// What the answer says about its rows without a confirmed target.
    pub(super) enum Decoded {
        /// No `unresolved` key: a daemon that predates this read.
        Unavailable,
        /// The rows (or a row) are malformed.
        Unreadable(String),
        /// `unresolved_count` is missing, not a non-negative integer, or disagrees with the rows.
        CountDisagrees { count: String, rows: usize },
        /// The rows, with an agreeing count.
        Rows(Vec<Row>),
    }

    /// The one decode rule of the block (SLICE_DOC §2.2 field states).
    pub(super) fn decode(unresolved: Option<&Value>, count: Option<&Value>) -> Decoded {
        let Some(unresolved) = unresolved else {
            return Decoded::Unavailable;
        };
        let rows = match decode_rows(unresolved) {
            Ok(rows) => rows,
            Err(why) => return Decoded::Unreadable(why),
        };
        match count {
            Some(c) if c.as_u64() == Some(rows.len() as u64) => Decoded::Rows(rows),
            Some(c) => Decoded::CountDisagrees {
                count: c.to_string(),
                rows: rows.len(),
            },
            None => Decoded::CountDisagrees {
                count: "missing".to_string(),
                rows: rows.len(),
            },
        }
    }

    fn decode_rows(v: &Value) -> Result<Vec<Row>, String> {
        let items = v
            .as_array()
            .ok_or_else(|| "the rows are not an array".to_string())?;
        items
            .iter()
            .enumerate()
            .map(|(i, item)| decode_row(item).map_err(|why| format!("row {}: {why}", i + 1)))
            .collect()
    }

    fn decode_row(v: &Value) -> Result<Row, String> {
        let obj = v.as_object().ok_or_else(|| "not an object".to_string())?;
        let string = |key: &str| -> Result<String, String> {
            obj.get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("{key} is not a string"))
        };
        let string_or_null = |key: &str| -> Result<Option<String>, String> {
            match obj.get(key) {
                Some(Value::String(s)) => Ok(Some(s.clone())),
                Some(Value::Null) => Ok(None),
                _ => Err(format!("{key} is neither a string nor null")),
            }
        };
        let line = match obj.get("line") {
            Some(Value::Null) => None,
            Some(n) => Some(
                n.as_u64()
                    .ok_or_else(|| "line is neither an integer nor null".to_string())?,
            ),
            None => return Err("line is neither an integer nor null".to_string()),
        };
        let candidates = match obj.get("candidates") {
            Some(Value::Null) => Candidates::NotRecorded,
            // F-001: the same rule as the storage reader (`unresolved_candidate_paths`) — a path
            // list is a NON-EMPTY array of strings; an empty list is malformed, never a measured
            // zero-candidate result.
            Some(Value::Array(items)) if items.is_empty() => {
                return Err("candidates is an empty array".to_string())
            }
            Some(Value::Array(items)) => Candidates::Paths(
                items
                    .iter()
                    .map(|p| p.as_str().map(str::to_string))
                    .collect::<Option<Vec<String>>>()
                    .ok_or_else(|| "candidates holds a non-string".to_string())?,
            ),
            Some(Value::Object(o)) if o.len() == 1 && o.contains_key("unreadable") => {
                match o.get("unreadable") {
                    Some(Value::String(text)) => Candidates::Unreadable(text.clone()),
                    _ => return Err("candidates.unreadable is not a string".to_string()),
                }
            }
            _ => {
                return Err(
                    "candidates is none of null, an array of strings, {\"unreadable\": string}"
                        .to_string(),
                )
            }
        };
        Ok(Row {
            target_key: string("target_key")?,
            recorded_specifier: string_or_null("recorded_specifier")?,
            decoded_target_path: string_or_null("decoded_target_path")?,
            line,
            category: string("category")?,
            classification: string("classification")?,
            basis_code: string("basis_code")?,
            // The metadata `basis` reaches the wire as a string (storage carries a non-string value
            // as its JSON text); any other value is printed verbatim and marked, never dropped.
            basis: match obj.get("basis") {
                None | Some(Value::Null) => None,
                Some(Value::String(b)) => Some(b.clone()),
                Some(other) => Some(other.to_string()),
            },
            candidates,
        })
    }

    /// The block outside the zero-state: the rows (header + one line each), the measured zero,
    /// or the unavailable / unreadable line.
    pub(super) fn block(decoded: &Decoded) -> String {
        match decoded {
            Decoded::Unavailable => {
                "imports without a confirmed target: unavailable from this daemon — upgrade rmapd\n"
                    .to_string()
            }
            Decoded::Unreadable(why) => {
                format!("imports without a confirmed target: unreadable ({why})\n")
            }
            Decoded::CountDisagrees { count, rows } => format!(
                "imports without a confirmed target: unreadable (count {count} disagrees with {rows} rows)\n"
            ),
            Decoded::Rows(rows) if rows.is_empty() => {
                "0 imports without a confirmed target\n".to_string()
            }
            Decoded::Rows(rows) => {
                let mut out = if rows.len() == 1 {
                    "1 import without a confirmed target:\n".to_string()
                } else {
                    format!("{} imports without a confirmed target:\n", rows.len())
                };
                for row in rows {
                    out.push_str(&row_line(row));
                }
                out
            }
        }
    }

    /// `  {target}  line {n}  {first part} · {classification phrase} ({basis phrase})`, the
    /// ` · ` dropped when the first part is empty.
    fn row_line(row: &Row) -> String {
        let target = match (&row.recorded_specifier, &row.decoded_target_path) {
            (Some(spec), _) => spec.clone(),
            (None, Some(path)) => {
                format!("{path} (decoded from the stored key — not a confirmed target)")
            }
            (None, None) => row.target_key.clone(),
        };
        let line = row
            .line
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        let first = first_part(row);
        let tail = format!(
            "{} ({})",
            classification_phrase(&row.classification),
            basis_phrase(&row.basis_code)
        );
        if first.is_empty() {
            format!("  {target}  line {line}  {tail}\n")
        } else {
            format!("  {target}  line {line}  {first} · {tail}\n")
        }
    }

    /// The two categories whose rows always carry the candidate clause.
    fn is_ambiguity_category(category: &str) -> bool {
        matches!(
            category,
            "imports_ambiguous_match" | "imports_ambiguous_suffix"
        )
    }

    /// The category phrase — the predicate at its assignment site — or `None` when the category has
    /// no phrase of its own (`imports_file_not_found`: the block header is its predicate,
    /// `indexer/src/resolver.rs:2574`, the TS workspace ambiguity included, `:690-704`).
    fn category_phrase(category: &str) -> Option<String> {
        match category {
            "imports_file_not_found" => None,
            // indexer/src/resolver.rs:552 (include-root `Ambiguous`) and :667 (the suffix/basename
            // stage's `AmbiguousSuffix`/`AmbiguousBasename`): more than one indexed file matched.
            "imports_ambiguous_match" => Some("several indexed files matched".to_string()),
            // indexer/src/resolver.rs:577 from :1479 (`resolve_java_suffix`, :1492-1522).
            "imports_ambiguous_suffix" => Some(
                "several indexed .java files end with the specifier's path (or a prefix of it)"
                    .to_string(),
            ),
            // indexer/src/resolver.rs:566 from :1465: the specifier ends with `.*`.
            "imports_wildcard" => Some("wildcard import".to_string()),
            other => Some(format!(
                "{other} (no phrase for this category in this build)"
            )),
        }
    }

    /// The reason text (without its parentheses) from the metadata `basis`; `{m}` only from
    /// recorded paths.
    fn reason_text(basis: &str, candidates: &Candidates) -> String {
        let recorded = match candidates {
            Candidates::Paths(paths) if !paths.is_empty() => Some(paths.len()),
            _ => None,
        };
        match (basis, recorded) {
            ("ambiguous_basename", Some(m)) => format!("same file name in {m} places"),
            ("ambiguous_basename", None) => "same file name in several places".to_string(),
            ("ambiguous_suffix", Some(m)) => format!("same path suffix in {m} places"),
            ("ambiguous_suffix", None) => "same path suffix in several places".to_string(),
            ("ambiguous_workspace_source_entry", _) => {
                "several workspace source entries".to_string()
            }
            // TS-ALIAS-RESOLUTION-1: written only when the file's one tsconfig `paths` mapping (the
            // sole inspected project covering it, repo-index/src/config.rs) selects a pattern whose
            // deciding substitution reaches two or more indexed files
            // (indexer/src/resolver.rs `resolve_tsconfig_paths_import`).
            ("ambiguous_tsconfig_paths", Some(m)) => {
                format!("the tsconfig paths that apply to this file reach {m} indexed files")
            }
            ("ambiguous_tsconfig_paths", None) => {
                "the tsconfig paths that apply to this file reach several indexed files".to_string()
            }
            // TS-ALIAS-RESOLUTION-1: written only when two or more wildcard patterns of that mapping
            // tie for the longest prefix and their deciding substitutions reach one or more indexed
            // files (repo-graph-import-resolver `tsconfig_alias_hits` → `TiedPatterns`).
            ("tied_tsconfig_paths_patterns", Some(1)) => {
                "two or more tsconfig paths patterns tie for the longest prefix; together they \
                 reach 1 indexed file"
                    .to_string()
            }
            ("tied_tsconfig_paths_patterns", Some(m)) => format!(
                "two or more tsconfig paths patterns tie for the longest prefix; together they \
                 reach {m} indexed files"
            ),
            ("tied_tsconfig_paths_patterns", None) => {
                "two or more tsconfig paths patterns tie for the longest prefix; together they \
                 reach indexed files"
                    .to_string()
            }
            (other, _) => format!("{other} (no phrase for this reason in this build)"),
        }
    }

    /// The candidate clause (without its ` — ` prefix).
    fn candidate_clause(candidates: &Candidates) -> String {
        match candidates {
            // TS-ALIAS-RESOLUTION-1: a tied-patterns row can record one candidate.
            Candidates::Paths(paths) if paths.len() == 1 => format!("1 candidate: {}", paths[0]),
            Candidates::Paths(paths) => format!("{} candidates: {}", paths.len(), paths.join(", ")),
            Candidates::NotRecorded => "candidate paths not recorded with this import".to_string(),
            Candidates::Unreadable(text) => format!("candidates unreadable (\"{text}\")"),
        }
    }

    /// `{category phrase}{reason clause}{candidate clause}` (SLICE_DOC §2.2).
    fn first_part(row: &Row) -> String {
        let phrase = category_phrase(&row.category);
        let mut out = String::new();
        match (&phrase, &row.basis) {
            (Some(p), Some(basis)) => {
                out.push_str(p);
                out.push_str(&format!(" ({})", reason_text(basis, &row.candidates)));
            }
            (Some(p), None) => {
                out.push_str(p);
                if row.category == "imports_ambiguous_match" {
                    out.push_str(" (reason not recorded)");
                }
            }
            // A category without a phrase prints a present reason bare, as the first part.
            (None, Some(basis)) => out.push_str(&reason_text(basis, &row.candidates)),
            (None, None) => {}
        }
        // TS-ALIAS-RESOLUTION-1: a row of the two `paths` ambiguity bases always says what became of
        // its candidates (the stage records them on every such row; their absence is stated).
        let with_candidates = is_ambiguity_category(&row.category)
            || !matches!(row.candidates, Candidates::NotRecorded)
            || matches!(
                row.basis.as_deref(),
                Some("ambiguous_tsconfig_paths" | "tied_tsconfig_paths_patterns")
            );
        if with_candidates {
            if !out.is_empty() {
                out.push_str(" — ");
            }
            out.push_str(&candidate_clause(&row.candidates));
        }
        out
    }

    /// The classification, carried as a QUESTION (never asserted).
    fn classification_phrase(classification: &str) -> String {
        match classification {
            "external_library_candidate" => "external library?".to_string(),
            "internal_candidate" => "this repository?".to_string(),
            "framework_boundary_candidate" => "framework boundary?".to_string(),
            "unknown" => "unknown".to_string(),
            other => format!("{other} (no phrase for this classification in this build)"),
        }
    }

    /// The basis phrase — the test at its assignment site in
    /// `classification/src/unresolved_classifier.rs`.
    fn basis_phrase(basis_code: &str) -> String {
        match basis_code {
            // :185 `is_relative` (:500-528) and :214 (`:FILE` in the specifier or the target key).
            "relative_import_target_unresolved" => "recorded specifier is crate, super or self, or starts with ., crate::, super:: or self::, or the specifier or target key contains :FILE".to_string(),
            // :193 `resolve_declared_dependency`.
            "specifier_matches_package_dependency" => {
                "specifier matches a dependency declared in the manifest".to_string()
            }
            // :204 `has_runtime_builtin_module` (signals.rs:33).
            "specifier_matches_runtime_module" => {
                "specifier or its part before :: is on the runtime-module list of this build"
                    .to_string()
            }
            // :209 `matches_any_alias`.
            "specifier_matches_project_alias" => {
                "specifier matches a tsconfig paths alias".to_string()
            }
            // :219 `is_rust_crate_internal_import` (:230-258, `is_rust_module_name` :263-270).
            "rust_crate_internal_module_heuristic" => "metadata carries a specifier and no rawPath, and the specifier's first segment before :: starts with a lowercase letter and holds only lowercase letters, digits or underscores".to_string(),
            // `unknown()` :433, reached from :223 and :116.
            "no_supporting_signal" => "no classifier signal".to_string(),
            other => format!("{other} (no phrase for this basis in this build)"),
        }
    }

    /// `a, b, c or d`.
    fn join_items(items: &[&str]) -> String {
        match items {
            [] => String::new(),
            [one] => one.to_string(),
            [init @ .., last] => format!("{} or {last}", init.join(", ")),
        }
    }

    /// The language when a non-empty string.
    fn language_of(language: Option<&Value>) -> Option<&str> {
        language.and_then(Value::as_str).filter(|l| !l.is_empty())
    }

    /// What `listing_coverage` says.
    enum Coverage<'a> {
        Absent,
        Omits(Vec<&'a str>),
        NotRecorded,
        Unreadable,
    }

    fn coverage_of(listing_coverage: Option<&Value>) -> Coverage<'_> {
        let Some(v) = listing_coverage else {
            return Coverage::Absent;
        };
        match v.as_object().and_then(|o| o.get("omitted_forms")) {
            Some(Value::Null) => Coverage::NotRecorded,
            Some(Value::Array(items)) if !items.is_empty() => items
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<&str>>>()
                .map_or(Coverage::Unreadable, Coverage::Omits),
            _ => Coverage::Unreadable,
        }
    }

    /// The zero-state count line: `0 imports (language: {lang}; this index lists no imports for
    /// this file{coverage})` — the coverage clause is never empty.
    pub(super) fn zero_state_line(
        language: Option<&Value>,
        listing_coverage: Option<&Value>,
    ) -> String {
        let lang = language_of(language);
        let coverage = match coverage_of(listing_coverage) {
            Coverage::Omits(items) => format!(" — this listing omits {}", join_items(&items)),
            Coverage::NotRecorded => match lang {
                Some(l) => format!(" — listing coverage not recorded for {l}"),
                None => " — listing coverage not recorded (language not recorded)".to_string(),
            },
            Coverage::Absent => " — listing coverage unavailable from this daemon".to_string(),
            Coverage::Unreadable => " — listing coverage unreadable".to_string(),
        };
        format!(
            "0 imports (language: {}; this index lists no imports for this file{coverage})\n",
            lang.unwrap_or("unknown")
        )
    }

    /// The one coverage line of every listing outside the zero-state.
    pub(super) fn coverage_line(
        language: Option<&Value>,
        listing_coverage: Option<&Value>,
    ) -> String {
        match coverage_of(listing_coverage) {
            Coverage::Omits(items) => format!(
                "listing limit: this listing omits {} — open the file for those\n",
                join_items(&items)
            ),
            Coverage::NotRecorded => match language_of(language) {
                Some(l) => format!("listing coverage: not recorded for {l} — open the file to confirm\n"),
                None => "listing coverage: not recorded (language not recorded) — open the file to confirm\n".to_string(),
            },
            Coverage::Absent => "listing coverage: unavailable from this daemon\n".to_string(),
            Coverage::Unreadable => "listing limit: unreadable\n".to_string(),
        }
    }

    /// D-IUR-ENGINE-SCOPE: the line a per-file `--engine livegraph|compare` render prints after
    /// its first line — that view does not list these imports, and the command that does.
    pub(super) fn view_limit_line(file: &str) -> String {
        format!(
            "view limit: this view does not list imports without a confirmed target — rmap imports {} does\n",
            crate::presentation::import_partition::shell_quote(file)
        )
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
        // IMPORTS-UNRESOLVED-REMAINDER-1 (D-IUR-ENGINE-SCOPE): a per-file read-model view says it
        // does not list the imports without a confirmed target, and names the command that does.
        if let Some(f) = &self.file_filter {
            out.push_str(&unresolved::view_limit_line(f));
        }
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
        // TS-WORKSPACE-RESOLUTION-1: the label is printed (in `by class`, or on a listed row) —
        // explain it once.
        let label_printed = self
            .observation_class_counts
            .contains_key(WORKSPACE_LOCAL_UNEDGEABLE)
            || self
                .observations
                .iter()
                .any(|o| o.class == WORKSPACE_LOCAL_UNEDGEABLE && (o.blocking || show_benign));
        if label_printed {
            out.push_str(&workspace_label_note(self.file_filter.as_deref()));
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
        out.push_str(&format!("Imports: {}\n", self.file));
        // IMPORTS-UNRESOLVED-REMAINDER-1 (D-IUR-ENGINE-SCOPE): the readiness view does not list the
        // imports without a confirmed target; say so after the first line.
        out.push_str(&unresolved::view_limit_line(&self.file));
        out.push('\n');
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
            // TS-WORKSPACE-RESOLUTION-1: explain the label once when a listed row carries it.
            if c.blocking_observations
                .iter()
                .any(|o| o.class == WORKSPACE_LOCAL_UNEDGEABLE)
            {
                out.push_str(&workspace_label_note(Some(&self.file)));
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
            // TS-WORKSPACE-RESOLUTION-1: explain the label once when the classes print it.
            if m.blocking_observation_by_class
                .contains_key(WORKSPACE_LOCAL_UNEDGEABLE)
            {
                out.push_str(&workspace_label_note(None));
            }
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
            unresolved: Some(serde_json::json!([])),
            unresolved_count: Some(serde_json::json!(0)),
            language: None,
            listing_coverage: None,
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
            unresolved: Some(serde_json::json!([])),
            unresolved_count: Some(serde_json::json!(0)),
            language: None,
            listing_coverage: None,
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
            },
            "unresolved": [],
            "unresolved_count": 0
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
            },
            "unresolved": [],
            "unresolved_count": 0
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
        // The count line (the third line) is singular; the measured zero below it is correctly
        // plural (`0 imports without a confirmed target`).
        let count_line = output.lines().nth(2).unwrap();
        assert!(!count_line.contains("imports"), "{output}"); // no plural
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
            "unresolved": [],
            "unresolved_count": 0,
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
                "Imports: src/core/ngx_config.h\n\n1 import\n\n  {LINUX}  depth=1  inferred: unique basename → {LINUX}\n0 imports without a confirmed target\nlisting coverage: unavailable from this daemon\n"
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
             src/y.h  depth=1  static\n\
             0 imports without a confirmed target\n\
             listing coverage: unavailable from this daemon\n"
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

    // ── TS-WORKSPACE-RESOLUTION-1: the workspace row and the LiveGraph label ──────────────

    const ENGINE_SRC: &str = "packages/engine/src/index.ts";
    const ENGINE_DIST: &str = "packages/engine/dist/index.js";

    #[test]
    fn inferred_workspace_row_renders_its_source_entry_and_every_declared_entry() {
        let resp: ImportsResponse = serde_json::from_value(serde_json::json!({
            "file": "packages/api/src/server.ts",
            "imports": [row_json(
                ENGINE_SRC,
                "inferred",
                "ts-core:0.2.0",
                Some(serde_json::json!({
                    "basis": "workspace_source_entry",
                    "candidates": [ENGINE_SRC, ENGINE_DIST]
                })),
            )],
            "import_view": {"include_tests": false, "include_inferred": true},
            "import_remainder": zero_remainder(),
            "unresolved": [],
            "unresolved_count": 0,
        }))
        .unwrap();
        assert_eq!(
            resp.render_human(),
            format!(
                "Imports: packages/api/src/server.ts\n\n1 import\n\n  {ENGINE_SRC}  depth=1  \
                 inferred: workspace source entry → {ENGINE_SRC} (declared entry {ENGINE_DIST} not indexed)\n\
                 0 imports without a confirmed target\nlisting coverage: unavailable from this daemon\n"
            )
        );
        // Several declared entries: every one listed, none chosen.
        let fx = "packages/effects/src/index.ts";
        let resp = listing_with_inferred(vec![row_json(
            fx,
            "inferred",
            "ts-core:0.2.0",
            Some(serde_json::json!({
                "basis": "workspace_source_entry",
                "candidates": [fx, "packages/effects/dist/index.d.ts", "packages/effects/dist/index.js"]
            })),
        )]);
        assert_eq!(
            only_row(&resp),
            format!(
                "  {fx}  depth=1  inferred: workspace source entry → {fx} (declared entries \
                 packages/effects/dist/index.d.ts, packages/effects/dist/index.js not indexed)"
            )
        );
    }

    #[test]
    fn inferred_workspace_row_whose_reason_disagrees_is_unreadable() {
        for reason in [
            // The source entry is not the row's file.
            serde_json::json!({"basis": "workspace_source_entry", "candidates": [ENGINE_DIST, ENGINE_SRC]}),
            // No declared entry beside the source entry.
            serde_json::json!({"basis": "workspace_source_entry", "candidates": [ENGINE_SRC]}),
            serde_json::json!({"basis": "workspace_source_entry", "candidates": []}),
            // The ambiguous basis is never a bound row.
            serde_json::json!({"basis": "ambiguous_workspace_source_entry", "candidates": [ENGINE_SRC, ENGINE_DIST]}),
        ] {
            let resp = listing_with_inferred(vec![row_json(
                ENGINE_SRC,
                "inferred",
                "ts-core:0.2.0",
                Some(reason.clone()),
            )]);
            let line = only_row(&resp);
            let prefix = format!("  {ENGINE_SRC}  depth=1  inferred: reason unreadable (");
            assert!(
                line.starts_with(&prefix) && line.ends_with(')'),
                "{reason}: {line}"
            );
            assert!(
                !line.contains('→'),
                "{reason}: never printed as evidence: {line}"
            );
        }
        // The storage read's own unreadable form.
        let resp = listing_with_inferred(vec![row_json(
            ENGINE_SRC,
            "inferred",
            "ts-core:0.2.0",
            Some(serde_json::json!({"unreadable": "candidate r1:x:FILE is not the edge's target"})),
        )]);
        assert_eq!(
            only_row(&resp),
            format!(
                "  {ENGINE_SRC}  depth=1  inferred: reason unreadable (candidate r1:x:FILE is not the edge's target)"
            )
        );
    }

    #[test]
    fn ts_inferred_row_without_a_reason_key_says_reason_missing_never_bare_inferred() {
        let resp = listing_with_inferred(vec![row_json(
            ENGINE_SRC,
            "inferred",
            "ts-core:0.2.0",
            None,
        )]);
        assert_eq!(
            only_row(&resp),
            format!("  {ENGINE_SRC}  depth=1  inferred (reason missing — re-index)")
        );
        let resp = listing_with_inferred(vec![row_json(
            ENGINE_SRC,
            "inferred",
            "ts-core:0.2.0",
            Some(serde_json::json!({"missing": "no reason carrier"})),
        )]);
        assert_eq!(
            only_row(&resp),
            format!("  {ENGINE_SRC}  depth=1  inferred (reason missing — re-index)")
        );
    }

    const WS_NOTE_HEAD: &str = "note: [WorkspaceLocalUnedgeable] is an import of this repo's own \
         workspace package; this route forms no edge for it until one rule serves both routes — \
         the default route may record it as an inferred import of the package's source entry, but \
         only for an import of the package root whose declared entries are not indexed (";
    /// What follows the command in the note.
    const WS_NOTE_TAIL: &str =
        "); for any other import with this label, open the import and look inside\n";

    fn count(out: &str, needle: &str) -> usize {
        out.matches(needle).count()
    }

    #[test]
    fn workspace_local_unedgeable_label_is_kept_and_explained_once_on_every_render() {
        // LiveGraph listing, repo-wide: the label stays, one note names the module view.
        let out = sample_lg().render_human();
        assert!(out.contains("[WorkspaceLocalUnedgeable] BLOCKING"), "{out}");
        assert!(out.contains("WorkspaceLocalUnedgeable=1"), "{out}");
        assert_eq!(count(&out, WS_NOTE_HEAD), 1, "{out}");
        assert!(
            out.contains(&format!(
                "{WS_NOTE_HEAD}rmap modules deps --include-inferred{WS_NOTE_TAIL}"
            )),
            "{out}"
        );
        assert!(
            out.ends_with(&format!(
                "  {WS_NOTE_HEAD}rmap modules deps --include-inferred{WS_NOTE_TAIL}"
            )),
            "after its observations: {out}"
        );
        // LiveGraph listing for one file: the note names that file's listing (shell-quoted).
        let mut r = sample_lg();
        r.file_filter = Some("a/src/my file.ts".to_string());
        let out = r.render_human();
        assert_eq!(count(&out, WS_NOTE_HEAD), 1, "{out}");
        assert!(
            out.ends_with(&format!(
                "  {WS_NOTE_HEAD}rmap imports 'a/src/my file.ts' --include-inferred{WS_NOTE_TAIL}"
            )),
            "{out}"
        );
        // A workspace-local observation counted only (not blocking, repo-wide) still prints the
        // label in `by class`, so the note is there.
        let mut r = sample_lg();
        r.observations
            .retain(|o| o.class != "WorkspaceLocalUnedgeable");
        let out = r.render_human();
        assert!(out.contains("WorkspaceLocalUnedgeable=1"));
        assert_eq!(count(&out, WS_NOTE_HEAD), 1, "{out}");

        // Compare listing: the blocking observation keeps its label; the note names the file.
        let compare: ImportsCompareResponse = serde_json::from_value(serde_json::json!({
            "file": "app/src/x.ts",
            "imports": [],
            "comparison": {
                "status": "NoLossLivegraphSuperset",
                "blocking_observations": [
                    {"raw_specifier": "@scope/wslocal", "class": "WorkspaceLocalUnedgeable"},
                    {"raw_specifier": "@/alias", "class": "PackageUnresolved"}
                ]
            },
        }))
        .unwrap();
        let out = compare.render_human();
        assert!(
            out.contains("! @scope/wslocal [WorkspaceLocalUnedgeable]"),
            "{out}"
        );
        assert_eq!(count(&out, WS_NOTE_HEAD), 1, "{out}");
        assert!(
            out.ends_with(&format!(
                "    ! @/alias [PackageUnresolved]\n  {WS_NOTE_HEAD}rmap imports app/src/x.ts --include-inferred{WS_NOTE_TAIL}"
            )),
            "{out}"
        );

        // Readiness report (repo-wide): the note follows the blocking classes.
        let r: ImportsReadinessReport = serde_json::from_value(serde_json::json!({
            "display_name": "amodx", "verdict": "GREEN", "coverage_complete": true,
            "metrics": {"blocking_observation_total": 5,
                        "blocking_observation_by_class": {"WorkspaceLocalUnedgeable": 5}},
            "regressions": [], "unknowns": []
        }))
        .unwrap();
        let out = r.render_human();
        assert!(out.contains("WorkspaceLocalUnedgeable=5"), "{out}");
        assert_eq!(count(&out, WS_NOTE_HEAD), 1, "{out}");
        assert!(
            out.contains(&format!(
                "    by class: WorkspaceLocalUnedgeable=5\n  {WS_NOTE_HEAD}rmap modules deps --include-inferred{WS_NOTE_TAIL}"
            )),
            "{out}"
        );
    }

    #[test]
    fn renders_without_the_workspace_label_print_no_note() {
        let mut r = sample_lg();
        r.observations
            .retain(|o| o.class != "WorkspaceLocalUnedgeable");
        r.observation_class_counts
            .remove("WorkspaceLocalUnedgeable");
        r.observation_count = 1;
        r.blocking_observation_count = 0;
        assert!(!r.render_human().contains("note:"));
        r.file_filter = Some("a/src/x.ts".to_string());
        assert!(!r.render_human().contains("note:"));

        let compare: ImportsCompareResponse = serde_json::from_value(serde_json::json!({
            "file": "app/src/x.ts",
            "imports": [],
            "comparison": {"status": "NoLossLivegraphSuperset",
                           "blocking_observations": [{"raw_specifier": "@/alias", "class": "PackageUnresolved"}]},
        }))
        .unwrap();
        assert!(!compare.render_human().contains("note:"));

        let report: ImportsReadinessReport = serde_json::from_value(serde_json::json!({
            "display_name": "x", "verdict": "GREEN", "coverage_complete": true,
            "metrics": {"blocking_observation_by_class": {"PackageUnresolved": 2}},
            "regressions": [], "unknowns": []
        }))
        .unwrap();
        assert!(!report.render_human().contains("note:"));
        // The default listing never prints the note.
        assert!(!sample_imports().render_human().contains("note:"));
    }

    // ── D-TWR-NOTE (OC-2): the note states only what the rule does, on every render ──

    /// The approved note line (SLICE_DOC §2.1 item 7) with its command.
    fn ws_note_line(command: &str) -> String {
        format!(
            "  note: [WorkspaceLocalUnedgeable] is an import of this repo's own workspace package; \
             this route forms no edge for it until one rule serves both routes — the default route \
             may record it as an inferred import of the package's source entry, but only for an \
             import of the package root whose declared entries are not indexed ({command}); for any \
             other import with this label, open the import and look inside\n"
        )
    }

    /// A LiveGraph listing filtered to `file` whose one blocking observation is `specifier`
    /// labelled `WorkspaceLocalUnedgeable`.
    fn lg_for_one_labelled_import(
        display: &str,
        file: &str,
        specifier: &str,
    ) -> LivegraphImportsResponse {
        LivegraphImportsResponse {
            display_name: display.to_string(),
            file_filter: Some(file.to_string()),
            edges: vec![],
            edge_count: 0,
            observations: vec![LgImportObservation {
                source_file: file.to_string(),
                raw_specifier: specifier.to_string(),
                class: "WorkspaceLocalUnedgeable".to_string(),
                blocking: true,
            }],
            observation_count: 1,
            blocking_observation_count: 1,
            observation_class_counts: BTreeMap::from([("WorkspaceLocalUnedgeable".to_string(), 1)]),
            module_cycle_completeness: "IncompleteImportClasses".to_string(),
            module_cycle_answer_class: "Exact".to_string(),
            freshness: "Fresh".to_string(),
            missing_partitions: vec![],
        }
    }

    fn compare_for_one_labelled_import(file: &str, specifier: &str) -> ImportsCompareResponse {
        serde_json::from_value(serde_json::json!({
            "file": file,
            "imports": [],
            "comparison": {
                "status": "NoLossLivegraphSuperset",
                "blocking_observations": [
                    {"raw_specifier": specifier, "class": "WorkspaceLocalUnedgeable"}
                ]
            },
        }))
        .unwrap()
    }

    fn report_counting_the_label(display: &str) -> ImportsReadinessReport {
        serde_json::from_value(serde_json::json!({
            "display_name": display, "verdict": "GREEN", "coverage_complete": true,
            "metrics": {"blocking_observation_total": 1,
                        "blocking_observation_by_class": {"WorkspaceLocalUnedgeable": 1}},
            "regressions": [], "unknowns": []
        }))
        .unwrap()
    }

    /// The note on one render: the label kept, exactly one note line equal to the approved text,
    /// its qualifying clauses present, and no line claiming that the import IS recorded.
    fn assert_note_states_only_what_the_rule_does(out: &str, label: &str, command: &str) {
        assert!(out.contains(label), "the label is kept: {out}");
        let notes: Vec<&str> = out.lines().filter(|l| l.contains("note:")).collect();
        assert_eq!(notes.len(), 1, "exactly one note line: {out}");
        assert_eq!(format!("{}\n", notes[0]), ws_note_line(command), "{out}");
        assert!(out.contains("may record"), "{out}");
        assert!(
            out.contains(
                "only for an import of the package root whose declared entries are not indexed"
            ),
            "{out}"
        );
        assert!(out.contains("open the import and look inside"), "{out}");
        assert!(
            !out.lines().any(|l| l.contains("records it as")),
            "no line claims the import is recorded: {out}"
        );
    }

    /// amodx `admin/src/components/editor/Toolbar.tsx:10`
    /// `import { getPluginList } from "@amodx/plugins/admin";` — a workspace SUBPATH import, which
    /// the LiveGraph classifier labels `WorkspaceLocalUnedgeable` and the workspace-source rule
    /// leaves unresolved: no render may claim the default route records it.
    #[test]
    fn workspace_note_never_claims_an_inferred_edge_for_a_subpath_import_on_every_render() {
        let file = "admin/src/components/editor/Toolbar.tsx";
        let spec = "@amodx/plugins/admin";
        let file_cmd = "rmap imports admin/src/components/editor/Toolbar.tsx --include-inferred";
        let out = lg_for_one_labelled_import("amodx", file, spec).render_human();
        assert_note_states_only_what_the_rule_does(
            &out,
            "@amodx/plugins/admin  [WorkspaceLocalUnedgeable] BLOCKING",
            file_cmd,
        );
        let out = compare_for_one_labelled_import(file, spec).render_human();
        assert_note_states_only_what_the_rule_does(
            &out,
            "! @amodx/plugins/admin [WorkspaceLocalUnedgeable]",
            file_cmd,
        );
        let out = report_counting_the_label("amodx").render_human();
        assert_note_states_only_what_the_rule_does(
            &out,
            "WorkspaceLocalUnedgeable=1",
            "rmap modules deps --include-inferred",
        );
    }

    /// storybook `code/frameworks/ember/template/cli/Button.stories.js:1`
    /// `import { linkTo } from '@storybook/addon-links';` — a ROOT import whose member declares an
    /// indexed target (`"code": "./src/index.ts"`), which the rule leaves unresolved: no render may
    /// claim the default route records it.
    #[test]
    fn workspace_note_never_claims_an_inferred_edge_for_a_root_import_whose_declared_entry_is_indexed_on_every_render(
    ) {
        let file = "code/frameworks/ember/template/cli/Button.stories.js";
        let spec = "@storybook/addon-links";
        let file_cmd =
            "rmap imports code/frameworks/ember/template/cli/Button.stories.js --include-inferred";
        let out = lg_for_one_labelled_import("storybook", file, spec).render_human();
        assert_note_states_only_what_the_rule_does(
            &out,
            "@storybook/addon-links  [WorkspaceLocalUnedgeable] BLOCKING",
            file_cmd,
        );
        let out = compare_for_one_labelled_import(file, spec).render_human();
        assert_note_states_only_what_the_rule_does(
            &out,
            "! @storybook/addon-links [WorkspaceLocalUnedgeable]",
            file_cmd,
        );
        let out = report_counting_the_label("storybook").render_human();
        assert_note_states_only_what_the_rule_does(
            &out,
            "WorkspaceLocalUnedgeable=1",
            "rmap modules deps --include-inferred",
        );
    }

    // ── IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12; SLICE_DOC §2.2 is binding) ──────────────

    /// One unresolved row as the daemon serializes it.
    #[allow(clippy::too_many_arguments)]
    fn urow(
        target_key: &str,
        recorded_specifier: Option<&str>,
        decoded_target_path: Option<&str>,
        line: Option<u64>,
        category: &str,
        classification: &str,
        basis_code: &str,
        basis: Option<&str>,
        candidates: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "target_key": target_key,
            "recorded_specifier": recorded_specifier,
            "decoded_target_path": decoded_target_path,
            "line": line,
            "category": category,
            "classification": classification,
            "basis_code": basis_code,
            "basis": basis,
            "candidates": candidates,
        })
    }

    /// A file-not-found row whose recorded specifier is its key, with no reason and no candidates.
    fn not_found(
        spec: &str,
        line: u64,
        classification: &str,
        basis_code: &str,
    ) -> serde_json::Value {
        urow(
            spec,
            Some(spec),
            None,
            Some(line),
            "imports_file_not_found",
            classification,
            basis_code,
            None,
            serde_json::Value::Null,
        )
    }

    /// A default-view listing of `file` with `imports` rows and the given new fields (each `None`
    /// is an ABSENT key).
    fn listing(
        file: &str,
        imports: Vec<serde_json::Value>,
        unresolved: Option<serde_json::Value>,
        unresolved_count: Option<serde_json::Value>,
        language: Option<serde_json::Value>,
        listing_coverage: Option<serde_json::Value>,
    ) -> ImportsResponse {
        let mut v = serde_json::json!({
            "file": file,
            "imports": imports,
            "import_view": default_view(),
            "import_remainder": zero_remainder(),
        });
        for (key, value) in [
            ("unresolved", unresolved),
            ("unresolved_count", unresolved_count),
            ("language", language),
            ("listing_coverage", listing_coverage),
        ] {
            if let Some(value) = value {
                v[key] = value;
            }
        }
        serde_json::from_value(v).expect("the listing always decodes")
    }

    /// A listing whose unresolved rows are `rows` (count agreeing).
    fn listing_with_rows(
        file: &str,
        imports: Vec<serde_json::Value>,
        rows: Vec<serde_json::Value>,
        language: &str,
        omitted_forms: serde_json::Value,
    ) -> ImportsResponse {
        let n = rows.len();
        listing(
            file,
            imports,
            Some(serde_json::Value::Array(rows)),
            Some(serde_json::json!(n)),
            Some(serde_json::json!(language)),
            Some(serde_json::json!({ "omitted_forms": omitted_forms })),
        )
    }

    fn ts_forms() -> serde_json::Value {
        serde_json::json!([
            "type-only imports and re-exports from a specifier without a leading dot",
            "dynamic import() and require() calls",
            "import … = require(…) statements",
            "import statements below the top level"
        ])
    }

    fn js_forms() -> serde_json::Value {
        serde_json::json!([
            "re-exports from a specifier without a leading dot",
            "dynamic import() and require() calls"
        ])
    }

    const TS_LIMIT: &str = "listing limit: this listing omits type-only imports and re-exports from a specifier without a leading dot, dynamic import() and require() calls, import … = require(…) statements or import statements below the top level — open the file for those";
    const JS_LIMIT: &str = "listing limit: this listing omits re-exports from a specifier without a leading dot or dynamic import() and require() calls — open the file for those";
    const RELATIVE: &str = "recorded specifier is crate, super or self, or starts with ., crate::, super:: or self::, or the specifier or target key contains :FILE";

    fn lines_starting(out: &str, prefix: &str) -> usize {
        out.lines().filter(|l| l.starts_with(prefix)).count()
    }

    #[test]
    fn imports_renders_unresolved_rows_with_their_category_classification_and_basis() {
        let rows = vec![
            not_found(
                "@tiptap/react",
                1,
                "external_library_candidate",
                "specifier_matches_package_dependency",
            ),
            not_found(
                "react",
                2,
                "external_library_candidate",
                "specifier_matches_package_dependency",
            ),
            not_found(
                "@/components/ui/button",
                3,
                "internal_candidate",
                "specifier_matches_project_alias",
            ),
            // A `./` TS specifier, stored under a `:FILE` key, recorded as written.
            urow(
                "r1:code/addons/a11y/dist/manager:FILE",
                Some("./dist/manager.js"),
                Some("code/addons/a11y/dist/manager"),
                Some(4),
                "imports_file_not_found",
                "internal_candidate",
                "relative_import_target_unresolved",
                None,
                serde_json::Value::Null,
            ),
            // A C/C++ quoted include, recorded `./…` by the extractor.
            not_found(
                "./gtest/gtest.h",
                10,
                "internal_candidate",
                "relative_import_target_unresolved",
            ),
            // A Rust `crate::` path (no rawPath: the stored key prints).
            urow(
                "crate::graph::node",
                None,
                None,
                Some(11),
                "imports_file_not_found",
                "internal_candidate",
                "relative_import_target_unresolved",
                None,
                serde_json::Value::Null,
            ),
            // A `:FILE` key with no recorded specifier: the decoded path, marked not confirmed.
            urow(
                "r1:src/missing:FILE",
                None,
                Some("src/missing"),
                Some(12),
                "imports_file_not_found",
                "internal_candidate",
                "relative_import_target_unresolved",
                None,
                serde_json::Value::Null,
            ),
            urow(
                "map",
                None,
                None,
                Some(13),
                "imports_file_not_found",
                "external_library_candidate",
                "specifier_matches_runtime_module",
                None,
                serde_json::Value::Null,
            ),
            urow(
                "helpers::util",
                None,
                None,
                None,
                "imports_file_not_found",
                "internal_candidate",
                "rust_crate_internal_module_heuristic",
                None,
                serde_json::Value::Null,
            ),
            not_found("aws-lambda", 14, "unknown", "no_supporting_signal"),
            urow(
                "ngx_time.h",
                None,
                None,
                Some(52),
                "imports_ambiguous_match",
                "unknown",
                "no_supporting_signal",
                Some("ambiguous_basename"),
                serde_json::json!(["src/os/unix/ngx_time.h", "src/os/win32/ngx_time.h"]),
            ),
        ];
        let resp = listing_with_rows(
            "admin/src/components/editor/Toolbar.tsx",
            vec![row_json("src/x.ts", "static", "ts-core:0.2.0", None)],
            rows,
            "tsx",
            ts_forms(),
        );
        let out = resp.render_human();
        // After the resolved rows and the partition lines.
        assert!(
            out.contains("  src/x.ts  depth=1  static\n11 imports without a confirmed target:\n"),
            "{out}"
        );
        for line in [
            "  @tiptap/react  line 1  external library? (specifier matches a dependency declared in the manifest)",
            "  react  line 2  external library? (specifier matches a dependency declared in the manifest)",
            "  @/components/ui/button  line 3  this repository? (specifier matches a tsconfig paths alias)",
            &format!("  ./dist/manager.js  line 4  this repository? ({RELATIVE})"),
            &format!("  ./gtest/gtest.h  line 10  this repository? ({RELATIVE})"),
            &format!("  crate::graph::node  line 11  this repository? ({RELATIVE})"),
            &format!("  src/missing (decoded from the stored key — not a confirmed target)  line 12  this repository? ({RELATIVE})"),
            "  map  line 13  external library? (specifier or its part before :: is on the runtime-module list of this build)",
            "  helpers::util  line ?  this repository? (metadata carries a specifier and no rawPath, and the specifier's first segment before :: starts with a lowercase letter and holds only lowercase letters, digits or underscores)",
            "  aws-lambda  line 14  unknown (no classifier signal)",
            "  ngx_time.h  line 52  several indexed files matched (same file name in 2 places) — 2 candidates: src/os/unix/ngx_time.h, src/os/win32/ngx_time.h · unknown (no classifier signal)",
        ] {
            assert!(out.lines().any(|l| l == line), "missing [{line}]:\n{out}");
        }
        // A decoded path is never printed bare, as if it were a confirmed target.
        assert!(
            !out.lines().any(|l| l.starts_with("  src/missing  ")),
            "{out}"
        );
        // A file-not-found row with no reason and no carrier has no category phrase and no ` · `.
        let react = out.lines().find(|l| l.starts_with("  react  ")).unwrap();
        assert!(!react.contains(" · "), "{react}");
        assert!(out.contains(&format!("{TS_LIMIT}\n")), "{out}");
        assert!(
            out.ends_with(&format!("{TS_LIMIT}\n")),
            "the limit follows the block: {out}"
        );
    }

    #[test]
    fn imports_ambiguous_unresolved_row_lists_its_candidates_never_picks_one() {
        let rows = vec![
            urow(
                "ngx_time.h",
                None,
                None,
                Some(52),
                "imports_ambiguous_match",
                "unknown",
                "no_supporting_signal",
                Some("ambiguous_basename"),
                serde_json::json!(["src/os/unix/ngx_time.h", "src/os/win32/ngx_time.h"]),
            ),
            // An include-root overlap: neither reason nor candidates recorded.
            urow(
                "port.h",
                Some("./port.h"),
                None,
                Some(3),
                "imports_ambiguous_match",
                "unknown",
                "no_supporting_signal",
                None,
                serde_json::Value::Null,
            ),
            // A Java ambiguous suffix: no candidates recorded.
            urow(
                "com.foo.Bar",
                None,
                None,
                Some(4),
                "imports_ambiguous_suffix",
                "unknown",
                "no_supporting_signal",
                None,
                serde_json::Value::Null,
            ),
            // An unreadable carrier: no count derived from candidates that are not there.
            urow(
                "env.h",
                None,
                None,
                Some(5),
                "imports_ambiguous_match",
                "unknown",
                "no_supporting_signal",
                Some("ambiguous_basename"),
                serde_json::json!({"unreadable": "r2:env.h:FILE"}),
            ),
            // The TS workspace ambiguity keeps its own category; its reason prints bare.
            urow(
                "@fx/engine",
                Some("@fx/engine"),
                None,
                Some(6),
                "imports_file_not_found",
                "external_library_candidate",
                "specifier_matches_package_dependency",
                Some("ambiguous_workspace_source_entry"),
                serde_json::json!([
                    "packages/a/src/index.ts",
                    "packages/b/src/index.ts",
                    "packages/a/dist/index.js"
                ]),
            ),
        ];
        let out = listing_with_rows(
            "src/core/ngx_core.h",
            vec![],
            rows,
            "c",
            serde_json::Value::Null,
        )
        .render_human();
        for line in [
            "  ngx_time.h  line 52  several indexed files matched (same file name in 2 places) — 2 candidates: src/os/unix/ngx_time.h, src/os/win32/ngx_time.h · unknown (no classifier signal)",
            "  ./port.h  line 3  several indexed files matched (reason not recorded) — candidate paths not recorded with this import · unknown (no classifier signal)",
            "  com.foo.Bar  line 4  several indexed .java files end with the specifier's path (or a prefix of it) — candidate paths not recorded with this import · unknown (no classifier signal)",
            "  env.h  line 5  several indexed files matched (same file name in several places) — candidates unreadable (\"r2:env.h:FILE\") · unknown (no classifier signal)",
            "  @fx/engine  line 6  several workspace source entries — 3 candidates: packages/a/src/index.ts, packages/b/src/index.ts, packages/a/dist/index.js · external library? (specifier matches a dependency declared in the manifest)",
        ] {
            assert!(out.lines().any(|l| l == line), "missing [{line}]:\n{out}");
        }
        // Never a count for candidates that were not recorded, never one candidate picked.
        assert!(!out.contains("0 candidates"), "{out}");
        assert!(!out.contains("→"), "{out}");
    }

    // ── TS-ALIAS-RESOLUTION-1: the alias rows (SLICE_DOC §2.2 is binding) ──

    /// An unresolved alias row of `several/src/main.ts`'s shape with the given reason and candidates.
    fn alias_row(basis: Option<&str>, candidates: serde_json::Value) -> serde_json::Value {
        urow(
            "@/x",
            Some("@/x"),
            None,
            Some(1),
            "imports_file_not_found",
            "internal_candidate",
            "specifier_matches_project_alias",
            basis,
            candidates,
        )
    }

    fn alias_listing_lines(rows: Vec<serde_json::Value>) -> Vec<String> {
        let out = listing_with_rows(
            "several/src/main.ts",
            vec![],
            rows,
            "typescript",
            ts_forms(),
        )
        .render_human();
        out.lines()
            .filter(|l| l.starts_with("  @/x  "))
            .map(str::to_string)
            .collect()
    }

    const ALIAS_TAIL: &str = " · this repository? (specifier matches a tsconfig paths alias)";

    #[test]
    fn unresolved_alias_row_whose_applicable_tsconfig_paths_reach_several_indexed_files_states_every_candidate(
    ) {
        assert_eq!(
            alias_listing_lines(vec![alias_row(
                Some("ambiguous_tsconfig_paths"),
                serde_json::json!(["several/lib/x.ts", "several/lib/x/index.ts"]),
            )]),
            vec![format!(
                "  @/x  line 1  the tsconfig paths that apply to this file reach 2 indexed files — 2 candidates: several/lib/x.ts, several/lib/x/index.ts{ALIAS_TAIL}"
            )]
        );
    }

    #[test]
    fn unresolved_alias_row_without_a_readable_candidate_list_says_several_indexed_files() {
        assert_eq!(
            alias_listing_lines(vec![alias_row(
                Some("ambiguous_tsconfig_paths"),
                serde_json::Value::Null,
            )]),
            vec![format!(
                "  @/x  line 1  the tsconfig paths that apply to this file reach several indexed files — candidate paths not recorded with this import{ALIAS_TAIL}"
            )]
        );
        assert_eq!(
            alias_listing_lines(vec![alias_row(
                Some("ambiguous_tsconfig_paths"),
                serde_json::json!({"unreadable": "r9:lib/x.ts:FILE"}),
            )]),
            vec![format!(
                "  @/x  line 1  the tsconfig paths that apply to this file reach several indexed files — candidates unreadable (\"r9:lib/x.ts:FILE\"){ALIAS_TAIL}"
            )]
        );
    }

    #[test]
    fn unresolved_alias_row_whose_tsconfig_paths_patterns_tie_states_the_tie_and_every_candidate() {
        let tie =
            "two or more tsconfig paths patterns tie for the longest prefix; together they reach";
        assert_eq!(
            alias_listing_lines(vec![alias_row(
                Some("tied_tsconfig_paths_patterns"),
                serde_json::json!(["tied/a/x.ts", "tied/b/x.ts"]),
            )]),
            vec![format!(
                "  @/x  line 1  {tie} 2 indexed files — 2 candidates: tied/a/x.ts, tied/b/x.ts{ALIAS_TAIL}"
            )]
        );
        assert_eq!(
            alias_listing_lines(vec![alias_row(
                Some("tied_tsconfig_paths_patterns"),
                serde_json::json!(["tied/b/x.ts"]),
            )]),
            vec![format!(
                "  @/x  line 1  {tie} 1 indexed file — 1 candidate: tied/b/x.ts{ALIAS_TAIL}"
            )]
        );
        assert_eq!(
            alias_listing_lines(vec![alias_row(
                Some("tied_tsconfig_paths_patterns"),
                serde_json::Value::Null,
            )]),
            vec![format!(
                "  @/x  line 1  {tie} indexed files — candidate paths not recorded with this import{ALIAS_TAIL}"
            )]
        );
    }

    #[test]
    fn static_tsconfig_paths_row_renders_its_basis_and_a_disagreeing_reason_is_unreadable() {
        const BUTTON: &str = "admin/src/components/ui/button.tsx";
        let resp = listing_with_inferred(vec![row_json(
            BUTTON,
            "static",
            "ts-core:0.2.0",
            Some(serde_json::json!({"basis": "tsconfig_paths", "candidates": [BUTTON]})),
        )]);
        assert_eq!(
            only_row(&resp),
            format!("  {BUTTON}  depth=1  static (resolved through tsconfig paths)")
        );
        for reason in [
            serde_json::json!({"basis": "tsconfig_paths", "candidates": ["admin/src/other.tsx"]}),
            serde_json::json!({"basis": "tsconfig_paths", "candidates": [BUTTON, "admin/src/y.ts"]}),
            serde_json::json!({"basis": "tsconfig_paths", "candidates": []}),
        ] {
            let resp = listing_with_inferred(vec![row_json(
                BUTTON,
                "static",
                "ts-core:0.2.0",
                Some(reason.clone()),
            )]);
            let line = only_row(&resp);
            assert!(
                line.starts_with(&format!("  {BUTTON}  depth=1  static: reason unreadable ("))
                    && line.ends_with(')'),
                "{reason}: {line}"
            );
            assert!(
                !line.contains("resolved through"),
                "{reason}: never evidence: {line}"
            );
        }
    }

    #[test]
    fn static_unique_suffix_row_still_renders_static() {
        let resp = listing_with_inferred(vec![
            row_json(
                "lib/src/util/foo.h",
                "static",
                "cpp-core:0.2.0",
                Some(
                    serde_json::json!({"basis": "unique_suffix", "candidates": ["lib/src/util/foo.h"]}),
                ),
            ),
            row_json("src/b.ts", "static", "ts-core:0.2.0", None),
        ]);
        let out = resp.render_human();
        for line in [
            "  lib/src/util/foo.h  depth=1  static",
            "  src/b.ts  depth=1  static",
        ] {
            assert!(out.lines().any(|l| l == line), "missing [{line}]:\n{out}");
        }
    }

    #[test]
    fn static_row_whose_stored_reason_is_unreadable_prints_static_reason_unreadable_for_every_basis(
    ) {
        // oracle-corrections.md OC-3, the served seam: storage decodes a carrier that is present but
        // malformed or contradicts its edge to `Unreadable` before the row reaches the renderer; a
        // static row prints that state with storage's text, whatever basis the carrier named.
        for (file, extractor, text) in [
            (
                "admin/src/components/ui/button.tsx",
                "ts-core:0.2.0",
                "candidate r1:src/other.ts:FILE is not the edge's target",
            ),
            (
                "lib/src/util/foo.h",
                "cpp-core:0.2.0",
                "2 candidates under the unique basis unique_suffix",
            ),
            ("src/a.ts", "ts-core:0.2.0", "candidates is empty"),
        ] {
            let resp = listing_with_inferred(vec![row_json(
                file,
                "static",
                extractor,
                Some(serde_json::json!({ "unreadable": text })),
            )]);
            assert_eq!(
                only_row(&resp),
                format!("  {file}  depth=1  static: reason unreadable ({text})")
            );
        }
    }

    #[test]
    fn imports_zero_state_names_the_language_and_the_listing_coverage() {
        let out = listing_with_rows(
            "renderer/next.config.ts",
            vec![],
            vec![],
            "typescript",
            ts_forms(),
        )
        .render_human();
        assert_eq!(
            out,
            "Imports: renderer/next.config.ts\n\n0 imports (language: typescript; this index lists no imports for this file — this listing omits type-only imports and re-exports from a specifier without a leading dot, dynamic import() and require() calls, import … = require(…) statements or import statements below the top level)\n"
        );
        let out =
            listing_with_rows("a.js", vec![], vec![], "javascript", js_forms()).render_human();
        assert!(
            out.contains("0 imports (language: javascript; this index lists no imports for this file — this listing omits re-exports from a specifier without a leading dot or dynamic import() and require() calls)\n"),
            "{out}"
        );
        assert_eq!(lines_starting(&out, "listing limit:"), 0, "{out}");
        assert!(!out.contains("without a confirmed target"), "{out}");
    }

    #[test]
    fn imports_zero_state_without_recorded_omitted_forms_says_not_recorded_never_complete() {
        let zero = |language: Option<serde_json::Value>, coverage: Option<serde_json::Value>| {
            listing(
                "table/merger.h",
                vec![],
                Some(serde_json::json!([])),
                Some(serde_json::json!(0)),
                language,
                coverage,
            )
            .render_human()
        };
        assert!(zero(Some(serde_json::json!("cpp")), Some(serde_json::json!({"omitted_forms": null})))
            .contains("0 imports (language: cpp; this index lists no imports for this file — listing coverage not recorded for cpp)\n"));
        for lang in [
            None,
            Some(serde_json::Value::Null),
            Some(serde_json::json!("")),
        ] {
            let out = zero(
                lang.clone(),
                Some(serde_json::json!({"omitted_forms": null})),
            );
            assert!(
                out.contains("0 imports (language: unknown; this index lists no imports for this file — listing coverage not recorded (language not recorded))\n"),
                "{lang:?}: {out}"
            );
        }
        assert!(zero(Some(serde_json::json!("cpp")), None)
            .contains("0 imports (language: cpp; this index lists no imports for this file — listing coverage unavailable from this daemon)\n"));
        for bad in [
            serde_json::json!({"omitted_forms": []}),
            serde_json::json!({"omitted_forms": [1]}),
            serde_json::json!({}),
            serde_json::json!([]),
            serde_json::Value::Null,
        ] {
            let out = zero(Some(serde_json::json!("cpp")), Some(bad.clone()));
            assert!(
                out.contains("0 imports (language: cpp; this index lists no imports for this file — listing coverage unreadable)\n"),
                "{bad}: an empty list is never complete coverage: {out}"
            );
        }
        // Never a bare zero-state line.
        assert!(!zero(
            Some(serde_json::json!("cpp")),
            Some(serde_json::json!({"omitted_forms": []}))
        )
        .contains("this file)\n"));
    }

    #[test]
    fn imports_unresolved_absent_renders_unavailable_never_zero() {
        let out = listing("a.ts", vec![], None, None, None, None).render_human();
        assert!(out.contains("\n0 imports\n"), "not the zero-state: {out}");
        assert!(
            out.contains("imports without a confirmed target: unavailable from this daemon — upgrade rmapd\n"),
            "{out}"
        );
        assert!(
            !out.contains("0 imports without a confirmed target"),
            "{out}"
        );
        assert!(!out.contains("this index lists no imports"), "{out}");
        let out = listing(
            "a.ts",
            vec![row_json("b.ts", "static", "", None)],
            None,
            None,
            None,
            None,
        )
        .render_human();
        assert!(out.contains("imports without a confirmed target: unavailable from this daemon — upgrade rmapd\nlisting coverage: unavailable from this daemon\n"), "{out}");
    }

    #[test]
    fn imports_unresolved_count_disagreeing_with_its_rows_renders_unreadable() {
        let rows = serde_json::json!([
            not_found(
                "react",
                1,
                "external_library_candidate",
                "specifier_matches_package_dependency"
            ),
            not_found(
                "vue",
                2,
                "external_library_candidate",
                "specifier_matches_package_dependency"
            ),
        ]);
        for (count, text) in [
            (Some(serde_json::json!(3)), "3"),
            (Some(serde_json::json!(0)), "0"),
            (Some(serde_json::json!("2")), "\"2\""),
            (Some(serde_json::json!(-2)), "-2"),
            (Some(serde_json::Value::Null), "null"),
            (None, "missing"),
        ] {
            let out = listing("a.ts", vec![], Some(rows.clone()), count, None, None).render_human();
            assert!(
                out.contains(&format!("imports without a confirmed target: unreadable (count {text} disagrees with 2 rows)\n")),
                "{text}: {out}"
            );
            assert!(
                !out.contains("  react  "),
                "rows are not printed under a disagreeing count: {out}"
            );
        }
    }

    #[test]
    fn imports_unknown_category_classification_or_basis_renders_verbatim_without_a_phrase() {
        let rows = vec![
            urow(
                "x",
                Some("x"),
                None,
                Some(1),
                "imports_mystery",
                "maybe_candidate",
                "new_basis_code",
                None,
                serde_json::Value::Null,
            ),
            urow(
                "y",
                Some("y"),
                None,
                Some(2),
                "imports_ambiguous_match",
                "unknown",
                "no_supporting_signal",
                Some("ambiguous_other"),
                serde_json::json!(["a/y", "b/y"]),
            ),
            // A non-string reason arrives as its JSON text: verbatim and marked.
            urow(
                "z",
                Some("z"),
                None,
                Some(3),
                "imports_wildcard",
                "unknown",
                "no_supporting_signal",
                Some("7"),
                serde_json::Value::Null,
            ),
        ];
        let out =
            listing_with_rows("a.ts", vec![], rows, "cpp", serde_json::Value::Null).render_human();
        for line in [
            "  x  line 1  imports_mystery (no phrase for this category in this build) · maybe_candidate (no phrase for this classification in this build) (new_basis_code (no phrase for this basis in this build))",
            "  y  line 2  several indexed files matched (ambiguous_other (no phrase for this reason in this build)) — 2 candidates: a/y, b/y · unknown (no classifier signal)",
            "  z  line 3  wildcard import (7 (no phrase for this reason in this build)) · unknown (no classifier signal)",
        ] {
            assert!(out.lines().any(|l| l == line), "missing [{line}]:\n{out}");
        }
        // An unknown string is never mapped to a known phrase.
        let x = out.lines().find(|l| l.starts_with("  x  ")).unwrap();
        for known in [
            "external library?",
            "this repository?",
            "several indexed",
            "no classifier signal",
            "wildcard import",
        ] {
            assert!(!x.contains(known), "{known}: {x}");
        }
    }

    #[test]
    fn imports_malformed_unresolved_payload_renders_unreadable_never_a_parse_failure() {
        let good = not_found(
            "react",
            1,
            "external_library_candidate",
            "specifier_matches_package_dependency",
        );
        let with = |field: &str, value: serde_json::Value| {
            let mut row = good.clone();
            row[field] = value;
            serde_json::json!([row])
        };
        let mut no_key = good.clone();
        no_key.as_object_mut().unwrap().remove("recorded_specifier");
        let cases = vec![
            serde_json::json!({"rows": []}),
            serde_json::json!("react"),
            serde_json::json!([1]),
            with("target_key", serde_json::json!(7)),
            with("category", serde_json::Value::Null),
            with("classification", serde_json::json!(["x"])),
            with("basis_code", serde_json::json!(true)),
            with("recorded_specifier", serde_json::json!(3)),
            with("decoded_target_path", serde_json::json!({})),
            with("line", serde_json::json!("1")),
            with("line", serde_json::json!(-1)),
            with("candidates", serde_json::json!(5)),
            with("candidates", serde_json::json!([1])),
            // F-001: an empty candidate list is never a measured zero (storage refuses it too).
            with("candidates", serde_json::json!([])),
            with("candidates", serde_json::json!({"unreadable": 5})),
            with("candidates", serde_json::json!({"other": "x"})),
            serde_json::json!([no_key]),
        ];
        for bad in cases {
            let resp = listing(
                "a.ts",
                vec![row_json("b.ts", "static", "", None)],
                Some(bad.clone()),
                Some(serde_json::json!(1)),
                None,
                None,
            );
            let out = resp.render_human();
            assert!(
                out.contains("  b.ts  depth=1  static\n"),
                "the listing still renders: {out}"
            );
            assert!(
                out.contains("imports without a confirmed target: unreadable ("),
                "{bad}: {out}"
            );
            assert!(!out.contains("  react  "), "{bad}: {out}");
            assert!(
                !out.contains("0 candidates"),
                "{bad}: never a zero count: {out}"
            );
        }
    }

    #[test]
    fn imports_ts_and_js_listings_state_their_listing_limit_exactly_once() {
        let rows = vec![not_found(
            "react",
            2,
            "external_library_candidate",
            "specifier_matches_package_dependency",
        )];
        let out =
            listing_with_rows("a.tsx", vec![], rows.clone(), "tsx", ts_forms()).render_human();
        assert_eq!(lines_starting(&out, "listing limit:"), 1, "{out}");
        assert!(out.ends_with(&format!("{TS_LIMIT}\n")), "{out}");
        let out = listing_with_rows(
            "a.ts",
            vec![row_json("b.ts", "static", "", None)],
            vec![],
            "typescript",
            ts_forms(),
        )
        .render_human();
        assert_eq!(lines_starting(&out, "listing limit:"), 1, "{out}");
        assert!(
            out.ends_with(&format!(
                "0 imports without a confirmed target\n{TS_LIMIT}\n"
            )),
            "{out}"
        );
        let out = listing_with_rows("Button.stories.js", vec![], rows, "javascript", js_forms())
            .render_human();
        assert_eq!(lines_starting(&out, "listing limit:"), 1, "{out}");
        assert!(out.contains(&format!("{JS_LIMIT}\n")), "{out}");
        // In the zero-state the count line carries it — never a second statement.
        let out = listing_with_rows("next.config.ts", vec![], vec![], "typescript", ts_forms())
            .render_human();
        assert_eq!(lines_starting(&out, "listing limit:"), 0, "{out}");
        assert_eq!(out.matches("this listing omits").count(), 1, "{out}");
        // A malformed coverage outside the zero-state.
        let out = listing(
            "a.ts",
            vec![row_json("b.ts", "static", "", None)],
            Some(serde_json::json!([])),
            Some(serde_json::json!(0)),
            Some(serde_json::json!("typescript")),
            Some(serde_json::json!({"omitted_forms": []})),
        )
        .render_human();
        assert!(out.ends_with("listing limit: unreadable\n"), "{out}");
        // A C/C++ listing (null) never prints it.
        let out = listing_with_rows(
            "a.cc",
            vec![row_json("b.h", "static", "", None)],
            vec![],
            "cpp",
            serde_json::Value::Null,
        )
        .render_human();
        assert_eq!(lines_starting(&out, "listing limit:"), 0, "{out}");
    }

    #[test]
    fn imports_non_ts_listing_states_listing_coverage_not_recorded() {
        let rows = vec![urow(
            "ngx_time.h",
            None,
            None,
            Some(52),
            "imports_ambiguous_match",
            "unknown",
            "no_supporting_signal",
            Some("ambiguous_basename"),
            serde_json::json!(["a/ngx_time.h", "b/ngx_time.h"]),
        )];
        let out = listing_with_rows(
            "src/core/ngx_core.h",
            vec![row_json(
                "src/core/ngx_config.h",
                "static",
                "c-core:0.1.0",
                None,
            )],
            rows,
            "c",
            serde_json::Value::Null,
        )
        .render_human();
        assert_eq!(lines_starting(&out, "listing coverage:"), 1, "{out}");
        assert!(
            out.ends_with("listing coverage: not recorded for c — open the file to confirm\n"),
            "{out}"
        );
        assert_eq!(lines_starting(&out, "listing limit:"), 0, "{out}");
        for lang in [
            None,
            Some(serde_json::Value::Null),
            Some(serde_json::json!("")),
        ] {
            let out = listing(
                "a.x",
                vec![row_json("b.x", "static", "", None)],
                Some(serde_json::json!([])),
                Some(serde_json::json!(0)),
                lang.clone(),
                Some(serde_json::json!({"omitted_forms": null})),
            )
            .render_human();
            assert!(out.ends_with("listing coverage: not recorded (language not recorded) — open the file to confirm\n"), "{lang:?}: {out}");
            assert_eq!(lines_starting(&out, "listing limit:"), 0, "{out}");
        }
    }

    #[test]
    fn imports_livegraph_and_compare_renders_name_the_listing_they_do_not_carry() {
        const VLIM: &str = "view limit: this view does not list imports without a confirmed target — rmap imports ";
        let mut lg = sample_lg();
        lg.file_filter = Some("admin/src/components/editor/Toolbar.tsx".to_string());
        let out = lg.render_human();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[1],
            format!("{VLIM}admin/src/components/editor/Toolbar.tsx does"),
            "{out}"
        );
        assert_eq!(out.matches("view limit:").count(), 1, "{out}");
        // Shell-quoted as a printed command runs.
        lg.file_filter = Some("a/my file.ts".to_string());
        assert_eq!(
            lg.render_human().lines().nth(1).unwrap(),
            format!("{VLIM}'a/my file.ts' does")
        );
        // The repo-wide view and the readiness report never print it.
        assert!(!sample_lg().render_human().contains("view limit:"));
        let report: ImportsReadinessReport = serde_json::from_value(serde_json::json!({
            "display_name": "x", "verdict": "GREEN", "coverage_complete": true,
            "metrics": {}, "regressions": [], "unknowns": []
        }))
        .unwrap();
        assert!(!report.render_human().contains("view limit:"));
        let compare: ImportsCompareResponse = serde_json::from_value(serde_json::json!({
            "file": "admin/src/components/editor/Toolbar.tsx",
            "imports": [],
            "comparison": {"status": "SqliteFallback"},
        }))
        .unwrap();
        let out = compare.render_human();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "Imports: admin/src/components/editor/Toolbar.tsx");
        assert_eq!(
            lines[1],
            format!("{VLIM}admin/src/components/editor/Toolbar.tsx does"),
            "{out}"
        );
        assert_eq!(out.matches("view limit:").count(), 1, "{out}");
        // Neither render lists the rows; the added line is no `note:` line, so the workspace note
        // stays the one `note:` line and the last line.
        assert!(!VLIM.contains("note:"));
        assert!(!out.contains(" without a confirmed target:"), "{out}");
        assert!(!out.contains("note:"), "{out}");
        let lg_out = lg.render_human();
        assert!(!lg_out.contains(" without a confirmed target:"), "{lg_out}");
        assert_eq!(lg_out.matches("note:").count(), 1, "{lg_out}");
        assert!(lg_out.lines().last().unwrap().contains("note:"), "{lg_out}");
    }

    #[test]
    fn imports_known_zero_unresolved_count_prints_explicitly_on_a_non_empty_listing() {
        let out = listing_with_rows(
            "packages/plugins/src/admin.ts",
            vec![row_json("a.ts", "static", "", None)],
            vec![],
            "typescript",
            ts_forms(),
        )
        .render_human();
        assert!(
            out.contains(
                "1 import\n\n  a.ts  depth=1  static\n0 imports without a confirmed target\n"
            ),
            "{out}"
        );
        // An empty listing that states an inferred remainder is not the zero-state: the measured
        // zero is stated too.
        let mut v = serde_json::json!({
            "file": "a.py", "imports": [],
            "import_view": default_view(),
            "import_remainder": {
                "tests": {"imports": 0, "edges": 0},
                "inferred": {"imports": 2, "edges": 0},
                "tests_and_inferred": {"imports": 0, "edges": 0}
            },
            "unresolved": [], "unresolved_count": 0, "language": "python",
            "listing_coverage": {"omitted_forms": null}
        });
        let out = serde_json::from_value::<ImportsResponse>(v.clone())
            .unwrap()
            .render_human();
        assert!(out.contains("\n0 imports\n"), "{out}");
        assert!(out.contains("+2 inferred imports, not shown — --include-inferred\n0 imports without a confirmed target\nlisting coverage: not recorded for python — open the file to confirm\n"), "{out}");
        // A partition the daemon did not state is not the zero-state either.
        v.as_object_mut().unwrap().remove("import_view");
        let out = serde_json::from_value::<ImportsResponse>(v)
            .unwrap()
            .render_human();
        assert!(
            out.contains("0 imports without a confirmed target\n"),
            "{out}"
        );
        assert!(!out.contains("this index lists no imports"), "{out}");
    }
}
