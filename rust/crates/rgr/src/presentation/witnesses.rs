//! RECON-M-R3a: the CLIENT-side witness-block rendering — ONE JSON→reader-lines projection
//! shared by every surface that shows the union accounting (trust section, orient/stats g1u
//! line, modules g2u footnote, map g3u label), so no surface hand-assembles a divergent
//! phrasing of the same block (the client half of the §5.4 shared-projection discipline).
//!
//! All readers are DEFENSIVE (`Value::get` + typed accessors): the daemon's witness block is an
//! additive `Option<Value>`, and a missing/malformed field renders as absence — never a panic,
//! never an invented zero (unknown stays unstated).

use serde_json::Value;

use crate::presentation::{bullet, heading};

fn u(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

fn nested_u(v: &Value, key: &str, sub: &str) -> Option<u64> {
    v.get(key).and_then(|x| x.get(sub)).and_then(Value::as_u64)
}

/// The coverage phrase of a measured/witness block: `"TypeScript (8 partitions)"`
/// (multi-language: joined by `+`). `None` when the basis is absent or malformed. Private:
/// every consumer goes through [`union_coverage_phrase`], which also demands the accounting
/// marker — the phrase alone is not the §5.3.0 gate.
fn coverage_phrase(measured: &Value) -> Option<String> {
    let cov = measured.get("coverage")?;
    // The COMPLETE §5.3.0 basis is languages + partitions + FINGERPRINT
    // (recon-design-1 §5.3.0; review-3 blocking defect): a missing/empty fingerprint,
    // an empty array, or any non-string/empty member is an incomplete basis and
    // SUPPRESSES the phrase — a reconciled value must never render over a partial
    // or malformed coverage claim.
    let fingerprint = cov.get("fingerprint").and_then(Value::as_str)?;
    if fingerprint.is_empty() {
        return None;
    }
    let languages = all_nonempty_strings(cov.get("languages")?)?;
    let n = all_nonempty_strings(cov.get("partitions")?)?.len();
    Some(format!(
        "{} ({} partition{})",
        languages.join("+"),
        n,
        if n == 1 { "" } else { "s" }
    ))
}

/// Every member a nonempty string, and the array itself nonempty — else `None`.
/// (A `filter_map` that silently drops malformed members would under-claim coverage
/// while still rendering: exactly the partial-basis defect the §5.3.0 gate forbids.)
fn all_nonempty_strings(v: &Value) -> Option<Vec<&str>> {
    let arr = v.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let strs: Vec<&str> = arr
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| !s.is_empty())
        .collect();
    if strs.len() != arr.len() {
        return None;
    }
    Some(strs)
}

/// The §5.3.0 labeling gate, in ONE place (review-2 item 1): the coverage phrase of a
/// union-accounting block, `Some` ONLY when the block carries `accounting: "union"` AND a
/// derivable coverage basis. `None` → the consumer renders NO union value at all — a
/// missing/malformed label SUPPRESSES the reconciled figure; it never renders unlabeled.
/// `pub`: shared by every renderer that consumes a union block (g1u_line +
/// measurement_lines here, the map witness fold, explain's union-degree suffix, the
/// modules_list/modules_show g2u reduction), so the gate cannot drift per surface.
pub fn union_coverage_phrase(block: &Value) -> Option<String> {
    if block.get("accounting").and_then(Value::as_str) != Some("union") {
        return None;
    }
    coverage_phrase(block)
}

/// The one-line g1u summary for orientation surfaces (§5.3.2 — ADDITIVE beside the pipeline
/// figure, never replacing it): pipeline calls · reconciled union calls (coverage) — agreement.
/// `None` when the block lacks its accounting marker or coverage basis (never an unlabeled
/// union figure).
pub fn g1u_line(block: &Value) -> Option<String> {
    let coverage = union_coverage_phrase(block)?;
    let pipeline = u(block, "pipeline_calls")?;
    let union = u(block, "union_calls")?;
    let dual = u(block, "dual_measured")?;
    let both = u(block, "both")?;
    let mut line = format!(
        "call graph: {pipeline} syntax-resolved (all languages) · reconciled: {union} \
         combined-analyses calls ({coverage})"
    );
    if dual > 0 {
        let pct = block
            .get("agreement_pct")
            .and_then(Value::as_f64)
            .map(|p| format!(" ({p:.1}%)"))
            .unwrap_or_default();
        line.push_str(&format!(
            " — of {dual} the compiler could measure, {both} corroborated{pct}"
        ));
    }
    Some(line)
}

