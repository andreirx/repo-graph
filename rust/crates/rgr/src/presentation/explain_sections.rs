//! Per-signal section renderers for the `explain` command. Split out of `explain.rs` to keep each module
//! under the 500-line structural guardrail. A second `impl ExplainResponse` block (inherent impls may span
//! modules within the defining crate); `explain.rs` keeps the response struct, header, and target/candidate
//! render.
//!
//! # TRUNCATION-AUDIT-1 — the SECOND (presentation) cap
//!
//! explain truncates in TWO independent places. (1) The agent data layer caps each section's item list at
//! `items_cap(budget)` and reports the cut via `*_truncated` — `--full` maps to `Budget::Full`, whose cap is
//! `usize::MAX`, so the daemon then emits EVERY item (proven at the daemon boundary by
//! `explain_full_budget_uncaps_over_cap_file_listing`). (2) These renderers cap the HUMAN display AGAIN with a
//! per-section `.take(N)` (10/15/5) plus a "... (N more)" note — a fixed readability limit independent of the
//! budget. Without honoring `--full` here, the JSON uncaps but the human render still truncates, defeating the
//! slice's acceptance use case `rmap explain <target> --full | grep <x>` (review-1 #1).
//!
//! So `full` threads in from `run_explain_cmd` (it already parsed the `--full` flag) through
//! [`ExplainResponse::render_human`] to each section renderer. Each computes `shown = if full { items.len() }
//! else { N }`: when `full`, the cap lifts to the full length, `take(shown)` keeps everything, and the
//! overflow guard `items.len() > shown` is naturally false (no "... more" line). When not `full`, behaviour is
//! byte-identical to before. We thread a `bool` rather than extract a closure-based helper: the per-section
//! formatting varies (overflow-note vs not; cycles enumerates and skips empty rings), so the shared helper
//! would carry more parameters than the one-line `shown` it would remove.

use repo_graph_agent::reliability;

use super::explain::{ExplainResponse, ExplainSignal};
use super::{anchor, bullet, heading};

/// RECON-M-R3a (g2u-b): the union-degree second-figure heading suffix — present ONLY when the
/// daemon attached the additive `union` object (W-BOTH with a current measured ledger AND the
/// union degree differs, §5.3.3b) AND the object passes the §5.3.0 labeling gate (review-2
/// item 1: `accounting: "union"` + a derivable coverage basis, via the ONE shared gate). The
/// coverage renders beside the reconciled value — the mandated human frame "reconciled —
/// combined analyses (coverage: …)"; a missing/malformed label SUPPRESSES the union figure
/// (the heading stays exactly the pipeline heading), never renders it unlabeled.
fn union_degree_suffix(evidence: &serde_json::Value) -> String {
    evidence
        .get("union")
        .and_then(|u| {
            let coverage = crate::presentation::witnesses::union_coverage_phrase(u)?;
            let n = u.get("count").and_then(|v| v.as_u64())?;
            Some(format!(
                " · reconciled {n} — combined analyses (coverage: {coverage})"
            ))
        })
        .unwrap_or_default()
}

/// ANCHORS-EVERYWHERE-1 (Tier 1): render ONE caller/callee row — `name (module)` plus the
/// callgraph endpoint's own `path:line` anchor. Shared by [`ExplainResponse::render_callers`]
/// and `render_callees` (two concrete callers, identical row shape — the real duplication that
/// earns the helper). The anchor is appended ONLY when BOTH `file` and `line` are present: they
/// are a single SQLite pair (STANDING HONESTY RULE #2), and a missing line renders no anchor at
/// all (byte-identical to the pre-anchor row), never a fabricated `:0`.
/// Whether [`render_callgraph_item`] renders a `path:line` anchor for `item` (a file and a
/// positive line; [`anchor`] omits a zero line).
fn item_has_anchor(item: &serde_json::Value) -> bool {
    item.get("file").and_then(|v| v.as_str()).is_some()
        && item
            .get("line")
            .and_then(|v| v.as_u64())
            .is_some_and(|l| l > 0)
}

fn render_callgraph_item(item: &serde_json::Value) -> String {
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)");
    let module = item.get("module").and_then(|v| v.as_str());
    let mut row = match module {
        Some(m) => format!("{name} ({m})"),
        None => name.to_string(),
    };
    if let (Some(file), Some(line)) = (
        item.get("file").and_then(|v| v.as_str()),
        item.get("line").and_then(|v| v.as_u64()),
    ) {
        row.push_str(&format!("  {}", anchor(file, Some(line))));
    }
    row
}

