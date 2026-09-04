//! Human rendering of the semantic-seed candidate surfaces. Two families live here
//! because they render two DIFFERENT wire shapes:
//!
//! - **Group A** (`orient`/`explain` focus no-match): a `FocusCandidate`, which
//!   serializes the path under `file` and carries no per-symbol span/qualified-name —
//!   rendered by [`render_candidate_body`] (the `file` field). Unchanged by
//!   CURSOR-ROUNDTRIP-1.
//! - **Group B** (`callers`/`callees`/`path` `symbol not found` fallback): the CURRENT
//!   SEED-CHUNK-1 candidate shape (`path`/`line`/`qualified_name`/`is_test`), the SAME
//!   shape `find` emits. CURSOR-ROUNDTRIP-1 (§2.2) routes it through the ONE current-DTO
//!   renderer [`render_seed_chunk_candidate`] (shared with `find`'s seed tier — not a
//!   second copy; STANDING HONESTY RULE 2). Before this slice Group B rendered through the
//!   pre-SEED-CHUNK-1 `file`-shaped [`render_candidate_body`], so every current-DTO
//!   candidate printed `(malformed candidate: missing file/…)` ×N.
//!
//! **Abstraction one-liner:** `render_seed_chunk_candidate` — a crate-private current-DTO
//! seed-candidate renderer; concrete current users: `find`'s `render_seed_tier`
//! (`commands::find::seed_render`) and Group B's `render_symbol_not_found_semantic`; axis:
//! none claimed — one renderer so the two surfaces cannot diverge (STANDING HONESTY RULE
//! 2). Rejected simpler: leave Group B on `render_candidate_body` (renders the current DTO
//! as malformed) or fork a Group-B copy (two renderers, drift).
//!
//! STANDING HONESTY RULE 1: a candidate that fails validation is COUNTED and STATED — the
//! renderer returns `Err(reason)` and the tier appends ONE honest
//! `N candidate(s) unreadable: <reason>` line ([`render_unreadable_summary`]); NEVER a
//! per-row `(malformed candidate: …)` placeholder, NEVER a fabricated score/model/path.

use serde_json::Value;

/// Render the semantic `hint` + `semantic_candidates` (if any) carried on a
/// `symbol not found` error's `data`. Returns `None` when `data` carries no seed
/// semantic keys at all (a plain not-found from a daemon without the tier) so the
/// caller renders exactly today's error. The `hint` is always printed when present;
/// the candidate block only when `semantic_candidates` is a non-empty array (§8.3
/// omit-when-empty).
pub fn render_symbol_not_found_semantic(data: Option<&Value>) -> Option<String> {
    let data = data?.as_object()?;
    let hint = data.get("hint").and_then(|v| v.as_str());
    let candidates = data
        .get("semantic_candidates")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty());
    // Nothing seed-related on this error ⇒ leave today's rendering untouched.
    if hint.is_none() && candidates.is_none() {
        return None;
    }

    let mut out = String::new();
    if let Some(hint) = hint {
        out.push_str(&format!("hint: {hint}\n"));
    }
    if let Some(cands) = candidates {
        out.push_str("Semantic candidates (Layer-3 embedding hints, not resolved facts):\n");
        // CURSOR-ROUNDTRIP-1 (§2.2): render through the CURRENT SEED-CHUNK-1 renderer
        // (`path`/`line`/`qualified_name`/`is_test`) — the SAME one `find` uses. A
        // candidate that fails validation is COUNTED (not rendered as a placeholder) and
        // stated ONCE below (STANDING HONESTY RULE 1).
        let mut unreadable: Vec<String> = Vec::new();
        for c in cands {
            match render_seed_chunk_candidate(c) {
                Ok(row) => out.push_str(&row),
                Err(reason) => unreadable.push(reason),
            }
        }
        if !unreadable.is_empty() {
            out.push_str(&render_unreadable_summary(&unreadable));
        }
    }
    Some(out)
}

