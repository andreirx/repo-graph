//! Tests for `docs.rs` (moved out per the 500-line guardrail; SELF-POLLUTION-1 review-6 #3).

use super::*;

// ── docs list tests ──────────────────────────────────────────────────────

fn sample_list_response() -> DocsListResponse {
    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("readme".to_string(), 2);
    counts_by_kind.insert("changelog".to_string(), 1);

    DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![
            DocEntry {
                path: "README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                content_hash: "abc123".to_string(),
                release_family: None,
            },
            DocEntry {
                path: "docs/README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                content_hash: "def456".to_string(),
                release_family: None,
            },
            DocEntry {
                path: "CHANGELOG.md".to_string(),
                kind: "changelog".to_string(),
                generated: true,
                content_hash: "ghi789".to_string(),
                release_family: None,
            },
        ],
        count: 3,
        counts_by_kind,
        generated_count: 1,
        unreadable: 0,
        unscanned_markup_outside_docs_tree: 0,
        discovery_rule: None,
    }
}

/// A response with NO generated entries (the byte-parity case: nothing for the
/// slice to filter).
fn sample_list_response_no_generated() -> DocsListResponse {
    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("readme".to_string(), 1);
    DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![DocEntry {
            path: "README.md".to_string(),
            kind: "readme".to_string(),
            generated: false,
            content_hash: "abc".to_string(),
            release_family: None,
        }],
        count: 1,
        counts_by_kind,
        generated_count: 0,
        unreadable: 0,
        unscanned_markup_outside_docs_tree: 0,
        discovery_rule: None,
    }
}

#[test]
fn list_render_surfaces_unreadable_count() {
    // operator RULING 3: unreadable sidecars are said out loud in the human listing
    // ("+N unreadable, counted"), never silently folded as authored.
    let mut resp = sample_list_response_no_generated();
    resp.unreadable = 3;
    let out = resp.render_human(false, false);
    assert!(
        out.contains("+3 unreadable, counted"),
        "unreadable surfaced in human render: {out}"
    );
    // Zero unreadable → no such line (no "+0 unreadable" noise).
    let clean = sample_list_response_no_generated();
    assert!(
        !clean.render_human(false, false).contains("unreadable"),
        "no unreadable line when zero"
    );
}

#[test]
fn list_render_shows_header() {
    let resp = sample_list_response();
    let out = resp.render_human(true, false);
    assert!(out.starts_with("Documentation\n"));
}

#[test]
fn list_render_shows_count() {
    let resp = sample_list_response();
    let out = resp.render_human(true, false);
    assert!(out.contains("3 documents"));
}

#[test]
fn list_render_shows_by_kind() {
    let resp = sample_list_response();
    let out = resp.render_human(true, false);
    assert!(out.contains("By kind:"));
    assert!(out.contains("readme  2"));
    assert!(out.contains("changelog  1"));
}

#[test]
fn list_render_shows_generated_count() {
    let resp = sample_list_response();
    let out = resp.render_human(true, false);
    assert!(out.contains("1 generated"));
}

#[test]
fn list_render_shows_entries_sorted_by_path() {
    let resp = sample_list_response();
    let out = resp.render_human(true, false);
    // Entries should be sorted: CHANGELOG.md, README.md, docs/README.md
    let changelog_pos = out.find("CHANGELOG.md").unwrap();
    let readme_pos = out.find("README.md").unwrap();
    let docs_readme_pos = out.find("docs/README.md").unwrap();
    assert!(changelog_pos < readme_pos);
    assert!(readme_pos < docs_readme_pos);
}

#[test]
fn list_render_shows_generated_marker() {
    let resp = sample_list_response();
    let out = resp.render_human(true, false);
    assert!(out.contains("CHANGELOG.md  changelog  [generated]"));
}

#[test]
fn list_render_shows_hint() {
    let resp = sample_list_response();
    let out = resp.render_human(true, false);
    assert!(out.contains("hint: run 'rmap docs extract' to scan for explicit rg: markers"));
}

#[test]
fn list_render_empty_shows_hint() {
    let resp = DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![],
        count: 0,
        counts_by_kind: BTreeMap::new(),
        generated_count: 0,
        unreadable: 0,
        unscanned_markup_outside_docs_tree: 0,
        discovery_rule: None,
    };
    let out = resp.render_human(true, false);
    assert!(out.contains("0 documents"));
    assert!(out.contains("hint: no documentation files detected"));
}

