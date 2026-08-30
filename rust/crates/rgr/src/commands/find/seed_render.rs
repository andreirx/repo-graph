//! The `find` DEMOTED semantic-seed tier renderer (FIND-FACTS-1 §2.3): the renamed
//! ranked-guesses header, then either the candidates or an explicit
//! `semantic seeds unavailable (<reason>)`. Every identity field is our OWN DTO; a
//! genuinely-absent required field is MALFORMED and surfaced, NEVER fabricated
//! (STANDING HONESTY RULE 1). Split off `find.rs` per the ≤500-line guardrail
//! (review-4 item 1).
//!
//! Abstraction record — module: `find::seed_render`; concrete current user:
//! `find::render_find_human`; axis: the ≤500-line guardrail; rejected simpler
//! alternative: inlining in `find.rs` (file stays >500).

/// Render the DEMOTED semantic-seed tier (§2.3): the renamed header, then either the
/// ranked guesses or an explicit `semantic seeds unavailable (<reason>)`.
pub(super) fn render_seed_tier(result: &serde_json::Value, out: &mut String) {
    out.push_str("Semantic seeds (embedding similarity — ranked guesses, not facts):\n");

    // `seeds_available` is our OWN DTO field, ALWAYS serialized (bool). A missing /
    // mistyped value is MALFORMED — surfaced, NEVER defaulted to `false` (which would
    // render a fabricated "unavailable" over a possibly-available tier; STANDING
    // HONESTY RULE 1). Absent ≠ false.
    let available = match result.get("seeds_available").and_then(|v| v.as_bool()) {
        Some(a) => a,
        None => {
            out.push_str("  (malformed find response: seeds_available missing or not a bool)\n");
            return;
        }
    };
    if !available {
        // When the tier is unavailable, the DTO ALWAYS carries the reason
        // (`build_find_response`: every `available == false` arm sets
        // `seeds_unavailable_reason`). Its absence here is MALFORMED — surfaced, never
        // a fabricated generic "reason unavailable" (STANDING HONESTY RULE 1).
        match result
            .get("seeds_unavailable_reason")
            .and_then(|v| v.as_str())
        {
            Some(reason) => out.push_str(&format!("  semantic seeds unavailable ({reason})\n")),
            None => {
                out.push_str("  (malformed find response: seeds unavailable but no reason given)\n")
            }
        }
        return;
    }

    // `candidates` is our OWN DTO field, ALWAYS serialized (`[]` when empty). A
    // missing key / non-array is MALFORMED, not a genuine zero.
    let candidates = match result.get("candidates") {
        Some(serde_json::Value::Array(a)) => a,
        _ => {
            out.push_str("  (malformed find response: candidates missing or not a list)\n");
            return;
        }
    };
    if candidates.is_empty() {
        out.push_str("  (no area scored above zero)\n");
        return;
    }
    for c in candidates {
        out.push_str(&render_seed_candidate(c));
    }
}

/// Render one seed candidate (unchanged validation from the prior slice): every
/// identity field is our own DTO; a genuinely-absent required field is MALFORMED and
/// surfaced, NEVER fabricated. The provenance label is the daemon's own VALIDATED
/// `source` (must be `embedding`), never a literal pasted over the payload.
fn render_seed_candidate(c: &serde_json::Value) -> String {
    let path = c.get("path").and_then(|v| v.as_str());
    let key = c.get("stable_key").and_then(|v| v.as_str());
    let score = c.get("score").and_then(|v| v.as_f64());
    let model = c.get("model_id").and_then(|v| v.as_str());
    let source = c.get("source").and_then(|v| v.as_str());
    let (Some(path), Some(key), Some(score), Some(model), Some(source)) =
        (path, key, score, model, source)
    else {
        return
            "  (malformed candidate: missing required field — path/stable_key/score/model_id/source)\n"
                .to_string();
    };
    if source != "embedding" {
        return format!(
            "  (malformed candidate: source {source:?} is not a Layer-3 embedding hint)\n"
        );
    }
    let Some(module) = render_module_hint(c.get("module")) else {
        return
            "  (malformed candidate: missing or invalid module hint — expected owning/unavailable)\n"
                .to_string();
    };
    let mut s = format!("  {path}  (score {score:.2}, {source}, model {model}{module})\n");
    s.push_str(&render_next(c.get("next"), key));
    s
}