/// EXPLAIN-CYCLES-HONEST-1 (§2.1.2): the body of ONE Import-cycles row — `(ring_or_members,
/// optional off-walk line)`. Draws a `->` ring ONLY over a walk the shared
/// [`crate::presentation::cycle_walk_display::validate_walk`] accepts (the SAME ring `cycles`/`orient`
/// draw); with no/empty walk falls back to the honest unordered member listing; with a
/// present-but-malformed walk renders the unreadable line — NEVER a fabricated ring (RC-4).
///
/// The PRESENCE and TYPE of `walk` are made explicit (ECH-IR-001, STANDING HONESTY RULE #1): only an
/// ABSENT key, a JSON `null`, or an EMPTY array is the honest "no ordered walk" state (LiveGraph
/// route, older daemon, truncated edge set) → the unordered member listing. A present `walk` that is
/// any OTHER JSON type (string / number / bool / object) is wire/schema DRIFT — the unknown is made
/// VISIBLE with the unreadable line, never silently normalized into the unordered fallback.
///
/// `length` is `Some(k)` only when the cycle's `length` fact is a readable number; off-walk members
/// can be COUNTED (`+ N more`) only then — an unreadable length yields no fabricated count.
fn cycle_body(item: &serde_json::Value, length: Option<u64>) -> (String, Option<String>) {
    let unreadable_walk = || {
        (
            "cycle walk unreadable on this snapshot — run `rmap cycles`".to_string(),
            None,
        )
    };
    match item.get("walk") {
        // Absent or JSON null = legitimate absence => the honest unordered listing, ZERO arrows.
        None | Some(serde_json::Value::Null) => (unordered_members(item), None),
        Some(serde_json::Value::Array(walk)) => {
            if walk.is_empty() {
                // An empty array is legitimate absence, the same as null.
                (unordered_members(item), None)
            } else {
                match crate::presentation::cycle_walk_display::validate_walk(walk) {
                    Some(names) => {
                        // The ring closes on its first member; off-walk members (the SCC is larger
                        // than the displayed loop) are reported as a count, exactly as `cycles` does
                        // — but ONLY when `length` is a readable fact (never synthesized).
                        let ring = format!("{} -> {}", names.join(" -> "), names[0]);
                        let offwalk = length.and_then(|k| {
                            let more = (k as usize).saturating_sub(names.len());
                            (more > 0).then(|| {
                                format!(
                                    "    (+ {more} more member{} in this cycle)\n",
                                    if more == 1 { "" } else { "s" }
                                )
                            })
                        });
                        (ring, offwalk)
                    }
                    // A present array that is malformed (non-string / empty-string / <2 members) =
                    // drift: make the unknown VISIBLE, never masquerade as a clean fallback or ring.
                    None => unreadable_walk(),
                }
            }
        }
        // Present but NOT an array (string / number / bool / object) = drift: unreadable, never the
        // silent unordered fallback (ECH-IR-001).
        Some(_) => unreadable_walk(),
    }
}

/// The no-arrows fallback body — `members (unordered): A, B, C` (capped at 8 with `(+ K more)`),
/// byte-mirroring `cycles::walk::render_unordered` so the two surfaces list the same members for one
/// cycle. A non-string member is DRIFT — surfaced, never silently dropped (the prior
/// `filter_map(as_str)` dropped non-strings, the honesty defect this replaces).
fn unordered_members(item: &serde_json::Value) -> String {
    const SHOWN: usize = 8;
    let Some(modules) = item.get("modules").and_then(|v| v.as_array()) else {
        return "cycle members unavailable on this snapshot — run `rmap cycles`".to_string();
    };
    let mut names: Vec<&str> = Vec::with_capacity(modules.len());
    for m in modules {
        match m.as_str() {
            Some(s) => names.push(s),
            None => {
                return "cycle members unreadable on this snapshot — run `rmap cycles`".to_string()
            }
        }
    }
    if names.len() <= SHOWN {
        format!("members (unordered): {}", names.join(", "))
    } else {
        let more = names.len() - SHOWN;
        format!(
            "members (unordered): {} (+ {more} more)",
            names[..SHOWN].join(", ")
        )
    }
}

/// EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04 / RG-REQ-012-L07): the body of ONE member row —
/// `<name> (<subtype>)  <path>:<line>`, or `<name> (<subtype>, decl)  …` for a forward declaration,
/// or bare `<name>  <path>` when subtype is absent. Returns `None` on ANY malformed field so the
/// caller renders the visible unreadable line instead of a fabricated/silently-shorter row (STANDING
/// HONESTY RULE): a non-string `name`, a non-string `file`, a PRESENT-but-non-string `subtype`, a
/// PRESENT-but-non-bool `forward_decl`, or a PRESENT-but-non-integer `line`. Only an ABSENT (or JSON
/// `null`) optional field is legitimate absence — `subtype`/`line` → omitted, `forward_decl` → false;
/// an absent or `0` line renders the bare path (never `:0`). A present field of the wrong JSON type is
/// wire/schema drift, never silently coerced to a default.
///
/// CPP-ATTRIBUTE-MACRO-1A (RG-REQ-002-L11): a member whose identity the index could not determine
/// renders `<name> (<subtype>[, decl], identity undetermined — 2 candidates: <c1>, <c2>)  <anchor>`
/// (`<name> (identity undetermined — …)` without a subtype) — see [`member_identity_marker`].
fn render_member_row(item: &serde_json::Value) -> Option<String> {
    let name = item.get("name")?.as_str()?;
    let file = item.get("file")?.as_str()?;
    // subtype: absent / null = legitimate absence (None). A PRESENT non-string value is drift → None.
    let subtype = match item.get("subtype") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(v.as_str()?),
    };
    // forward_decl: absent / null = false. A PRESENT non-bool value is drift → None.
    let forward_decl = match item.get("forward_decl") {
        None | Some(serde_json::Value::Null) => false,
        Some(v) => v.as_bool()?,
    };
    // line: absent / null = legitimate absence (None). A PRESENT non-integer value is drift → None
    // for the WHOLE row (the caller renders the unreadable line).
    let line = match item.get("line") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(v.as_u64()?),
    };
    let marker = member_identity_marker(item)?;
    let head = match (subtype, marker) {
        (Some(st), None) if forward_decl => format!("{name} ({st}, decl)"),
        (Some(st), None) => format!("{name} ({st})"),
        (None, None) => name.to_string(),
        (Some(st), Some(m)) if forward_decl => format!("{name} ({st}, decl, {m})"),
        (Some(st), Some(m)) => format!("{name} ({st}, {m})"),
        (None, Some(m)) => format!("{name} ({m})"),
    };
    Some(format!("{head}  {}", anchor(file, line)))
}