#[test]
fn list_render_singular_document() {
    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("readme".to_string(), 1);

    let resp = DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![DocEntry {
            path: "README.md".to_string(),
            kind: "readme".to_string(),
            generated: false,
            content_hash: "abc".to_string(),
            release_family: None,
        }],
        count: 1,
        counts_by_kind,
        generated_count: 0,
        unreadable: 0,
        unscanned_markup_outside_docs_tree: 0,
        discovery_rule: None,
    };
    let out = resp.render_human(true, false);
    assert!(out.contains("1 document\n")); // singular
}

// ── SELF-POLLUTION-1 §3: default generated-map exclusion ──────────────────

/// A response mixing the reader's docs with rmap's own generated maps.
fn response_with_generated_maps() -> DocsListResponse {
    DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![
            DocEntry {
                path: "README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                content_hash: "a".to_string(),
                release_family: None,
            },
            DocEntry {
                path: "src/MAP.md".to_string(),
                kind: "map".to_string(),
                generated: true,
                content_hash: "b".to_string(),
                release_family: None,
            },
            DocEntry {
                path: "src/core/MAP.md".to_string(),
                kind: "map".to_string(),
                generated: true,
                content_hash: "c".to_string(),
                release_family: None,
            },
        ],
        count: 3,
        counts_by_kind: BTreeMap::new(),
        generated_count: 2,
        unreadable: 0,
        unscanned_markup_outside_docs_tree: 0,
        discovery_rule: None,
    }
}

#[test]
fn list_default_excludes_generated_maps_and_states_the_count() {
    // Default (include_generated = false): rmap's own maps are hidden, only the
    // reader's doc is shown, and the excluded count is stated (never silently hidden).
    let resp = response_with_generated_maps();
    let out = resp.render_human(false, false);
    assert!(
        out.contains("1 document\n"),
        "only the reader's doc counts: {out}"
    );
    assert!(
        out.contains(
            "2 generated maps excluded (tool-generated map summaries; use --include-generated to show)"
        ),
        "excluded count stated: {out}"
    );
    assert!(out.contains("README.md"), "reader's doc listed: {out}");
    assert!(
        !out.contains("src/MAP.md"),
        "generated map hidden by default: {out}"
    );
    // Singular wording when exactly one map is excluded.
    assert!(
        !out.contains("1 generated maps"),
        "plural only when >1: {out}"
    );
}

#[test]
fn list_include_generated_shows_maps_and_no_exclusion_line() {
    // Opt-in: all entries render, and NO "excluded" line is printed.
    let resp = response_with_generated_maps();
    let out = resp.render_human(true, false);
    assert!(out.contains("3 documents"), "{out}");
    assert!(
        out.contains("src/MAP.md"),
        "generated map shown on opt-in: {out}"
    );
    assert!(out.contains("src/core/MAP.md"), "{out}");
    assert!(
        !out.contains("excluded (tool-generated"),
        "no exclusion line when opted in: {out}"
    );
    assert!(
        out.contains("2 generated"),
        "visible generated count on opt-in: {out}"
    );
}

#[test]
fn list_all_generated_does_not_claim_no_docs() {
    // When EVERY doc is an rmap map, the default listing must not misrepresent the
    // repo as having no documentation — it points at --include-generated instead.
    let resp = DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries: vec![DocEntry {
            path: "MAP.md".to_string(),
            kind: "map".to_string(),
            generated: true,
            content_hash: "a".to_string(),
            release_family: None,
        }],
        count: 1,
        counts_by_kind: BTreeMap::new(),
        generated_count: 1,
        unreadable: 0,
        unscanned_markup_outside_docs_tree: 0,
        discovery_rule: None,
    };
    let out = resp.render_human(false, false);
    assert!(out.contains("0 documents"), "{out}");
    assert!(
        out.contains("1 generated map excluded"),
        "singular map excluded wording: {out}"
    );
    assert!(
        out.contains("all documentation here is tool-generated maps"),
        "honest hint, never a false 'no documentation': {out}"
    );
    assert!(
        !out.contains("no documentation files detected"),
        "must not claim the repo has no docs when it has generated maps: {out}"
    );
}

