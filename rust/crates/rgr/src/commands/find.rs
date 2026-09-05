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
mod text_render;

#[cfg(test)]
mod test_fixtures;

fn print_find_usage() {
    eprintln!("usage: rmap find \"<query>\" [--exact] [--full] [--json]");
    eprintln!("       rmap find --text \"<pattern>\" [-F] [--full] [--json]");
    eprintln!("  Default: searches the indexed fact tables FIRST (deterministic), each hit");
    eprintln!("  labeled with its fact class and the command that renders it; then demoted");
    eprintln!("  semantic (embedding) guesses below. --exact = facts only, endpoint never");
    eprintln!("  consulted.");
    eprintln!("  --text: a LIVE regex scan of the working tree (comments, expressions, and");
    eprintln!("  library calls the fact tables do not index), each hit annotated with its");
    eprintln!("  enclosing stored symbol; -F matches the pattern as a fixed string.");
}

pub fn run_find(args: &[String]) -> ExitCode {
    let mut query: Option<String> = None;
    let mut json_mode = false;
    let mut exact = false;
    let mut full = false;
    // FIND-GREP-1: `--text` selects the live working-tree scan; `-F`/`--fixed` makes the
    // pattern a literal (only meaningful with `--text`).
    let mut text = false;
    let mut fixed = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--json" => json_mode = true,
            "--exact" => exact = true,
            "--full" => full = true,
            "--text" => text = true,
            "-F" | "--fixed" => fixed = true,
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

    // FIND-GREP-1 flag-combination guards. `-F` and `--exact` are text/facts-mode
    // qualifiers respectively; using one against the wrong mode is a user error we
    // reject loudly rather than silently ignore.
    if fixed && !text {
        eprintln!("error: -F/--fixed only applies with --text");
        print_find_usage();
        return ExitCode::from(1);
    }
    if text && exact {
        eprintln!("error: --exact (facts-only) and --text (live scan) are mutually exclusive");
        print_find_usage();
        return ExitCode::from(1);
    }

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
        "text": text,
        "fixed": fixed,
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
            } else if text {
                // FIND-GREP-1: the live-scan response has its own shape and renderer.
                // Exit code follows the classic `find` convention (STATED in the slice
                // report): a valid response is SUCCESS whether or not it matched — `find`
                // is a discovery verb, not a match/no-match gate.
                print!("{}", text_render::render_text_scan(&result));
                ExitCode::SUCCESS
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

/// FIND-GREP-1 (§2.4) — whether the FACTS tier established a MISS, the sole gate on the
/// seed tier's capability close ("nothing matched; try `find --text`"). Produced by
/// `facts_render::render_facts_tier` (the one traversal that already validates every
/// group — single source of truth, no second classifier to drift from it) and consumed
/// by `seed_render::render_seed_tier`.
///
/// Abstraction one-liner — enum `FactTierOutcome`; users: producer
/// `facts_render::render_facts_tier`, consumer `seed_render::render_seed_tier`; axis:
/// §2.4's honesty condition — the capability close is a claim about the repo that is only
/// TRUE on an established fact miss; rejected simpler alternative: a bare `bool` param —
/// an enum forces the seed tier to match BOTH arms, so the honesty-critical default
/// (never say "nothing matched" unless a miss was proven) cannot be reintroduced by a
/// stray `unwrap_or(false)`, and a hit or a malformed payload can never silently enable
/// the close.
#[derive(Clone, Copy)]
pub(super) enum FactTierOutcome {
    /// The facts payload was well-formed, envelope-complete, and EVERY class matched
    /// nothing — an HONESTLY-ESTABLISHED fact-table miss. The ONLY state in which the
    /// seed tier may render the §2.4 capability close.
    EstablishedMiss,
    /// EITHER a fact class matched, OR the payload was malformed / a class read was
    /// unavailable / the envelope was incomplete, so NO miss can be honestly established.
    /// The capability "nothing matched" close is WITHHELD: a match makes it false, and a
    /// malformed-or-uncertain payload must never be treated as an empty result
    /// (review-1; STANDING HONESTY RULE 1 — malformed/unknown ≠ absent).
    MissNotEstablished,
}

/// ECONOMY-2 (§2.1, ruling economy_2_cursor_metric): the ONE pattern-header line (with its
/// trailing newline) that replaces every in-root row's per-row `→ rmap explain …` cursor
/// line. It states the composition pattern once — the reader reassembles the runnable short
/// cursor from each row's own visible `path` / `qualified_name` / `[KIND]`, and the daemon's
/// syntax-gated `explain` reattach alias (keyed on `:SYMBOL`) resolves it. This is how the
/// LITERAL ≤15%-of-bytes-on-cursor-lines target is met BY DESIGN. The SINGLE source of both
/// the printed text and (via `len()`) the amortization gate — so the two never drift. The
/// repo uid rides the alias, not this line, so no uid is restated per output.
fn pattern_header_line() -> &'static str {
    "→ explain any row below: rmap explain '<path>#<qualified_name>:SYMBOL:<KIND>' \
     (compose it from the row's path, name, and [KIND])\n"
}