/// The reader-frame measurement lines of a trust/doctor `measured` block (§5.4's example,
/// condensed; every figure's population labeled). Empty when unmeasured OR when the block
/// fails the §5.3.0 gate (no accounting marker / no coverage basis → the whole measurement
/// renders absence, never unlabeled figures). `pub`: the doctor probe renders the same lines
/// (one phrasing, two surfaces).
pub fn measurement_lines(measured: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(coverage) = union_coverage_phrase(measured) else {
        return lines;
    };
    let (Some(pipeline), Some(dual), Some(both)) = (
        u(measured, "pipeline_calls"),
        u(measured, "dual_measured"),
        nested_u(measured, "both", "instances"),
    ) else {
        return lines;
    };

    // Line 1: the corroboration rate on its exact population + the syntax-only causes.
    let mut line =
        format!("{coverage}: {pipeline} syntax-resolved calls; the compiler could measure {dual}");
    if dual > 0 {
        let pct = measured
            .get("agreement_pct")
            .and_then(Value::as_f64)
            .map(|p| format!(" ({p:.1}%)"))
            .unwrap_or_default();
        line.push_str(&format!(" — {both} corroborated{pct}"));
        // Review-1 item 5: the syntax-only aggregate requires ALL its component fields — a
        // missing field must never coerce to a measured zero (a partial sum would present an
        // incomplete cause breakdown as complete). Malformed → the clause is absent.
        if let Some((boundary, file_scope, uncorr, multiplicity)) =
            measured.get("syntactic_only").and_then(|syn| {
                Some((
                    u(syn, "boundary")?,
                    u(syn, "file_scope")?,
                    u(syn, "uncorroborated")?,
                    u(syn, "multiplicity")?,
                ))
            })
        {
            let total = boundary + file_scope + uncorr + multiplicity;
            if total > 0 {
                let mut causes = Vec::new();
                if boundary > 0 {
                    causes.push(format!("{boundary} across compiler-run boundaries"));
                }
                if file_scope > 0 {
                    causes.push(format!(
                        "{file_scope} module-initialization (outside the compiler's call model)"
                    ));
                }
                if uncorr > 0 {
                    causes.push(format!("{uncorr} the compiler did not confirm"));
                }
                if multiplicity > 0 {
                    causes.push(format!(
                        "{multiplicity} with fewer occurrences confirmed than found"
                    ));
                }
                line.push_str(&format!(", {total} syntax-only ({})", causes.join(", ")));
            }
        }
    }
    if let Some(unmeasured) = nested_u(measured, "unmeasured_edges", "instances") {
        if unmeasured > 0 {
            line.push_str(&format!(
                ", and {unmeasured} more it could not measure here (shown, excluded from the rate)"
            ));
        }
    }
    lines.push(line);

    // Line 2: beyond the syntax graph — the union-only calls + the reference tier (its OWN
    // population, never a closure term). Review-1 item 5: the semantic aggregate requires BOTH
    // its components; a missing field renders the part absent (unknown), never a partial sum.
    let semantic = measured
        .get("semantic_only_calls")
        .and_then(|s| Some(u(s, "new_pair")? + u(s, "multiplicity")?));
    let references = u(measured, "references");
    let mut parts = Vec::new();
    if let Some(n) = semantic.filter(|n| *n > 0) {
        parts.push(format!("{n} compiler-resolved calls"));
    }
    if let Some(n) = references.filter(|n| *n > 0) {
        parts.push(format!(
            "{n} compiler-verified references (reads / writes / type references)"
        ));
    }
    if !parts.is_empty() {
        lines.push(format!("beyond the syntax graph: {}", parts.join(" · ")));
    }

    // Collision + suspect guards — visible in the block itself, never absorbed (§5.4).
    // Review-0 defect (b) — unit truth: `identity_collision.instances` counts WITHHELD
    // compiler-witnessed call INSTANCES (the ledger's actual unit), `identities` the distinct
    // withheld call pairs; the line labels both, never "N identities collide" over an
    // instance count. (The colliding-KEY population renders on doctor beside its keys.)
    if let Some(instances) = nested_u(measured, "identity_collision", "instances") {
        if instances > 0 {
            let pairs = nested_u(measured, "identity_collision", "identities")
                .map(|n| format!(" ({n} call pair{})", if n == 1 { "" } else { "s" }))
                .unwrap_or_default();
            lines.push(format!(
                "identity collisions between the syntax index and the compiler index: \
                 {instances} compiler-witnessed call instance{}{pairs} withheld — shown \
                 separately, never merged",
                if instances == 1 { "" } else { "s" },
            ));
        }
    }
    if let Some(suspects) = u(measured, "identity_suspect") {
        if suspects > 0 {
            lines.push(format!(
                "{suspects} adoption suspects (syntax and compiler resolutions may disagree on \
                 identity here)"
            ));
        }
    }

    // Projection answerability — a separately named population (symbol×direction lookups).
    if let Some(proj) = measured.get("projections") {
        if let (Some(unanswerable), Some(total)) = (u(proj, "unanswerable"), u(proj, "total")) {
            if unanswerable > 0 {
                lines.push(format!(
                    "{unanswerable} of {total} symbol-direction lookups had no compiler-side \
                     answer"
                ));
            }
        }
    }
    lines
}

/// The per-partition regime posture lines (W-ONE reason lines with next actions; W-BOTH rows
/// stay quiet here — the measurement speaks for them). `pub`: shared with the doctor probe.
pub fn regime_lines(block: &Value) -> Vec<String> {
    let Some(rows) = block.get("regimes").and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter(|r| r.get("regime").and_then(Value::as_str) == Some("W-ONE"))
        .filter_map(|r| {
            let partition = r.get("partition").and_then(Value::as_str)?;
            let posture = r.get("posture").and_then(Value::as_str)?;
            let next = r.get("next_action").and_then(Value::as_str);
            Some(match next {
                Some(n) => format!("{partition}: {posture} — {n}"),
                None => format!("{partition}: {posture}"),
            })
        })
        .collect()
}