// ── DOCS-LIST-2 §2: vendored demotion / release-notes grouping / budget ───────

fn entry(path: &str, kind: &str) -> DocEntry {
    DocEntry {
        path: path.to_string(),
        kind: kind.to_string(),
        generated: false,
        content_hash: "h".to_string(),
        release_family: None,
    }
}

/// A `release-notes` entry carrying the daemon-attached family subtree the renderer groups by
/// (DOCS-LIST-2 §2 — the `release_family` DTO field replaces the old cross-crate `release_subtree`).
fn release_entry(path: &str, family: &str) -> DocEntry {
    DocEntry {
        path: path.to_string(),
        kind: "release-notes".to_string(),
        generated: false,
        content_hash: "h".to_string(),
        release_family: Some(family.to_string()),
    }
}

fn response_with(entries: Vec<DocEntry>) -> DocsListResponse {
    let count = entries.len();
    DocsListResponse {
        command: "docs list".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        entries,
        count,
        counts_by_kind: BTreeMap::new(),
        generated_count: 0,
        unreadable: 0,
        unscanned_markup_outside_docs_tree: 0,
        discovery_rule: None,
    }
}

#[test]
fn vendored_demoted_from_headline_and_listing_but_stated() {
    let resp = response_with(vec![
        entry("README.md", "readme"),
        entry(
            "packages/engine/fraktag-env/lib/python3.11/site-packages/tqdm/README.md",
            "vendored",
        ),
        entry(
            "packages/engine/fraktag-env/lib/python3.11/site-packages/numpy/README.md",
            "vendored",
        ),
    ]);
    let out = resp.render_human(false, false);
    // Headline counts only the reader's doc, not the two vendored ones.
    assert!(
        out.contains("1 document\n"),
        "vendored excluded from headline: {out}"
    );
    // Stated, never silently hidden — contract form "+N vendored docs (excluded)" (review-0 F5).
    assert!(
        out.contains("+2 vendored docs (excluded"),
        "vendored count line present in contract form: {out}"
    );
    // Not listed individually.
    assert!(
        !out.contains("site-packages/tqdm"),
        "vendored not listed: {out}"
    );
    assert!(out.contains("README.md"), "reader's doc listed: {out}");
}

#[test]
fn all_vendored_does_not_claim_no_docs() {
    let resp = response_with(vec![entry(
        "venv/lib/python3.11/site-packages/a/README.md",
        "vendored",
    )]);
    let out = resp.render_human(false, false);
    assert!(out.contains("0 documents"), "{out}");
    assert!(
        out.contains("all documentation here is vendored dependency docs"),
        "honest hint, never a false 'no documentation': {out}"
    );
    assert!(!out.contains("no documentation files detected"), "{out}");
}

#[test]
fn release_notes_grouped_one_line_per_family() {
    let mut entries = vec![entry("README.md", "readme")];
    for v in ["1.4.x", "1.5.x", "2.0"] {
        entries.push(release_entry(
            &format!("docs/releases/{v}.txt"),
            "docs/releases",
        ));
    }
    let resp = response_with(entries);
    let out = resp.render_human(false, false);
    // Headline counts release notes (3) + readme (1) = 4.
    assert!(out.contains("4 documents"), "release notes counted: {out}");
    // Grouped to ONE family line, not three entry rows.
    assert!(
        out.contains("repo_test release notes: 3 files under docs/releases/ — --full to list"),
        "release notes grouped: {out}"
    );
    assert!(
        !out.contains("1.4.x.txt"),
        "not listed individually by default: {out}"
    );
}

#[test]
fn release_notes_full_lists_them() {
    let mut entries = vec![entry("README.md", "readme")];
    entries.push(release_entry("docs/releases/1.4.x.txt", "docs/releases"));
    let resp = response_with(entries);
    let out = resp.render_human(false, true);
    // --full folds them into the flat list; no grouping line.
    assert!(
        out.contains("docs/releases/1.4.x.txt"),
        "listed under --full: {out}"
    );
    assert!(
        !out.contains("— --full to list"),
        "no grouping pointer under --full: {out}"
    );
}

