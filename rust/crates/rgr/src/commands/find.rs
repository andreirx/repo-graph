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

/// FIND-EVIDENCE-1 (§2.3): the once-per-output uid header line (with its trailing
/// newline). The SINGLE source of both the printed text AND its byte length used by the
/// amortization gate — so the two can never drift.
fn uid_header_line(uid: &str) -> String {
    format!("repo-uid {uid} (symbol cursors below are relative to it)\n")
}

/// FIND-EVIDENCE-1 (§2.3): the cursor-diet eligibility of a `find` response — either an
/// EXACT count of the symbol rows the diet would shorten, or that the `facts` payload
/// shape is MALFORMED and so cannot be classified at all. Only SYMBOL hits carry a
/// `<uid>:`-prefixed `key` (file/module keys are paths; dependency/framework/boundary keys
/// are names/none), and the symbol class renders via `explain` — so a uid-prefixed key is
/// EXACTLY the set `fact_hit::relative_cursor` shortens.
enum DietEligibility {
    /// `facts` is a well-formed array of well-formed groups: the exact number of
    /// uid-prefixed symbol rows the diet would shorten.
    Countable(usize),
    /// `facts` is absent / not an array, or a group's `hits` is present-but-not-a-list, or
    /// a hit's `key` is present-but-not-a-string. The count cannot be TRUSTED, so the diet
    /// is WITHHELD (full self-contained cursors) — never silently classified as "0 dietable
    /// rows" (STANDING HONESTY RULE 1: a fallible read whose result classifies rendered
    /// output is never papered over with a default). The malformed shape is itself surfaced
    /// in the rendered output by `facts_render::render_facts_tier`, which validates the same
    /// payload right after and emits the honest `(malformed …)` degradation line.
    Malformed,
}

