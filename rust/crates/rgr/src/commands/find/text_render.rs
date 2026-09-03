//! The `find --text` live-scan renderer (FIND-GREP-1 §2). Consumes the
//! `TextScanResponse` of our OWN DTO ACROSS the daemon boundary and renders it grouped
//! by file. Each hit is a self-describing `path:line: text` line + the enclosing-symbol
//! annotation (review-4: `path:line` per match so every hit is independently
//! actionable), with a once-per-file staleness note and a count header + cap disclosure.
//!
//! Like the facts/seed renderers it does NOT trust the payload: every required field is
//! re-validated here; a missing/mistyped required field is surfaced as MALFORMED, never
//! rendered as a fabricated default (STANDING HONESTY RULE 1). In particular a hit with
//! no `annotation` renders WITHOUT one (visible absence — the hit was outside every
//! stored span), never a guessed symbol.
//!
//! Abstraction record — module: `find::text_render`; concrete current user:
//! `find::run_find` (the `--text` branch); axis: the ≤500-line guardrail — the live-scan
//! rendering is its own responsibility rather than growing the already-oversized
//! `find.rs`; rejected simpler alternative: inlining in `find.rs` (file stays >500).

/// Render the `find --text` response. On a FATAL `error` (bad pattern / walk failure)
/// it prints that and returns — never a false empty result.
pub(super) fn render_text_scan(result: &serde_json::Value) -> String {
    let mut out = String::new();

    // `query` / `fixed` are our OWN DTO fields, always serialized. A missing/mistyped
    // one is MALFORMED (old daemon / serialization bug), surfaced — never a fabricated echo.
    let query = match result.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => {
            out.push_str("find --text (malformed response: query missing or not a string)\n");
            return out;
        }
    };
    let fixed = match result.get("fixed").and_then(|v| v.as_bool()) {
        Some(f) => f,
        None => {
            out.push_str("find --text (malformed response: fixed missing or not a bool)\n");
            return out;
        }
    };
    let mode = if fixed { " (-F fixed string)" } else { "" };
    out.push_str(&format!("find --text \"{query}\"{mode}\n"));

    // Scope note (§ disclosed scope bound): always present.
    match result.get("scope_note").and_then(|v| v.as_str()) {
        Some(s) => out.push_str(&format!("{s}\n")),
        None => out.push_str("(malformed response: scope_note missing or not a string)\n"),
    }

    // A FATAL error means no scan happened — render it and stop.
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        out.push_str(&format!("scan failed: {err}\n"));
        return out;
    }

    // Snapshot identity (§2.3 header states the snapshot the spans came from). Empty =
    // not indexed; the `context_unavailable` line below then carries the reason.
    match result.get("snapshot").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => out.push_str(&format!("snapshot {s}\n")),
        Some(_) => {}
        None => out.push_str("(malformed response: snapshot missing or not a string)\n"),
    }
    // Symbol-context / staleness withheld with a reason (never a silent "all fresh").
    if let Some(reason) = result.get("context_unavailable").and_then(|v| v.as_str()) {
        out.push_str(&format!("note: {reason}\n"));
    }

    // ── Count header + cap disclosure (§2.5 volume honesty). All three are our OWN DTO
    //    fields, always serialized; a missing/mistyped one is MALFORMED, surfaced.
    let total = match u64_field(result, "total_matches") {
        Ok(n) => n,
        Err(msg) => {
            out.push_str(&msg);
            return out;
        }
    };
    let shown = match u64_field(result, "shown_matches") {
        Ok(n) => n,
        Err(msg) => {
            out.push_str(&msg);
            return out;
        }
    };
    let capped = match result.get("capped").and_then(|v| v.as_bool()) {
        Some(b) => b,
        None => {
            out.push_str("(malformed response: capped missing or not a bool)\n");
            return out;
        }
    };
    // Skip accounting (review-0 finding (b)): our OWN DTO fields, always serialized. A
    // scan that omitted files (unreadable / search-errored) may NOT claim `total_matches`
    // exact — the count header below states it as a lower bound and names the reason.
    let skipped_unreadable = match u64_field(result, "skipped_unreadable") {
        Ok(n) => n,
        Err(msg) => {
            out.push_str(&msg);
            return out;
        }
    };
    let skipped_search_error = match u64_field(result, "skipped_search_error") {
        Ok(n) => n,
        Err(msg) => {
            out.push_str(&msg);
            return out;
        }
    };
    // review-0 finding 2: walk-enumeration failures (unreadable directories) are an
    // omission class too — the earlier build could not surface them because the reused
    // scanner aborted on walk errors. Counted here so an incomplete walk also downgrades
    // the total to a lower bound.
    let skipped_walk_error = match u64_field(result, "skipped_walk_error") {
        Ok(n) => n,
        Err(msg) => {
            out.push_str(&msg);
            return out;
        }
    };
    let scan_complete =
        skipped_unreadable == 0 && skipped_search_error == 0 && skipped_walk_error == 0;

    // `files` is our OWN DTO field, always an array (`[]` when nothing matched).
    let files = match result.get("files") {
        Some(serde_json::Value::Array(a)) => a,
        _ => {
            out.push_str("(malformed response: files missing or not a list)\n");
            return out;
        }
    };

    out.push('\n');

    // ── Count HEADER (review-0 finding (a)): the total is stated up front, BEFORE the
    //    groups, and ALWAYS — including the zero-result case. When the scan omitted files
    //    the total is a LOWER BOUND, never an exact claim (review-0 finding (b)).
    if scan_complete {
        out.push_str(&format!("{total} match(es)\n"));
    } else {
        out.push_str(&format!(
            "{total} match(es) so far — scan incomplete; total is a lower bound (see note)\n"
        ));
    }
    // Incomplete-scan note: the skipped-file count broken out by reason class.
    if !scan_complete {
        let mut classes = Vec::new();
        if skipped_unreadable > 0 {
            classes.push(format!("unreadable: {skipped_unreadable}"));
        }
        if skipped_search_error > 0 {
            classes.push(format!("search error: {skipped_search_error}"));
        }
        if skipped_walk_error > 0 {
            classes.push(format!("walk error: {skipped_walk_error}"));
        }
        let total_skipped = skipped_unreadable + skipped_search_error + skipped_walk_error;
        out.push_str(&format!(
            "⚠ incomplete scan: {total_skipped} file(s) skipped ({})\n",
            classes.join(", ")
        ));
    }

    if total == 0 {
        if scan_complete {
            // Honest empty for a COMPLETE live scan — a capability-truthful statement,
            // never a claim about the repo (the retired-sentence lesson applies here too).
            // The `0 match(es)` header above already carries the count.
            out.push_str(
                "nothing in the live working-tree scan (all non-ignored files, repo ignore rules) matched.\n",
            );
        } else {
            // review-2 finding 1: an INCOMPLETE scan with zero matches so far may NOT claim
            // a global no-match — a skipped (unreadable / walk-errored / search-errored)
            // file could contain matches. State only the qualified lower-bound truth; the
            // header + incomplete-scan note above already named the skipped files.
            out.push_str(
                "no matches in the files scanned so far — the scan was incomplete (see note above); a skipped file may contain matches.\n",
            );
        }
        return out;
    }

    out.push('\n');
    for f in files {
        render_file(f, &mut out);
    }

    // Cap disclosure (§2.5) sits at the FOOT of the (capped) list so the reader sees it
    // after the shown hits. The exact/lower-bound total already rendered in the header.
    if capped {
        out.push('\n');
        // review-3 finding 1: `total` is EXACT only for a COMPLETE scan. On an incomplete
        // scan the header already rendered `total` as a lower bound (a skipped file may
        // hold more matches), so the cap line must NOT reassert it as exact — it says
        // "at least M". Restating a lower bound as an exact denominator is the same
        // honesty violation the header fixed (`total_matches` claims exactness only when
        // the scan was complete).
        let total_phrase = if scan_complete {
            total.to_string()
        } else {
            format!("at least {total}")
        };
        out.push_str(&format!(
            "showing {shown} of {total_phrase} matches — --full for all\n"
        ));
    }
    out
}