/// CPP-ATTRIBUTE-MACRO-1A (RG-REQ-002-L11): the undetermined-identity marker of one member row,
/// read ALL-OR-NOTHING from the flat keys `identity`, `identity_candidates`, `identity_basis`.
/// `Some(None)`: all three absent (or JSON `null`) — a determined member, today's row. `Some(Some(
/// text))`: `identity` is `"undetermined"`, `identity_candidates` exactly two distinct non-empty
/// strings and `identity_basis` a non-empty string — `identity undetermined — 2 candidates: <c1>,
/// <c2>` in carried order (the order ranks nothing). `None`: any other state (a partial marker, a
/// non-array, a wrong count, a non-string, empty or duplicate candidate, a non-string basis,
/// another `identity` value) — the caller renders the whole section's unreadable line, never a
/// coerced or silently shorter row.
fn member_identity_marker(item: &serde_json::Value) -> Option<Option<String>> {
    let present = |key: &str| item.get(key).filter(|v| !v.is_null());
    match (
        present("identity"),
        present("identity_candidates"),
        present("identity_basis"),
    ) {
        (None, None, None) => Some(None),
        (Some(identity), Some(candidates), Some(basis)) => {
            if identity.as_str()? != "undetermined" || basis.as_str()?.is_empty() {
                return None;
            }
            let [first, second] = candidates.as_array()?.as_slice() else {
                return None;
            };
            let (first, second) = (first.as_str()?, second.as_str()?);
            if first.is_empty() || second.is_empty() || first == second {
                return None;
            }
            Some(Some(format!(
                "identity undetermined — 2 candidates: {first}, {second}"
            )))
        }
        _ => None,
    }
}

/// EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04, F-ETS-01): the unreadable render of the `Members`
/// section — the section heading followed by the SINGLE `members unreadable on this snapshot` line,
/// with NO rows. The heading keeps the real `count` only when it is itself a readable `u64` (never a
/// fabricated `0`); a malformed count drops the parenthetical entirely rather than inventing one.
fn members_unreadable(count: Option<u64>) -> String {
    let mut out = match count {
        Some(c) => heading(&format!("Members ({c})")),
        None => heading("Members"),
    };
    out.push_str(&bullet("members unreadable on this snapshot"));
    out
}

/// EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04, F-ETS-01): the unreadable render of the `Referenced by`
/// section — the heading followed by the SINGLE `referenced-by unreadable on this snapshot` line, no
/// rows and no `top modules:` line. Keeps the real `count` only when it is a readable `u64`.
fn referenced_by_unreadable(count: Option<u64>) -> String {
    let mut out = match count {
        Some(c) => heading(&format!("Referenced by ({c} files)")),
        None => heading("Referenced by"),
    };
    out.push_str(&bullet("referenced-by unreadable on this snapshot"));
    out
}

/// PYTHON-SUBMODULE-IMPORT-1: the validated inferred remainder of a `Referenced by` section.
struct InferredReferencedBy {
    /// Distinct inferred-only referencing files (the heading's `+M inferred`).
    count: u64,
    /// Rendered rows, one per carried inferred item, in carried order.
    rows: Vec<String>,
    /// Inferred rows the budget cut before the wire.
    omitted: u64,
}

