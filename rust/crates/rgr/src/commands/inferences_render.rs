//! `inferences_render.rs` — INFERENCES-SURFACE-1 human rendering for
//! `rmap inferences list`.
//!
//! Turns the daemon's `inferences_list` response into the agent-facing surface:
//!   - default: a GROUPED SUMMARY — what inferences are, which detectors apply +
//!     their produced counts, then `kind × count` with per-kind top symbols
//!     (≤ ~40 lines on the audit repos);
//!   - `--limit N`: COMPACT DETAIL — up to N records (kind, name, `file[:line]`,
//!     reason, confidence), with an explicit truncation line when the total exceeds N;
//!   - zero inferences: the explanatory empty state (a detector applies to these
//!     languages but recorded nothing vs no detector exists for these languages), in
//!     the reader's language.
//!
//! Abstraction (INFERENCES-SURFACE-1): crate-private module.
//!   - what: pure presentation for the inferences surface.
//!   - current user: `commands::inferences::run_inferences_list` (sole caller).
//!   - axis of variation: presentation needs a daemon-free test seam and keeps the
//!     thin command file under the 500-line guardrail.
//!   - rejected simpler: inline in `inferences.rs` — rejected (test seam + guardrail).
//!
//! STANDING HONESTY RULE: the response is our OWN DTO. Required fields are expected
//! present; a genuinely-absent one means a MALFORMED response (old daemon /
//! serialization bug) and is surfaced as such, NEVER papered over with a fabricated
//! count, total, or record. Internal basis rules (`basis_json`) are pipeline
//! diagnostics and are intentionally NOT rendered here — only the reader-facing
//! `reason` a detector provides (VISION: labels speak the reader's language).

use serde_json::Value;

const TOP_PER_KIND: usize = 3;

/// Render the whole surface. `limit_active` is true when the caller passed
/// `--limit N` (compact-detail mode); false selects the grouped summary.
pub fn render(result: &Value, limit_active: bool) -> String {
    // `count` is our own always-present field (the TRUE total). Missing/!number ⇒
    // malformed — surface it, never assume zero.
    let Some(count) = result.get("count").and_then(|v| v.as_u64()) else {
        return "(malformed inferences response: count missing or not a number)\n".to_string();
    };

    let mut out = String::new();
    out.push_str(&header());

    // Detector inventory line(s) — always shown; states which detectors this build
    // has, whether each applies to the snapshot's languages, and its produced count.
    out.push_str(&render_detectors(result.get("detectors")));

    if count == 0 {
        out.push('\n');
        out.push_str(&render_empty(result.get("empty")));
        return out;
    }

    let records = match result.get("results") {
        Some(Value::Array(a)) => a,
        _ => {
            out.push_str("\n(malformed inferences response: results missing or not a list)\n");
            return out;
        }
    };

    out.push('\n');
    if limit_active {
        out.push_str(&render_detail(result, records, count));
    } else {
        out.push_str(&render_summary(records, count));
    }
    out
}

fn header() -> String {
    "Inferences — Layer-3 orientation hints: what a symbol IS in its framework \
     (bean, component, hook),\ndetected from source and confidence-scored. Not \
     extracted call-graph facts — open the files.\n\n"
        .to_string()
}

/// One line per shipped detector: whether it applies to the snapshot's languages
/// (derived from the file-language mix) + how many rows it produced (a fact).
/// `detectors` is our own always-present array; a missing/!array value is surfaced as
/// malformed. We do NOT claim a detector "ran" (execution is not recorded) — only its
/// produced count and its (derived) applicability.
fn render_detectors(detectors: Option<&Value>) -> String {
    let arr = match detectors {
        Some(Value::Array(a)) => a,
        _ => {
            return "(malformed inferences response: detectors missing or not a list)\n".to_string()
        }
    };
    let mut out =
        String::from("Detectors on this build (applicability derived from file languages):\n");
    for d in arr {
        let label = d.get("label").and_then(|v| v.as_str());
        let subjects = d.get("subjects").and_then(|v| v.as_str());
        let applicable = d.get("applicable").and_then(|v| v.as_bool());
        let dcount = d.get("count").and_then(|v| v.as_u64());
        let (Some(label), Some(subjects), Some(applicable), Some(dcount)) =
            (label, subjects, applicable, dcount)
        else {
            out.push_str("  (malformed detector entry)\n");
            continue;
        };
        let status = if !applicable {
            "n/a — no files in its language".to_string()
        } else if dcount > 0 {
            format!("{dcount} inferences")
        } else {
            "applies by file language; none recorded".to_string()
        };
        out.push_str(&format!("  {label} ({subjects}) — {status}\n"));
    }
    out
}

