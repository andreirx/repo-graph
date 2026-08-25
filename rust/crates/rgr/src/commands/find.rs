//! `rmap find "<concept>"` — the affirmative concept-search verb (EMBED-SEED-IMPL-1,
//! spec §8B). Extracted from `commands/orient.rs` (operator ruling 2026-08-25 /
//! review-1 #5: `orient.rs` was over the 500-line guardrail and must not grow).
//!
//! One positional concept string; resolves the repo from cwd; consults the semantic
//! store via the daemon; prints ≤10 labeled Layer-3 candidates under an honesty
//! header (or a labeled empty result when the substrate is unavailable). Mirrors
//! `run_orient`'s request/emit shape. Read-only.

use std::process::ExitCode;

use crate::daemon_client::{DaemonClient, DaemonClientError};

fn print_find_usage() {
    eprintln!("usage: rmap find \"<concept>\" [--json]");
    eprintln!("  Semantic (embedding) concept search — Layer-3 hints, not resolved facts.");
    eprintln!("  Prints likely files to open; open one and re-run explain/callers on it.");
}

pub fn run_find(args: &[String]) -> ExitCode {
    let mut query: Option<String> = None;
    let mut json_mode = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--json" => json_mode = true,
            "--help" | "-h" => {
                print_find_usage();
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                print_find_usage();
                return ExitCode::from(1);
            }
            _ => {
                if query.is_some() {
                    eprintln!("error: unexpected argument: {}", arg);
                    print_find_usage();
                    return ExitCode::from(1);
                }
                query = Some(arg.clone());
            }
        }
        i += 1;
    }

    let query = match query {
        Some(q) => q,
        None => {
            eprintln!("error: missing concept argument");
            print_find_usage();
            return ExitCode::from(1);
        }
    };

    // Resolve repo from cwd (same convention as orient/explain).
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot get current directory: {}", e);
            return ExitCode::from(2);
        }
    };
    let repo_path = match cwd.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot canonicalize current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let params = serde_json::json!({
        "repo": repo_path,
        "query": query,
    });

    match client.request("find", Some(params)) {
        Ok(result) => {
            if json_mode {
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                print!("{}", render_find_human(&result));
                ExitCode::SUCCESS
            }
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo");
            } else {
                eprintln!("error: {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// Render the `find` response (its own DTO — §8B.2) for human mode: the
/// always-present honesty `summary` line, then each candidate's path + score +
/// model + `next` follow-up. An empty `candidates: []` prints just the summary
/// (the honest "no hints"/"declined" line), never a fabricated zero.
///
/// STANDING HONESTY RULE: a candidate is our own DTO, so every field is expected
/// present. A genuinely-absent required field means a MALFORMED response (an old
/// daemon, a serialization bug) — it is surfaced as such, NEVER papered over with a
/// fabricated default score/path.
fn render_find_human(result: &serde_json::Value) -> String {
    let mut out = String::new();
    let summary = result
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("(malformed find response: no summary line)");
    out.push_str(summary);
    out.push('\n');
    // `candidates` is our OWN DTO field (`FindResponse.candidates: Vec<_>`), ALWAYS
    // serialized — `[]` when empty, never omitted. A MISSING key (or a non-array)
    // is therefore a MALFORMED response (old daemon / serialization bug), NOT a
    // genuine zero-candidate result — surface it as such rather than silently
    // rendering the honest "no hints" shape (STANDING HONESTY RULE — review-4 #1).
    let candidates = match result.get("candidates") {
        Some(serde_json::Value::Array(a)) => a,
        _ => {
            out.push_str("  (malformed find response: candidates missing or not a list)\n");
            return out;
        }
    };
    if candidates.is_empty() {
        return out;
    }
    out.push('\n');
    for c in candidates {
        // The identity fields below are our OWN DTO (`FindCandidate`), always
        // present. A genuinely-absent REQUIRED field means a MALFORMED response (old
        // daemon / serialization bug) — surface it, NEVER fabricate a path, score,
        // model identity, or provenance label (review-2 #3: no fake "?" model;
        // review-11: no hardcoded "embedding" pasted over the payload's `source`).
        let path = c.get("path").and_then(|v| v.as_str());
        let key = c.get("stable_key").and_then(|v| v.as_str());
        let score = c.get("score").and_then(|v| v.as_f64());
        let model = c.get("model_id").and_then(|v| v.as_str());
        // STANDING HONESTY RULE (review-11, mirrors presentation/seed.rs review-10 #4):
        // the provenance label is the daemon's OWN `source`, VALIDATED — never a literal
        // "embedding" printed regardless of the payload. `FindCandidate.source` is a
        // required field always == "embedding" by construction; a foreign / old-daemon /
        // serialization-bug payload whose `source` is absent or not "embedding" is
        // surfaced as malformed, never relabeled as a Layer-3 embedding hint.
        let source = c.get("source").and_then(|v| v.as_str());
        let (Some(path), Some(key), Some(score), Some(model), Some(source)) =
            (path, key, score, model, source)
        else {
            out.push_str(
                "  (malformed candidate: missing required field — path/stable_key/score/model_id/source)\n",
            );
            continue;
        };
        if source != "embedding" {
            out.push_str(&format!(
                "  (malformed candidate: source {source:?} is not a Layer-3 embedding hint)\n"
            ));
            continue;
        }
        // `module` is a REQUIRED field of our own `FindCandidate` DTO — every candidate
        // carries a self-labeling `ModuleHint` (a genuine `{owning}` OR an explicit
        // `{unavailable: <reason>}`; operator ruling 2026-08-25). A MISSING `module`,
        // or one that is neither shape, is therefore a MALFORMED response — surfaced as
        // such, NEVER rendered as though no module information were required (review-6 #3
        // / STANDING HONESTY RULE).
        let Some(module) = render_module_hint(c.get("module")) else {
            out.push_str(
                "  (malformed candidate: missing or invalid module hint — expected owning/unavailable)\n",
            );
            continue;
        };
        // `source` is validated == "embedding"; render the daemon's own label, not a literal.
        out.push_str(&format!(
            "  {path}  (score {score:.2}, {source}, model {model}{module})\n"
        ));
        out.push_str(&render_next(c.get("next"), key));
    }
    out
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
    use serde_json::json;

    fn well_formed_candidate(source: serde_json::Value) -> serde_json::Value {
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
        serde_json::Value::Object(c)
    }

    #[test]
    fn well_formed_embedding_candidate_renders_the_validated_label() {
        let result = json!({
            "summary": "likely areas for \"auth\" (semantic hints — open the files)",
            "candidates": [well_formed_candidate(json!("embedding"))],
        });
        let out = render_find_human(&result);
        assert!(out.contains("src/auth.ts"), "renders the path: {out}");
        assert!(
            out.contains("score 0.71, embedding, model nomic-embed-text-v1.5"),
            "renders the validated embedding label: {out}"
        );
        assert!(
            out.contains("module backend/auth"),
            "renders the module: {out}"
        );
        assert!(
            out.contains("cd /repo && rmap explain glamCRM:auth.ts:FILE"),
            "renders the follow-up: {out}"
        );
    }

    #[test]
    fn non_embedding_source_is_malformed_never_relabeled_embedding() {
        // review-11: a fully-formed `find` candidate whose `source` is NOT "embedding"
        // (a foreign / old-daemon / serialization-bug payload) must NOT be presented as
        // an embedding hint. Every identity field is present; only the `source` guard rejects.
        let result = json!({
            "summary": "likely areas for \"auth\"",
            "candidates": [well_formed_candidate(json!("lexical"))],
        });
        let out = render_find_human(&result);
        assert!(
            out.contains("malformed candidate"),
            "surfaced as malformed: {out}"
        );
        assert!(
            out.contains("\"lexical\""),
            "names the offending source: {out}"
        );
        assert!(
            !out.contains("0.71, embedding"),
            "must NOT relabel a non-embedding source as an embedding hint: {out}"
        );
    }

    #[test]
    fn missing_source_is_malformed_never_fabricated_embedding() {
        // The daemon's `source` is a required `FindCandidate` field; a candidate arriving
        // without it (old daemon / bug) is malformed, never a fabricated "embedding" label.
        let result = json!({
            "summary": "likely areas for \"auth\"",
            "candidates": [well_formed_candidate(serde_json::Value::Null)],
        });
        let out = render_find_human(&result);
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