/// Render a candidate's `next` follow-up line. `cwd` is OPTIONAL (operator ruling 2):
/// when the registry resolved the repo root it prints the `cd <cwd> && …` hint; when
/// the lookup was unavailable it prints the honest reason from `next.cwd_unavailable`
/// (never a fabricated empty cwd). A `next` object carrying NEITHER is malformed.
fn render_next(next: Option<&serde_json::Value>, key: &str) -> String {
    let Some(n) = next.and_then(|v| v.as_object()) else {
        return "    (malformed candidate: missing next follow-up)\n".to_string();
    };
    let cwd = n.get("cwd").and_then(|v| v.as_str());
    let unavailable = n.get("cwd_unavailable").and_then(|v| v.as_str());
    match (cwd, unavailable) {
        (Some(cwd), _) => format!("    → (cd {cwd} && rmap explain {key})\n"),
        (None, Some(reason)) => {
            format!(
                "    → rmap explain {key}  (run from the repo root — working directory {reason})\n"
            )
        }
        (None, None) => {
            "    (malformed candidate: next has neither cwd nor a reason)\n".to_string()
        }
    }
}

/// Render the owning-module hint (`ModuleHint`, externally tagged) for human mode:
/// `Some(", module <path>")` when a genuine module is known, `Some(", module: <reason>")`
/// when explicitly unavailable. Returns `None` when the field is absent or is neither
/// tagged shape — a MALFORMED candidate (our own DTO always carries one of the two), which
/// the caller surfaces rather than rendering as "no module required". Never fabricates.
fn render_module_hint(module: Option<&serde_json::Value>) -> Option<String> {
    let m = module?.as_object()?;
    if let Some(path) = m.get("owning").and_then(|v| v.as_str()) {
        return Some(format!(", module {path}"));
    }
    if let Some(reason) = m.get("unavailable").and_then(|v| v.as_str()) {
        return Some(format!(", module: {reason}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::find::test_fixtures::{empty_facts, well_formed_candidate};
    use serde_json::json;

    #[test]
    fn well_formed_candidate_renders_with_validated_label() {
        let mut out = String::new();
        let result = json!({
            "seeds_available": true,
            "candidates": [well_formed_candidate(json!("embedding"))],
        });
        render_seed_tier(&result, &mut out);
        assert!(
            out.contains("score 0.71, embedding, model nomic-embed-text-v1.5"),
            "candidate label: {out}"
        );
        assert!(out.contains(", module backend/auth"), "module hint: {out}");
    }

    #[test]
    fn non_embedding_seed_source_is_malformed_never_relabeled() {
        let _ = empty_facts(); // fixture parity with the other tiers' tests.
        let mut out = String::new();
        let result = json!({
            "seeds_available": true,
            "candidates": [well_formed_candidate(json!("lexical"))],
        });
        render_seed_tier(&result, &mut out);
        assert!(
            out.contains("malformed candidate"),
            "surfaced malformed: {out}"
        );
        assert!(out.contains("\"lexical\""), "names offending source: {out}");
        assert!(!out.contains("0.71, embedding"), "not relabeled: {out}");
    }

    #[test]
    fn seeds_available_missing_is_malformed_never_defaulted() {
        let mut out = String::new();
        render_seed_tier(&json!({"candidates": []}), &mut out);
        assert!(
            out.contains("malformed find response: seeds_available missing or not a bool"),
            "{out}"
        );
    }

    #[test]
    fn unavailable_without_reason_is_malformed() {
        let mut out = String::new();
        render_seed_tier(&json!({"seeds_available": false}), &mut out);
        assert!(
            out.contains("malformed find response: seeds unavailable but no reason given"),
            "{out}"
        );
    }
}