#[test]
fn entry_list_budgeted_with_remainder() {
    let mut entries = Vec::new();
    for i in 0..40 {
        entries.push(entry(&format!("docs/page{i:02}.md"), "architecture"));
    }
    let resp = response_with(entries);
    let out = resp.render_human(false, false);
    // Default caps at HUMAN_ROW_BUDGET (25) with an honest remainder.
    assert!(
        out.contains("(+15 more — --full)"),
        "budget remainder present: {out}"
    );
    // --full uncaps: no remainder line, last entry present.
    let full = resp.render_human(false, true);
    assert!(
        !full.contains("more — --full"),
        "no remainder under --full: {full}"
    );
    assert!(
        full.contains("docs/page39.md"),
        "all rows under --full: {full}"
    );
}

#[test]
fn release_family_group_lines_are_budgeted() {
    // DOCS-LIST-2 review-2 item 2: the release-family GROUP lines are default display rows too, so
    // many families must NOT emit unbounded group lines — they share the same HUMAN_ROW_BUDGET as the
    // flat entries, with one truthful remainder. 40 distinct families → 25 shown, 15 in the remainder.
    let mut entries = Vec::new();
    for i in 0..40 {
        entries.push(release_entry(
            &format!("docs/rel{i:02}/1.0.txt"),
            &format!("docs/rel{i:02}"),
        ));
    }
    let resp = response_with(entries);
    let out = resp.render_human(false, false);
    let shown_group_lines = out.matches(" release notes: ").count();
    assert_eq!(
        shown_group_lines, HUMAN_ROW_BUDGET,
        "family group lines capped at the row budget: {out}"
    );
    assert!(
        out.contains("(+15 more — --full)"),
        "one truthful remainder over the omitted family rows: {out}"
    );
    // --full uncaps: every family's file is listed individually, no remainder.
    let full = resp.render_human(false, true);
    assert!(
        !full.contains("more — --full"),
        "no remainder under --full: {full}"
    );
    assert!(
        full.contains("docs/rel39/1.0.txt"),
        "every release note listed under --full: {full}"
    );
}

#[test]
fn family_lines_and_entries_share_one_budget() {
    // The combined budget covers BOTH row kinds: 20 families + 20 reader docs = 40 rows → 25 shown
    // (all 20 family lines come first, then 5 docs), remainder 15.
    let mut entries = Vec::new();
    for i in 0..20 {
        entries.push(release_entry(
            &format!("docs/rel{i:02}/1.0.txt"),
            &format!("docs/rel{i:02}"),
        ));
    }
    for i in 0..20 {
        entries.push(entry(&format!("docs/guide{i:02}.md"), "architecture"));
    }
    let resp = response_with(entries);
    let out = resp.render_human(false, false);
    let group_lines = out.matches(" release notes: ").count();
    let doc_lines = out.matches("  architecture\n").count();
    assert_eq!(group_lines, 20, "all 20 family lines fit: {out}");
    assert_eq!(doc_lines, 5, "then 5 docs fill the budget of 25: {out}");
    assert!(
        out.contains("(+15 more — --full)"),
        "one remainder over the combined omitted rows: {out}"
    );
}

// ── SELF-POLLUTION-1 §2.3 + review-5 finding 1: `--json` filter + byte-parity ─

#[test]
fn json_default_excludes_generated_and_reports_excluded_count() {
    // Default (include_generated = false) WITH generated maps present: the machine
    // view drops rmap's own maps, and `excluded_generated` states how many — a
    // consumer never has to infer what was hidden. This is the AFFECTED case, so
    // `filtered_json_view` returns Some.
    let resp = response_with_generated_maps();
    let v = resp
        .filtered_json_view(false)
        .expect("generated maps present → filtered view");

    assert_eq!(v["count"], 1, "only the reader's doc counts: {v}");
    assert_eq!(v["excluded_generated"], 2, "excluded count reported: {v}");
    assert_eq!(v["generated_count"], 0, "no generated in visible set: {v}");

    let entries = v["entries"].as_array().unwrap();
    let paths: Vec<&str> = entries
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["README.md"], "generated maps dropped: {v}");
}

#[test]
fn json_include_generated_is_raw_passthrough_no_filtered_view() {
    // Opt-in: nothing is excluded → `filtered_json_view` returns None, so the
    // command prints the RAW daemon value UNCHANGED (byte-parity with pre-slice —
    // review-5 finding 1). The CLI never rebuilds/sorts/annotates in this case.
    let resp = response_with_generated_maps();
    assert!(
        resp.filtered_json_view(true).is_none(),
        "--include-generated excludes nothing → raw passthrough, not a rebuilt view"
    );
}

