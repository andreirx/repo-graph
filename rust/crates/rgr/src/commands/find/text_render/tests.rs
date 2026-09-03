//! Tests for the `find --text` renderer (`super`). Relocated out of `text_render.rs`
//! per the ≤500-line structural guardrail (FIND-GREP-1 review-2 finding 4) — a test-only
//! child module, NOT a runtime abstraction: `super::*` still reaches the parent's private
//! render helpers (`render_text_scan`, `u64_field`, `render_file`, `render_hit`).

use super::*;
use serde_json::json;

fn base(files: serde_json::Value, total: u64, shown: u64, capped: bool) -> serde_json::Value {
    json!({
        "schema": "rgr.agent.v1",
        "command": "find --text",
        "repo": "demo",
        "snapshot": "snap_01",
        "query": "fsync",
        "fixed": false,
        "scope_note": "live scan of the working tree (all non-ignored files, repo ignore rules applied)",
        "total_matches": total,
        "shown_matches": shown,
        "capped": capped,
        "cap": 200,
        "skipped_unreadable": 0,
        "skipped_search_error": 0,
        "skipped_walk_error": 0,
        "files": files,
    })
}

#[test]
fn hit_in_span_is_annotated_hit_outside_is_bare() {
    let result = base(
        json!([{
            "path": "db/env_posix.cc",
            "staleness": "fresh",
            "hits": [
                {"line": 411, "text": "  fsync(fd);", "annotation": "[function PosixEnv::Fsync]"},
                {"line": 900, "text": "// fsync note"}
            ]
        }]),
        2,
        2,
        false,
    );
    let out = render_text_scan(&result);
    // review-4: each match is a self-describing `path:line: text  annotation` line —
    // no standalone path heading; the leveldb-shaped hit carries its own `path:line`.
    assert!(
        out.contains("db/env_posix.cc:411:   fsync(fd);  [function PosixEnv::Fsync]"),
        "annotated hit is path:line-prefixed: {out}"
    );
    // No standalone path-only heading line is emitted (grouping is preserved by the
    // per-hit path prefix, not by a heading).
    assert!(
        !out.contains("db/env_posix.cc\n"),
        "no standalone path heading: {out}"
    );
    // The bare hit renders path:line-prefixed with NO annotation appended.
    assert!(
        out.contains("db/env_posix.cc:900: // fsync note\n"),
        "bare hit is path:line-prefixed: {out}"
    );
    // The count is a HEADER (review-0 finding (a)) and, for a complete scan, exact.
    assert!(out.contains("2 match(es)\n"), "count header: {out}");
    // The header precedes the file group.
    let header_at = out.find("2 match(es)").expect("header present");
    let group_at = out.find("db/env_posix.cc").expect("group present");
    assert!(header_at < group_at, "count header precedes groups: {out}");
}

#[test]
fn staleness_note_renders_once_for_a_changed_file() {
    let result = base(
        json!([{
            "path": "src/x.rs",
            "staleness": "stale",
            "staleness_note": "file changed since snapshot — symbol context may be stale",
            "hits": [{"line": 5, "text": "let y = f();"}, {"line": 6, "text": "return y;"}]
        }]),
        2,
        2,
        false,
    );
    let out = render_text_scan(&result);
    // review-4: the once-per-file staleness note is path-prefixed so it stays tied to
    // its file (there is no standalone heading), and it renders exactly ONCE even though
    // the file has two hits.
    assert!(
        out.contains("src/x.rs: ⚠ file changed since snapshot — symbol context may be stale"),
        "path-prefixed staleness note: {out}"
    );
    assert_eq!(
        out.matches("⚠ file changed since snapshot").count(),
        1,
        "staleness note renders exactly once per file: {out}"
    );
    // Both hits are path:line-prefixed.
    assert!(out.contains("src/x.rs:5: let y = f();"), "hit 1: {out}");
    assert!(out.contains("src/x.rs:6: return y;"), "hit 2: {out}");
}

#[test]
fn cap_disclosure_when_capped() {
    let result = base(
        json!([{"path": "a.rs", "staleness": "fresh",
                    "hits": [{"line": 1, "text": "x"}]}]),
        260,
        200,
        true,
    );
    let out = render_text_scan(&result);
    assert!(
        out.contains("showing 200 of 260 matches — --full for all"),
        "cap disclosure: {out}"
    );
}

