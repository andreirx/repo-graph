//! Shared `#[cfg(test)]` fixtures for the `find` renderer tests, so the entry
//! orchestrator and both tier modules build identical DTO shapes from ONE source
//! (no per-file drift). Compiled only under test (`#[cfg(test)] mod test_fixtures`).

use serde_json::{json, Value};

/// All seven fact classes present, none with hits — the honest searched set. Each
/// group carries its certainty tag (extracted/inferred/hint/governance), always
/// present. The `boundary` governance-declaration class carries NO `render_command`
/// (its renderer varies per hit — violations|gate; review-6 re-home).
pub(crate) fn empty_facts() -> Value {
    json!([
        {"fact_class": "symbol", "render_command": "explain", "certainty": "extracted", "hits": [], "matched": 0, "matched_is_floor": false},
        {"fact_class": "file", "render_command": "explain", "certainty": "extracted", "hits": [], "matched": 0, "matched_is_floor": false},
        {"fact_class": "module", "render_command": "map --dry-run", "certainty": "inferred", "hits": [], "matched": 0, "matched_is_floor": false},
        {"fact_class": "http-surface", "render_command": "boundaries list", "certainty": "inferred", "hits": [], "matched": 0, "matched_is_floor": false},
        {"fact_class": "dependency", "render_command": "deps list", "certainty": "extracted", "hits": [], "matched": 0, "matched_is_floor": false},
        {"fact_class": "framework", "render_command": "inferences list", "certainty": "hint", "hits": [], "matched": 0, "matched_is_floor": false},
        {"fact_class": "boundary", "certainty": "governance", "hits": [], "matched": 0, "matched_is_floor": false}
    ])
}

/// One well-formed seed candidate; `source` is passed in so a test can inject a
/// non-`embedding` source to exercise the malformed-source rejection.
pub(crate) fn well_formed_candidate(source: Value) -> Value {
    let mut c = serde_json::Map::new();
    c.insert("path".to_string(), json!("src/auth.ts"));
    c.insert("stable_key".to_string(), json!("glamCRM:auth.ts:FILE"));
    c.insert("score".to_string(), json!(0.71));
    c.insert("model_id".to_string(), json!("nomic-embed-text-v1.5"));
    c.insert("module".to_string(), json!({"owning": "backend/auth"}));
    c.insert("next".to_string(), json!({"cwd": "/repo"}));
    if !source.is_null() {
        c.insert("source".to_string(), source);
    }
    Value::Object(c)
}

/// A well-formed `embedding` seed candidate at a caller-chosen `score` and
/// `stable_key`, for the FIND-RANK-1 §2.3 similarity-floor tests. All other identity
/// fields are valid so the ONLY variable under test is the score relative to the floor.
pub(crate) fn candidate_with_score(stable_key: &str, score: f64) -> Value {
    json!({
        "path": "src/x.ts",
        "stable_key": stable_key,
        "score": score,
        "model_id": "nomic-embed-text-v1.5",
        "source": "embedding",
        "module": {"owning": "backend/x"},
        "next": {"cwd": "/repo"},
    })
}