/// Render the semantic-hint header line + the daemon's per-cause `reasons`
/// (EMBED-SEED-IMPL-1 review-9 #2). The fixed `summary` is the Layer-3 honesty
/// contract line; each `reason` is the SPECIFIC cause (a dead endpoint, a pins
/// mismatch, the stale-subset count) surfaced verbatim underneath — never collapsed
/// into the generic summary. Shared by the `orient` and `explain` no-match renderers
/// (two concrete current callers, real duplication); axis: none claimed — a shared
/// honesty-preserving formatter so both surfaces render the cause identically.
pub(crate) fn render_semantic_header(summary: &str, reasons: &[String]) -> String {
    let mut out = format!("Semantic hints: {summary}\n");
    for reason in reasons {
        out.push_str(&format!("  ({reason})\n"));
    }
    out
}

/// CURSOR-ROUNDTRIP-1 (§2.2): render ONE current-DTO SEED-CHUNK-1 seed candidate — the
/// SAME shape `find` emits (`path`/`line`/`qualified_name`/`is_test`) and the SAME renderer
/// `find`'s seed tier uses. Every identity field is validated; a genuinely-absent required
/// field returns `Err(reason)` so the caller can COUNT it and state ONE honest line
/// ([`render_unreadable_summary`]) — never a per-row placeholder, never a fabricated value
/// (STANDING HONESTY RULE 1). The `Ok` row is byte-identical to `find`'s prior
/// `render_seed_candidate` output (2-space anchor indent, `path:line` + qualified name,
/// validated `source == "embedding"` label, `[test]`/`[is_test unknown]` partition,
/// 4-space `next` line).
pub(crate) fn render_seed_chunk_candidate(c: &Value) -> Result<String, String> {
    let path = c.get("path").and_then(|v| v.as_str());
    let key = c.get("stable_key").and_then(|v| v.as_str());
    let score = c.get("score").and_then(|v| v.as_f64());
    let model = c.get("model_id").and_then(|v| v.as_str());
    let source = c.get("source").and_then(|v| v.as_str());
    let (Some(path), Some(key), Some(score), Some(model), Some(source)) =
        (path, key, score, model, source)
    else {
        return Err("missing required field — path/stable_key/score/model_id/source".to_string());
    };
    // STANDING HONESTY RULE: the provenance label is the daemon's own VALIDATED `source`,
    // never a hardcoded "embedding" pasted over a foreign/old-daemon payload.
    if source != "embedding" {
        return Err(format!("source {source:?} is not a Layer-3 embedding hint"));
    }
    let module = render_seed_chunk_module_hint(c.get("module")).ok_or_else(|| {
        "missing or invalid module hint — expected owning/unavailable".to_string()
    })?;
    let next = render_seed_chunk_next(c.get("next"), key)?;
    // SEED-CHUNK-1 anchor: `path:line` when a span is stored, else bare path (never a
    // fabricated 0). `qualified_name` is the human label when stored.
    let anchor = match c.get("line").and_then(|v| v.as_i64()) {
        Some(line) => format!("{path}:{line}"),
        None => path.to_string(),
    };
    let symbol = c
        .get("qualified_name")
        .and_then(|v| v.as_str())
        .map(|q| format!("  {q}"))
        .unwrap_or_default();
    // `is_test` is always serialized (SEED-CHUNK-1 §5 moat): production is the unlabeled
    // default; a test chunk is `[test]`; a MISSING/non-bool value is UNKNOWN classification,
    // rendered explicitly — never left blank to masquerade as production.
    let test_label = match c.get("is_test") {
        Some(Value::Bool(true)) => "  [test]",
        Some(Value::Bool(false)) => "",
        _ => "  [is_test unknown]",
    };
    let mut row = format!(
        "  {anchor}{symbol}  (score {score:.2}, {source}, model {model}{module}){test_label}\n"
    );
    row.push_str(&next);
    Ok(row)
}