#[test]
fn honest_empty_is_capability_not_repo_absence() {
    let result = base(json!([]), 0, 0, false);
    let out = render_text_scan(&result);
    // review-0 finding (a): the count header renders even for zero results.
    assert!(out.contains("0 match(es)\n"), "zero-count header: {out}");
    assert!(
        out.contains("nothing in the live working-tree scan"),
        "{out}"
    );
    // Never claims the concept has no home in the repo.
    assert!(
        !out.contains("distinct home"),
        "no false repo-absence claim: {out}"
    );
}

#[test]
fn incomplete_scan_is_a_lower_bound_with_reason_classes() {
    // review-0 finding (b): a scan that skipped files may NOT claim an exact total.
    let mut result = base(
        json!([{"path": "a.rs", "staleness": "fresh",
                    "hits": [{"line": 1, "text": "x"}]}]),
        1,
        1,
        false,
    );
    result["skipped_unreadable"] = json!(3);
    result["skipped_search_error"] = json!(1);
    let out = render_text_scan(&result);
    assert!(
        out.contains("1 match(es) so far — scan incomplete; total is a lower bound"),
        "lower-bound header: {out}"
    );
    assert!(
        out.contains("⚠ incomplete scan: 4 file(s) skipped (unreadable: 3, search error: 1)"),
        "reason-class breakdown: {out}"
    );
    // Never the bare exact claim.
    assert!(
        !out.contains("1 match(es)\n"),
        "no exact claim when incomplete: {out}"
    );
}

#[test]
fn walk_error_is_its_own_reason_class_in_the_incomplete_note() {
    // review-0 finding 2: a walk-enumeration failure (unreadable directory) is an
    // omission class of its own — it must downgrade the total to a lower bound and
    // name its reason, exactly like an unreadable file.
    let mut result = base(
        json!([{"path": "a.rs", "staleness": "fresh",
                    "hits": [{"line": 1, "text": "x"}]}]),
        1,
        1,
        false,
    );
    result["skipped_walk_error"] = json!(2);
    let out = render_text_scan(&result);
    assert!(
        out.contains("1 match(es) so far — scan incomplete; total is a lower bound"),
        "lower-bound header: {out}"
    );
    assert!(
        out.contains("⚠ incomplete scan: 2 file(s) skipped (walk error: 2)"),
        "walk-error reason class: {out}"
    );
}

#[test]
fn missing_skipped_walk_error_is_surfaced_as_malformed() {
    // The field is our OWN DTO field, always serialized. An old daemon that omits it
    // is a malformed response, surfaced — never silently treated as a complete scan.
    let mut result = base(json!([]), 0, 0, false);
    result.as_object_mut().unwrap().remove("skipped_walk_error");
    let out = render_text_scan(&result);
    assert!(
        out.contains("malformed response: skipped_walk_error missing or not a number"),
        "missing skip field surfaced: {out}"
    );
}

#[test]
fn stale_file_without_its_required_note_is_surfaced_not_rendered_as_fresh() {
    // review-0 finding (c): a malformed stale DTO (missing note) must SURFACE, never
    // render as a fresh-by-default file that silently drops the stale-context label.
    let result = base(
        json!([{"path": "src/x.rs", "staleness": "stale",
                    "hits": [{"line": 5, "text": "let y = f();"}]}]),
        1,
        1,
        false,
    );
    let out = render_text_scan(&result);
    assert!(
        out.contains("malformed file group: staleness=stale without its required note"),
        "stale-without-note surfaced: {out}"
    );
}

#[test]
fn unrecognized_staleness_value_is_surfaced_never_defaulted_fresh() {
    // review-0 finding (c): an unknown staleness VALUE is malformed, not "fresh".
    let result = base(
        json!([{"path": "src/x.rs", "staleness": "sortof",
                    "hits": [{"line": 5, "text": "let y = f();"}]}]),
        1,
        1,
        false,
    );
    let out = render_text_scan(&result);
    assert!(
        out.contains("malformed file group: unrecognized staleness \"sortof\""),
        "unrecognized staleness surfaced: {out}"
    );
}

#[test]
fn fatal_error_is_surfaced_not_a_false_empty() {
    let mut result = base(json!([]), 0, 0, false);
    result["error"] = json!("invalid pattern: unclosed group");
    let out = render_text_scan(&result);
    assert!(out.contains("scan failed: invalid pattern"), "{out}");
}

#[test]
fn context_unavailable_reason_is_shown() {
    let mut result = base(
        json!([{"path": "a.rs", "staleness": "unknown",
                    "hits": [{"line": 1, "text": "x"}]}]),
        1,
        1,
        false,
    );
    result["snapshot"] = json!("");
    result["context_unavailable"] =
            json!("repo not indexed yet — run `rmap index .`; symbol context and staleness unavailable this run");
    let out = render_text_scan(&result);
    assert!(
        out.contains("note: repo not indexed yet"),
        "reason shown: {out}"
    );
}