/// Read a required non-negative count field, or an error line to surface.
fn u64_field(result: &serde_json::Value, field: &str) -> Result<u64, String> {
    result
        .get(field)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("(malformed response: {field} missing or not a number)\n"))
}

/// Render one matched file's group. The file's hits stay contiguous (grouping
/// preserved) and its staleness note renders exactly once, but there is NO standalone
/// path heading: each hit is self-describing as `path:line: text  annotation` so every
/// match is independently actionable (review-4 finding — the grep/rg `--no-heading`
/// idiom the ratified "imitate the output shape" direction targets). The once-per-file
/// staleness note is path-prefixed so it stays unambiguously tied to its file.
fn render_file(f: &serde_json::Value, out: &mut String) {
    let path = match f.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            out.push_str("(malformed file group: path missing or not a string)\n");
            return;
        }
    };

    // Staleness (§2.3, review-0 finding (c)): our OWN DTO enum — exactly one of
    // fresh|stale|not_indexed|unknown. A missing or unrecognized VALUE is MALFORMED and
    // surfaced — NEVER silently defaulted to "fresh" (which would drop a stale-context
    // warning and render stale spans as if current). STANDING HONESTY RULE 2.
    let staleness = match f.get("staleness").and_then(|v| v.as_str()) {
        Some(s @ ("fresh" | "stale" | "not_indexed" | "unknown")) => s,
        Some(other) => {
            out.push_str(&format!(
                "{path}: (malformed file group: unrecognized staleness \"{other}\")\n"
            ));
            return;
        }
        None => {
            out.push_str(&format!(
                "{path}: (malformed file group: staleness missing or not a string)\n"
            ));
            return;
        }
    };
    // The mandated stale-context label. A `stale` file MUST carry a non-empty note; a
    // stale file without it is MALFORMED (would read as fresh) and is surfaced, not
    // hidden. A note on a NON-stale file is an equal contradiction — surfaced, never
    // rendered as a silent mislabel.
    //
    // review-2 finding 2: `staleness_note` is an OPTIONAL string field. ABSENT (key
    // omitted) is the legitimate no-note state; PRESENT-but-not-a-string is a MALFORMED
    // payload (old daemon / serialization bug) and MUST surface — the earlier
    // `.and_then(as_str)` collapsed a non-string value to `None`, which on a fresh/unknown
    // file silently DROPPED it (looked note-free) and on a stale file masqueraded as the
    // "missing required note" case. Distinguish absent from present-but-mistyped here so a
    // malformed note is never rendered as a trustworthy absence (STANDING HONESTY RULE).
    let note = match f.get("staleness_note") {
        None => None,
        Some(serde_json::Value::String(s)) => Some(s.as_str()),
        Some(_) => {
            out.push_str(&format!(
                "{path}: (malformed file group: staleness_note present but not a string)\n"
            ));
            return;
        }
    };
    match (staleness, note) {
        ("stale", Some(n)) if !n.is_empty() => out.push_str(&format!("{path}: ⚠ {n}\n")),
        ("stale", _) => {
            out.push_str(&format!(
                "{path}: (malformed file group: staleness=stale without its required note)\n"
            ));
            return;
        }
        (_, Some(_)) => {
            out.push_str(&format!(
                "{path}: (malformed file group: staleness={staleness} carries a stale note)\n"
            ));
            return;
        }
        (_, None) => {}
    }

    let hits = match f.get("hits") {
        Some(serde_json::Value::Array(a)) => a,
        _ => {
            out.push_str(&format!(
                "{path}: (malformed file group: hits missing or not a list)\n"
            ));
            return;
        }
    };
    for h in hits {
        render_hit(h, path, out);
    }
}