/// The grouped summary: `kind × count` (kinds ordered by count desc, then name),
/// each with up to `TOP_PER_KIND` representative symbols. The count here IS the true
/// total (default mode ships full records), so no truncation caveat is needed.
fn render_summary(records: &[Value], count: u64) -> String {
    // Group records by kind, preserving first-seen (deterministic: the daemon
    // orders by kind then target_stable_key).
    use std::collections::BTreeMap;
    let mut by_kind: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for r in records {
        let kind = r
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown-kind)");
        by_kind.entry(kind.to_string()).or_default().push(r);
    }

    // Order kinds by descending count, then name.
    let mut kinds: Vec<(&String, &Vec<&Value>)> = by_kind.iter().collect();
    kinds.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));

    let mut out = format!(
        "{count} inferences across {} kind(s) — complete (true total):\n",
        by_kind.len()
    );
    for (kind, recs) in kinds {
        let n = recs.len();
        let tops: Vec<String> = recs
            .iter()
            .take(TOP_PER_KIND)
            .map(|r| symbol_at(r))
            .collect();
        let mut line = format!("  {kind:<26} {n:>5}");
        if !tops.is_empty() {
            line.push_str("  — ");
            line.push_str(&tops.join(", "));
        }
        if n > TOP_PER_KIND {
            line.push_str(&format!(", … (+{} more)", n - TOP_PER_KIND));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("\nCompact detail: rmap inferences list --limit 50   ·   full records: --json\n");
    out
}

/// Compact detail — up to N records, one line each. Leads with an explicit
/// truncation line whenever the payload is a strict subset of the true total, so the
/// cap is never silent.
fn render_detail(result: &Value, records: &[Value], count: u64) -> String {
    // `returned`/`truncated` are our OWN always-present fields — a missing one means
    // a malformed (old-daemon / serialization-bug) response. Surface it; NEVER
    // fabricate a value from the record count (that would hide a real truncation).
    let Some(returned) = result.get("returned").and_then(|v| v.as_u64()) else {
        return "(malformed inferences response: returned missing or not a number)\n".to_string();
    };
    let Some(truncated) = result.get("truncated").and_then(|v| v.as_bool()) else {
        return "(malformed inferences response: truncated missing or not a boolean)\n".to_string();
    };

    let mut out = if truncated {
        format!(
            "inference detail — showing {returned} of {count} (truncated; \
             --limit {count} or --json for all):\n"
        )
    } else {
        format!("inference detail — {returned} of {count}:\n")
    };
    for r in records {
        out.push_str(&render_record_line(r));
    }
    out
}

/// One compact record line: kind, name, file:line, reader-facing reason (when the
/// detector provides one), confidence. No uid / snapshot / created_at boilerplate.
fn render_record_line(r: &Value) -> String {
    let kind = r
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown-kind)");
    let name = symbol_at(r);
    let conf = r.get("confidence").and_then(|v| v.as_f64());
    let reason = r
        .get("value")
        .and_then(|v| v.get("reason"))
        .and_then(|v| v.as_str());
    let mut line = format!("  {kind:<26} {name}");
    if let Some(reason) = reason {
        line.push_str(&format!("  — {reason}"));
    }
    if let Some(conf) = conf {
        line.push_str(&format!("  (conf {conf:.2})"));
    }
    // Surface a malformed value payload the daemon flagged — never hide it.
    if r.get("value_error").and_then(|v| v.as_str()).is_some() {
        line.push_str("  (malformed value)");
    }
    line.push('\n');
    line
}