#[test]
fn json_no_generated_maps_is_raw_passthrough() {
    // A repo with NO generated maps: default `--json` filters nothing → None →
    // raw passthrough. This is the byte-parity guarantee review-5 finding 1
    // requires (no gratuitous `excluded_generated: 0`, no re-sorting).
    let resp = sample_list_response_no_generated();
    assert!(
        resp.filtered_json_view(false).is_none(),
        "nothing generated → nothing filtered → raw passthrough"
    );
}

#[test]
fn json_filtered_view_preserves_entry_fields_and_recomputes_counts_by_kind() {
    // In the AFFECTED case the visible entries keep their full field shape (incl.
    // content_hash), and counts_by_kind reflects the VISIBLE set.
    let resp = response_with_generated_maps();
    let v = resp.filtered_json_view(false).expect("filtered view");

    let entry = &v["entries"].as_array().unwrap()[0];
    assert_eq!(entry["path"], "README.md");
    assert_eq!(entry["kind"], "readme");
    assert_eq!(entry["generated"], false);
    assert_eq!(entry["content_hash"], "a");

    // Only the reader's readme is visible → counts_by_kind has just that.
    assert_eq!(v["counts_by_kind"]["readme"], 1);
    assert!(
        v["counts_by_kind"].get("map").is_none(),
        "generated 'map' kind excluded by default: {v}"
    );
    // No unreadable in this fixture → the key is absent (not `0`).
    assert!(
        v.get("unreadable").is_none(),
        "unreadable key omitted when zero: {v}"
    );
}

#[test]
fn json_filtered_view_surfaces_unreadable_when_present() {
    // operator RULING 3: when the daemon reported unreadable sidecars, the filtered
    // machine view carries the count so a consumer sees the UNKNOWN, never a silent
    // omission. Emitted only when > 0.
    let mut resp = response_with_generated_maps();
    resp.unreadable = 2;
    let v = resp.filtered_json_view(false).expect("filtered view");
    assert_eq!(
        v["unreadable"], 2,
        "unreadable count surfaced in machine view: {v}"
    );
}

// ── DOCS-DISCOVERY-1: unscanned markup (RG-REQ-008-L06) ─────────────────

#[test]
fn list_render_states_unscanned_markup_outside_docs_tree() {
    let mut resp = sample_list_response_no_generated();
    resp.unscanned_markup_outside_docs_tree = 3;
    let out = resp.render_human(false, false);
    assert!(
        out.lines().any(|l| l
            == "+3 markdown/prose files outside a docs tree not scanned (--json for the rule)"),
        "plural unscanned line:\n{out}"
    );
    resp.unscanned_markup_outside_docs_tree = 1;
    let out = resp.render_human(false, false);
    assert!(
        out.lines()
            .any(|l| l
                == "+1 markdown/prose file outside a docs tree not scanned (--json for the rule)"),
        "singular unscanned line:\n{out}"
    );
    // Placed after the unreadable line (both honesty lines above the listing).
    resp.unreadable = 2;
    let out = resp.render_human(false, false);
    let unreadable_at = out.find("+2 unreadable, counted").expect("unreadable line");
    let unscanned_at = out.find("+1 markdown/prose file").expect("unscanned line");
    assert!(unreadable_at < unscanned_at, "order:\n{out}");
}

#[test]
fn list_render_omits_the_unscanned_line_when_nothing_was_refused() {
    let resp = sample_list_response_no_generated();
    let out = resp.render_human(false, false);
    assert!(
        !out.contains("outside a docs tree not scanned"),
        "no unscanned line at count 0:\n{out}"
    );
    // An empty headline still states the refusals (before the empty-headline early return).
    let mut empty = response_with(Vec::new());
    empty.unscanned_markup_outside_docs_tree = 2;
    let out = empty.render_human(false, false);
    assert!(
        out.contains(
            "+2 markdown/prose files outside a docs tree not scanned (--json for the rule)"
        ),
        "stated even with no admitted docs:\n{out}"
    );
}

