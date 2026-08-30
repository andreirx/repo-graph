//! `rmap find "<query>"` — the concept/identifier search verb.
//!
//! FIND-FACTS-1: `find` answers FIRST from a deterministic lexical match over the
//! indexed FACT TABLES (symbols, files, modules, HTTP routes, dependencies,
//! framework inferences, and governance boundary/requirement/quality-policy
//! declarations — review-6 re-home), each hit labeled with its fact class and the
//! command that renders it. BELOW that it renders the DEMOTED
//! semantic-seed tier — embedding similarity, ranked guesses, never facts — which
//! degrades to unavailable-with-reason when the local model is down. The verb
//! answers without the embedding endpoint; `--exact` renders the facts tier alone
//! and never touches the endpoint.
//!
//! Read-only. Resolves the repo from cwd (same convention as orient/explain).
//!
//! STANDING HONESTY RULE: the response is our OWN DTO (`FindResponse`). Every
//! required field is expected present; a genuinely-absent required field means a
//! MALFORMED response (old daemon / serialization bug) and is surfaced as such,
//! NEVER papered over with a fabricated default.
//!
//! This file is the command ENTRY + tier ORCHESTRATOR only; the two tiers'
//! rendering lives in crate-private children — [`facts_render`] (the FACTS tier:
//! group envelope + hit rendering) and [`seed_render`] (the DEMOTED semantic-seed
//! tier). Split from the former 988-line single file per the ≤500-line guardrail
//! (review-4 item 1). Abstraction one-liners head each child.

use std::process::ExitCode;

use crate::daemon_client::{DaemonClient, DaemonClientError};

mod fact_hit;
mod facts_render;
mod seed_render;

#[cfg(test)]
mod test_fixtures;

fn print_find_usage() {
    eprintln!("usage: rmap find \"<query>\" [--exact] [--full] [--json]");
    eprintln!("  Searches the indexed fact tables FIRST (deterministic), each hit labeled");
    eprintln!("  with its fact class and the command that renders it; then demoted semantic");
    eprintln!("  (embedding) guesses below. --exact = facts only, endpoint never consulted.");
}