/// The reader-facing name + location of an inference: a framework-meaningful name
/// (`component_name`/`hook_name`) or the symbol parsed from the stable key, plus
/// `basename[:line]`. `file`/`line` are the daemon-projected location fields
/// (`file` handles both the `#…:SYMBOL` and `:FILE` key shapes; `line` is present
/// only when the detector recorded one — Spring beans have none, so we render the
/// file alone, NEVER a fabricated `:0`).
fn symbol_at(r: &Value) -> String {
    let name = r
        .get("value")
        .and_then(|v| v.get("component_name").or_else(|| v.get("hook_name")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| symbol_from_key(r))
        .unwrap_or_else(|| "(unknown)".to_string());

    let file = r.get("file").and_then(|v| v.as_str());
    let line = r.get("line").and_then(|v| v.as_u64());
    match (file, line) {
        (Some(f), Some(l)) => format!("{name} ({}:{l})", basename(f)),
        (Some(f), None) => format!("{name} ({})", basename(f)),
        (None, _) => name,
    }
}

/// Parse the symbol out of a `target_stable_key` (between `#` and the next `:`).
fn symbol_from_key(r: &Value) -> Option<String> {
    let key = r.get("target_stable_key").and_then(|v| v.as_str())?;
    let after_hash = key.split('#').nth(1)?;
    let sym = after_hash.split(':').next()?;
    if sym.is_empty() {
        None
    } else {
        Some(sym.to_string())
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Render the empty-state honesty line from the daemon's `empty` object. A missing
/// object when count==0 is malformed (the daemon always populates it) — surfaced,
/// never replaced with a fabricated "0 inferences" with no explanation.
fn render_empty(empty: Option<&Value>) -> String {
    match empty.and_then(|v| v.get("message")).and_then(|v| v.as_str()) {
        Some(msg) => format!("{msg}\n"),
        None => "(malformed inferences response: no empty-state explanation for a zero-inference snapshot)\n".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Mirrors what `inferences_serve::record_json` emits: the daemon-projected
    // `file`/`line` location fields alongside the raw value.
    fn record(kind: &str, path: &str, sym: &str, extra: Value) -> Value {
        let mut value = json!({ "line_start": 10 });
        if let (Value::Object(dst), Value::Object(src)) = (&mut value, extra) {
            dst.extend(src);
        }
        json!({
            "inference_uid": "inf-x",
            "target_stable_key": format!("repo_01a:{path}#{sym}:SYMBOL:FUNCTION"),
            "kind": kind,
            "file": path,
            "line": 10,
            "value": value,
            "confidence": 0.9,
            "extractor": "test:1.0",
            "created_at": "2026-01-01T00:00:00Z",
        })
    }

    fn detectors_glamcrm() -> Value {
        json!([
            {"detector":"react","label":"React","subjects":"components & hooks",
             "kinds":["react_component","react_hook_usage"],"applicable":true,"count":629},
            {"detector":"spring","label":"Spring","subjects":"container-managed beans",
             "kinds":["spring_container_managed"],"applicable":true,"count":123}
        ])
    }

    #[test]
    fn summary_groups_by_kind_with_true_total_and_top_symbols() {
        let results: Vec<Value> = (0..5)
            .map(|i| {
                record(
                    "react_hook_usage",
                    "web/src/App.tsx",
                    &format!("Comp{i}"),
                    json!({"hook_name":"useEffect"}),
                )
            })
            .chain(std::iter::once(record(
                "spring_container_managed",
                "backend/App.java",
                "AppConfig",
                json!({"reason":"@Configuration"}),
            )))
            .collect();
        let result = json!({
            "count": 6, "returned": 6, "truncated": false, "limit": Value::Null,
            "detectors": detectors_glamcrm(), "empty": Value::Null, "results": results,
        });
        let out = render(&result, false);
        assert!(
            out.contains("6 inferences across 2 kind(s) — complete (true total)"),
            "{out}"
        );
        assert!(out.contains("react_hook_usage"), "{out}");
        assert!(out.contains("(+2 more)"), "top-3 cap with overflow: {out}");
        assert!(
            out.contains("useEffect (App.tsx:10)"),
            "reader-facing name+loc: {out}"
        );
        assert!(out.contains("Detectors on this build"), "{out}");
        assert!(
            out.contains("React (components & hooks) — 629 inferences"),
            "{out}"
        );
    }

    #[test]
    fn detail_truncation_is_never_silent() {
        let results: Vec<Value> = (0..2)
            .map(|i| {
                record(
                    "react_component",
                    "a.tsx",
                    &format!("C{i}"),
                    json!({"component_name":format!("C{i}")}),
                )
            })
            .collect();
        let result = json!({
            "count": 752, "returned": 2, "truncated": true, "limit": 2,
            "detectors": detectors_glamcrm(), "empty": Value::Null, "results": results,
        });
        let out = render(&result, true);
        assert!(
            out.contains("showing 2 of 752 (truncated; --limit 752 or --json for all)"),
            "explicit truncation line: {out}"
        );
        assert!(out.contains("C0 (a.tsx:10)"), "compact record: {out}");
        assert!(
            !out.contains("inf-x"),
            "no uid boilerplate in human mode: {out}"
        );
    }

    #[test]
    fn empty_state_renders_the_daemon_reason() {
        let result = json!({
            "count": 0, "returned": 0, "truncated": false, "limit": Value::Null,
            "detectors": json!([
                {"detector":"react","label":"React","subjects":"components & hooks",
                 "kinds":["react_component","react_hook_usage"],"applicable":false,"count":0},
                {"detector":"spring","label":"Spring","subjects":"container-managed beans",
                 "kinds":["spring_container_managed"],"applicable":false,"count":0}
            ]),
            "empty": json!({
                "reason":"no_detector_for_languages",
                "message":"No inference detector on this build covers C, C++ (this build's inference detectors: React → JS/TS, Spring → Java)."
            }),
            "results": [],
        });
        let out = render(&result, false);
        assert!(
            out.contains("No inference detector on this build covers C, C++"),
            "{out}"
        );
        assert!(
            out.contains("React (components & hooks) — n/a"),
            "n/a status: {out}"
        );
    }

    #[test]
    fn malformed_count_is_surfaced_not_fabricated() {
        let result = json!({ "detectors": detectors_glamcrm(), "results": [] });
        let out = render(&result, false);
        assert!(
            out.contains("malformed inferences response: count"),
            "{out}"
        );
    }

    #[test]
    fn malformed_empty_when_zero_is_surfaced() {
        let result = json!({
            "count": 0, "detectors": detectors_glamcrm(), "results": [], "empty": Value::Null,
        });
        let out = render(&result, false);
        assert!(
            out.contains("no empty-state explanation"),
            "a zero-count snapshot with no empty object is malformed: {out}"
        );
    }

    #[test]
    fn detail_missing_returned_is_surfaced_not_fabricated() {
        // `returned`/`truncated` are our own fields; a missing one must surface as
        // malformed, never be reconstructed from the record count (which would hide a
        // real truncation).
        let result = json!({
            "count": 752, "truncated": true, "limit": 2,
            "detectors": detectors_glamcrm(), "empty": Value::Null,
            "results": [record("react_component", "a.tsx", "C0", json!({"component_name":"C0"}))],
        });
        let out = render(&result, true);
        assert!(
            out.contains("malformed inferences response: returned"),
            "missing `returned` must surface, not be fabricated: {out}"
        );
    }

    #[test]
    fn record_line_flags_malformed_value() {
        let mut rec = record(
            "react_component",
            "a.tsx",
            "C0",
            json!({"component_name":"C0"}),
        );
        if let Value::Object(m) = &mut rec {
            m.insert("value_error".to_string(), json!("expected value at line 1"));
        }
        let result = json!({
            "count": 1, "returned": 1, "truncated": false, "limit": 1,
            "detectors": detectors_glamcrm(), "empty": Value::Null, "results": [rec],
        });
        let out = render(&result, true);
        assert!(
            out.contains("(malformed value)"),
            "value_error surfaced: {out}"
        );
    }
}