/// Render a current-DTO candidate's `next` follow-up (4-space indent — `find`'s form).
/// `cwd` present ⇒ the `cd <cwd> && …` hint; absent-with-reason ⇒ the honest reason;
/// a `next` carrying neither, or no `next` object at all, is `Err` (malformed).
fn render_seed_chunk_next(next: Option<&Value>, key: &str) -> Result<String, String> {
    let Some(n) = next.and_then(|v| v.as_object()) else {
        return Err("missing next follow-up".to_string());
    };
    let cwd = n.get("cwd").and_then(|v| v.as_str());
    let unavailable = n.get("cwd_unavailable").and_then(|v| v.as_str());
    match (cwd, unavailable) {
        (Some(cwd), _) => Ok(format!("    → (cd {cwd} && rmap explain {key})\n")),
        (None, Some(reason)) => Ok(format!(
            "    → rmap explain {key}  (run from the repo root — working directory {reason})\n"
        )),
        (None, None) => Err("next has neither cwd nor a reason".to_string()),
    }
}

/// Render the current-DTO owning-module hint (`ModuleHint`, externally tagged):
/// `Some(", module <path>")` when genuine, `Some(", module: <reason>")` when explicitly
/// unavailable, `None` (malformed) otherwise — the caller surfaces it, never "no module".
fn render_seed_chunk_module_hint(module: Option<&Value>) -> Option<String> {
    let m = module?.as_object()?;
    if let Some(path) = m.get("owning").and_then(|v| v.as_str()) {
        return Some(format!(", module {path}"));
    }
    if let Some(reason) = m.get("unavailable").and_then(|v| v.as_str()) {
        return Some(format!(", module: {reason}"));
    }
    None
}

/// CURSOR-ROUNDTRIP-1 (§2.2, STANDING HONESTY RULE 1): the ONE honest line for candidates
/// that failed validation — the count plus the distinct reasons (order-preserving), never
/// a per-row placeholder. Shared by Group B and `find`'s seed tier.
pub(crate) fn render_unreadable_summary(reasons: &[String]) -> String {
    let mut distinct: Vec<&String> = Vec::new();
    for r in reasons {
        if !distinct.contains(&r) {
            distinct.push(r);
        }
    }
    let joined = distinct
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let n = reasons.len();
    let plural = if n == 1 { "" } else { "s" };
    format!("  {n} candidate{plural} unreadable: {joined}\n")
}

/// Shared candidate body (identity line + `next` follow-up) for the Group-A `orient`/
/// `explain` no-match candidate list (`orient_sections`), whose `FocusCandidate`
/// serializes the path under `file` and carries no per-symbol span/qualified-name. `c`
/// uses the §8.2 field `file`. (Group B moved to [`render_seed_chunk_candidate`] in
/// CURSOR-ROUNDTRIP-1 §2.2; this now has ONE concrete caller.) A genuinely-absent required
/// field is surfaced as a malformed candidate, NEVER fabricated (STANDING HONESTY RULE).
pub(crate) fn render_candidate_body(c: &Value, file_field: &str) -> String {
    let file = c.get(file_field).and_then(|v| v.as_str());
    let key = c.get("stable_key").and_then(|v| v.as_str());
    let score = c.get("score").and_then(|v| v.as_f64());
    let model = c.get("model_id").and_then(|v| v.as_str());
    // STANDING HONESTY RULE (review-10 #4): the provenance label is the daemon's own
    // `source`, VALIDATED — never a hardcoded "embedding" pasted over the payload. Group A
    // (`orient_sections`/`explain`) pre-filters `source == "embedding"`, so for it this
    // guard always passes (byte-identical); Group B (the `symbol not found` error `data`,
    // not our DTO) has NO such filter — a malformed / foreign / old-daemon candidate whose
    // `source` is absent or not `"embedding"` is surfaced as malformed here, never relabeled
    // as an embedding hint.
    let source = c.get("source").and_then(|v| v.as_str());
    let (Some(file), Some(key), Some(score), Some(model), Some(source)) =
        (file, key, score, model, source)
    else {
        return format!(
            "(malformed candidate: missing {file_field}/stable_key/score/model_id/source)\n"
        );
    };
    if source != "embedding" {
        return format!(
            "(malformed candidate: source {source:?} is not a Layer-3 embedding hint)\n"
        );
    }
    let module = render_module_hint(c.get("module"));
    // `source` is validated == "embedding"; render the daemon's own label, not a literal.
    let mut out = format!("{file}  (score {score:.2}, {source}, model {model}{module})\n");
    out.push_str(&render_next(c.get("next"), key));
    out
}