/// Read the optional inferred fields of an `EXPLAIN_REFERENCED_BY` evidence object. An ABSENT field
/// means no inferred importer (the pre-slice shape renders exactly as before). `None` = the carried
/// remainder cannot be trusted, and the caller renders the unreadable line (RG-REQ-002-L04), never
/// a partial list or an unsupported count:
/// - a PRESENT field that is mistyped — including JSON `null` — is malformed (never a known zero);
/// - `inferred_count` must be supported by the carried rows (RG-REQ-002-L02): with no rows cut
///   (`inferred_items_omitted_count` absent or 0) it equals the number of distinct files among the
///   rows; with rows cut, at least one row is carried and the count lies between the distinct
///   carried files and that number plus the cut rows.
///
/// A basis renders in the reader's words (`python submodule`); an unknown basis renders without its
/// code (RG-REQ-002-L08).
fn read_inferred_referenced_by(evidence: &serde_json::Value) -> Option<InferredReferencedBy> {
    let read_u64 = |key: &str| match evidence.get(key) {
        None => Some(0),
        Some(v) => v.as_u64(),
    };
    let count = read_u64("inferred_count")?;
    let omitted = read_u64("inferred_items_omitted_count")?;
    let items: &[serde_json::Value] = match evidence.get("inferred_items") {
        None => &[],
        Some(serde_json::Value::Array(items)) => items,
        Some(_) => return None,
    };
    let optional_str = |item: &serde_json::Value, key: &str| -> Option<Option<String>> {
        match item.get(key) {
            None => Some(None),
            Some(v) => v.as_str().map(|s| Some(s.to_string())),
        }
    };
    let mut rows = Vec::with_capacity(items.len());
    let mut files = std::collections::BTreeSet::new();
    for item in items {
        let file = item.get("file")?.as_str()?;
        optional_str(item, "module")?;
        let basis = item.get("basis")?.as_str().filter(|b| !b.is_empty())?;
        let alternate = optional_str(item, "alternate")?;
        let row = match (basis, alternate) {
            ("python_submodule", Some(alt)) => {
                format!("{file}  (inferred: python submodule — alternate {alt})")
            }
            // A python-submodule inference always names its alternate (the package init).
            ("python_submodule", None) => return None,
            (_, Some(alt)) => format!("{file}  (inferred — alternate {alt})"),
            (_, None) => format!("{file}  (inferred)"),
        };
        files.insert(file);
        rows.push(row);
    }
    let distinct = files.len() as u64;
    // Every sum over carried wire counts is checked: a carried total that overflows `u64` cannot be
    // supported by any rows, so it is malformed (the unreadable line), never a panic or a wrap. The
    // carried-plus-cut row total bounds the renderer's `... (N more inferred)` sum, and `distinct`
    // never exceeds the carried rows, so this one check covers both sums.
    (rows.len() as u64).checked_add(omitted)?;
    let supported = if omitted == 0 {
        count == distinct
    } else {
        !rows.is_empty() && distinct <= count && count <= distinct.checked_add(omitted)?
    };
    if !supported {
        return None;
    }
    Some(InferredReferencedBy {
        count,
        rows,
        omitted,
    })
}

impl ExplainResponse {
    /// Render a single signal's section. `full` lifts the per-section display cap (see the module doc):
    /// it is the `--full` flag, threaded so the human render is uncapped for grep.
    pub(super) fn render_signal_section(
        &self,
        signal: &ExplainSignal,
        full: bool,
    ) -> Option<String> {
        let evidence = signal.evidence.as_ref()?;

        match signal.code.as_str() {
            "EXPLAIN_CALLERS" => Some(self.render_callers(evidence, full)),
            "EXPLAIN_CALLEES" => Some(self.render_callees(evidence, full)),
            "EXPLAIN_MEMBERS" => Some(self.render_members(evidence, full)),
            "EXPLAIN_REFERENCED_BY" => Some(self.render_referenced_by(evidence, full)),
            "EXPLAIN_IMPORTS" => Some(self.render_imports(evidence, full)),
            "EXPLAIN_SYMBOLS" => Some(self.render_symbols(evidence, full)),
            "EXPLAIN_FILES" => Some(self.render_files(evidence, full)),
            "EXPLAIN_CYCLES" => self.render_cycles(evidence, full),
            "EXPLAIN_BOUNDARY" => self.render_boundary(evidence, full),
            "EXPLAIN_GATE" => self.render_gate(evidence, full),
            "EXPLAIN_TRUST" => Some(self.render_trust(evidence)),
            "EXPLAIN_IDENTITY" => None, // Handled in header
            "EXPLAIN_MEASUREMENTS" => self.render_measurements(evidence, full),
            _ => None,
        }
    }