#[test]
fn json_filtered_view_surfaces_unscanned_and_rule_when_present() {
    let mut resp = response_with_generated_maps();
    resp.unscanned_markup_outside_docs_tree = 4;
    resp.discovery_rule = Some(sample_rule_view());
    let v = resp.filtered_json_view(false).expect("filtered view");
    assert_eq!(v["unscanned_markup_outside_docs_tree"], 4, "{v}");
    assert_eq!(v["discovery_rule"]["stems"][0], "readme", "{v}");
    assert_eq!(v["discovery_rule"], sample_rule_json(), "{v}");

    // Nothing refused → neither key in the filtered view.
    let resp = response_with_generated_maps();
    let v = resp.filtered_json_view(false).expect("filtered view");
    assert!(v.get("unscanned_markup_outside_docs_tree").is_none(), "{v}");
    assert!(v.get("discovery_rule").is_none(), "{v}");
}

/// The rule object exactly as the daemon serializes doc-facts' `DiscoveryRule`.
fn sample_rule_json() -> serde_json::Value {
    serde_json::json!({
        "stems": ["readme", "contributing", "changelog", "architecture", "design",
                  "overview", "install", "building", "authors", "news"],
        "extensions": [".md", ".markdown", ".txt", ".rst", ".adoc"],
        "doc_tree_dirs": ["docs", "doc", "design"],
        "doc_tree_conventions": ["src/site"],
        "unscanned_extensions": [".md", ".markdown", ".rst", ".adoc"]
    })
}

fn sample_rule_view() -> DiscoveryRuleView {
    serde_json::from_value(sample_rule_json()).expect("well-formed rule")
}

/// A daemon `docs_list` payload with NO generated maps (so `--json` is the raw passthrough),
/// one readme entry, and the given extra keys merged in.
fn raw_payload(extra: serde_json::Value) -> serde_json::Value {
    let mut p = serde_json::json!({
        "command": "docs list",
        "repo": "r",
        "repo_path": "/tmp/r",
        "entries": [{"path": "README.md", "kind": "readme", "generated": false, "content_hash": "a"}],
        "count": 1,
        "counts_by_kind": {"readme": 1},
        "generated_count": 0
    });
    for (k, v) in extra.as_object().expect("object").iter() {
        p[k] = v.clone();
    }
    p
}

// RG-REQ-008-L06 raw-JSON amendment: the count and the rule decode as ONE fact at the single
// `from_value::<DocsListResponse>` both modes pass through (commands/docs.rs) — any other pairing
// is a named malformed-payload error, so it is never rendered in either mode.
#[test]
fn malformed_unscanned_payload_is_rejected_never_rendered() {
    let mut rule_missing_stems = sample_rule_json();
    rule_missing_stems.as_object_mut().unwrap().remove("stems");
    let malformed = [
        (
            "count without rule",
            serde_json::json!({"unscanned_markup_outside_docs_tree": 3}),
        ),
        (
            "count with null rule",
            serde_json::json!({"unscanned_markup_outside_docs_tree": 3, "discovery_rule": null}),
        ),
        (
            "rule missing stems",
            serde_json::json!({"unscanned_markup_outside_docs_tree": 3, "discovery_rule": rule_missing_stems}),
        ),
        (
            "rule without count",
            serde_json::json!({"discovery_rule": sample_rule_json()}),
        ),
        (
            "explicit count 0",
            serde_json::json!({"unscanned_markup_outside_docs_tree": 0, "discovery_rule": sample_rule_json()}),
        ),
        (
            "explicit count 0 without rule",
            serde_json::json!({"unscanned_markup_outside_docs_tree": 0}),
        ),
    ];
    for (case, extra) in malformed {
        let err = serde_json::from_value::<DocsListResponse>(raw_payload(extra))
            .expect_err(case)
            .to_string();
        assert!(
            err.contains("malformed docs_list payload"),
            "{case}: the error names the malformed payload: {err}"
        );
    }

    // Both keys absent (every pre-slice and nothing-refused payload): decodes, count 0, no rule,
    // and no unscanned line.
    let ok = serde_json::from_value::<DocsListResponse>(raw_payload(serde_json::json!({})))
        .expect("both keys absent decodes");
    assert_eq!(ok.unscanned_markup_outside_docs_tree, 0);
    assert!(ok.discovery_rule.is_none());
    assert!(!ok
        .render_human(false, false)
        .contains("outside a docs tree not scanned"));
}