/// The trust `Witnesses` section (recon-design-1 §5.4): heading + measurement lines (or the
/// honest unknown line — never a stale number) + the W-ONE regime posture lines. Empty string
/// when `block` is `None` (zero-SCIP repos: the section is ABSENT — R-0).
pub fn render_trust_section(block: Option<&Value>) -> String {
    let Some(block) = block else {
        return String::new();
    };
    let mut out = heading("Call-Graph Witnesses  (union accounting — syntax + compiler analyses)");
    match block.get("measured") {
        Some(m) if !m.is_null() => {
            for line in measurement_lines(m) {
                out.push_str(&bullet(&line));
            }
        }
        _ => {
            let reason = block
                .get("unknown_reason")
                .and_then(Value::as_str)
                .unwrap_or("not yet measured");
            out.push_str(&bullet(&format!("corroboration: unknown — {reason}")));
        }
    }
    for line in regime_lines(block) {
        out.push_str(&bullet(&line));
    }
    out
}

/// RECON-M-R3b: the reference-tier section (recon-design-1 §5.2 — "compiler-verified references
/// (reads / writes / type references)"): the §5.3.0-gated coverage label, the NAMED truncation
/// count (never silent), and the listed endpoint symbols. Empty string when `block` is `None`
/// (absent tier — R-0/R-1) or fails the §5.3.0 gate (no accounting marker / no coverage basis →
/// absence, never unlabeled figures). Defensive AND fail-closed on the count metadata (review-0
/// item 1): the listing array + `shown` + `truncated` must all be present and mutually consistent
/// (`shown == |references|` and `total == shown + truncated`) or the WHOLE section is suppressed —
/// a malformed block never renders a total its listing contradicts (which would be an UNNAMED
/// truncation, the §5.2 violation this tier exists to prevent). A missing/malformed field renders
/// absence, never a panic or an invented zero. `pub`: callers/callees/explain client renders share
/// this ONE projection — no per-surface phrasing drift.
pub fn render_reference_tier_section(block: Option<&Value>) -> String {
    let Some(block) = block else {
        return String::new();
    };
    // §5.3.0 gate: a union value renders ONLY with its accounting marker + a complete coverage
    // basis. `union_coverage_phrase` enforces both — a label-less block suppresses the section.
    let Some(coverage) = union_coverage_phrase(block) else {
        return String::new();
    };
    let (Some(total), Some(direction)) = (
        u(block, "total"),
        block.get("direction").and_then(Value::as_str),
    ) else {
        return String::new();
    };
    // Reader-frame noun for the direction (incoming = who references this; outgoing = what this
    // references). An unknown direction id suppresses the section (never a guessed frame).
    let noun = match direction {
        "incoming" => "referencing",
        "outgoing" => "referenced",
        _ => return String::new(),
    };
    // FAIL-CLOSED count metadata (review-0 item 1): §5.2 truncation must be NAMED, never silent, so
    // the totals and the listing must be mutually consistent or the whole section is suppressed.
    // Require the listing array AND both counts; demand `shown == |references|` and
    // `total == shown + truncated`. Without this, a malformed block (a dropped/short `references`
    // array, an absent `truncated`, a `shown` disagreeing with the listing) could render "30
    // referencing symbols", list none, and omit the "showing N of M" phrase — an UNNAMED
    // truncation. `checked_add` keeps a crafted overflow from panicking (the module's never-panic
    // contract). A byte-well-formed daemon block ALWAYS satisfies all three
    // (`reference_tier_block` emits `shown == items.len()`, `truncated == total - shown`), so this
    // rejects only corruption, never a valid answer.
    let (Some(items), Some(shown), Some(truncated)) = (
        block.get("references").and_then(Value::as_array),
        u(block, "shown"),
        u(block, "truncated"),
    ) else {
        return String::new();
    };
    if shown != items.len() as u64 || shown.checked_add(truncated) != Some(total) {
        return String::new();
    }

    let mut out = heading(&format!(
        "Compiler-Verified References  (reads / writes / type references — reconciled, {coverage})"
    ));
    // NAMED truncation (§5.2 — never silent): "showing N of M" only when the daemon truncated.
    let count_line = if truncated > 0 {
        format!("showing {shown} of {total} {noun} symbols")
    } else {
        format!("{total} {noun} symbol{}", if total == 1 { "" } else { "s" })
    };
    out.push_str(&bullet(&count_line));
    for item in items {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| item.get("stable_key").and_then(Value::as_str))
            .unwrap_or("-");
        let file = item.get("file").and_then(Value::as_str).unwrap_or("-");
        out.push_str(&bullet(&format!("{name}  {file}")));
    }
    out
}