    fn render_callers(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!(
            "Callers ({}{}){}",
            count,
            union_degree_suffix(evidence),
            self.type_not_called_suffix(count),
        ));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                out.push_str(&bullet(&render_callgraph_item(item)));
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
            // PYTHON-RECEIVER-BINDING-1 review F-1 (RG-REQ-002-L02): these rows anchor each
            // caller's DECLARATION (the LiveGraph-served rows can carry no call-site line), while
            // `rmap callers` anchors the CALL SITE. The differing basis is stated inline, with the
            // command that gives the call sites, whenever a row shows a line.
            if items.iter().take(shown).any(item_has_anchor) {
                let target = match self.focus_qualified_name() {
                    Some(name) => name,
                    None => "<symbol>".to_string(),
                };
                out.push_str(&bullet(&format!(
                    "lines above are where each caller is defined — for the call sites run rmap callers {target}"
                )));
            }
        }
        out.push_str(&self.render_call_remainder(evidence, "callers", "unresolved_naming", full));

        out
    }

    /// PYTHON-RECEIVER-BINDING-1 (RG-REQ-002-L11, RG-REQ-005-L09): after the certain rows, the
    /// inferred remainder with its next action (`+M inferred (name-only) — investigate with rmap
    /// callers <name> --include-inferred`) and the unresolved calls bearing on the symbol; under
    /// `--full` the carried inferred rows follow, each marked. Absent fields render nothing (an
    /// explain without a remainder is byte-identical to before).
    fn render_call_remainder(
        &self,
        evidence: &serde_json::Value,
        command: &str,
        unresolved_key: &str,
        full: bool,
    ) -> String {
        let mut out = String::new();
        // A PRESENT field that cannot be read renders unknown with its reason — never as if the
        // field were absent (RG-REQ-002-L04); only an absent field renders nothing.
        let unreadable = |what: &str| {
            bullet(&format!(
                "{what}: unknown — this answer's {what} field is unreadable"
            ))
        };
        if let Some(v) = evidence.get("inferred_count") {
            let Some(n) = v.as_u64() else {
                out.push_str(&unreadable(&format!("inferred {command}")));
                return out + &self.render_unresolved_remainder(evidence, unresolved_key);
            };
            // "(name-only)" only when the per-basis counts are readable AND all name-only; an
            // absent or malformed map never earns the label.
            let name_only = match evidence.get("inferred_by_basis") {
                Some(v) => {
                    match serde_json::from_value::<std::collections::BTreeMap<String, u64>>(
                        v.clone(),
                    ) {
                        Ok(by_basis) => crate::presentation::graph_edges::all_name_only(&by_basis),
                        Err(_) => false,
                    }
                }
                None => false,
            };
            let label = repo_graph_agent::reliability::inferred_remainder_label(n, name_only);
            let target = match self.focus_qualified_name() {
                Some(name) => name,
                None => "<symbol>".to_string(),
            };
            out.push_str(&bullet(&format!(
                "+{label} — investigate with rmap {command} {target} --include-inferred"
            )));
            if full {
                if let Some(v) = evidence.get("inferred_items") {
                    let Some(items) = v.as_array() else {
                        out.push_str(&unreadable(&format!("inferred {command} rows")));
                        return out + &self.render_unresolved_remainder(evidence, unresolved_key);
                    };
                    for item in items {
                        let marker = crate::presentation::graph_edges::inferred_row_marker(
                            item.get("basis").and_then(|v| v.as_str()),
                            item.get("extractor").and_then(|v| v.as_str()),
                            None,
                        );
                        out.push_str(&bullet(&format!("{}{marker}", render_callgraph_item(item))));
                    }
                }
            }
        }
        out + &self.render_unresolved_remainder(evidence, unresolved_key)
    }

    /// The unresolved calls bearing on the symbol (`unresolved_naming` / `unresolved_from`): the
    /// count line with its reader classes; a present block whose count or classes cannot be read
    /// renders unknown with its reason; an absent block renders nothing.
    fn render_unresolved_remainder(
        &self,
        evidence: &serde_json::Value,
        unresolved_key: &str,
    ) -> String {
        let Some(u) = evidence.get(unresolved_key) else {
            return String::new();
        };
        let count = u.get("count").and_then(|v| v.as_u64());
        let by_basis = u.get("by_basis").and_then(|v| {
            serde_json::from_value::<std::collections::BTreeMap<String, u64>>(v.clone()).ok()
        });
        let name = match u.get("name") {
            None => Some(None),
            Some(v) => v.as_str().map(Some),
        };
        match (count, by_basis, name) {
            (Some(count), Some(by_basis), Some(name)) => bullet(
                &crate::presentation::graph_edges::unresolved_line(count, name, &by_basis),
            ),
            _ => bullet(
                "unresolved calls: unknown — this answer's unresolved-call count is unreadable",
            ),
        }
    }

    fn render_callees(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!(
            "Callees ({}{}){}",
            count,
            union_degree_suffix(evidence),
            self.type_not_called_suffix(count),
        ));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                out.push_str(&bullet(&render_callgraph_item(item)));
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }
        out.push_str(&self.render_call_remainder(evidence, "callees", "unresolved_from", full));

        out
    }

    /// EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): render the `Members` section of a type focus. Each
    /// row is `<name> (<subtype>)  <path>:<line>` (a `forward_decl` member renders `(<subtype>, decl)`),
    /// the anchor through the shared [`anchor`] chokepoint so an absent/0 line renders no `:N`
    /// (RG-REQ-012-L07). The default view caps at 15 rows and names the omitted remainder from the
    /// PRE-truncation `count` (RG-REQ-012-L04); `--full` renders all.
    ///
    /// F-ETS-01 (STANDING HONESTY RULE — the class, not the instance): the COMPLETE carried evidence
    /// is validated BEFORE anything renders. `count` must be a `u64` (a missing or non-integer count
    /// is malformed — never `unwrap_or(0)`), `items` must be an array, and EVERY carried item must be
    /// a fully typed member row (see [`render_member_row`]). On any malformed field the section is
    /// exactly its heading followed by the single `members unreadable on this snapshot` line — never a
    /// fabricated `Members (0)`, a bare heading, a partial row list, or a silently shortened set.
    fn render_members(&self, evidence: &serde_json::Value, full: bool) -> String {
        const DISPLAY_CAP: usize = 15;
        // count MUST be a readable u64; items MUST be an array. Either malformed ⇒ unreadable.
        let count = evidence.get("count").and_then(|v| v.as_u64());
        let items = evidence.get("items").and_then(|v| v.as_array());
        let (Some(count), Some(items)) = (count, items) else {
            return members_unreadable(count);
        };
        // Validate every carried item up front (close the class): a single malformed row makes the
        // WHOLE section unreadable, so no partial list can ever escape.
        let mut rows = Vec::with_capacity(items.len());
        for item in items {
            let Some(row) = render_member_row(item) else {
                return members_unreadable(Some(count));
            };
            rows.push(row);
        }
        let shown = if full {
            rows.len()
        } else {
            rows.len().min(DISPLAY_CAP)
        };
        let mut out = heading(&format!("Members ({count})"));
        for row in rows.iter().take(shown) {
            out.push_str(&bullet(row));
        }
        let remaining = count.saturating_sub(shown as u64);
        if remaining > 0 {
            out.push_str(&format!("  ... ({} more)\n", remaining));
        }
        out
    }

    /// EXPLAIN-TYPE-SECTIONS-1 (RG-REQ-005-L04): render the `Referenced by (N files)` section — the
    /// files that reference the focused type's file. Names the top owning modules (`top modules:`),
    /// then the file rows (capped at 15, remainder named from the PRE-truncation `count`).
    ///
    /// F-ETS-01 (STANDING HONESTY RULE — the class, not the instance): the COMPLETE carried evidence
    /// is validated BEFORE anything renders. `count` must be a `u64` (never `unwrap_or(0)`); `items`
    /// must be an array of fully typed rows (`file` a string, `module` a string or absent); and, when
    /// present, `top_modules` must be an array whose every entry is `{module: string, count: u64}`. On
    /// any malformed field — including a single malformed top-module entry — the section is exactly its
    /// heading followed by the single `referenced-by unreadable on this snapshot` line: never a
    /// fabricated count, a shortened row list, or a SILENTLY DROPPED top-module entry.
    fn render_referenced_by(&self, evidence: &serde_json::Value, full: bool) -> String {
        const DISPLAY_CAP: usize = 15;
        // count MUST be a readable u64; items MUST be an array. Either malformed ⇒ unreadable.
        let count = evidence.get("count").and_then(|v| v.as_u64());
        let items = evidence.get("items").and_then(|v| v.as_array());
        let (Some(count), Some(items)) = (count, items) else {
            return referenced_by_unreadable(count);
        };

        // top_modules: absent / null is legitimate (no line). A PRESENT value MUST be an array whose
        // every entry carries a string `module` and a u64 `count`; any malformed entry is drift → the
        // WHOLE section is unreadable (never a silently dropped entry).
        let top_parts: Vec<String> = match evidence.get("top_modules") {
            None | Some(serde_json::Value::Null) => Vec::new(),
            Some(serde_json::Value::Array(top)) => {
                let mut parts = Vec::with_capacity(top.len());
                for m in top {
                    match (
                        m.get("module").and_then(|v| v.as_str()),
                        m.get("count").and_then(|v| v.as_u64()),
                    ) {
                        (Some(name), Some(c)) => parts.push(format!("{name} ({c})")),
                        _ => return referenced_by_unreadable(Some(count)),
                    }
                }
                parts
            }
            // Present but not an array = wire/schema drift.
            Some(_) => return referenced_by_unreadable(Some(count)),
        };

        // Validate every carried file row up front (`file` string required; `module` string or absent).
        let mut rows = Vec::with_capacity(items.len());
        for item in items {
            let Some(file) = item.get("file").and_then(|v| v.as_str()) else {
                return referenced_by_unreadable(Some(count));
            };
            match item.get("module") {
                None | Some(serde_json::Value::Null) => {}
                Some(v) if v.as_str().is_some() => {}
                Some(_) => return referenced_by_unreadable(Some(count)),
            }
            rows.push(file.to_string());
        }

        // PYTHON-SUBMODULE-IMPORT-1 (RG-REQ-002-L11): the inferred remainder, validated as a whole
        // before anything renders — any malformed inferred field makes the section unreadable.
        let Some(inferred) = read_inferred_referenced_by(evidence) else {
            return referenced_by_unreadable(Some(count));
        };

        let mut out = if inferred.count > 0 {
            heading(&format!(
                "Referenced by ({count} files, +{} inferred — investigate)",
                inferred.count
            ))
        } else {
            heading(&format!("Referenced by ({count} files)"))
        };
        if !top_parts.is_empty() {
            out.push_str(&format!("  top modules: {}\n", top_parts.join(", ")));
        }
        let shown = if full {
            rows.len()
        } else {
            rows.len().min(DISPLAY_CAP)
        };
        for row in rows.iter().take(shown) {
            out.push_str(&bullet(row));
        }
        let remaining = count.saturating_sub(shown as u64);
        if remaining > 0 {
            out.push_str(&format!("  ... ({} more)\n", remaining));
        }
        let inferred_shown = if full {
            inferred.rows.len()
        } else {
            inferred.rows.len().min(DISPLAY_CAP)
        };
        for row in inferred.rows.iter().take(inferred_shown) {
            out.push_str(&bullet(row));
        }
        // Cannot overflow: `read_inferred_referenced_by` checked `rows.len() + omitted`.
        let inferred_remaining = (inferred.rows.len() - inferred_shown) as u64 + inferred.omitted;
        if inferred_remaining > 0 {
            out.push_str(&format!("  ... ({inferred_remaining} more inferred)\n"));
        }
        out
    }

    fn render_imports(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Imports ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 15 };
            for item in items.iter().take(shown) {
                let target = item
                    .get("target_file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                out.push_str(&bullet(target));
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        // TEST-EDGE-SCOPE-1B (RG-REQ-002-L11): the inferred imports not listed, with the command
        // that lists them for this file (worded by `import_partition`).
        if let Some(rem) = evidence.get("import_remainder") {
            use crate::presentation::import_partition as ip;
            let file = self
                .focus
                .resolved_path
                .as_deref()
                .or(self.focus.input.as_deref())
                .unwrap_or("<file>");
            match ip::remainder_of(rem) {
                Ok(r) => {
                    if let Some(clause) = ip::imports_command_clause(r.inferred.imports, file) {
                        out.push_str(&bullet(&clause));
                    }
                }
                Err(()) => out.push_str(&bullet(ip::PARTITION_UNREADABLE)),
            }
        }

        out
    }

    fn render_symbols(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Symbols ({})", count));

        // ANCHORS-EVERYWHERE-1 (Tier 0): the Symbols section is emitted for a FILE target,
        // so every symbol shares the resolved file path; the per-item `line_start` (already
        // on the wire) completes the `path:line` anchor. No path → no anchor.
        let file_path = self.focus.resolved_path.as_deref();

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 15 };
            for item in items.iter().take(shown) {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let subtype = item.get("subtype").and_then(|v| v.as_str());
                let mut row = match subtype {
                    Some(st) => format!("{} ({})", name, st),
                    None => name.to_string(),
                };
                // Append `path:line` only when BOTH the file path and a stored line exist
                // (byte-identical to the pre-anchor row otherwise — never a fabricated line).
                if let (Some(path), Some(line)) =
                    (file_path, item.get("line_start").and_then(|v| v.as_u64()))
                {
                    row.push_str(&format!("  {}", anchor(path, Some(line))));
                }
                out.push_str(&bullet(&row));
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        out
    }

    fn render_files(&self, evidence: &serde_json::Value, full: bool) -> String {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut out = heading(&format!("Files ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 15 };
            for item in items.iter().take(shown) {
                let path = item
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                let symbol_count = item
                    .get("symbol_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                out.push_str(&bullet(&format!("{} ({} symbols)", path, symbol_count)));
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        out
    }

    /// EXPLAIN-CYCLES-HONEST-1 (§2.1.2): render the Import-cycles block. An arrow (`->`) is drawn
    /// ONLY over a VERIFIED walk — the SAME `walk` the SQLite serving computation precomputed via the
    /// shared `cycle_walk` kernel, the SAME ring `cycles` and `orient` draw. With no walk (the
    /// LiveGraph route, an older daemon, or a truncated edge set) the honest `members (unordered)`
    /// form renders with ZERO arrows; a present-but-malformed walk makes the unknown VISIBLE. This
    /// replaces the RC-4 defect where the member SET (lexically sorted by
    /// `ordering::canonicalize_cycles`) was joined with `->` as if it were a directed ring — arrows
    /// over edges the import graph does not hold.
    fn render_cycles(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let count = evidence.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        // TEST-EDGE-SCOPE-1B (RG-REQ-004-L12): the cycles only through excluded imports that
        // involve the focus — the block renders when one exists even if none remains.
        // One decode rule (D-TESB-17 row U9): no `import_view` → the unavailable line; a partial
        // or malformed payload → ONE unreadable line, never "none excluded".
        use crate::presentation::import_partition as ip;
        let partition = ip::surface_partition(
            evidence.get("import_view"),
            evidence.get("import_remainder"),
            evidence.get("excluded_cycles"),
            evidence.get("importer_test_status_undetermined"),
            ip::Requires::CYCLES,
        );
        let stated = matches!(partition, ip::Partition::Stated(_));
        let excluded_clause = match partition {
            ip::Partition::Unavailable => Some(ip::PARTITION_UNAVAILABLE.to_string()),
            ip::Partition::Unreadable => Some(ip::PARTITION_UNREADABLE.to_string()),
            ip::Partition::Stated(_) => match ip::excluded_cycles_of(evidence) {
                Some(Ok(cycles)) => ip::excluded_cycle_clause(&cycles),
                _ => Some(ip::PARTITION_UNREADABLE.to_string()),
            },
        };
        if count == 0 && excluded_clause.is_none() {
            return None;
        }

        let mut out = heading(&format!("Import cycles ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 5 };
            for (i, item) in items.iter().take(shown).enumerate() {
                // K = the cycle's full member count, taken from the `length` FACT ALONE — never
                // synthesized from `modules.len()` (ECH-IR-001). A ring may visit only a closed loop
                // within a larger SCC; the header states the full size, exactly as `cycles` does. An
                // absent or non-numeric `length` renders a NAMED unreadable size, never a count the
                // store does not carry (STANDING HONESTY RULE #1).
                let length = item.get("length").and_then(|v| v.as_u64());
                let size_label = match length {
                    Some(k) => format!("{k} modules"),
                    None => "size unreadable".to_string(),
                };
                let (body, offwalk) = cycle_body(item, length);
                out.push_str(&bullet(&format!(
                    "Cycle {} ({}): {}",
                    i + 1,
                    size_label,
                    body
                )));
                if let Some(line) = offwalk {
                    out.push_str(&line);
                }
            }
            if items.len() > shown {
                out.push_str(&format!("  ... ({} more)\n", items.len() - shown));
            }
        }

        if let Some(clause) = excluded_clause {
            out.push_str(&bullet(&clause));
        }
        if stated {
            if let Some(line) = crate::presentation::test_status::undetermined_files_line(
                evidence.get("importer_test_status_undetermined"),
            ) {
                out.push_str(&bullet(&line));
            }
        }

        Some(out)
    }

    fn render_boundary(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let count = evidence
            .get("violation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if count == 0 {
            return None;
        }

        let mut out = heading(&format!("Boundary violations ({})", count));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                let source = item
                    .get("source_module")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let target = item
                    .get("target_module")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let edges = item.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
                out.push_str(&bullet(&format!(
                    "{} -> {} ({} edges)",
                    source, target, edges
                )));
            }
        }

        Some(out)
    }

    fn render_gate(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let outcome = evidence
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let count = evidence
            .get("obligation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut out = heading(&format!(
            "Gate ({}: {} obligations)",
            outcome.to_uppercase(),
            count
        ));

        if let Some(items) = evidence.get("items").and_then(|v| v.as_array()) {
            let shown = if full { items.len() } else { 10 };
            for item in items.iter().take(shown) {
                let req_id = item.get("req_id").and_then(|v| v.as_str()).unwrap_or("?");
                let method = item.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                let verdict = item
                    .get("effective_verdict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                out.push_str(&bullet(&format!("{}: {} ({})", req_id, method, verdict)));
            }
        }

        Some(out)
    }

    fn render_trust(&self, evidence: &serde_json::Value) -> String {
        let mut out = heading("Trust");

        // RELIABILITY-REFRAME-1: the reader's frame, not repo-graph's pipeline — via the ONE shared
        // projection, so this explain surface never forks from orient/trust/check. review-1 §1: prefer
        // the in-scope COUNTS so a 0-of-0 repo renders the honest "no in-scope calls measured"; the
        // `call_resolution_rate` field carries the trust service's 1.0 sentinel for 0-of-0 (a fabricated
        // 100% if trusted). The band folds into the same line rather than a standalone
        // "Call graph reliability: …" (grades-us) bullet. Falls back to the rate-only path only when the
        // additive counts are absent (evidence from a daemon that predates them).
        let band = evidence
            .get("call_graph_reliability")
            .and_then(|v| v.as_str());
        let counts = evidence
            .get("resolved_in_scope")
            .and_then(|v| v.as_u64())
            .zip(
                evidence
                    .get("in_scope_or_unclassified_total")
                    .and_then(|v| v.as_u64()),
            );
        // PYTHON-RECEIVER-BINDING-1 (D-PRB-RATE-1 = A): the inferred calls inside the universe. The
        // field is serialized only when positive, so ABSENT is the measured none; a PRESENT value
        // that is not a count is unreadable — the counts path is not taken (never a guessed 0).
        let inferred = match evidence.get("inferred_calls") {
            None => Some(0),
            Some(v) => v.as_u64(),
        };
        let counts = counts.zip(inferred);
        if let Some(((resolved, total), inferred)) = counts {
            let view = reliability::CallReliabilityView::derive(
                resolved,
                total.saturating_sub(resolved).saturating_sub(inferred),
                inferred,
                0,
                total,
                Vec::new(),
                band.and_then(reliability::band_from_wire),
            );
            out.push_str(&bullet(&view.resolved_with_band()));
        } else {
            let rate = evidence
                .get("call_resolution_rate")
                .and_then(|v| v.as_f64());
            match (rate, band) {
                (Some(r), Some(b)) => out.push_str(&bullet(
                    &reliability::resolved_phrase_with_band(r * 100.0, &b.to_uppercase()),
                )),
                (Some(r), None) => {
                    out.push_str(&bullet(&reliability::resolved_phrase_pct(r * 100.0)))
                }
                (None, Some(b)) => {
                    out.push_str(&bullet(&format!("your code's call resolution is {}", b)))
                }
                (None, None) => {}
            }
        }
        if let Some(enrichment) = evidence.get("enrichment_state").and_then(|v| v.as_str()) {
            out.push_str(&bullet(&format!("Enrichment: {}", enrichment)));
        }

        out
    }

    fn render_measurements(&self, evidence: &serde_json::Value, full: bool) -> Option<String> {
        let items = evidence.get("items").and_then(|v| v.as_array())?;
        if items.is_empty() {
            return None;
        }

        let mut out = heading("Measurements");

        let shown = if full { items.len() } else { 10 };
        for item in items.iter().take(shown) {
            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let value = item.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let aggregation = item
                .get("aggregation")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            out.push_str(&bullet(&format!(
                "{} ({}): {:.2}",
                kind, aggregation, value
            )));
        }

        Some(out)
    }
}