/// Classify the response's cursor-diet eligibility (review-1 fix): an EXPLICIT, checked
/// traversal of `facts` — no `.flatten()`, no `.filter_map()` that would DISCARD a malformed
/// group, no `.unwrap_or(0)` collapsing an unreadable shape to a fabricated zero. Any
/// deviation from the ratified array-of-groups-of-hits shape short-circuits to `Malformed`
/// so a partially-corrupt payload withholds the diet entirely (full cursors for every row)
/// rather than applying a byte optimization computed off data we could not fully read.
fn diet_eligibility(result: &serde_json::Value, uid: &str) -> DietEligibility {
    let prefix = format!("{uid}:");
    // `facts` is our OWN DTO field, ALWAYS serialized as the per-class group array. Any
    // other shape (absent / non-array) is malformed — we cannot count, so we do not guess.
    let groups = match result.get("facts") {
        Some(serde_json::Value::Array(groups)) => groups,
        _ => return DietEligibility::Malformed,
    };
    let mut count = 0usize;
    for g in groups {
        // A group may LEGITIMATELY carry no `hits` (an error/unavailable group renders no
        // rows — absent key is normal). A `hits` PRESENT but not a list is a malformed
        // shape we refuse to count around.
        let hits = match g.get("hits") {
            None => continue,
            Some(serde_json::Value::Array(hits)) => hits,
            Some(_) => return DietEligibility::Malformed,
        };
        for h in hits {
            // `key` is OPTIONAL (only the uid-prefixed argument classes carry one); ABSENT
            // → not a dietable row. PRESENT but not a string → malformed: the key drives
            // the diet classification, so a corrupt key must not silently pass as "0".
            match h.get("key") {
                None => {}
                Some(serde_json::Value::String(k)) => {
                    if k.starts_with(&prefix) {
                        count += 1;
                    }
                }
                Some(_) => return DietEligibility::Malformed,
            }
        }
    }
    DietEligibility::Countable(count)
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
        Some(q) => out.push_str(&format!("find \"{q}\"\n")),
        None => out.push_str("find (malformed find response: query missing or not a string)\n"),
    }

    // FIND-EVIDENCE-1 (§2.3) — cursor diet: the repo uid is printed ONCE here so the
    // per-row symbol cursors below can drop it (`explain <suffix>`, which the daemon's
    // additive alias resolves). `repo_uid` is our OWN DTO field, always present; an
    // empty value is the no-snapshot degraded state (nothing to anchor) — the header
    // line is then omitted and cursors render in full (never a fabricated uid). A
    // genuinely-absent field (old daemon) degrades the same way: no header, full cursors.
    let repo_uid = result
        .get("repo_uid")
        .and_then(|v| v.as_str())
        .filter(|u| !u.is_empty());
    // §2.5 boilerplate economy: the header amortizes only when the once-per-output uid it
    // adds costs FEWER bytes than the per-row `<uid>:` restatement it removes. On a 0- or
    // 1-symbol-row result the header cannot pay for itself (measured: the single-row
    // `witness_epoch` probe would GROW boilerplate ~+51 B), so the diet is WITHHELD there:
    // no header, full self-contained cursors — boilerplate then never grows. When applied
    // (break-even ≈ 3 symbol rows for a 26-char uid) it strictly shrinks it. This is a
    // BYTE-ECONOMY heuristic, not a fact. A MALFORMED `facts` shape (review-1 honesty fix)
    // withholds the diet the same way — full cursors, no header — but is a DISTINCT
    // classification, never conflated with "0 dietable rows": the corrupt payload is
    // surfaced honestly by `render_facts_tier` below, never papered over.
    let diet_uid = repo_uid.filter(|uid| match diet_eligibility(result, uid) {
        DietEligibility::Countable(rows) => {
            let header_bytes = uid_header_line(uid).len();
            let per_row_saving = uid.len() + 1; // "<uid>:" dropped from each relative cursor
            rows * per_row_saving > header_bytes
        }
        // Unclassifiable payload → withhold the diet (full cursors); the malformed shape
        // is rendered as an honest degradation by the facts renderer that follows.
        DietEligibility::Malformed => false,
    });
    if let Some(uid) = diet_uid {
        out.push_str(&uid_header_line(uid));
    }
    out.push('\n');

    // Pass `diet_uid`, not `repo_uid`: when the diet is withheld, `None` makes every
    // cursor render in its full self-contained form (matching the omitted header).
    facts_render::render_facts_tier(result, diet_uid, &mut out);

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
    fn cursor_diet_applies_and_header_prints_when_it_amortizes() {
        // ≥3 uid-prefixed symbol rows: the once-per-output header pays for itself, so it
        // prints AND the per-row cursors drop the uid (`explain <suffix>`). Keys carry `#`
        // (path#name), so the daemon single-quotes the cursor arg — mirrored here.
        let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
        let hit = |k: &str| {
            json!({"display": "sym", "path": "src/x.ts", "key": format!("{uid}:{k}"),
                   "next": format!("explain '{uid}:{k}'")})
        };
        let mut facts = empty_facts();
        facts[0] = json!({"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
            "hits": [hit("a#a:SYMBOL:FUNCTION"), hit("b#b:SYMBOL:FUNCTION"), hit("c#c:SYMBOL:FUNCTION")],
            "matched": 3, "matched_is_floor": false});
        let result = json!({
            "query": "sym", "repo_uid": uid, "facts": facts,
            "seeds_available": true, "candidates": [],
        });
        let out = render_find_human(&result, true);
        assert!(
            out.contains("repo-uid repo_01m1kvv00zgrtr3t23xrfe6veg (symbol cursors below are relative to it)"),
            "header prints when the diet amortizes: {out}"
        );
        assert!(
            out.contains("→ rmap explain 'a#a:SYMBOL:FUNCTION'\n"),
            "cursor is relative (uid dropped): {out}"
        );
        assert!(
            !out.contains("explain 'repo_01m1kvv00zgrtr3t23xrfe6veg:"),
            "the uid is not restated per row: {out}"
        );
    }

    #[test]
    fn cursor_diet_is_withheld_on_a_single_row_so_boilerplate_never_grows() {
        // 1 uid-prefixed symbol row: the header would cost more than the single uid it
        // saves (§2.5), so the diet is WITHHELD — NO header, and the cursor stays FULL and
        // self-contained (still runnable without a header to anchor to).
        let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
        let key = format!("{uid}:src/x.rs#witness_epoch:SYMBOL:FUNCTION");
        let mut facts = empty_facts();
        facts[0] = json!({"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
            "hits": [{"display": "witness_epoch", "path": "src/x.rs", "key": key,
                      "next": format!("explain '{key}'")}],
            "matched": 1, "matched_is_floor": false});
        let result = json!({
            "query": "witness_epoch", "repo_uid": uid, "facts": facts,
            "seeds_available": true, "candidates": [],
        });
        let out = render_find_human(&result, true);
        assert!(
            !out.contains("repo-uid "),
            "no header when the diet cannot amortize: {out}"
        );
        assert!(
            out.contains("→ rmap explain 'repo_01m1kvv00zgrtr3t23xrfe6veg:src/x.rs#witness_epoch:SYMBOL:FUNCTION'\n"),
            "the single-row cursor stays full and self-contained: {out}"
        );
    }

    #[test]
    fn malformed_top_level_facts_withholds_diet_never_fabricates_header_or_cursor() {
        // review-1 regression: a `facts` payload that is NOT the ratified array (here an
        // object) cannot be classified for the cursor diet. The diet is WITHHELD — NO
        // header line, NO relative cursor selected — and the malformed shape is surfaced
        // honestly by the facts renderer, never silently classified as "0 dietable rows".
        let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
        let result = json!({
            "query": "sym", "repo_uid": uid,
            "facts": {"not": "an array"},
            "seeds_available": true, "candidates": [],
        });
        let out = render_find_human(&result, true);
        assert!(
            !out.contains("repo-uid "),
            "no diet header on malformed facts: {out}"
        );
        assert!(
            out.contains("malformed find response: facts missing or not a list"),
            "malformed facts surfaced honestly: {out}"
        );
    }

    #[test]
    fn malformed_group_hits_withholds_diet_keeps_full_self_contained_cursors() {
        // review-1 regression: three uid-prefixed symbol rows WOULD amortize the header,
        // but a second group's `hits` is a non-list. The old classifier silently discarded
        // that group (filter_map) and applied the diet off the survivors; the checked
        // traversal short-circuits to Malformed, so the diet is WITHHELD ENTIRELY — every
        // symbol cursor stays FULL and self-contained (uid restated, still runnable), no
        // header, and the corrupt group is surfaced by the renderer.
        let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
        let hit = |k: &str| {
            json!({"display": "sym", "path": "src/x.ts", "key": format!("{uid}:{k}"),
                   "next": format!("explain '{uid}:{k}'")})
        };
        let mut facts = empty_facts();
        facts[0] = json!({"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
            "hits": [hit("a#a:SYMBOL:FUNCTION"), hit("b#b:SYMBOL:FUNCTION"), hit("c#c:SYMBOL:FUNCTION")],
            "matched": 3, "matched_is_floor": false});
        // A malformed second group: `hits` present but not a list.
        facts[1] = json!({"fact_class": "file", "render_command": "explain", "certainty": "extracted",
            "hits": {"not": "a list"}, "matched": 0, "matched_is_floor": false});
        let result = json!({
            "query": "sym", "repo_uid": uid, "facts": facts,
            "seeds_available": true, "candidates": [],
        });
        let out = render_find_human(&result, true);
        assert!(
            !out.contains("repo-uid "),
            "diet withheld — no header — on a malformed group: {out}"
        );
        assert!(
            out.contains("→ rmap explain 'repo_01m1kvv00zgrtr3t23xrfe6veg:a#a:SYMBOL:FUNCTION'\n"),
            "symbol cursor stays full and self-contained (no relative cursor fabricated): {out}"
        );
        assert!(
            out.contains("malformed fact group: hits missing or not a list"),
            "malformed group surfaced honestly: {out}"
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