#[test]
fn raw_json_path_prints_only_payloads_whose_count_carries_the_rule() {
    let with_rule = raw_payload(serde_json::json!({
        "unscanned_markup_outside_docs_tree": 2,
        "discovery_rule": sample_rule_json()
    }));
    let resp = serde_json::from_value::<DocsListResponse>(with_rule.clone())
        .expect("count + well-formed rule decodes");
    assert_eq!(resp.unscanned_markup_outside_docs_tree, 2);
    assert_eq!(resp.discovery_rule, Some(sample_rule_view()));
    // No generated maps → `filtered_json_view` is None → the command prints the RAW payload,
    // which carries the rule object beside the count.
    assert!(resp.filtered_json_view(false).is_none());
    assert_eq!(with_rule["unscanned_markup_outside_docs_tree"], 2);
    assert_eq!(with_rule["discovery_rule"], sample_rule_json());
    // The human line points at that rule.
    assert!(resp
        .render_human(false, false)
        .contains("+2 markdown/prose files outside a docs tree not scanned (--json for the rule)"));

    // The same payload with the rule removed does not decode — the command prints its
    // "failed to parse response" error, never the payload.
    let mut without_rule = with_rule;
    without_rule
        .as_object_mut()
        .unwrap()
        .remove("discovery_rule");
    assert!(serde_json::from_value::<DocsListResponse>(without_rule).is_err());
}

// ── docs extract tests ───────────────────────────────────────────────────

fn sample_extract_response() -> DocsExtractResponse {
    let mut files_by_kind = BTreeMap::new();
    files_by_kind.insert("readme".to_string(), 2);

    let mut counts_by_kind = BTreeMap::new();
    counts_by_kind.insert("api_endpoint".to_string(), 5);
    counts_by_kind.insert("config_key".to_string(), 3);

    DocsExtractResponse {
        command: "docs extract".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        files_scanned: 2,
        files_by_kind,
        facts_extracted: 8,
        facts_inserted: 6,
        facts_deleted: 2,
        counts_by_kind,
        generated_docs_count: 1,
        warnings: vec![],
    }
}

#[test]
fn extract_render_shows_header() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.starts_with("Documentation Extraction\n"));
}

#[test]
fn extract_render_shows_files_scanned() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("2 files scanned"));
}

#[test]
fn extract_render_shows_files_by_kind() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("By kind:"));
    assert!(out.contains("readme  2"));
}

#[test]
fn extract_render_shows_extraction_results() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("Extraction results:"));
    assert!(out.contains("8 facts extracted"));
    assert!(out.contains("6 facts inserted"));
    assert!(out.contains("2 facts deleted"));
    assert!(out.contains("1 generated docs"));
}

#[test]
fn extract_render_shows_facts_by_kind() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("Facts by kind:"));
    assert!(out.contains("api_endpoint  5"));
    assert!(out.contains("config_key  3"));
}

#[test]
fn extract_render_shows_no_warnings() {
    let resp = sample_extract_response();
    let out = resp.render_human();
    assert!(out.contains("No warnings."));
}

#[test]
fn extract_render_shows_warnings() {
    let mut resp = sample_extract_response();
    resp.warnings = vec![
        "Failed to parse docs/api.md".to_string(),
        "Unknown format in CHANGELOG.md".to_string(),
    ];
    let out = resp.render_human();
    assert!(out.contains("2 warnings:"));
    assert!(out.contains("- Failed to parse docs/api.md"));
    assert!(out.contains("- Unknown format in CHANGELOG.md"));
}

#[test]
fn extract_render_singular_file() {
    let mut files_by_kind = BTreeMap::new();
    files_by_kind.insert("readme".to_string(), 1);

    let resp = DocsExtractResponse {
        command: "docs extract".to_string(),
        repo: "repo_test".to_string(),
        repo_path: "/test/repo".to_string(),
        files_scanned: 1,
        files_by_kind,
        facts_extracted: 0,
        facts_inserted: 0,
        facts_deleted: 0,
        counts_by_kind: BTreeMap::new(),
        generated_docs_count: 0,
        warnings: vec![],
    };
    let out = resp.render_human();
    assert!(out.contains("1 file scanned")); // singular
}