/// Render the owning-module hint (`ModuleHint`, externally tagged) — `, module <path>`
/// when genuine, `, module: <reason>` when explicitly unavailable, and an explicit
/// malformed marker otherwise (never "no module").
pub(crate) fn render_module_hint(module: Option<&Value>) -> String {
    let Some(m) = module.and_then(|v| v.as_object()) else {
        return ", module: (malformed — no hint)".to_string();
    };
    if let Some(path) = m.get("owning").and_then(|v| v.as_str()) {
        return format!(", module {path}");
    }
    if let Some(reason) = m.get("unavailable").and_then(|v| v.as_str()) {
        return format!(", module: {reason}");
    }
    ", module: (malformed — expected owning/unavailable)".to_string()
}

/// Render a candidate's `next` follow-up. `cwd` optional (operator ruling 2): present
/// ⇒ the `cd <cwd> && …` hint; absent ⇒ the honest reason, never a fabricated cwd.
pub(crate) fn render_next(next: Option<&Value>, key: &str) -> String {
    let Some(n) = next.and_then(|v| v.as_object()) else {
        return "     (malformed candidate: missing next follow-up)\n".to_string();
    };
    let cwd = n.get("cwd").and_then(|v| v.as_str());
    let unavailable = n.get("cwd_unavailable").and_then(|v| v.as_str());
    match (cwd, unavailable) {
        (Some(cwd), _) => format!("     → (cd {cwd} && rmap explain {key})\n"),
        (None, Some(reason)) => format!(
            "     → rmap explain {key}  (run from the repo root — working directory {reason})\n"
        ),
        (None, None) => {
            "     (malformed candidate: next has neither cwd nor a reason)\n".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_fired_candidates_with_labels() {
        // CURSOR-ROUNDTRIP-1 (§2.2): the Group-B fallback now renders the CURRENT
        // SEED-CHUNK-1 shape (`path`/`line`/`qualified_name`/`is_test`) through the same
        // renderer `find` uses — the `path:line` anchor + qualified name, not a bare file.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:src/a.ts#svc:SYMBOL:FUNCTION",
                "path": "src/a.ts",
                "line": 12,
                "qualified_name": "svc",
                "is_test": false,
                "score": 0.71,
                "source": "embedding",
                "model_id": "m",
                "module": {"owning": "backend/services"},
                "next": {"cmd": "explain", "args": ["glamCRM:src/a.ts#svc:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "no such symbol; these symbols are semantically near your query"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("hint: no such symbol"));
        assert!(out.contains("src/a.ts:12  svc"));
        assert!(out.contains("embedding"));
        assert!(out.contains("module backend/services"));
        assert!(out.contains("(cd /repo && rmap explain glamCRM:src/a.ts#svc:SYMBOL:FUNCTION)"));
        // The bug this slice fixes: a well-formed current-DTO candidate must NOT render as
        // a malformed placeholder (the pre-slice `file`-shaped renderer printed exactly that).
        assert!(
            !out.contains("malformed"),
            "no placeholder for a valid candidate: {out}"
        );
        assert!(
            !out.contains("unreadable"),
            "nothing unreadable here: {out}"
        );
    }

    #[test]
    fn test_classified_chunk_is_labeled_and_anchored() {
        // SEED-CHUNK-1 §5 moat: a test-classified chunk is anchored AND labeled `[test]`.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "r:src/a_test.ts#t:SYMBOL:FUNCTION",
                "path": "src/a_test.ts", "line": 5, "qualified_name": "t", "is_test": true,
                "score": 0.63, "source": "embedding", "model_id": "m",
                "module": {"owning": "db"},
                "next": {"cmd": "explain", "args": ["r:src/a_test.ts#t:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("src/a_test.ts:5  t"), "{out}");
        assert!(out.contains("[test]"), "test chunk labeled: {out}");
    }

    #[test]
    fn degraded_hint_only_no_candidate_block() {
        let data = json!({ "hint": "no local embedding model reachable" });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("hint: no local embedding model reachable"));
        assert!(!out.contains("Semantic candidates"));
    }

    #[test]
    fn no_seed_data_returns_none_so_todays_error_is_untouched() {
        // An error with no seed keys (e.g. AmbiguousSymbol's `matches`, or no data).
        assert!(render_symbol_not_found_semantic(None).is_none());
        assert!(render_symbol_not_found_semantic(Some(&json!({ "matches": [] }))).is_none());
    }

    #[test]
    fn absent_required_field_is_counted_one_line_never_per_row_placeholder() {
        // STANDING HONESTY RULE 1: a candidate missing required fields is COUNTED and
        // stated on ONE honest line — never a per-row `(malformed candidate: …)` placeholder.
        let data = json!({
            "semantic_candidates": [{ "path": "src/a.ts" }], // missing score/model/key/source
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(
            out.contains("1 candidate unreadable: missing required field"),
            "one honest counted line: {out}"
        );
        assert!(
            !out.contains("(malformed candidate"),
            "no per-row placeholder (RULE 1): {out}"
        );
    }

    #[test]
    fn multiple_unreadable_are_counted_once_with_distinct_reasons() {
        // Two candidates fail for the SAME reason → one line, count 2, reason stated once.
        let data = json!({
            "semantic_candidates": [{ "path": "src/a.ts" }, { "path": "src/b.ts" }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(
            out.contains("2 candidates unreadable: missing required field"),
            "counted once with count 2: {out}"
        );
    }

    #[test]
    fn non_embedding_source_is_counted_never_relabeled_embedding() {
        // A fully-formed candidate whose `source` is NOT "embedding" (a foreign/old-daemon
        // payload) must NOT be presented as an embedding hint; it is counted unreadable with
        // the offending source named.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:src/a.ts#s:SYMBOL:FUNCTION",
                "path": "src/a.ts", "line": 1, "qualified_name": "s", "is_test": false,
                "score": 0.71,
                "source": "lexical",
                "model_id": "m",
                "module": {"owning": "db"},
                "next": {"cmd": "explain", "args": ["glamCRM:src/a.ts#s:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("unreadable"), "surfaced as unreadable: {out}");
        assert!(
            out.contains("\"lexical\""),
            "names the offending source: {out}"
        );
        assert!(
            !out.contains("score 0.71, embedding"),
            "must NOT relabel a non-embedding source as an embedding hint: {out}"
        );
    }

    #[test]
    fn missing_source_is_counted_never_relabeled_embedding() {
        // A candidate with every other field but no `source` at all is unreadable, never a
        // fabricated "embedding" label.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:src/a.ts#s:SYMBOL:FUNCTION",
                "path": "src/a.ts", "line": 1, "qualified_name": "s", "is_test": false,
                "score": 0.71,
                "model_id": "m",
                "module": {"owning": "db"},
                "next": {"cmd": "explain", "args": ["glamCRM:src/a.ts#s:SYMBOL:FUNCTION"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("unreadable"), "surfaced as unreadable: {out}");
        assert!(
            !out.contains("embedding, model"),
            "no fabricated embedding label: {out}"
        );
    }

    #[test]
    fn valid_and_unreadable_mix_renders_row_then_one_counted_line() {
        // A valid candidate renders its row; a malformed sibling is counted on one line —
        // the valid row is never suppressed, the bad one never a placeholder.
        let data = json!({
            "semantic_candidates": [
                {
                    "stable_key": "r:src/a.ts#ok:SYMBOL:FUNCTION",
                    "path": "src/a.ts", "line": 3, "qualified_name": "ok", "is_test": false,
                    "score": 0.7, "source": "embedding", "model_id": "m",
                    "module": {"owning": "db"},
                    "next": {"cmd": "explain", "args": ["r:src/a.ts#ok:SYMBOL:FUNCTION"], "cwd": "/repo"}
                },
                { "path": "src/bad.ts" }
            ],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("src/a.ts:3  ok"), "valid row rendered: {out}");
        assert!(
            out.contains("1 candidate unreadable"),
            "one counted line for the bad one: {out}"
        );
    }
}