pub fn run_find(args: &[String]) -> ExitCode {
    let mut query: Option<String> = None;
    let mut json_mode = false;
    let mut exact = false;
    let mut full = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--json" => json_mode = true,
            "--exact" => exact = true,
            "--full" => full = true,
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
            eprintln!("error: missing query argument");
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
        "exact": exact,
        "full": full,
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
                print!("{}", render_find_human(&result, exact));
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

/// Render the `find` response (FIND-FACTS-1 §2): the FACTS tier first, then (unless
/// `exact`) the demoted semantic-seed tier. `exact` is the CLI's own flag — in exact
/// mode the seed section is omitted entirely (the endpoint was never consulted).
fn render_find_human(result: &serde_json::Value, exact: bool) -> String {
    let mut out = String::new();
    // `query` is our OWN DTO field, ALWAYS serialized (FindResponse.query: String).
    // A missing / non-string value is a MALFORMED response — surfaced as such, never
    // papered over with an empty-string echo (STANDING HONESTY RULE 1).
    match result.get("query").and_then(|v| v.as_str()) {
        Some(q) => out.push_str(&format!("find \"{q}\"\n\n")),
        None => out.push_str("find (malformed find response: query missing or not a string)\n\n"),
    }

    facts_render::render_facts_tier(result, &mut out);

    if !exact {
        out.push('\n');
        seed_render::render_seed_tier(result, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::find::test_fixtures::{empty_facts, well_formed_candidate};
    use serde_json::json;

    #[test]
    fn facts_render_above_seeds_with_class_and_command_labels() {
        let result = json!({
            "query": "bnr",
            "facts": [
                {"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
                 "hits": [{"display": "bnrService", "path": "src/bnr.ts", "key": "k1", "next": "explain k1"}],
                 "matched": 1, "matched_is_floor": false},
                {"fact_class": "http-surface", "render_command": "boundaries list", "certainty": "inferred",
                 "hits": [{"display": "provider GET /api/offers", "path": "src/offer.ts", "next": "boundaries list"}],
                 "matched": 1, "matched_is_floor": false},
                {"fact_class": "file", "render_command": "explain", "certainty": "extracted", "hits": [], "matched": 0, "matched_is_floor": false},
                {"fact_class": "module", "render_command": "map --dry-run", "certainty": "inferred", "hits": [], "matched": 0, "matched_is_floor": false},
                {"fact_class": "dependency", "render_command": "deps list", "certainty": "extracted", "hits": [], "matched": 0, "matched_is_floor": false},
                {"fact_class": "framework", "render_command": "inferences list", "certainty": "hint", "hits": [], "matched": 0, "matched_is_floor": false},
                {"fact_class": "boundary", "certainty": "governance", "hits": [], "matched": 0, "matched_is_floor": false}
            ],
            "seeds_available": true,
            "summary": "ranked guesses for \"bnr\" (embedding similarity — not facts)",
            "candidates": [well_formed_candidate(json!("embedding"))],
        });
        let out = render_find_human(&result, false);
        // Facts tier appears BEFORE the seed tier.
        let facts_pos = out
            .find("[symbol · extracted → rmap explain]")
            .expect("symbol label with certainty");
        let seed_pos = out.find("Semantic seeds").expect("seed header");
        assert!(facts_pos < seed_pos, "facts render above seeds:\n{out}");
        assert!(
            out.contains("bnrService  — src/bnr.ts"),
            "symbol hit: {out}"
        );
        // The runnable per-hit next command is rendered (review-1 item 1).
        assert!(
            out.contains("→ rmap explain k1"),
            "runnable per-hit next command: {out}"
        );
        assert!(
            out.contains("[http-surface · inferred → rmap boundaries list]"),
            "http label with certainty: {out}"
        );
        assert!(
            out.contains("provider GET /api/offers  — src/offer.ts"),
            "route hit: {out}"
        );
        // Seed candidate still rendered below, with its validated label.
        assert!(
            out.contains("score 0.71, embedding, model nomic-embed-text-v1.5"),
            "seed candidate below: {out}"
        );
    }

    #[test]
    fn endpoint_down_renders_facts_and_seed_unavailable_with_reason() {
        let mut facts = empty_facts();
        facts[0] = json!({
            "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
            "hits": [{"display": "bnrService", "path": "src/bnr.ts", "key": "k1", "next": "explain k1"}],
            "matched": 1, "matched_is_floor": false
        });
        let result = json!({
            "query": "bnr", "facts": facts,
            "seeds_available": false,
            "seeds_unavailable_reason": "no local embedding model reachable; seeding is optional, resolution is unaffected",
            "summary": "no local embedding model reachable — semantic hints unavailable (find is optional)",
            "candidates": [],
        });
        let out = render_find_human(&result, false);
        // Facts intact.
        assert!(
            out.contains("bnrService  — src/bnr.ts"),
            "facts intact: {out}"
        );
        // Seeds explicitly unavailable WITH reason.
        assert!(
            out.contains("semantic seeds unavailable (no local embedding model reachable"),
            "seed unavailable with reason: {out}"
        );
    }

    #[test]
    fn exact_mode_omits_seed_section_entirely() {
        let result = json!({
            "query": "bnr", "facts": empty_facts(),
            "seeds_available": false,
            "seeds_unavailable_reason": "not consulted (--exact — facts only)",
            "candidates": [],
        });
        let out = render_find_human(&result, true);
        assert!(
            !out.contains("Semantic seeds"),
            "no seed section in --exact: {out}"
        );
        assert!(
            out.contains("Facts (deterministic lexical match over the indexed tables"),
            "facts present: {out}"
        );
    }

    #[test]
    fn honest_empty_names_searched_classes() {
        let result = json!({
            "query": "zzz", "facts": empty_facts(),
            "seeds_available": true, "candidates": [],
        });
        let out = render_find_human(&result, false);
        assert!(
            out.contains(
                "no matches: symbol, file, module, http-surface, dependency, framework, boundary"
            ),
            "honest empty names searched classes: {out}"
        );
        assert!(
            out.contains("(no area scored above zero)"),
            "seed empty stated: {out}"
        );
    }

    #[test]
    fn query_missing_is_malformed_never_empty_echo() {
        let result = json!({"facts": empty_facts(), "seeds_available": true, "candidates": []});
        let out = render_find_human(&result, false);
        assert!(
            out.contains("malformed find response: query missing or not a string"),
            "missing query surfaced: {out}"
        );
    }
}