/// RECON-M-R4 (§5.5): the Layer-2 attribution section — "likely resolves to X" hints
/// (the compiler resolved a same-named call the syntax pipeline could not confirm) + the
/// contested-resolution signals (syntax and compiler resolutions disagree). Empty when `block` is
/// `None` (R-0/R-1) or fails the layer2 accounting/coverage gate (absence, never unlabeled
/// figures). Every line is a Layer-2 CLAIM per the labels rule (§5.5 #4): "likely" (certainty
/// distinct from "resolves"), the BASIS named (compiler resolution + same-name), never implying
/// syntax/pipeline confirmation. Defensive: a malformed item renders absence, never a panic or an
/// invented target. NAMED truncation (§5.2 — never silent). `pub`: trust + explain share this ONE
/// projection — no per-surface phrasing drift.
pub fn render_layer2_resolution_section(block: Option<&Value>) -> String {
    let Some(block) = block else {
        return String::new();
    };
    // The layer2 accounting marker + a complete coverage basis (a label-less block suppresses the
    // section — a Layer-2 hint never renders without its certainty class + coverage).
    if block.get("accounting").and_then(Value::as_str) != Some("layer2") {
        return String::new();
    }
    let Some(coverage) = coverage_phrase(block) else {
        return String::new();
    };

    // A reader-frame endpoint phrase "name (file)" from a target object; `None` when neither is
    // present (the item is then skipped — never a half-blank claim).
    let endpoint = |t: Option<&Value>| -> Option<String> {
        let t = t?;
        let name = t
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let file = t
            .get("file")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        match (name, file) {
            (Some(n), Some(f)) => Some(format!("{n} ({f})")),
            (Some(n), None) => Some(n.to_string()),
            (None, Some(f)) => Some(f.to_string()),
            (None, None) => None,
        }
    };
    let caller_of = |item: &Value| -> Option<String> {
        item.get("caller_name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| item.get("caller").and_then(Value::as_str))
            .map(str::to_string)
    };

    let mut out = String::new();

    // ── Case 1: unresolved calls the compiler resolved — the likely resolutions AND the
    //    ambiguity guard's refusals. Both are outcomes of the SAME §5.5 name-guarded join over
    //    pipeline-unresolved sites (some sites resolve to exactly one same-named compiler target →
    //    "likely"; some to ≥ 2 → REFUSED as ambiguous), so they share ONE Layer-2/coverage
    //    heading. An ambiguity-ONLY state is still a Layer-2 attribution outcome and renders under
    //    that heading — never an orphaned bullet, and the refusal reads grammatically whether or
    //    not a likely hint preceded it (review-0 required-change #3). ──
    let likely_items = block
        .get("likely")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    // Render the likely bullets into a buffer FIRST so the heading and the "more" prefix reflect
    // what ACTUALLY rendered — a defensively-skipped malformed item never leaves a headed-but-empty
    // section, nor a dangling "more".
    let mut likely_body = String::new();
    for item in likely_items {
        let (Some(caller), Some(call), Some(target)) = (
            caller_of(item),
            item.get("call").and_then(Value::as_str),
            endpoint(item.get("resolves_to")),
        ) else {
            continue;
        };
        // Per-site (§5.5 / review-1 #2): append THIS site's own call-site line when the floor
        // recorded it, so two unresolved sites of the same call in one caller are distinct and
        // locatable (the location is the site's, never an arbitrary first). Appended AT THE END so
        // the basis clause the labels-rule audit checks is a stable prefix.
        let at = item
            .get("line")
            .and_then(Value::as_i64)
            .map(|l| format!(" (line {l})"))
            .unwrap_or_default();
        likely_body.push_str(&bullet(&format!(
            "in {caller}, {call}(…) likely resolves to {target} — a same-named call the \
             compiler resolved; syntax did not confirm it{at}"
        )));
    }
    let likely_rendered = !likely_body.is_empty();
    let ambiguous = u(block, "ambiguous").filter(|n| *n > 0);
    if likely_rendered || ambiguous.is_some() {
        // ONE umbrella heading honest for BOTH sub-outcomes AND for the CERTAINTY class (review-1
        // #3): "Compiler Evidence for Unresolved Calls" names the BASIS (compiler-side evidence)
        // without claiming exact resolution — the bullets say "likely resolves", the section must
        // not upgrade that to "…the Compiler Resolved". Honest for a likely hint, an ambiguity
        // refusal, or both.
        out.push_str(&heading(&format!(
            "Compiler Evidence for Unresolved Calls  (Layer-2 — syntax could not confirm these, \
             {coverage})"
        )));
        // Named truncation of the likely list (§5.2 — never silent), only when hints rendered.
        if likely_rendered && u(block, "likely_truncated").unwrap_or(0) > 0 {
            let total = u(block, "likely_total").unwrap_or(likely_items.len() as u64);
            let shown = u(block, "likely_shown").unwrap_or(likely_items.len() as u64);
            out.push_str(&bullet(&format!("showing {shown} of {total}")));
        }
        out.push_str(&likely_body);
        // The ambiguity guard's refusals — counted, never guessed (§5.5). "more" ONLY after a
        // shown likely hint (else this is the first/only line → grammatical without it).
        if let Some(ambiguous) = ambiguous {
            let more = if likely_rendered { "more " } else { "" };
            out.push_str(&bullet(&format!(
                "{ambiguous} {more}unresolved call{} had multiple same-named compiler candidates \
                 — not attributed (ambiguous)",
                if ambiguous == 1 { "" } else { "s" }
            )));
        }
    }

    // ── Case 2: contested resolutions ──
    if let Some(items) = block.get("contested").and_then(Value::as_array) {
        if !items.is_empty() {
            out.push_str(&heading(&format!(
                "Contested Resolutions  (Layer-2 — syntax and compiler resolutions disagree, \
                 {coverage})"
            )));
            let total = u(block, "contested_total").unwrap_or(items.len() as u64);
            if u(block, "contested_truncated").unwrap_or(0) > 0 {
                let shown = u(block, "contested_shown").unwrap_or(items.len() as u64);
                out.push_str(&bullet(&format!("showing {shown} of {total}")));
            }
            for item in items {
                let (Some(caller), Some(call), Some(syntax), Some(compiler)) = (
                    caller_of(item),
                    item.get("call").and_then(Value::as_str),
                    endpoint(item.get("syntax_target")),
                    endpoint(item.get("compiler_target")),
                ) else {
                    continue;
                };
                out.push_str(&bullet(&format!(
                    "in {caller}, {call}: syntax points to {syntax}; the compiler resolved a \
                     same-named call to {compiler}"
                )));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn measured_fixture() -> Value {
        json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["a", "b"], "fingerprint": "fp"},
            "union_calls": 579, "pipeline_calls": 531, "dual_measured": 507,
            "agreement_pct": 97.43589743589743,
            "both": {"instances": 494, "identities": 454},
            "semantic_only_calls": {"new_pair": 48, "multiplicity": 0, "identities": 37},
            "syntactic_only": {"boundary": 11, "file_scope": 1, "uncorroborated": 1, "multiplicity": 0, "identities": 12},
            "unmeasured_edges": {"instances": 24, "identities": 14},
            "identity_suspect": 0,
            "identity_collision": {"instances": 0, "identities": 0},
            "projections": {"total": 4860, "unanswerable": 1071},
            "references": 12189,
        })
    }

    /// The daemon's g1u block shape (§5.3.2 — FLAT counts, unlike the trust block's
    /// instance/identity objects).
    fn g1u_fixture() -> Value {
        json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["a", "b"], "fingerprint": "fp"},
            "union_calls": 579, "pipeline_calls": 531, "dual_measured": 507,
            "agreement_pct": 97.43589743589743,
            "both": 494, "semantic_only_calls": 48, "syntactic_only": 13, "unmeasured_edges": 24,
        })
    }

    #[test]
    fn g1u_line_renders_the_labeled_reconciled_figures() {
        let line = g1u_line(&g1u_fixture()).expect("labeled block renders");
        assert!(line.contains("531 syntax-resolved (all languages)"));
        assert!(line.contains("579 combined-analyses calls (TypeScript (2 partitions))"));
        assert!(line.contains("of 507 the compiler could measure, 494 corroborated (97.4%)"));
    }

    #[test]
    fn g1u_line_refuses_an_unlabeled_block() {
        // A union value without its coverage basis never renders (§5.3.0 labeling rule).
        assert_eq!(
            g1u_line(&json!({"union_calls": 5, "pipeline_calls": 5})),
            None
        );
    }

    /// Review-2 item 1: the §5.3.0 gate requires BOTH halves — a block with a valid coverage
    /// basis but no `accounting: "union"` marker (or a wrong one) suppresses the union value
    /// entirely, on the shared gate and on every consumer.
    #[test]
    fn union_values_without_the_accounting_marker_never_render() {
        let mut block = g1u_fixture();
        block.as_object_mut().unwrap().remove("accounting");
        assert_eq!(union_coverage_phrase(&block), None);
        assert_eq!(g1u_line(&block), None);

        block["accounting"] = json!("pipeline"); // wrong family, coverage intact
        assert_eq!(union_coverage_phrase(&block), None);
        assert_eq!(g1u_line(&block), None);

        // The measurement lines refuse the same way: absence, never unlabeled figures.
        let mut m = measured_fixture();
        m.as_object_mut().unwrap().remove("accounting");
        assert!(measurement_lines(&m).is_empty());
        let s = render_trust_section(Some(&json!({"regimes": [], "measured": m})));
        assert!(
            !s.contains("corroborated") && !s.contains("syntax-resolved"),
            "a marker-less measured block must render no figures: {s}"
        );
    }

    #[test]
    fn trust_section_renders_measurement_and_reference_tier() {
        let block = json!({
            "producer": {"name": "scip-typescript", "provisioned": true},
            "regimes": [{"partition": "a", "language": "TypeScript", "regime": "W-BOTH"}],
            "measured": measured_fixture(),
        });
        let s = render_trust_section(Some(&block));
        assert!(s.contains("Call-Graph Witnesses"));
        assert!(s.contains("494 corroborated (97.4%)"));
        assert!(s.contains("11 across compiler-run boundaries"));
        assert!(s.contains("48 compiler-resolved calls"));
        assert!(s.contains("12189 compiler-verified references"));
        assert!(s.contains("1071 of 4860 symbol-direction lookups"));
        // Guards silent at zero (absence, not zero-noise).
        assert!(!s.contains("collide"));
        assert!(!s.contains("adoption suspects"));
    }

    #[test]
    fn trust_section_renders_unknown_never_a_number_when_unmeasured() {
        let block = json!({
            "producer": {"name": "scip-typescript", "provisioned": false},
            "regimes": [],
            "measured": null,
            "unknown_reason": "not yet measured (computed on the next call-graph read)",
        });
        let s = render_trust_section(Some(&block));
        assert!(s.contains("corroboration: unknown — not yet measured"));
        assert!(!s.contains("corroborated"));
    }

    #[test]
    fn trust_section_absent_when_block_absent() {
        assert_eq!(render_trust_section(None), "");
    }

    /// Review-0 defect (b) — unit truth: the trust line renders the ledger's ACTUAL unit
    /// (withheld call instances, with the distinct-pair sub-count), never "N identities
    /// collide" over an instance count.
    #[test]
    fn collision_line_renders_when_the_guard_fired() {
        let mut m = measured_fixture();
        m["identity_collision"] = json!({"instances": 3, "identities": 2});
        let block = json!({"regimes": [], "measured": m});
        let s = render_trust_section(Some(&block));
        assert!(
            s.contains(
                "identity collisions between the syntax index and the compiler index: \
                 3 compiler-witnessed call instances (2 call pairs) withheld"
            ),
            "{s}"
        );
        assert!(s.contains("never merged"));
        assert!(
            !s.contains("3 identities collide"),
            "the instance count must never be labeled as identities: {s}"
        );
    }

    /// Review-1 item 5: a malformed/additive payload must never render an absent measurement
    /// as a measured zero — an aggregate missing a component field renders ABSENCE, and the
    /// present fields still render (partial payloads degrade honestly, never invent).
    #[test]
    fn malformed_syntactic_aggregate_renders_absence_never_a_partial_sum() {
        let mut m = measured_fixture();
        // `uncorroborated` missing: 11+1+0 = 12 would silently present an incomplete cause
        // breakdown as complete. The whole clause must be absent instead.
        m["syntactic_only"] = json!({"boundary": 11, "file_scope": 1, "multiplicity": 0});
        let block = json!({"regimes": [], "measured": m});
        let s = render_trust_section(Some(&block));
        assert!(
            !s.contains("syntax-only"),
            "a partial aggregate must not render as a total: {s}"
        );
        // The strictly-required corroboration line still renders (its fields are intact).
        assert!(s.contains("494 corroborated (97.4%)"), "{s}");
    }

    #[test]
    fn malformed_semantic_aggregate_omits_its_part_but_keeps_references() {
        let mut m = measured_fixture();
        m["semantic_only_calls"] = json!({"new_pair": 48}); // `multiplicity` missing
        let block = json!({"regimes": [], "measured": m});
        let s = render_trust_section(Some(&block));
        assert!(
            !s.contains("compiler-resolved calls"),
            "a partial semantic sum must not render: {s}"
        );
        assert!(
            s.contains("12189 compiler-verified references"),
            "the intact references field still renders: {s}"
        );
    }

    #[test]
    fn missing_references_field_renders_absence_not_zero() {
        let mut m = measured_fixture();
        m.as_object_mut().unwrap().remove("references");
        m["semantic_only_calls"] = json!({"new_pair": 0, "multiplicity": 0});
        let block = json!({"regimes": [], "measured": m});
        let s = render_trust_section(Some(&block));
        assert!(
            !s.contains("beyond the syntax graph"),
            "no invented figures on a missing field: {s}"
        );
        assert!(!s.contains("0 compiler-verified references"), "{s}");
    }

    #[test]
    fn w_one_regime_lines_render_with_next_actions() {
        let block = json!({
            "regimes": [
                {"partition": "app", "language": "TypeScript", "regime": "W-ONE",
                 "reason": "stale",
                 "posture": "compiler-side analysis here is out of date (the source changed after the compiler last ran)",
                 "next_action": "refresh `app` to re-enable corroboration"},
            ],
            "measured": null,
            "unknown_reason": "not yet measured",
        });
        let s = render_trust_section(Some(&block));
        assert!(s.contains("app: compiler-side analysis here is out of date"));
        assert!(s.contains("refresh `app` to re-enable corroboration"));
    }

    /// Review-3 blocking defect: an INCOMPLETE §5.3.0 coverage basis (missing/empty
    /// fingerprint, empty arrays, malformed members) must SUPPRESS the reconciled value
    /// in every shared-gate consumer — never render it unlabeled or partially labeled.
    #[test]
    fn incomplete_coverage_basis_suppresses_the_union_value() {
        let mutations: [(&str, Value); 6] = [
            (
                "missing fingerprint",
                json!({"languages": ["TypeScript"], "partitions": ["a"]}),
            ),
            (
                "empty fingerprint",
                json!({"languages": ["TypeScript"], "partitions": ["a"], "fingerprint": ""}),
            ),
            (
                "empty languages",
                json!({"languages": [], "partitions": ["a"], "fingerprint": "fp"}),
            ),
            (
                "empty partitions",
                json!({"languages": ["TypeScript"], "partitions": [], "fingerprint": "fp"}),
            ),
            (
                "non-string language member",
                json!({"languages": ["TypeScript", 7], "partitions": ["a"], "fingerprint": "fp"}),
            ),
            (
                "empty partition member",
                json!({"languages": ["TypeScript"], "partitions": ["a", ""], "fingerprint": "fp"}),
            ),
        ];
        for (case, cov) in mutations {
            let mut block = g1u_fixture();
            block["coverage"] = cov;
            assert!(
                union_coverage_phrase(&block).is_none(),
                "{case}: phrase must suppress"
            );
            assert!(g1u_line(&block).is_none(), "{case}: g1u line must suppress");
            let mut trust = measured_fixture();
            trust["coverage"] = block["coverage"].clone();
            assert!(
                union_coverage_phrase(&trust).is_none(),
                "{case}: trust block must suppress"
            );
        }
    }

    #[test]
    fn complete_coverage_basis_still_renders() {
        assert_eq!(
            union_coverage_phrase(&g1u_fixture()).as_deref(),
            Some("TypeScript (2 partitions)")
        );
    }

    // ── RECON-M-R3b: the reference-tier section ─────────────────────────────────────────────

    fn reference_block(total: u64, item_count: usize, direction: &str) -> Value {
        let items: Vec<Value> = (0..item_count)
            .map(|i| {
                json!({
                    "stable_key": format!("repo:src/f{i}.ts#s{i}:SYMBOL:FUNCTION"),
                    "name": format!("s{i}"),
                    "file": format!("src/f{i}.ts"),
                })
            })
            .collect();
        let shown = item_count as u64;
        json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "direction": direction,
            "total": total,
            "shown": shown,
            "truncated": total - shown,
            "references": items,
        })
    }

    #[test]
    fn reference_tier_section_renders_labeled_with_named_truncation() {
        // total 30, only 3 shown → NAMED truncation (never silent).
        let block = reference_block(30, 3, "incoming");
        let s = render_reference_tier_section(Some(&block));
        assert!(s.contains("Compiler-Verified References"));
        assert!(s.contains("reads / writes / type references"));
        assert!(s.contains("reconciled, TypeScript (1 partition)"));
        assert!(
            s.contains("showing 3 of 30 referencing symbols"),
            "named truncation: {s}"
        );
        assert!(s.contains("s0  src/f0.ts"));
    }

    #[test]
    fn reference_tier_section_full_listing_has_no_truncation_phrase() {
        let block = reference_block(2, 2, "outgoing");
        let s = render_reference_tier_section(Some(&block));
        assert!(s.contains("2 referenced symbols"), "{s}");
        assert!(
            !s.contains("showing"),
            "no truncation phrase when complete: {s}"
        );
    }

    #[test]
    fn reference_tier_section_absent_when_block_absent() {
        assert_eq!(render_reference_tier_section(None), "");
    }

    #[test]
    fn reference_tier_section_suppressed_without_the_accounting_marker() {
        // §5.3.0 gate: a union value without its accounting marker never renders (even with a
        // valid coverage basis) — the shared gate governs the reference tier too.
        let mut block = reference_block(2, 2, "incoming");
        block.as_object_mut().unwrap().remove("accounting");
        assert_eq!(render_reference_tier_section(Some(&block)), "");
        block["accounting"] = json!("pipeline");
        assert_eq!(render_reference_tier_section(Some(&block)), "");
    }

    #[test]
    fn reference_tier_section_suppressed_on_incomplete_coverage_or_bad_direction() {
        // Incomplete coverage basis (empty fingerprint) suppresses (the §5.3.0 gate).
        let mut block = reference_block(2, 2, "incoming");
        block["coverage"]["fingerprint"] = json!("");
        assert_eq!(render_reference_tier_section(Some(&block)), "");
        // An unknown direction id is never given a guessed reader frame.
        let bad_dir = reference_block(2, 2, "sideways");
        assert_eq!(render_reference_tier_section(Some(&bad_dir)), "");
    }

    /// Review-0 (iteration 0) item 1 — FAIL CLOSED on inconsistent count metadata: a block whose
    /// `total` its listing + counts cannot back must suppress the WHOLE section, never render an
    /// UNNAMED truncation ("30 referencing symbols" listing none, no "showing N of M"). Each case
    /// breaks exactly one consistency rule from a known-good (total 30 / shown 3 / truncated 27)
    /// block; each must render "".
    #[test]
    fn reference_tier_section_fails_closed_on_inconsistent_count_metadata() {
        // The exact defect the reviewer named: a totals-only block (no listing array, no `shown`,
        // no `truncated`). The old renderer showed "30 referencing symbols" — a phantom count.
        let phantom = json!({
            "accounting": "union",
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "direction": "incoming",
            "total": 30,
        });
        assert_eq!(
            render_reference_tier_section(Some(&phantom)),
            "",
            "a totals-only block must suppress, never render an unnamed truncation"
        );

        // Each mutation breaks exactly one consistency rule of a known-good
        // (total 30 / shown 3 / truncated 27) block; each must suppress the whole section.
        let suppressed = |mutate: &dyn Fn(&mut Value)| {
            let mut b = reference_block(30, 3, "incoming");
            mutate(&mut b);
            render_reference_tier_section(Some(&b)).is_empty()
        };
        assert!(
            suppressed(&|b| {
                b.as_object_mut().unwrap().remove("references");
            }),
            "references array absent must suppress"
        );
        assert!(
            suppressed(&|b| {
                b.as_object_mut().unwrap().remove("shown");
            }),
            "shown absent must suppress"
        );
        assert!(
            suppressed(&|b| {
                b.as_object_mut().unwrap().remove("truncated");
            }),
            "truncated absent must suppress"
        );
        assert!(
            suppressed(&|b| {
                b["shown"] = json!(25); // claims 25 shown, lists 3
            }),
            "shown disagreeing with the listing length must suppress"
        );
        assert!(
            suppressed(&|b| {
                b["truncated"] = json!(0); // 30 != 3 + 0
            }),
            "total != shown + truncated must suppress"
        );

        // The guard is not over-broad: the known-good block still renders its named truncation.
        assert!(
            render_reference_tier_section(Some(&reference_block(30, 3, "incoming")))
                .contains("showing 3 of 30 referencing symbols"),
            "a consistent block must still render"
        );
    }

    // ── RECON-M-R4: the Layer-2 attribution section (labels-rule audit) ──────────────────────

    fn layer2_block() -> Value {
        json!({
            "accounting": "layer2",
            "coverage": {"languages": ["TypeScript"], "partitions": ["p"], "fingerprint": "fp"},
            "likely": [{
                "caller": "repo:src/Toolbar.tsx#Toolbar:SYMBOL:FUNCTION",
                "caller_name": "Toolbar", "call": "cn",
                "resolves_to": {"name": "cn", "file": "src/utils.ts",
                                "stable_key": "repo:src/utils.ts#cn:SYMBOL:FUNCTION"},
                "line": 12, "col": 4,
            }],
            "likely_total": 1, "likely_shown": 1, "likely_truncated": 0,
            "ambiguous": 2,
            "contested": [{
                "caller": "repo:src/cart.ts#getStoredConsent:SYMBOL:FUNCTION",
                "caller_name": "getStoredConsent", "call": "removeItem",
                "syntax_target": {"name": "removeItem", "file": "src/cart.ts", "stable_key": "a"},
                "compiler_target": {"name": "removeItem", "file": "src/other.ts", "stable_key": "b"},
            }],
            "contested_total": 1, "contested_shown": 1, "contested_truncated": 0,
        })
    }

    /// The labels rule (§5.5 #4): certainty distinct ("likely resolves" ≠ "resolves"), basis
    /// named, never implying pipeline/syntax confirmation, Layer-2 marked, coverage labeled.
    #[test]
    fn layer2_section_renders_likely_contested_with_labels_rule_wording() {
        let s = render_layer2_resolution_section(Some(&layer2_block()));
        // review-1 #3: the heading names the BASIS, never claims exact resolution ("…Resolved").
        assert!(s.contains("Compiler Evidence for Unresolved Calls"));
        assert!(
            !s.contains("the Compiler Resolved"),
            "the heading must not upgrade 'likely resolves' to an exact-resolution claim: {s}"
        );
        assert!(
            s.contains("Layer-2"),
            "the Layer-2 certainty class is stated"
        );
        assert!(s.contains("TypeScript (1 partition)"), "coverage labeled");
        assert!(
            s.contains("in Toolbar, cn(…) likely resolves to cn (src/utils.ts)"),
            "certainty distinct — 'likely resolves', target named: {s}"
        );
        assert!(
            s.contains("a same-named call the compiler resolved; syntax did not confirm it"),
            "basis named + never implies syntax confirmation: {s}"
        );
        // review-1 #2: the per-site call-site line is rendered (the fixture site is line 12).
        assert!(
            s.contains("syntax did not confirm it (line 12)"),
            "the site's own call-site line renders (per-site, not arbitrary): {s}"
        );
        // The ambiguity refusal is counted, never guessed.
        assert!(s.contains(
            "2 more unresolved calls had multiple same-named compiler candidates — not attributed \
             (ambiguous)"
        ));
        // Contested: disagreement stated, both targets' files shown.
        assert!(s.contains("Contested Resolutions"));
        assert!(s.contains("disagree"));
        assert!(s.contains(
            "in getStoredConsent, removeItem: syntax points to removeItem (src/cart.ts); the \
             compiler resolved a same-named call to removeItem (src/other.ts)"
        ));
    }

    #[test]
    fn layer2_section_suppressed_on_none_wrong_accounting_or_incomplete_coverage() {
        assert_eq!(render_layer2_resolution_section(None), "");
        // Wrong certainty class → never rendered as a Layer-2 hint.
        let mut b = layer2_block();
        b["accounting"] = json!("union");
        assert_eq!(render_layer2_resolution_section(Some(&b)), "");
        // Incomplete coverage basis → suppressed (a Layer-2 hint never renders unlabeled).
        let mut b2 = layer2_block();
        b2["coverage"]["fingerprint"] = json!("");
        assert_eq!(render_layer2_resolution_section(Some(&b2)), "");
    }

    #[test]
    fn layer2_section_names_truncation_and_is_silent_at_zero_ambiguous() {
        let mut b = layer2_block();
        b["likely_total"] = json!(30);
        b["likely_truncated"] = json!(29);
        let s = render_layer2_resolution_section(Some(&b));
        assert!(
            s.contains("showing 1 of 30"),
            "named truncation, never silent: {s}"
        );

        let mut b2 = layer2_block();
        b2["ambiguous"] = json!(0);
        b2.as_object_mut().unwrap().remove("contested");
        let s2 = render_layer2_resolution_section(Some(&b2));
        assert!(
            !s2.contains("ambiguous") && !s2.contains("Contested"),
            "zero ambiguous + no contested → no noise: {s2}"
        );
    }

    /// review-0 required-change #3: the AMBIGUITY-ONLY production state (`likely=[]`,
    /// `ambiguous>0`, `contested=[]`) — the state `layer2_refuses_an_ambiguous_site_never_a_guess`
    /// produces. It must render UNDER a Layer-2/coverage heading (never an orphaned bullet) with a
    /// GRAMMATICAL refusal: no dangling "more" when no likely hint preceded it.
    #[test]
    fn layer2_ambiguity_only_state_is_headed_and_grammatical() {
        let mut b = layer2_block();
        b["likely"] = json!([]);
        b["likely_total"] = json!(0);
        b["likely_shown"] = json!(0);
        b["likely_truncated"] = json!(0);
        b["ambiguous"] = json!(1);
        b.as_object_mut().unwrap().remove("contested");
        let s = render_layer2_resolution_section(Some(&b));

        // The Layer-2/coverage heading is present — the refusal is never an orphaned bullet.
        assert!(
            s.contains("Compiler Evidence for Unresolved Calls") && s.contains("Layer-2"),
            "ambiguity-only state must carry the Layer-2 heading: {s}"
        );
        assert!(
            s.contains("TypeScript (1 partition)"),
            "coverage labeled: {s}"
        );
        // Grammatical singular refusal, and NO dangling "more" (nothing preceded it).
        assert!(
            s.contains(
                "1 unresolved call had multiple same-named compiler candidates — not attributed \
                 (ambiguous)"
            ),
            "grammatical singular refusal: {s}"
        );
        assert!(
            !s.contains("more unresolved"),
            "no 'more' without a preceding shown hint: {s}"
        );
    }
}