/// ECONOMY-2 (§2.1): the cursor-diet eligibility of a `find` response — either an EXACT
/// count of the CURSOR-COMPOSABLE rows (fact symbol rows + seed rows whose own path +
/// qualified_name reassemble the runnable short cursor, so the pattern header can cover them
/// and their per-row cursor line is dropped), or that the payload shape is MALFORMED and so
/// cannot be classified at all. Computed with the SAME `composable_cursor_kind` the renderers
/// use, so the header-on/off count and the per-row drop decision cannot drift.
enum DietEligibility {
    /// `facts`/`candidates` are well-formed: the exact number of composable rows the diet
    /// would strip of their per-row cursor line.
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
            // `key` is OPTIONAL (only the argument classes carry one); ABSENT → not a
            // dietable row. PRESENT but not a string → malformed: a corrupt key must not
            // silently pass as "0". A composable row is one whose Known path + display
            // reassemble the runnable short cursor — exactly the rows whose cursor line the
            // pattern header replaces (`composable_cursor_kind`, the renderer's own gate).
            let k = match h.get("key") {
                None => continue,
                Some(serde_json::Value::String(k)) => k.as_str(),
                Some(_) => return DietEligibility::Malformed,
            };
            if let (Some(p), Some(d)) = (
                h.get("path").and_then(|v| v.as_str()),
                h.get("display").and_then(|v| v.as_str()),
            ) {
                if crate::presentation::seed::composable_cursor_kind(Some(uid), k, p, d).is_some() {
                    count += 1;
                }
            }
        }
    }
    // ECONOMY-2 (§1): the SAME header amortizes the seed tier too, so seed-only results
    // (the common seed-bearing `find` — no fact symbol hit) can still shorten cursors. Count
    // the seed candidates the seed tier WILL RENDER (above the similarity floor, carrying
    // this repo's `<uid>:` prefix) — the exact rows whose `<uid>:` restatement the header
    // removes. `candidates` is ABSENT in `--exact` mode / when seeds are unavailable (0
    // dietable seed rows); a present-but-non-array `candidates`, or a candidate whose
    // `stable_key` is present-but-not-a-string, is a shape we cannot count → withhold the
    // diet (STANDING HONESTY RULE 1 — never guess around an unreadable shape). A sub-floor or
    // missing/invalid `score` candidate is NOT counted: the renderer drops it (sub-floor) or
    // surfaces it unreadable, so it contributes no dietable cursor.
    match result.get("candidates") {
        None => {}
        Some(serde_json::Value::Array(cands)) => {
            for c in cands {
                let above_floor = matches!(
                    c.get("score").and_then(|v| v.as_f64()),
                    Some(s) if s >= seed_render::SEED_SIMILARITY_FLOOR
                );
                if !above_floor {
                    continue;
                }
                let k = match c.get("stable_key") {
                    None => continue,
                    Some(serde_json::Value::String(k)) => k.as_str(),
                    Some(_) => return DietEligibility::Malformed,
                };
                if let (Some(p), Some(q)) = (
                    c.get("path").and_then(|v| v.as_str()),
                    c.get("qualified_name").and_then(|v| v.as_str()),
                ) {
                    if crate::presentation::seed::composable_cursor_kind(Some(uid), k, p, q)
                        .is_some()
                    {
                        count += 1;
                    }
                }
            }
        }
        Some(_) => return DietEligibility::Malformed,
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

    // ECONOMY-2 (§2.1) — cursor diet: when composable rows exist, the ONE pattern header
    // is printed here and every composable row below drops its per-row `→ rmap explain …`
    // line (the header + the row's own `path`/`qualified_name`/`[KIND]` reassemble the
    // runnable cursor, which the daemon's additive alias resolves). `repo_uid` is our OWN
    // DTO field, always present; an empty value is the no-snapshot degraded state (nothing
    // to anchor the composability strip to) — the header is then omitted and every cursor
    // renders in full. A genuinely-absent field (old daemon) degrades the same way.
    let repo_uid = result
        .get("repo_uid")
        .and_then(|v| v.as_str())
        .filter(|u| !u.is_empty());
    // ECONOMY-2 (§2.1, ruling economy_2_cursor_metric): the ONE pattern header replaces each
    // composable row's WHOLE per-row cursor line. §2.1 permits an explicit per-row cursor
    // ONLY where the row CANNOT compose one — so whenever ANY row is composable (≥1) the
    // header prints and every composable row drops its per-row `→ rmap explain …` line. The
    // header is a single fixed-cost line; the contract (never emit a redundant per-row cursor
    // a reader would treat as the only runnable handle) governs over a per-output byte
    // count — a lone composable row would otherwise keep a full uid-restating cursor in
    // breach of §2.1 (review-0 finding 1). A `Countable(0)` result has no composable row to
    // amortize, so no header and every (non-composable) cursor stays full. A MALFORMED
    // payload (review-1 honesty fix) withholds the diet the same way — full cursors, no
    // header — but is a DISTINCT classification, never conflated with "0 composable rows":
    // the corrupt payload is surfaced honestly by `render_facts_tier` below, never papered
    // over.
    let diet_uid = repo_uid.filter(|uid| match diet_eligibility(result, uid) {
        DietEligibility::Countable(composable) => composable >= 1,
        // Unclassifiable payload → withhold the diet (full cursors); the malformed shape
        // is rendered as an honest degradation by the facts renderer that follows.
        DietEligibility::Malformed => false,
    });
    if diet_uid.is_some() {
        out.push_str(pattern_header_line());
    }
    out.push('\n');

    // Pass `diet_uid`, not `repo_uid`: when the diet is withheld, `None` makes every
    // cursor render in its full self-contained form (matching the omitted header).
    // The facts tier's own single traversal REPORTS whether it established a miss — the
    // seed tier's §2.4 capability close is gated on it (review-1: no false "nothing
    // matched" when facts hit or the payload is malformed).
    let fact_outcome = facts_render::render_facts_tier(result, diet_uid, &mut out);

    if !exact {
        out.push('\n');
        // ECONOMY-2 (§2.1): the seed tier shares the ONE pattern header. When the diet is
        // applied, composable in-root seed rows drop their whole per-row cursor line (the
        // header covers them, `[KIND]` shown inline); when withheld (`None`) every seed
        // cursor stays full — matching the header's absence.
        seed_render::render_seed_tier(result, fact_outcome, diet_uid, &mut out);
    }
    out
}

#[cfg(test)]
mod tests;
