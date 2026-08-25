//! Human rendering of the Group-B semantic fallback that rides a `symbol not found`
//! error's `data` (EMBED-SEED-IMPL-1, spec §8.2 Group B / §8.3). Used by
//! `commands::graph::handle_daemon_error` for `callers`/`callees`/`path` — the exact
//! same additive-`data` idiom the CLI already renders for `AmbiguousSymbol`'s
//! `matches`. Kept out of the >500-line `graph.rs` per the structural guardrail.
//!
//! **Abstraction one-liner:** a crate-private presentation helper for ONE concrete
//! caller (`handle_daemon_error`); axis of variation: none claimed — a cohesion/size
//! split so `graph.rs` does not grow a new rendering responsibility. Rejected simpler:
//! inline in `handle_daemon_error` (grows an already-oversized file).
//!
//! STANDING HONESTY RULE: the daemon's error `data` is not our own DTO, so a field may
//! be genuinely absent (an old daemon, no seed store). Absence is reported as such /
//! skipped — never papered over with a fabricated score, model, or path.

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
        for (i, c) in cands.iter().enumerate() {
            render_candidate(&mut out, i + 1, c);
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

/// Render one Group-B candidate. Every identity field is from the daemon's error
/// `data` (not our DTO), so a genuinely-absent required field is surfaced as a
/// malformed candidate, NEVER fabricated (STANDING HONESTY RULE). `c` uses the §8.2
/// Group-B field `file`; the shared [`render_candidate_body`] renders the identity
/// line + `next` for both Group B (error `data`) and Group A (`focus.candidates`).
fn render_candidate(out: &mut String, n: usize, c: &Value) {
    out.push_str(&format!("  {n}. "));
    out.push_str(&render_candidate_body(c, "file"));
}

/// Shared candidate body (identity line + `next` follow-up) for BOTH the Group-B
/// error-`data` renderer here and the Group-A `orient`/`explain` no-match candidate
/// list (`orient_sections`). `file_field` is `"file"` for Group B (§8.2) and for
/// Group A's `FocusCandidate` (which also serializes `file`). Two concrete current
/// callers (Group B error render, Group A focus render); axis: none claimed — a
/// shared honesty-preserving formatter so both surfaces render candidates identically.
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
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:a.ts:FILE",
                "file": "src/a.ts",
                "score": 0.71,
                "source": "embedding",
                "model_id": "m",
                "module": {"owning": "backend/services"},
                "next": {"cmd": "explain", "args": ["glamCRM:a.ts:FILE"], "cwd": "/repo"}
            }],
            "hint": "no such symbol; these files are semantically near your query"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("hint: no such symbol"));
        assert!(out.contains("src/a.ts"));
        assert!(out.contains("embedding"));
        assert!(out.contains("module backend/services"));
        assert!(out.contains("(cd /repo && rmap explain glamCRM:a.ts:FILE)"));
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
    fn absent_required_field_is_malformed_never_fabricated() {
        let data = json!({
            "semantic_candidates": [{ "file": "src/a.ts" }], // missing score/model/key/source
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(out.contains("malformed candidate"));
    }

    #[test]
    fn non_embedding_source_is_malformed_never_relabeled_embedding() {
        // review-10 #4: a fully-formed candidate whose `source` is NOT "embedding" (a
        // malformed / foreign / old-daemon payload) must NOT be presented as an embedding
        // hint. Every identity field is present, so only the `source` guard rejects it.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:a.ts:FILE",
                "file": "src/a.ts",
                "score": 0.71,
                "source": "lexical",
                "model_id": "m",
                "next": {"cmd": "explain", "args": ["glamCRM:a.ts:FILE"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(
            out.contains("malformed candidate"),
            "surfaced as malformed: {out}"
        );
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
    fn missing_source_is_malformed_never_relabeled_embedding() {
        // The Group-B error `data` is not our DTO: a candidate can arrive with every
        // other field but no `source` at all. It is malformed, never a fabricated
        // "embedding" label.
        let data = json!({
            "semantic_candidates": [{
                "stable_key": "glamCRM:a.ts:FILE",
                "file": "src/a.ts",
                "score": 0.71,
                "model_id": "m",
                "next": {"cmd": "explain", "args": ["glamCRM:a.ts:FILE"], "cwd": "/repo"}
            }],
            "hint": "h"
        });
        let out = render_symbol_not_found_semantic(Some(&data)).expect("renders");
        assert!(
            out.contains("malformed candidate"),
            "surfaced as malformed: {out}"
        );
        assert!(
            !out.contains("embedding, model"),
            "no fabricated embedding label: {out}"
        );
    }
}