/// Render one hit line: `<path>:<line>: <text>` with the enclosing-symbol annotation
/// appended when present. The `path:line` prefix makes each match independently
/// actionable (review-4). An ABSENT annotation renders nothing extra — the hit was
/// outside every stored span (visible absence, never a guess — §2.2).
fn render_hit(h: &serde_json::Value, path: &str, out: &mut String) {
    let line = h.get("line").and_then(|v| v.as_u64());
    let text = h.get("text").and_then(|v| v.as_str());
    let (Some(line), Some(text)) = (line, text) else {
        out.push_str(&format!(
            "{path}: (malformed hit: line/text missing or mistyped)\n"
        ));
        return;
    };
    // review-2 finding 2: `annotation` is an OPTIONAL string. ABSENT (key omitted) is the
    // visible-absence case — the hit was outside every stored span, so it renders bare
    // (never a guess — §2.2). PRESENT-but-not-a-string is a MALFORMED payload and MUST
    // surface: the earlier `.and_then(as_str)` collapsed a non-string annotation to `None`,
    // rendering it bare and thereby indistinguishable from a genuine outside-every-span
    // hit. Render the (valid) hit line but flag the annotation as malformed rather than
    // silently swallow it (STANDING HONESTY RULE 1 — malformed ≠ absent).
    match h.get("annotation") {
        None => out.push_str(&format!("{path}:{line}: {text}\n")),
        Some(serde_json::Value::String(annotation)) => {
            out.push_str(&format!("{path}:{line}: {text}  {annotation}\n"))
        }
        Some(_) => out.push_str(&format!(
            "{path}:{line}: {text}  (malformed hit: annotation present but not a string)\n"
        )),
    }
}

#[cfg(test)]
mod tests;