#[test]
fn malformed_query_is_surfaced() {
    let result = json!({"fixed": false, "scope_note": "s", "files": []});
    let out = render_text_scan(&result);
    assert!(out.contains("malformed response: query missing"), "{out}");
}

#[test]
fn zero_matches_on_an_incomplete_scan_is_a_qualified_lower_bound_not_a_global_no_match() {
    // review-2 finding 1: zero matches SO FAR + an incomplete scan (a skipped file)
    // may NOT claim the global "nothing … matched" — a skipped file could contain
    // matches. Only the qualified lower-bound truth is honest here.
    let mut result = base(json!([]), 0, 0, false);
    result["skipped_unreadable"] = json!(2);
    let out = render_text_scan(&result);
    assert!(
        out.contains("0 match(es) so far — scan incomplete; total is a lower bound"),
        "lower-bound header on an incomplete zero result: {out}"
    );
    assert!(
        out.contains("no matches in the files scanned so far — the scan was incomplete"),
        "qualified no-match wording: {out}"
    );
    // NEVER the global claim — a skipped file may contain matches.
    assert!(
        !out.contains("nothing in the live working-tree scan"),
        "no global no-match claim on an incomplete scan: {out}"
    );
}

#[test]
fn cap_line_on_an_incomplete_scan_states_the_total_as_a_lower_bound_not_exact() {
    // review-3 finding 1: when the scan skipped files, `total` is a LOWER BOUND (the
    // header already says so). The cap disclosure must NOT reassert it as an exact
    // denominator — it renders `at least M`, matching the header's honesty.
    let mut result = base(
        json!([{"path": "a.rs", "staleness": "fresh",
                    "hits": [{"line": 1, "text": "x"}]}]),
        260,
        200,
        true,
    );
    result["skipped_unreadable"] = json!(3);
    let out = render_text_scan(&result);
    assert!(
        out.contains("showing 200 of at least 260 matches — --full for all"),
        "cap line states a lower-bound total on an incomplete scan: {out}"
    );
    // NEVER the exact-denominator form the complete-scan case renders.
    assert!(
        !out.contains("showing 200 of 260 matches"),
        "no exact denominator when the scan was incomplete: {out}"
    );
}

#[test]
fn cap_line_on_a_complete_scan_states_the_exact_total() {
    // The complementary guard: a COMPLETE scan keeps the exact `showing N of M` form
    // (no spurious "at least").
    let result = base(
        json!([{"path": "a.rs", "staleness": "fresh",
                    "hits": [{"line": 1, "text": "x"}]}]),
        260,
        200,
        true,
    );
    let out = render_text_scan(&result);
    assert!(
        out.contains("showing 200 of 260 matches — --full for all"),
        "exact denominator on a complete scan: {out}"
    );
    assert!(
        !out.contains("at least"),
        "no lower-bound qualifier on a complete scan: {out}"
    );
}

#[test]
fn malformed_annotation_present_but_not_a_string_is_surfaced_never_rendered_bare() {
    // review-2 finding 2: a hit whose `annotation` is PRESENT but not a string is a
    // malformed payload — it must SURFACE, never render as a bare hit (which would be
    // indistinguishable from a genuine outside-every-span hit).
    let result = base(
        json!([{
            "path": "a.rs",
            "staleness": "fresh",
            "hits": [{"line": 1, "text": "x", "annotation": 42}]
        }]),
        1,
        1,
        false,
    );
    let out = render_text_scan(&result);
    assert!(
        out.contains("(malformed hit: annotation present but not a string)"),
        "malformed annotation surfaced: {out}"
    );
    // The valid hit line still renders (line/text are well-formed), path:line-prefixed.
    assert!(
        out.contains("a.rs:1: x"),
        "the hit itself still renders: {out}"
    );
}

#[test]
fn malformed_staleness_note_present_but_not_a_string_is_surfaced_never_dropped() {
    // review-2 finding 2: on a fresh/unknown file a `staleness_note` that is PRESENT
    // but not a string was silently dropped (rendered as note-free). It must SURFACE
    // as malformed — a malformed note is never a trustworthy absence.
    let result = base(
        json!([{
            "path": "a.rs",
            "staleness": "fresh",
            "staleness_note": 7,
            "hits": [{"line": 1, "text": "x"}]
        }]),
        1,
        1,
        false,
    );
    let out = render_text_scan(&result);
    assert!(
        out.contains("(malformed file group: staleness_note present but not a string)"),
        "malformed staleness_note surfaced: {out}"
    );
}
